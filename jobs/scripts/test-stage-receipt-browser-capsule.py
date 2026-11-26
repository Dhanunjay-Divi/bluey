"""Verify the staged capsule's boundaries without starting any browser."""
import importlib.util
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "capsule", Path(__file__).with_name("stage-receipt-browser-capsule.py")
)
CAPSULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CAPSULE)


class CapsulePolicy(unittest.TestCase):
    def test_portal_has_no_claim_to_runtime_directories(self):
        rows = CAPSULE.portal_inventory({"driver.mjs": (b"source", 0o600)})
        self.assertEqual(rows, [("driver.mjs", CAPSULE.digest(b"source"), 6, 0o600)])
        for name in ("chromium", "chromium/chrome", "node_modules/playwright-core/index.mjs"):
            with self.subTest(name=name), self.assertRaisesRegex(RuntimeError, "^portal_runtime_overlap$"):
                CAPSULE.portal_inventory({name: (b"fixture", 0o600)})

    def test_portal_cannot_supply_executable_modes(self):
        with self.assertRaisesRegex(RuntimeError, "^portal_runtime_overlap$"):
            CAPSULE.portal_inventory({"driver.mjs": (b"fixture", 0o700)})

    def test_browser_path_partition_excludes_node_and_near_prefixes(self):
        def project(name):
            return list(CAPSULE.inside([(name, 0, 0o700, io.BytesIO())], "chromium"))
        self.assertEqual(project("receipt-prep-browser/chromium/chrome")[0][0], "chromium/chrome")
        for name in ("receipt-journey-node/toolchain/bin/node", "receipt-prep-browser/chromium2/chrome",
                     "receipt-prep-browser/chromium/"):
            with self.subTest(name=name), self.assertRaisesRegex(RuntimeError, "^runtime_scope$"):
                project(name)

    def test_replaced_destination_is_not_the_held_leaf(self):
        runtime = CAPSULE.verified_helper("stage-browser-receipt-runtime.py", CAPSULE.RUNTIME)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            path = root / CAPSULE.LEAF
            path.mkdir(mode=0o700)
            fd = os.open(path, runtime["DIR_FLAGS"])
            try:
                self.assertTrue(CAPSULE.named_leaf(fd, root, CAPSULE.LEAF, runtime))
                path.rename(path.with_name("prior"))
                path.mkdir(mode=0o700)
                self.assertFalse(CAPSULE.named_leaf(fd, root, CAPSULE.LEAF, runtime))
            finally:
                os.close(fd)


def real_inputs(inputs):
    """Run once in a new canonical root using the sealed, read-only input set."""
    staged = CAPSULE.assemble("stage", *inputs)
    observed = CAPSULE.assemble("readback", *inputs)
    assert {**staged, "mode": "readback"} == observed
    assert observed["files"] == 450 and observed["runtimeAdmitted"] is False
    assert observed["capsuleInventorySha256"] == (
        "194d2393e5b922807110f98009a9c6815cffd957f632dda2605cb616cdce4f30"
    )
    root = Path(os.environ["BLUEY_TEST_RUN_ROOT"])
    assert not (root / "receipt-journey-node").exists()
    with unittest.TestCase().assertRaises(FileExistsError):
        CAPSULE.assemble("stage", *inputs)
    chrome = root / CAPSULE.LEAF / "chromium/chrome"
    chrome.chmod(0o700)  # Deliberate owned fixture drift; outer exec fence remains active.
    with unittest.TestCase().assertRaisesRegex(RuntimeError, "^stage_file_readback$"):
        CAPSULE.assemble("readback", *inputs)
    print("450-file exact capsule readback matched; overwrite and execute-mode drift refused")


if __name__ == "__main__":
    if len(sys.argv) == 5 and sys.argv[1] == "--exact-inputs":
        real_inputs(tuple(map(Path, sys.argv[2:])))
    else:
        unittest.main()
