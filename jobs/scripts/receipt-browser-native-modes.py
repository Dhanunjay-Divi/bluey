"""Read-only CLI and a guarded mode transition for the Linux receipt fixture."""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import platform
import stat
import sys
import tarfile
import zipfile

CAPSULE_HELPER = "7a6a3e7a5824bbd94ca5beac62be97b37c240d90b7d79da954d32b5208ea01b5"
INVENTORY = "194d2393e5b922807110f98009a9c6815cffd957f632dda2605cb616cdce4f30"
EXECUTABLES = {
    "chromium/chrome": (278_568_152, "2d18db9d8608b052b6a552ee00ec1e830f93692e928b65ecc67d693bd33fe801"),
    "chromium/chrome_crashpad_handler": (
        1_984_208, "a81943f71526397a3fb7f531e82785f982ca2d6dec181ef5f43303b2d1bd73ee",
    ),
}
DIR = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW | os.O_CLOEXEC
READ = os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW | os.O_CLOEXEC


def check(ok, reason):
    if not ok:
        raise RuntimeError(reason)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encoded(rows):
    return json.dumps(rows, separators=(",", ":")).encode()


def expect_archive_inventory(rows):
    check(len(rows) == 450 and sha(encoded(rows)) == INVENTORY, "native_capsule_inventory")
    found = {path: (size, digest) for path, digest, size, mode in rows
             if path in EXECUTABLES and mode == 0o755}
    check(found == EXECUTABLES, "native_browser_elf_pins")


def readback(fd, rows, executable, runtime):
    by_path = {row[0]: row for row in rows}
    expected_dirs = {parent.as_posix() for path in by_path for parent in Path(path).parents}
    seen = set()
    seen_dirs = set()
    for folder, dirs, files, parent in os.fwalk(".", dir_fd=fd, follow_symlinks=False):
        rel = Path(folder)
        parent_stat = os.fstat(parent)
        check(stat.S_IMODE(parent_stat.st_mode) == 0o700 and parent_stat.st_uid == os.getuid()
              and rel.as_posix() in expected_dirs, "native_mode_directory")
        seen_dirs.add(rel.as_posix())
        for child in dirs:
            info = os.stat(child, dir_fd=parent, follow_symlinks=False)
            check(stat.S_ISDIR(info.st_mode) and (rel / child).as_posix() in expected_dirs,
                  "native_mode_directory")
        for name in files:
            path = (rel / name).as_posix()
            check(path in by_path and path not in seen, "native_mode_entry")
            _, digest, size, _ = by_path[path]
            mode = 0o700 if executable and path in EXECUTABLES else 0o600
            with os.fdopen(os.open(name, READ, dir_fd=parent), "rb") as source:
                before = os.fstat(source.fileno())
                check(stat.S_ISREG(before.st_mode) and before.st_nlink == 1
                      and before.st_uid == os.getuid() and stat.S_IMODE(before.st_mode) == mode
                      and before.st_size == size, "native_mode_file")
                check(runtime["file_digest"](source, size) == digest, "native_mode_content")
                identify = runtime["identity"]
                check(identify(before) == identify(os.fstat(source.fileno()))
                      == identify(os.stat(name, dir_fd=parent, follow_symlinks=False)),
                      "native_mode_replaced")
            seen.add(path)
    check(seen == by_path.keys() and seen_dirs == expected_dirs, "native_mode_missing")


def transition(fd, rows, runtime):
    """Internal permission change only; caller must already hold launch authority."""
    readback(fd, rows, False, runtime)
    by_path = {row[0]: row for row in rows}
    with ExitStack() as stack:
        chrome = os.open("chromium", DIR, dir_fd=fd)
        stack.callback(os.close, chrome)
        handles = {}
        for path in EXECUTABLES:
            name = path.split("/")[-1]
            handle = os.open(name, READ, dir_fd=chrome)
            source = stack.enter_context(os.fdopen(handle, "rb"))
            before = os.fstat(handle)
            _, digest, size, _ = by_path[path]
            check(stat.S_ISREG(before.st_mode) and before.st_uid == os.getuid()
                  and before.st_nlink == 1 and stat.S_IMODE(before.st_mode) == 0o600
                  and before.st_size == size and runtime["file_digest"](source, size) == digest,
                  "native_transition_file")
            check(runtime["identity"](before) == runtime["identity"](os.fstat(handle))
                  == runtime["identity"](os.stat(name, dir_fd=chrome, follow_symlinks=False)),
                  "native_transition_replaced")
            handles[name] = source
        for name, source in handles.items():
            handle = source.fileno()
            check(os.path.samestat(os.fstat(handle), os.stat(name, dir_fd=chrome, follow_symlinks=False)),
                  "native_transition_replaced")
            os.fchmod(handle, 0o700)
        readback(fd, rows, True, runtime)


def evaluate(inputs, playwright, chrome, *, action="plan", independently_admitted=None):
    check(action in {"plan", "readback-executable", "materialize"}, "native_action")
    if action == "materialize":
        check(sys.platform == "linux" and platform.machine() == "x86_64" and os.getuid() != 0,
              "native_linux_actor")
        # No environment value or staged file can substitute for the owning
        # supervisor's live admission check. The CLI never selects this path.
        check(callable(independently_admitted), "native_missing_supervisor")
    path = Path(__file__).resolve().with_name("stage-receipt-browser-capsule.py")
    with path.open("rb") as stream:
        code = stream.read(32 * 1024)
    check(sha(code) == CAPSULE_HELPER, "native_capsule_helper")
    scope = {"__name__": "verified_native_capsule", "__file__": str(path)}
    exec(compile(code, str(path), "exec"), scope)
    original = scope["verified_helper"]

    def bind(name, expected):
        runtime = original(name, expected)
        if name == "stage-browser-receipt-runtime.py":
            def inspect(fd, rows):
                expect_archive_inventory(rows)
                if action == "materialize":
                    check(independently_admitted(INVENTORY, os.fstat(fd)) is True,
                          "native_supervisor_refused")
                    transition(fd, rows, runtime)
                    check(independently_admitted(INVENTORY, os.fstat(fd)) is True,
                          "native_supervisor_refused")
                else:
                    readback(fd, rows, action == "readback-executable", runtime)
            runtime["readback"] = inspect
        return runtime

    scope["verified_helper"] = bind
    receipt = scope["assemble"]("readback", inputs, playwright, chrome)
    # Report only observed/planned permission changes; never authorize browsers.
    return {"tree": receipt["tree"], "action": action, "inventorySha256": INVENTORY,
            "files": receipt["files"], "modeChangePaths": sorted(EXECUTABLES),
            "observedExecutableFiles": 0 if action == "plan" else len(EXECUTABLES),
            "otherFileMode": "0600", "runtimeAdmitted": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("plan", "readback-executable"))
    for name in ("inputs", "playwright", "chrome"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    print(json.dumps(evaluate(args.inputs, args.playwright, args.chrome, action=args.action)))


if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, ValueError, RuntimeError, tarfile.TarError, zipfile.BadZipFile) as error:
        print(f"browser mode check refused: {error if isinstance(error, RuntimeError) else type(error).__name__}",
              file=sys.stderr)
        raise SystemExit(1)
