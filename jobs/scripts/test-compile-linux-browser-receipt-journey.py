"""Policy-decision controls; the Linux kernel and actual compile are separate."""
import errno
import importlib.util
from contextlib import ExitStack
from pathlib import Path
import unittest
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location(
    "linux_compile", Path(__file__).with_name("compile-linux-browser-receipt-journey.py")
)
COMPILER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(COMPILER)


class Fences(unittest.TestCase):
    def setUp(self):
        self.stack = ExitStack()
        self.addCleanup(self.stack.close)
        for obj, name, result in (
            (COMPILER.platform, "system", "Linux"), (COMPILER.platform, "machine", "x86_64"),
            (COMPILER.platform, "libc_ver", ("glibc", "2.39")),
            (COMPILER.os, "getuid", 61001), (COMPILER.os, "geteuid", 61001),
        ):
            self.stack.enter_context(patch.object(obj, name, return_value=result))
        for name, number in (("AF_PACKET", 17), ("AF_NETLINK", 16)):
            self.stack.enter_context(patch.object(COMPILER.socket, name, number, create=True))
        self.status = self.stack.enter_context(patch.object(
            COMPILER.Path, "read_text", return_value="NoNewPrivs:\t1\nSeccomp:\t2\n"
        ))
        self.socket = self.stack.enter_context(patch.object(
            COMPILER.socket, "socket", side_effect=OSError(errno.EAFNOSUPPORT, "test")
        ))
        self.open = self.stack.enter_context(patch.object(
            COMPILER.os, "open", side_effect=OSError(errno.EROFS, "test")
        ))

    def test_all_ip_and_management_socket_probes_are_required(self):
        COMPILER.linux_build_fences()
        sock = COMPILER.socket
        self.assertEqual([call.args for call in self.socket.call_args_list], [
            (sock.AF_INET, sock.SOCK_STREAM), (sock.AF_INET, sock.SOCK_DGRAM),
            (sock.AF_INET6, sock.SOCK_STREAM), (sock.AF_INET6, sock.SOCK_DGRAM),
            (sock.AF_PACKET, sock.SOCK_DGRAM), (sock.AF_NETLINK, sock.SOCK_DGRAM),
        ])
        self.assertEqual(self.open.call_count, 2)

    def test_tcp_availability_refuses_even_if_datagrams_denied(self):
        allowed = Mock()
        self.socket.side_effect = lambda family, kind: (
            allowed if kind == COMPILER.socket.SOCK_STREAM else
            OSError(errno.EAFNOSUPPORT, "test")
        )
        with self.assertRaisesRegex(RuntimeError, "^internet_socket_not_denied$"):
            COMPILER.linux_build_fences()
        allowed.close.assert_called_once()
        self.open.assert_not_called()

    def test_ambient_seccomp_is_not_enough_without_no_new_privs(self):
        self.status.return_value = "NoNewPrivs:\t0\nSeccomp:\t2\n"
        with self.assertRaisesRegex(RuntimeError, "^linux_seccomp_required$"):
            COMPILER.linux_build_fences()
        self.socket.assert_not_called()

    def test_write_open_refusal_requires_policy_errno(self):
        self.open.side_effect = FileNotFoundError(errno.ENOENT, "test")
        with self.assertRaisesRegex(RuntimeError, "^source_write_probe$"):
            COMPILER.linux_build_fences()


if __name__ == "__main__":
    unittest.main()
