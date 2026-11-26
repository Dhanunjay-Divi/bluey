#!/usr/bin/env python3
"""Portable syntax and parser controls for the root C2C manager observer."""
import importlib.util
from pathlib import Path
import unittest


PATH = Path(__file__).with_name("local-qa-linux-c2c-manager-observer.py")
SPEC = importlib.util.spec_from_file_location("c2c_manager_observer", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def unit_lines(**changed):
    value = {
        "Id": "bluey-jobs-qa-" + "a" * 32 + ".service",
        "InvocationID": "b" * 32,
        "MainPID": "1234",
        "ControlGroup": "/system.slice/bluey-jobs-qa-" + "a" * 32 + ".service",
        "PrivateNetwork": "yes", "NoNewPrivileges": "yes", "CapabilityBoundingSet": "",
        "Restart": "no", "KillMode": "control-group", "ActiveState": "active", "SubState": "running",
        "Type": "exec", "NetworkNamespacePath": "", "JoinsNamespaceOf": "", "PrivateTmp": "no",
        "AmbientCapabilities": "", "ProtectSystem": "strict", "ProtectHome": "yes", "ExitType": "cgroup",
        "ReadWritePaths": "/tmp/bluey-test-runs-aaaaaaaaaaaa",
        "RestrictAddressFamilies": "AF_INET AF_INET6 AF_NETLINK AF_UNIX",
    }
    value.update(changed)
    return ("\n".join("{}={}".format(name, value[name]) for name in MODULE.FIELDS) + "\n").encode("ascii")


def waiting(**changed):
    value = {
        "schema": "bluey-jobs-c2c-waiting-v1", "run": "a" * 32,
        "unitName": "bluey-jobs-qa-" + "a" * 32 + ".service", "bootId": "b" * 32,
        "invocationId": "c" * 32, "unitCgroup": "/system.slice/bluey-jobs-qa-" + "a" * 32 + ".service",
        "networkNamespace": "net:[123]", "root": "/tmp/bluey-test-runs-aaaaaaaaaaaa/run.1.1234.123",
        "rootDevice": 1, "rootInode": 2, "markerSha256": "d" * 64, "actorUid": 999,
        "mainPid": 1234, "mainStartTime": 5, "actorPid": 1235, "actorStartTime": 6,
        "nonce": "e" * 32, "waitingNs": 7,
    }
    value.update(changed)
    return value


class ParserTests(unittest.TestCase):
    def test_exact_systemctl_projection_parses(self):
        parsed = MODULE.parse_unit_show(unit_lines())
        self.assertEqual(set(parsed), set(MODULE.FIELDS))
        plan = {"run": "a" * 32, "unit": parsed["Id"], "uid": 999, "gid": 988,
                "runParent": "/tmp/bluey-test-runs-aaaaaaaaaaaa"}
        MODULE.validate_unit_show(parsed, plan, 1234, "b" * 32, parsed["ControlGroup"])

    def test_extra_or_missing_or_unsafe_systemctl_property_refuses(self):
        for raw in (unit_lines() + b"Other=value\n", unit_lines().replace(b"Type=exec\n", b""),
                    unit_lines(PrivateNetwork="no")):
            with self.assertRaisesRegex(RuntimeError, MODULE.REFUSED):
                value = MODULE.parse_unit_show(raw)
                if raw == unit_lines(PrivateNetwork="no"):
                    MODULE.validate_unit_show(value, {"run": "a" * 32, "unit": value["Id"], "uid": 999,
                                                       "gid": 988, "runParent": "/tmp/bluey-test-runs-aaaaaaaaaaaa"},
                                              1234, "b" * 32, value["ControlGroup"])

    def test_waiting_is_only_a_strict_selector(self):
        parsed = MODULE.parse_waiting_selector(waiting())
        self.assertEqual(parsed["actorPid"], 1235)
        for delta in ({"actorPid": 1}, {"root": "/tmp/not-owned/run.1.2.123"},
                      {"unexpected": 1}, {"markerSha256": "D" * 64}, {"actorPid": True},
                      {"nonce": "0" * 32}):
            value = waiting()
            value.update(delta)
            with self.assertRaisesRegex(RuntimeError, MODULE.REFUSED):
                MODULE.parse_waiting_selector(value)

    def test_proc_id_columns_must_not_change_identity(self):
        valid = "Uid:\t999\t999\t999\t999\nGid:\t988\t988\t988\t988\n"
        self.assertEqual(MODULE.parse_proc_ids(valid), (999, 988))
        for invalid in ("Uid:\t999\t1000\t999\t999\nGid:\t988\t988\t988\t988\n",
                        "Uid:\t999\t999\t999\t999\nGid:\t988\t989\t988\t988\n"):
            with self.assertRaisesRegex(RuntimeError, MODULE.REFUSED):
                MODULE.parse_proc_ids(invalid)


if __name__ == "__main__":
    unittest.main()
