"""Assemble the exact portal/browser fixture without granting launch permission."""
import argparse
import contextlib
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import tarfile
import zipfile

TREE = "f9b3cf55de39cd0c86a50b5de7546008bbd49210"
RUNTIME = "793273419b521bcdecaf4a649fa43d100a8c089940c073b781d522511b4a6936"
INTAKE = "07426bad5f55f7fe3ea99ccab025571ffabffe310e204c7cf64ec4c20dd682dc"
LEAF = "receipt-prep-browser"
PLAYWRIGHT = "node_modules/playwright-core"
BROWSER = "chromium"
REQUIRED = {
    "driver.mjs", "interview-prep-fixture-gateway.mjs", "interview-prep-witness.mjs",
    f"{PLAYWRIGHT}/index.mjs", f"{PLAYWRIGHT}/package.json", f"{PLAYWRIGHT}/browsers.json",
    f"{BROWSER}/chrome",
}
MIN_FREE_AFTER_STAGE = 16 * 1024**3


def require(ok, reason):
    if not ok:
        raise RuntimeError(reason)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def verified_helper(name, expected):
    path = Path(__file__).resolve().with_name(name)
    with path.open("rb") as stream:
        code = stream.read(32 * 1024)
    require(digest(code) == expected, "capsule_helper_pin")
    scope = {"__name__": "verified_browser_capsule_helper", "__file__": str(path)}
    exec(compile(code, str(path), "exec"), scope)
    return scope


def portal_inventory(portal):
    reserved = {"node_modules", BROWSER}
    rows = []
    for name, (data, mode) in sorted(portal.items()):
        require(name.split("/")[0] not in reserved and mode == 0o600, "portal_runtime_overlap")
        rows.append((name, digest(data), len(data), mode))
    return rows


def inside(members, target):
    for name, size, mode, stream in members:
        prefix = f"{LEAF}/{target}/"
        require(name.startswith(prefix) and len(name) > len(prefix), "runtime_scope")
        yield name[len(LEAF) + 1:], size, mode, stream


def named_leaf(fd, parent, leaf, runtime):
    info = os.fstat(fd)
    return (info.st_uid == os.getuid() and stat.S_IMODE(info.st_mode) == 0o700
            and stat.S_ISDIR(info.st_mode)
            and (info.st_dev, info.st_ino) == runtime["inode"](parent / leaf))


def assemble(mode, inputs, playwright_path, chrome_path):
    runtime = verified_helper("stage-browser-receipt-runtime.py", RUNTIME)
    intake = verified_helper("stage-browser-receipt-intake.py", INTAKE)
    receipt, payload = intake["load_inputs"](inputs)
    require(receipt["tree"] == TREE, "capsule_product_tree")
    portal = payload["portal"]
    portal_rows = portal_inventory(portal)
    # Reuse the pinned archive parser, writer and physical readback. Narrow
    # its required file/ELF policy to this capsule; Node is transferred
    # independently by JourneyNode and must not be added here.
    runtime["REQUIRED"] = {name for name in REQUIRED if "/" in name}
    runtime["ELF"] = {f"{BROWSER}/chrome"}
    with contextlib.ExitStack() as stack:
        pw, pw_check = runtime["open_verified"](stack, playwright_path, runtime["ARCHIVES"]["playwright"])
        chrome, chrome_check = runtime["open_verified"](stack, chrome_path, runtime["ARCHIVES"]["chrome"])
        groups = [(inside(runtime["tar_members"](pw, False), PLAYWRIGHT), 106, 12_701_224),
                  (inside(runtime["browser_members"](chrome), BROWSER), 303, 396_335_288)]
        root, parent, recheck = runtime["running_root"](stack)
        if mode == "stage":
            space = os.fstatvfs(parent)
            require(space.f_frsize * space.f_bavail >= MIN_FREE_AFTER_STAGE
                    + sum(size for _, _, size in groups)
                    + sum(row[2] for row in portal_rows), "capsule_disk_floor")
            intake["stage"](root, LEAF, portal, recheck)
        fd = os.open(LEAF, runtime["DIR_FLAGS"], dir_fd=parent)
        stack.callback(os.close, fd)
        require(named_leaf(fd, root, LEAF, runtime), "capsule_leaf_identity")
        rows = runtime["consume"](groups, fd if mode == "stage" else None)
        rows = sorted(portal_rows + rows)
        require(len(rows) == 450 and REQUIRED <= {row[0] for row in rows},
                "combined_capsule_inventory")
        runtime["readback"](fd, rows)
        require(named_leaf(fd, root, LEAF, runtime), "capsule_leaf_identity")
        pw_check()
        chrome_check()
        recheck()
    encoded = json.dumps(rows, separators=(",", ":")).encode()
    return {"tree": TREE, "mode": mode, "portalReceiptSha256": intake["RECEIPT"],
            "capsuleInventorySha256": digest(encoded), "files": len(rows),
            "bytes": sum(row[2] for row in rows),
            "fileMode": "0600", "chromiumLaunchEnabled": False,
            "publisherTrustEstablished": False, "runtimeAdmitted": False}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("stage", "readback"))
    for argument in ("inputs", "playwright", "chrome"):
        parser.add_argument(argument, type=Path)
    args = parser.parse_args()
    os.umask(0o077)
    print(json.dumps(assemble(args.mode, args.inputs, args.playwright, args.chrome)))


if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, ValueError, RuntimeError, tarfile.TarError, zipfile.BadZipFile) as error:
        print(f"receipt browser capsule refused: {error if isinstance(error, RuntimeError) else type(error).__name__}",
              file=sys.stderr)
        raise SystemExit(1)
