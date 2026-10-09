#!/usr/bin/env python3
"""Operator-only, owner-approved two-key transfer through strict SSH pipes.

Credentials never touch local disk, argv or output. No production service change.
Destination creation is exclusive; an existing credential file is never replaced.
"""
import pathlib
import signal
import subprocess
import sys
import tempfile

SOURCE = r'''
import json, os, pathlib, shlex, socket, sys
try:
    assert os.geteuid() == 0 and socket.gethostname() == "bluey-brain"
    allowed = {"OPENAI_API_KEY", "OPENAI_API_KEYS", "ANTHROPIC_API_KEY", "ANTHROPIC_API_KEYS"}
    values = {}
    for line in pathlib.Path("/etc/bluey-api/bluey-api.env").read_text().splitlines():
        name, sep, raw = line.partition("=")
        if sep and name in allowed:
            parsed = shlex.split(raw, comments=True)
            assert len(parsed) == 1
            values[name] = parsed[0].split(",")[0].strip()
    chosen = {}
    for provider in ("OPENAI", "ANTHROPIC"):
        key = values.get(provider + "_API_KEYS") or values.get(provider + "_API_KEY")
        assert key and 16 <= len(key) <= 512 and all(c.isascii() and (c.isalnum() or c in "_-.") for c in key)
        chosen[provider + "_API_KEYS"] = key
    sys.stdout.write(json.dumps(chosen))
except Exception:
    sys.stderr.write("Selected provider export failed; no credential output\n")
    sys.exit(1)
'''

DESTINATION = r'''
import json, os, pathlib, signal, socket, stat, sys, tempfile
path = "/opt/bluey-assist-preprod/secrets/provider.env"
temporary = None
def interrupted(signum, frame):
    raise RuntimeError("interrupted")
signal.signal(signal.SIGTERM, interrupted)
signal.signal(signal.SIGHUP, interrupted)
signal.signal(signal.SIGINT, interrupted)
try:
    assert os.geteuid() == 0 and socket.gethostname() == "bluey-pinky-assist-preprod"
    parent = pathlib.Path(path).parent
    info = parent.lstat()
    assert stat.S_ISDIR(info.st_mode) and info.st_uid == 0 and stat.S_IMODE(info.st_mode) == 0o700
    raw = sys.stdin.buffer.read(4097)
    assert len(raw) <= 4096
    chosen = json.loads(raw)
    assert set(chosen) == {"OPENAI_API_KEYS", "ANTHROPIC_API_KEYS"}
    for key in chosen.values():
        assert isinstance(key, str) and 16 <= len(key) <= 512
        assert all(c.isascii() and (c.isalnum() or c in "_-.") for c in key)
    payload = "".join(name + "=" + chosen[name] + "\n" for name in sorted(chosen)).encode()
    assert not os.path.lexists(path)
    fd, temporary = tempfile.mkstemp(prefix=".provider-transfer-", dir=parent)
    os.fchmod(fd, 0o600)
    with os.fdopen(fd, "wb") as output:
        output.write(payload)
        output.flush()
        os.fsync(output.fileno())
    info = os.lstat(temporary)
    assert stat.S_ISREG(info.st_mode) and info.st_uid == 0 and stat.S_IMODE(info.st_mode) == 0o600
    assert pathlib.Path(temporary).read_bytes() == payload
    # Atomic create-if-absent. A final file can never be partial or overwritten.
    os.link(temporary, path, follow_symlinks=False)
    directory = os.open(parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
    print("Installed exactly two selected provider credentials; root-only destination")
except Exception:
    sys.stderr.write("Selected provider installation failed; credentials not printed\n")
    sys.exit(1)
finally:
    if temporary is not None:
        os.unlink(temporary)
'''


def main():
    if sys.argv[1:] != ["--owner-approved-existing-keys", "--dedicated-preprod-only"]:
        raise ValueError("Explicit operation guards required")
    def interrupted(signum, frame):
        raise RuntimeError("Operator interrupted; inspect destination metadata before retry")
    for signum in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT):
        signal.signal(signum, interrupted)
    # The source uses the existing independently established strict host entry.
    common = ["ssh", "-F", "/dev/null", "-o", "BatchMode=yes", "-o", "StrictHostKeyChecking=yes",
              "-o", "GlobalKnownHostsFile=/dev/null", "-o", "HostKeyAlgorithms=ssh-ed25519",
              "-o", "ConnectTimeout=10", "-o", "IdentitiesOnly=yes",
              "-i", str(pathlib.Path.home() / ".ssh/id_ed25519")]
    known = str(pathlib.Path.home() / ".config/bluey-pinky-integration/known_hosts")
    destination = common + ["-o", "UserKnownHostsFile=" + known,
                            "-o", "HostKeyAlgorithms=ssh-ed25519",
                            "root@162.243.248.189"]
    # Only trusted code enters the remote command string. No key is an argument.
    import shlex
    destination += ["python3 -I -c " + shlex.quote(DESTINATION)]
    # Source public key comes from the pre-existing trusted Bluey host entry,
    # SHA256:PChMFYjuNreusoSvgkDhA5uIGibArQ63sSo59Cw2k/4; never keyscan.
    with tempfile.TemporaryDirectory(prefix="bluey-provider-host-pin-") as directory:
        pin = pathlib.Path(directory) / "known_hosts"
        pin.write_text("165.227.77.152 ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICMDHEADA1mc647vt+dOHuae2UAEVUUSH/oAr9J3ou+M\n")
        pin.chmod(0o600)
        source = common + ["-o", "UserKnownHostsFile=" + str(pin), "root@165.227.77.152", "python3 -I -"]
        exporter = subprocess.Popen(source, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        importer = None
        try:
            importer = subprocess.Popen(destination, stdin=exporter.stdout)
            exporter.stdout.close()
            exporter.stdin.write(SOURCE.encode())
            exporter.stdin.close()
            source_code = exporter.wait(timeout=30)
            destination_code = importer.wait(timeout=30)
            if source_code or destination_code:
                raise RuntimeError("Guarded transfer failed; inspect destination metadata, do not blindly retry")
        finally:
            for process in (exporter, importer):
                if process is not None and process.poll() is None:
                    process.kill()
                    process.wait()


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("Provider transfer failed: " + type(error).__name__, file=sys.stderr)
        sys.exit(1)
