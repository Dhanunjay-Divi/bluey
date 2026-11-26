#!/usr/bin/env python3
"""Portable actor-plan controls; no guest, browser, or network is started."""

from copy import deepcopy
import hashlib
import json
from pathlib import Path
import runpy
import unittest


HERE = Path(__file__).resolve().parent
PLAN_MODULE = HERE / "local-qa-linux-c2c-actor-plan.py"
WAITER_MODULE = HERE / "local-qa-linux-c2c-wait-actor.py"
REFUSED = r"c2c_actor_plan_refused:"
RUN = "1" * 32
NODE_SHA256 = "94efd062ed066c1a0b332e18249859c905e920eeada5df55d24c99b919629a90"
CLOSURE_ROOT = f"/run/bluey-jobs-c2c-browser/{RUN}"


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def capability(**changes):
    value = {
        "run": RUN,
        "reservationSha256": "2" * 64,
        "packetSha256": "3" * 64,
        "codeInventorySha256": "4" * 64,
        "sourceTree": "5" * 40,
        "sourceInventorySha256": "6" * 64,
    }
    value.update(changes)
    return value


def stage(**changes):
    value = {
        "run": RUN,
        "packetSha256": "3" * 64,
        "codeInventorySha256": "4" * 64,
        "actorFiles": 31,
        "managerFiles": 11,
        "releasePublished": False,
        "runtimeQualified": False,
    }
    value.update(changes)
    return value


def inventory_rows():
    return [
        {"path": "receipt-journey-node/toolchain/bin/node", "sha256": NODE_SHA256,
         "size": 124902584, "mode": 0o555},
        {"path": "receipt-prep-browser/chromium/chrome", "sha256": "7" * 64,
         "size": 278568152, "mode": 0o555},
        {"path": "receipt-prep-browser/node_modules/playwright-core/browsers.json",
         "sha256": "8" * 64, "size": 1939, "mode": 0o444},
        {"path": "receipt-prep-browser/node_modules/playwright-core/index.mjs",
         "sha256": "9" * 64, "size": 1063, "mode": 0o444},
        {"path": "receipt-prep-browser/node_modules/playwright-core/package.json",
         "sha256": "a" * 64, "size": 876, "mode": 0o444},
    ]


def inventory(rows=None):
    value = {"version": 1, "files": inventory_rows() if rows is None else rows}
    return (json.dumps(value, separators=(",", ":")) + "\n").encode("ascii")


class ActorPlanTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = runpy.run_path(str(PLAN_MODULE), run_name="_portable_c2c_actor_plan_test")
        cls.inventory = inventory()
        cls.inventory_sha256 = sha(cls.inventory)

    def make(self, cap=None, staged=None, root=CLOSURE_ROOT, raw=None, digest=None):
        return self.module["make_plan"](
            capability() if cap is None else cap,
            stage() if staged is None else staged,
            root,
            self.inventory if raw is None else raw,
            self.inventory_sha256 if digest is None else digest,
        )

    def refused(self, *args, **kwargs):
        with self.assertRaisesRegex(ValueError, REFUSED):
            self.make(*args, **kwargs)

    def test_plan_is_deterministic_exact_and_accepted_by_real_waiter_parser(self):
        first, second = self.make(), self.make()
        expected = {
            "schema": "bluey-jobs-c2c-actor-plan-v1",
            "run": RUN,
            "node": {
                "path": CLOSURE_ROOT + "/receipt-journey-node/toolchain/bin/node",
                "sha256": NODE_SHA256,
            },
            "codeInventorySha256": capability()["codeInventorySha256"],
            "sourceTree": capability()["sourceTree"],
            "sourceInventorySha256": capability()["sourceInventorySha256"],
        }
        expected_raw = (json.dumps(expected, sort_keys=True, separators=(",", ":")) + "\n").encode("ascii")
        self.assertEqual((first, second), (expected_raw, expected_raw))

        waiter = runpy.run_path(str(WAITER_MODULE), run_name="_portable_actor_plan_waiter_parser")
        stamp, calls, gate = ("synthetic-plan-stamp",), [], object()

        def named_bytes(fd, name, cap, *, uid, mode, digest=None):
            calls.append((fd, name, cap, uid, mode, digest))
            return first, stamp

        waiter["read_plan"].__globals__["named_bytes"] = named_bytes
        parsed, observed_stamp = waiter["read_plan"](gate, sha(first), RUN)
        self.assertEqual((parsed, observed_stamp), (expected, stamp))
        self.assertEqual(calls, [(gate, "plan.json", 4096, 0, 0o444, sha(first))])

    def test_stale_capability_or_stage_binding_is_refused(self):
        cases = [
            {"cap": capability(run="0" * 32)},
            {"cap": dict(capability(), extra=True)},
            {"cap": capability(packetSha256="0" * 64)},
            {"staged": stage(run="2" * 32)},
            {"staged": stage(packetSha256="b" * 64)},
            {"staged": stage(codeInventorySha256="c" * 64)},
            {"staged": stage(actorFiles=28)},
            {"staged": stage(managerFiles=10)},
            {"staged": stage(releasePublished=True)},
            {"staged": stage(runtimeQualified=True)},
            {"staged": dict(stage(), extra=True)},
        ]
        for index, case in enumerate(cases):
            with self.subTest(index=index):
                self.refused(**case)

    def test_browser_inventory_digest_node_mode_and_size_are_bound(self):
        self.refused(digest="b" * 64)
        mutations = [
            (0, {"sha256": "b" * 64}),
            (0, {"mode": 0o444}),
            (0, {"size": 124902583}),
            (1, {"mode": 0o444}),
            (2, {"mode": 0o555}),
            (2, {"size": -1}),
            (2, {"size": 320 * 1024 * 1024 + 1}),
        ]
        for index, (row_index, change) in enumerate(mutations):
            rows = inventory_rows()
            rows[row_index] = dict(rows[row_index], **change)
            raw = inventory(rows)
            with self.subTest(index=index, change=change):
                self.refused(raw=raw, digest=sha(raw))

    def test_missing_unsorted_duplicate_and_ancestor_rows_are_refused(self):
        variants = []
        for index in range(len(inventory_rows())):
            rows = inventory_rows()
            rows.pop(index)
            variants.append(rows)
        rows = inventory_rows()
        rows[0], rows[1] = rows[1], rows[0]
        variants.append(rows)
        rows = inventory_rows()
        rows.insert(1, deepcopy(rows[0]))
        variants.append(rows)
        rows = inventory_rows()
        rows.insert(1, {"path": "receipt-prep-browser/chromium", "sha256": "b" * 64,
                        "size": 1, "mode": 0o444})
        variants.append(rows)
        rows = inventory_rows()
        rows.append({"path": "receipt-prep-browser/chromium/extra", "sha256": "b" * 64,
                     "size": 220 * 1024 * 1024, "mode": 0o444})
        rows.sort(key=lambda row: row["path"])
        variants.append(rows)
        for index, rows in enumerate(variants):
            raw = inventory(rows)
            with self.subTest(index=index):
                self.refused(raw=raw, digest=sha(raw))

    def test_invalid_closure_and_link_like_inventory_paths_are_refused(self):
        roots = [
            f"run/bluey-jobs-c2c-browser/{RUN}",
            f"/run/bluey-jobs-c2c-browser/{RUN}/",
            f"/run/bluey-jobs-c2c-browser/{RUN}/../{RUN}",
            "/run/bluey-jobs-c2c-browser/" + "2" * 32,
            Path(CLOSURE_ROOT),
        ]
        for root in roots:
            with self.subTest(root=root):
                self.refused(root=root)

        paths = [
            "../receipt-journey-node/toolchain/bin/node",
            "/receipt-journey-node/toolchain/bin/node",
            "receipt-journey-node/../toolchain/bin/node",
            "receipt-prep-browser/.git/config",
            "receipt-prep-browser/chromium//chrome",
            "other/chrome",
        ]
        for path in paths:
            rows = inventory_rows()
            rows[0] = dict(rows[0], path=path)
            rows.sort(key=lambda row: row["path"])
            raw = inventory(rows)
            with self.subTest(path=path):
                self.refused(raw=raw, digest=sha(raw))

    def test_noncanonical_inventory_and_argument_types_are_refused(self):
        value = {"version": 1, "files": inventory_rows()}
        noncanonical = [
            (json.dumps(value, indent=2) + "\n").encode("ascii"),
            inventory() + b"\n",
            json.dumps(value, separators=(",", ":")).encode("ascii"),
            bytearray(inventory()),
        ]
        for index, raw in enumerate(noncanonical):
            with self.subTest(index=index):
                self.refused(raw=raw, digest=sha(bytes(raw)))
        self.refused(digest=9)
        self.refused(cap=[])
        self.refused(staged=[])


if __name__ == "__main__":
    unittest.main()
