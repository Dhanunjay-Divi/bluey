#!/usr/bin/env python3
"""Portable validation controls; never create a unit or open a namespace."""
import importlib.util
from pathlib import Path
import unittest


PATH = Path(__file__).with_name("local-qa-linux-c2c-launch-custody.py")
SPEC = importlib.util.spec_from_file_location("launch_custody", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def claim(**changes):
    value = {"run": "a" * 32, "unit": "bluey-jobs-qa-" + "a" * 32 + ".service",
             "invocation": "b" * 32, "boot": "12345678-abcd-1234-abcd-123456789abc",
             "actorUid": 999, "workloadNetns": 4026532555, "mainPid": 1234, "mainStartTime": 99}
    value.update(changes)
    return value


def manager(**changes):
    value = {"Id": "bluey-jobs-qa-" + "a" * 32 + ".service", "InvocationID": "", "MainPID": "0",
             "ControlGroup": "", "PrivateNetwork": "yes", "NoNewPrivileges": "yes", "CapabilityBoundingSet": "",
             "Restart": "no", "KillMode": "control-group", "ActiveState": "inactive", "SubState": "dead",
             "Type": "exec", "NetworkNamespacePath": "", "JoinsNamespaceOf": "", "PrivateTmp": "no",
             "AmbientCapabilities": "", "ProtectSystem": "strict", "ProtectHome": "yes", "ExitType": "cgroup",
             "ReadWritePaths": "/tmp/bluey-test-runs-aaaaaaaaaaaa",
             "RestrictAddressFamilies": "AF_INET AF_INET6 AF_NETLINK AF_UNIX", "LoadState": "not-found", "Job": ""}
    value.update(changes)
    return value


def encode_manager(value):
    return ("\n".join("{}={}".format(key, value[key]) for key in MODULE.MANAGER_FIELDS) + "\n").encode()


class LaunchClaimTests(unittest.TestCase):
    def test_exact_claim_is_closed(self):
        self.assertEqual(MODULE.validate_launch_claim(claim()), claim())

    def test_extra_and_cross_bound_fields_refuse(self):
        for value in (claim(extra=True), claim(unit="bluey-jobs-qa-" + "c" * 32 + ".service"),
                      claim(boot="B" * 36), claim(workloadNetns=True), claim(invocation="0" * 32)):
            with self.subTest(value=value):
                with self.assertRaisesRegex(RuntimeError, MODULE.REFUSED):
                    MODULE.validate_launch_claim(value)

    def test_only_closed_vacant_and_start_states_are_transient(self):
        plan = {"run": "a" * 32, "unit": "bluey-jobs-qa-" + "a" * 32 + ".service", "uid": 999,
                "gid": 988, "runParent": "/tmp/bluey-test-runs-aaaaaaaaaaaa"}
        vacant = MODULE.parse_manager_show(encode_manager(manager()))
        self.assertEqual(MODULE._phase(vacant, plan, None), (True, None))
        starting = MODULE.parse_manager_show(encode_manager(manager(LoadState="loaded", ActiveState="activating",
            SubState="start", Job="42", InvocationID="b" * 32)))
        self.assertEqual(MODULE._phase(starting, plan, None), (True, "b" * 32))
        running = MODULE.parse_manager_show(encode_manager(manager(LoadState="loaded", ActiveState="active",
            SubState="running", Job="", InvocationID="b" * 32, MainPID="1234",
            ControlGroup="/system.slice/bluey-jobs-qa-" + "a" * 32 + ".service")))
        self.assertEqual(MODULE._phase(running, plan, "b" * 32), (False, "b" * 32))
        with self.assertRaisesRegex(RuntimeError, MODULE.REFUSED):
            MODULE._phase(dict(starting, InvocationID="c" * 32), plan, "b" * 32)


if __name__ == "__main__":
    unittest.main()
