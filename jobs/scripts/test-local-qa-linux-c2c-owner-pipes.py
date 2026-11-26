"""Real local pipe tests; no network, guest, or file-staging receiver."""
import json
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import time
import unittest


HERE = Path(__file__).resolve().parent
PIPES = runpy.run_path(str(HERE / "local-qa-linux-c2c-owner-pipes.py"))
CODEC_PATH = HERE / "local-qa-linux-c2c-preparation-transport.py"
CODEC = runpy.run_path(str(CODEC_PATH))
RESERVATION = runpy.run_path(str(HERE / "local-qa-linux-c2c-host-reservation.py"))
CHILD_CODEC = """import json, runpy, sys
t = runpy.run_path(sys.argv[1])
cap = json.loads(sys.argv[2])
_, _, request_digest = t["read_request"](sys.stdin.buffer, cap)
reply = t["encode_response"](t["expected_result"](cap), cap, request_digest)
sys.stdout.buffer.write(reply)
sys.stdout.flush()
if sys.argv[3] == "ambiguous":
    raise SystemExit(17)
"""


class OwnerPipeTests(unittest.TestCase):
    def setUp(self):
        self.clients = []
        self.peers = []

    def tearDown(self):
        for client in self.clients:
            for stream in (client.stdin, client.stdout, client.stderr):
                if stream is not None and not stream.closed:
                    stream.close()
            if client.poll() is None:
                client.kill()  # Only the synthetic subprocess created by this test.
            client.wait(timeout=5)

    def open_for(self, code, args=lambda _: [], *, bufsize=0):
        def open_client(cap, deadline):
            client = subprocess.Popen(
                [sys.executable, "-I", "-S", "-B", "-c", code, *args(cap)],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                bufsize=bufsize, env={"LC_ALL": "C"},
            )
            self.clients.append(client)
            return client
        return open_client

    def peer(self, client, cap, deadline):
        self.assertIs(client, self.clients[-1])
        self.peers.append((client.returncode, dict(cap), deadline))
        return True

    def exchange(self, child, *, timeout=3, peer=None):
        return PIPES["make_exchange"](self.open_for(child), peer or self.peer, timeout_seconds=timeout)

    def assert_closed(self):
        client = self.clients[-1]
        self.assertTrue(all(stream.closed for stream in (client.stdin, client.stdout, client.stderr)))

    def test_real_transport_roundtrip_and_terminal_ambiguity_preserve_real_hold(self):
        packet = Path(os.environ["C2C_CODE_PACKET"]).read_bytes()
        bootstrap = {name: (HERE / name).read_bytes() for name in CODEC["BOOTSTRAP"]}
        run = "1" * 32
        binding = {
            "instance": "test-only", "instanceIdentity": [101, 102], "diskIdentity": [103, 104],
            "configSha256": "3" * 64, "imageSha256": "4" * 64,
            **{key: CODEC["PINNED"][key] for key in ("sourceTree", "sourceInventorySha256", "packetSha256")},
            "projection": {"run": run, "unit": f"bluey-jobs-qa-{run}.service", "uid": 1500, "gid": 1500,
                           "runParent": "/tmp/bluey-test-runs-123456789abc"},
        }
        for terminal in ("ok", "ambiguous"):
            with self.subTest(terminal=terminal), tempfile.TemporaryDirectory() as tmp:
                slot = Path(tmp).resolve(strict=True)
                identity = (slot.stat().st_dev, slot.stat().st_ino)
                holds = []
                def reserve(value):
                    hold = RESERVATION["reserve_guest"](slot, identity, value,
                        stopped=lambda exact: exact == binding, verify_instance=lambda exact: exact == binding)
                    holds.append(hold)
                    return hold

                opener = self.open_for(CHILD_CODEC, lambda cap: [
                    str(CODEC_PATH), json.dumps(cap), terminal,
                ])
                exchange = PIPES["make_exchange"](opener, self.peer, timeout_seconds=3)
                prior_peers, prior_clients = len(self.peers), len(self.clients)
                prepare = lambda: CODEC["prepare_at_reserved_guest"](binding, packet, bootstrap,
                                                                     reserve=reserve, exchange=exchange)
                if terminal == "ok":
                    self.assertEqual(prepare(), CODEC["expected_result"](self.peers[-1][1]))
                    start, end = self.peers[prior_peers:]
                    self.assertEqual((start[0], end[0]), (None, 0))
                    self.assertEqual(start[1:], end[1:])
                else:
                    with self.assertRaisesRegex(ValueError, "client_status"):
                        prepare()
                    self.assertEqual(len(self.peers) - prior_peers, 1)
                self.assertEqual(len(holds), 1)
                self.assertTrue(holds[0]["held"]())
                self.assertTrue((slot / "reservation.json").is_file())
                self.assertFalse((slot / "workload-closed.json").exists())
                with self.assertRaisesRegex(ValueError, "already_reserved"):
                    prepare()
                self.assertEqual(len(self.clients) - prior_clients, 1)
                self.assert_closed()

    def test_rejects_observable_reply_before_request_eof(self):
        exchange = self.exchange("import os,time; os.write(1,b'early'); time.sleep(10)")
        with self.assertRaisesRegex(ValueError, "early_reply"):
            exchange(b"x" * PIPES["MAX_REQUEST"], {})
        self.assert_closed()
        with self.assertRaisesRegex(ValueError, "already_attempted"):
            exchange(b"x", {})
        self.assertEqual(len(self.clients), 1)

    def test_diagnostics_are_rejected_as_raw_bytes(self):
        exchange = self.exchange(r"import os,sys; sys.stdin.buffer.read(); os.write(2,b'\xff'); os.write(1,b'ok')")
        with self.assertRaisesRegex(ValueError, r"^c2c_owner_pipes_refused:unexpected_diagnostics$"):
            exchange(b"x", {})
        self.assert_closed()

    def test_oversize_output_and_exact_limit(self):
        prefix = "import sys; sys.stdin.buffer.read(); sys.stdout.buffer.write(b'x' * "
        for size in (PIPES["MAX_REPLY"], PIPES["MAX_REPLY"] + 1):
            with self.subTest(size=size):
                exchange = self.exchange(prefix + str(size) + ")")
                if size == PIPES["MAX_REPLY"]:
                    self.assertEqual(exchange(b"x", {}), b"x" * size)
                else:
                    with self.assertRaisesRegex(ValueError, "reply_size"):
                        exchange(b"x", {})
                self.assert_closed()

    def test_closed_child_input_is_not_treated_as_delivery(self):
        exchange = self.exchange("import os,time; os.close(0); time.sleep(10)")
        with self.assertRaisesRegex(ValueError, "client_io"):
            exchange(b"x" * PIPES["MAX_REQUEST"], {})
        self.assert_closed()

    def test_reply_does_not_qualify_without_client_exit(self):
        exchange = self.exchange(
            "import os,sys,time; sys.stdin.buffer.read(); os.write(1,b'complete'); "
            "os.close(1); os.close(2); time.sleep(10)", timeout=0.4,
        )
        with self.assertRaisesRegex(ValueError, "deadline"):
            exchange(b"x", {})
        self.assert_closed()
        self.assertIsNone(self.clients[-1].poll())  # The adapter didn't kill the client.
        self.assertEqual(len(self.peers), 1)

    def test_preflight_and_postflight_peer_veto(self):
        for approved in ([False], [True, False]):
            with self.subTest(approved=approved):
                approvals = iter(approved)
                exchange = self.exchange("import sys; sys.stdin.buffer.read(); print('reply')",
                                         peer=lambda *_: next(approvals))
                label = "peer_before_exchange" if len(approved) == 1 else "peer_after_exchange"
                with self.assertRaisesRegex(ValueError, label):
                    exchange(b"x", {})
                self.assert_closed()

    def test_veto_reaps_an_already_exited_local_client(self):
        def refuse_after_exit(client, _cap, deadline):
            client.stdin.close()  # The fixture exits cleanly only after EOF.
            while time.monotonic() < deadline:
                result = subprocess.run(["/bin/ps", "-o", "stat=", "-p", str(client.pid)],
                                        capture_output=True, check=True, timeout=0.5)
                if result.stdout.lstrip().startswith(b"Z"):
                    self.assertIsNone(client.returncode)  # No Popen wait or poll yet.
                    return False
                time.sleep(0.01)
            self.fail("synthetic client did not exit during the peer veto")

        exchange = self.exchange("import sys; sys.stdin.buffer.read()", peer=refuse_after_exit)
        with self.assertRaisesRegex(ValueError, "peer_before_exchange"):
            exchange(b"x", {})
        self.assertEqual(self.clients[-1].returncode, 0)
        self.assert_closed()

    def test_refuses_buffered_channels_and_consumes_invalid_frame(self):
        exchange = PIPES["make_exchange"](
            self.open_for("import sys; sys.stdin.buffer.read()", bufsize=8192), self.peer, timeout_seconds=3,
        )
        with self.assertRaisesRegex(ValueError, "client_pipes"):
            exchange(b"x", {})
        self.assertEqual(self.peers, [])
        uncalled = self.exchange("")
        with self.assertRaisesRegex(ValueError, "request"):
            uncalled(bytearray(b"x"), {})
        with self.assertRaisesRegex(ValueError, "already_attempted"):
            uncalled(b"x", {})
        self.assertEqual(len(self.clients), 1)

    def test_open_failure_is_bounded_and_not_masked_by_cleanup(self):
        attempts = []
        def cannot_open(cap, _deadline):
            attempts.append(cap)
            raise OSError("private transport detail")

        exchange = PIPES["make_exchange"](cannot_open, self.peer, timeout_seconds=3)
        with self.assertRaisesRegex(ValueError, r"^c2c_owner_pipes_refused:client_io$"):
            exchange(b"x", {})
        with self.assertRaisesRegex(ValueError, "already_attempted"):
            exchange(b"x", {})
        self.assertEqual(attempts, [{}])
        self.assertEqual(self.peers, [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
