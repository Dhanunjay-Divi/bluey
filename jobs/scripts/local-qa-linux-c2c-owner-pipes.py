"""One-shot host I/O for an independently authenticated guest client.

The owner adapter must supply a fresh, binary, unbuffered Popen channel over
an already authenticated private connection to the reserved guest. This
module neither launches a connection nor interprets receipt authority.
"""
import io
import os
import selectors
import stat
import subprocess
import threading
import time


MAX_REQUEST = 1 << 20
MAX_REPLY = 4096  # Existing preparation transport's complete reply limit.
MAX_SECONDS = 600
IO_CHUNK = 64 << 10


def need(condition, reason):
    if not condition:
        raise ValueError("c2c_owner_pipes_refused:" + reason)


def remaining(deadline):
    seconds = deadline - time.monotonic()
    need(seconds > 0, "deadline")
    return seconds


def validate_pipes(client, streams):
    need(client.poll() is None and all(type(stream) is io.FileIO and not stream.closed
                                       for stream in streams), "client_pipes")
    writer, output, diagnostics = streams
    need(writer.writable() and not writer.readable() and output.readable() and not output.writable()
         and diagnostics.readable() and not diagnostics.writable(), "client_directions")
    descriptors = [stream.fileno() for stream in streams]
    stats = [os.fstat(fd) for fd in descriptors]
    need(len(set(descriptors)) == len({(info.st_dev, info.st_ino) for info in stats}) == 3
         and all(fd > 2 and stat.S_ISFIFO(info.st_mode) for fd, info in zip(descriptors, stats)), "client_fds")


def send_and_read(streams, frame, deadline):
    writer, output, diagnostics = streams
    offset = 0
    reply = bytearray()
    with selectors.PollSelector() as ready:
        for stream in streams:
            os.set_blocking(stream.fileno(), False)
            ready.register(stream, selectors.EVENT_WRITE if stream is writer else selectors.EVENT_READ)
        while ready.get_map():
            events = ready.select(remaining(deadline))
            # Read already-ready output before a final write could close stdin.
            for key, _ in sorted(events, key=lambda item: item[0].fileobj is writer):
                remaining(deadline)
                stream = key.fileobj
                if stream is writer:
                    try:
                        count = os.write(key.fd, frame[offset:offset + IO_CHUNK])
                    except BlockingIOError:
                        continue
                    need(count > 0, "short_write")
                    offset += count
                    if offset == len(frame):
                        ready.unregister(stream)
                        stream.close()  # The request parser requires EOF before staging.
                    continue
                need(stream is diagnostics or writer.closed, "early_reply")
                allowance = 1 if stream is diagnostics else MAX_REPLY + 1 - len(reply)
                try:
                    received = os.read(key.fd, allowance)
                except BlockingIOError:
                    continue
                if not received:
                    ready.unregister(stream)
                    stream.close()
                    continue
                need(stream is output, "unexpected_diagnostics")
                reply.extend(received)
                need(len(reply) <= MAX_REPLY, "reply_size")
    need(writer.closed and offset == len(frame) and bool(reply), "missing_reply")
    return bytes(reply)


def make_exchange(open_authenticated, peer_current, *, timeout_seconds):
    """Return a one-use exchange(frame, cap) for prepare_at_reserved_guest.

    Both owner callbacks receive the same absolute monotonic deadline and
    must themselves enforce it. The opener retains supervision/cancellation
    of its local client and any remote process; this adapter closes only the
    adopted pipes. The peer callback must independently bind the identical
    connection and reservation before I/O and again after client exit.
    The adapter does not retry, reconnect, stop the guest or clear a hold.
    """
    need(callable(open_authenticated) and callable(peer_current)
         and type(timeout_seconds) in (int, float) and 0 < timeout_seconds <= MAX_SECONDS, "configuration")
    used = threading.Lock()

    def exchange(frame, cap):
        need(used.acquire(blocking=False), "already_attempted")
        need(type(frame) is bytes and 0 < len(frame) <= MAX_REQUEST and type(cap) is dict, "request")
        retained = dict(cap)
        deadline = time.monotonic() + timeout_seconds
        client = None
        streams = ()
        succeeded = False
        try:
            client = open_authenticated(dict(retained), deadline)
            need(type(client) is subprocess.Popen, "client")
            streams = client.stdin, client.stdout, client.stderr
            remaining(deadline)
            validate_pipes(client, streams)
            need(peer_current(client, dict(retained), deadline) is True, "peer_before_exchange")
            reply = send_and_read(streams, frame, deadline)
            need(client.wait(timeout=remaining(deadline)) == 0, "client_status")
            need(peer_current(client, dict(retained), deadline) is True, "peer_after_exchange")
            remaining(deadline)
            succeeded = True
            return reply  # The reservation/codec caller still validates the frame.
        except subprocess.TimeoutExpired:
            raise ValueError("c2c_owner_pipes_refused:deadline") from None
        except OSError:
            raise ValueError("c2c_owner_pipes_refused:client_io") from None
        finally:
            cleanup_failed = False
            for stream in streams:
                if type(stream) is io.FileIO and not stream.closed:
                    try:
                        stream.close()
                    except OSError:
                        cleanup_failed = True
            if type(client) is subprocess.Popen and client.returncode is None:
                try:
                    client.poll()  # Nonblocking reap; a live client still belongs to the owner.
                except OSError:
                    cleanup_failed = True
            need(not succeeded or not cleanup_failed, "cleanup")

    return exchange
