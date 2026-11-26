"""Portable capability, framing and bootstrap writes for the C2C receiver.

The guest namespace, authority and final actor staging are simulated.
"""
from functools import partial
import importlib.util
import io
import os
from pathlib import Path
import stat
import sys
import tempfile
import unittest
from unittest.mock import patch


HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("receive", HERE / "local-qa-linux-c2c-receive-preparation.py")
receive = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(receive)


def cap():
    return {"run": "a" * 32, "reservationSha256": "b" * 64, "packetSha256": "c" * 64,
            "codeInventorySha256": "d" * 64, "sourceTree": "e" * 40,
            "sourceInventorySha256": "f" * 64, "browserInventorySha256": "1" * 64}


class ReceivePreparationTests(unittest.TestCase):
    def test_closed_capability_accepts_only_all_typed_nonzero_fields(self):
        self.assertEqual(receive._capability(cap()), cap())
        bad = {**cap(), "browserInventorySha256": "0" * 64}
        with self.assertRaisesRegex(RuntimeError, "capability"):
            receive._capability(bad)
        bad = {**cap(), "sourceTree": "x" * 40}
        with self.assertRaisesRegex(RuntimeError, "capability"):
            receive._capability(bad)

    def test_closed_capability_rejects_extra_or_missing_fields(self):
        bad = {**cap(), "extra": "z"}
        with self.assertRaisesRegex(RuntimeError, "capability"):
            receive._capability(bad)
        bad = cap()
        del bad["run"]
        with self.assertRaisesRegex(RuntimeError, "capability"):
            receive._capability(bad)

    def test_exact_frame_stages_bootstrap_once_and_retains_ambiguous_state(self):
        sources = {name: (HERE / name).read_bytes() for name in receive.TRIO}
        packet = Path(os.environ["C2C_CODE_PACKET"]).read_bytes()
        browser = Path(os.environ["C2C_BROWSER_INVENTORY"]).read_bytes()
        module = receive._module_from_bytes
        transport_path = HERE / "local-qa-linux-c2c-preparation-transport.py"
        transport_raw = transport_path.read_bytes()
        transport = module(transport_path, transport_raw)
        self.assertEqual(transport.sha(transport_raw), receive.TRANSPORT_SHA256)
        cap_value = {**transport.PINNED, "run": "a" * 32, "reservationSha256": "b" * 64}
        frame = transport.encode_request(cap_value, packet, sources)
        for scenario in ("clean", "extra-byte", "existing", "late-change"):
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory() as temporary:
                base = Path(temporary).resolve()
                base.chmod(0o755)
                initial = base.stat()
                group = initial.st_gid if sys.platform == "darwin" else os.getegid()
                boot = base / cap_value["run"]
                staged_calls = []

                def base_current(fd):
                    self.assertTrue(os.path.samestat(initial, os.fstat(fd)))
                    self.assertTrue(os.path.samestat(initial, base.lstat()))

                def read_module(path, digest):
                    self.assertEqual(path, transport_path)
                    self.assertEqual(digest, receive.TRANSPORT_SHA256)
                    return transport

                def load_plan(path, raw):
                    self.assertEqual(path, boot / receive.ACTOR_PLAN)
                    self.assertEqual(raw, path.read_bytes())
                    planner = module(path, raw)

                    def simulate_stage(incoming, actual_cap, browser_pin, *, admitted):
                        staged_calls.append(True)
                        result = transport.expected_result(cap_value)
                        self.assertEqual(incoming, packet)
                        self.assertTrue(admitted({**actual_cap, "browserInventorySha256": browser_pin,
                                                 "browserRoot": result["browserRoot"]}))
                        receipt = {key: result[key] for key in ("run", "packetSha256", "codeInventorySha256",
                                    "actorFiles", "managerFiles", "releasePublished", "runtimeQualified")}
                        plan = planner.make_plan(actual_cap, receipt, result["browserRoot"], browser, browser_pin)
                        self.assertEqual(transport.sha(plan), result["actorPlanSha256"])
                        if scenario == "late-change":
                            leaf = boot / receive.ACTOR_PLAN
                            original = leaf.read_bytes()
                            leaf.write_bytes(bytes([original[0] ^ 1]) + original[1:])
                        return result

                    planner.stage_code_and_plan = simulate_stage
                    return planner

                if scenario == "existing":
                    boot.mkdir(0o700)
                with patch.multiple(receive, BOOTSTRAP_BASE=base, TRANSPORT=transport_path, _root_linux=lambda: True,
                                    _open_base=lambda: os.open(base, receive.DIR), _base_current=base_current,
                                    _read_module=read_module, _module_from_bytes=load_plan,
                                    _install=partial(receive._install, owner=os.getuid(), group=group),
                                    _verify_bootstrap=partial(receive._verify_bootstrap, owner=os.getuid(), group=group)):
                    stream = io.BytesIO(frame + b"x" if scenario == "extra-byte" else frame)
                    admitted = lambda value: value == cap_value and value is not cap_value
                    if scenario == "clean":
                        result = receive.receive_preparation(stream, cap_value, admitted=admitted)
                        self.assertEqual(transport.read_response(result, cap_value, transport.sha(frame)),
                                         transport.expected_result(cap_value))
                    else:
                        error = {"extra-byte": "request_extra_bytes", "existing": "bootstrap_present",
                                 "late-change": "bootstrap_leaf"}[scenario]
                        with self.assertRaisesRegex((RuntimeError, ValueError), error):
                            receive.receive_preparation(stream, cap_value, admitted=admitted)
                self.assertEqual(len(staged_calls), int(scenario in {"clean", "late-change"}))
                self.assertEqual(boot.exists(), scenario != "extra-byte")
                if scenario in {"clean", "late-change"}:
                    self.assertEqual({entry.name for entry in boot.iterdir()}, sources.keys())
                    self.assertEqual(stat.S_IMODE(boot.stat().st_mode), 0o700)
                    self.assertTrue(all(stat.S_IMODE(entry.stat().st_mode) == 0o600 for entry in boot.iterdir()))
                if scenario == "existing":
                    self.assertEqual(list(boot.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
