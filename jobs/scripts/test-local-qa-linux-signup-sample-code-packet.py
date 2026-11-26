import json
from pathlib import Path
import runpy
import unittest


HERE = Path(__file__).resolve().parent
PACKET = runpy.run_path(str(HERE / "local-qa-linux-signup-sample-code-packet.py"))
OLD_MANAGER = "61e026cf2e5216e44a0a3a06893f9de45824581996a05f51768824b4b3d94256"
TREE, INVENTORY = "1" * 40, "2" * 64


def frame(event):
    return (json.dumps({"event": event, "sourceTree": TREE, "sourceInventorySha256": INVENTORY,
        "evidence": {"browserSignup": True, "candidateWorkspace": True, "signupMailCount": 1,
                     "applicationCount": 0, "submittedCount": 0, "sampleResumeImported": True}},
        separators=(",", ":")) + "\n").encode("ascii")


class CodePacketTests(unittest.TestCase):
    def test_waiter_only_changes_the_fixed_actor(self):
        raw = (HERE / PACKET["BASE_WAITER"]).read_bytes()
        waiter = PACKET["render_waiter"](raw)
        self.assertEqual(waiter.replace(PACKET["ACTOR"].encode(), b"local-qa-linux-c2c-native.mjs"), raw)
        with self.assertRaisesRegex(ValueError, ":waiter_preimage"):
            PACKET["render_waiter"](raw + b"\n")

    def test_manager_reuses_launch_controls_and_pins_the_new_result_validator(self):
        previous_path = HERE / PACKET["BASE_MANAGER"]
        waiter = PACKET["render_waiter"]((HERE / PACKET["BASE_WAITER"]).read_bytes())
        previous = previous_path.read_bytes()
        new = PACKET["render_manager"](previous, OLD_MANAGER, PACKET["sha"](waiter))
        control = {"__file__": str(previous_path), "__name__": "_base"}
        result = {"__file__": str(previous_path), "__name__": "_test"}
        exec(compile(previous, str(previous_path), "exec"), control)
        exec(compile(new, PACKET["MANAGER"], "exec"), result)
        self.assertEqual(result["EXPECTED_WAITER_SHA"], PACKET["sha"](waiter))
        self.assertEqual(result["RUNTIME_HELPERS"], control["RUNTIME_HELPERS"])
        self.assertEqual(result["HELPERS"], control["HELPERS"])
        args = ("1" * 32, 1001, 1001, "/tmp/bluey-test-runs-" + "2" * 12, "3" * 64, "/usr/bin/python3.12")
        old_plan = control["build_plan"](*args)
        new_plan = result["build_plan"](*args)
        self.assertEqual(new_plan, {**old_plan, "argv": tuple(
            flag.replace(PACKET["BASE_WAITER"], PACKET["WAITER"]) for flag in old_plan["argv"])})

        def pinned_file(path, digest, limit, *, retain):
            self.assertEqual(path, HERE / PACKET["TERMINAL"])
            body = path.read_bytes()
            self.assertTrue(len(body) < limit and retain)
            self.assertEqual(PACKET["sha"](body), digest)
            return (), body

        result["pinned_file"] = pinned_file
        expected = "local_qa_linux_browser_sample_resume_completed"
        self.assertEqual(result["terminal"](frame(expected), TREE, INVENTORY)["event"], expected)
        with self.assertRaises(ValueError):
            result["terminal"](frame("local_qa_linux_c2c_native_completed"), TREE, INVENTORY)
        with self.assertRaises(ValueError):
            control["terminal"](frame(expected), TREE, INVENTORY)
        with self.assertRaisesRegex(ValueError, ":manager_preimage"):
            PACKET["render_manager"](previous + b"\n", OLD_MANAGER, "9" * 64)

    def test_builder_rejects_a_fake_base_before_touching_code(self):
        with self.assertRaisesRegex(ValueError, ":base_packet"):
            PACKET["build_candidate"](b"fake" * 138240, HERE / "nonexistent")


if __name__ == "__main__":
    unittest.main()
