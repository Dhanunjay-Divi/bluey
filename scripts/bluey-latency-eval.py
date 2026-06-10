#!/usr/bin/env python3
"""Local latency/quality harness for Bluey's managed answer paths.

Reads:
  BLUEY_API_BASE       e.g. https://bluey.sh or http://127.0.0.1:3000
  BLUEY_ACCESS_TOKEN   managed account access token, used only from env

The script intentionally avoids printing request/response text and never
prints bearer tokens. It reports timings, status codes, response sizes, and
non-sensitive routing metadata only.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import socket
import sys
import time
import uuid
from dataclasses import dataclass
from typing import Any, Dict, Iterable, List, Optional, Tuple
from urllib import error, parse, request


DEFAULT_QUESTION = (
    "In one short sentence, what should I verify before shipping a latency fix?"
)
DEFAULT_SYSTEM = (
    "Answer as Bluey in one concise sentence. Do not include secrets or credentials."
)
DEFAULT_TIMEOUT_SECONDS = 60.0

THRESHOLDS_MS = {
    "health": (1_000.0, 3_000.0),
    "first_event": (3_000.0, 8_000.0),
    "first_token": (3_000.0, 8_000.0),
    "complete": (15_000.0, 30_000.0),
    "stream_full": (15_000.0, 30_000.0),
}

SECRET_PATTERNS = [
    re.compile(r"Bearer\s+[A-Za-z0-9._~+/=-]+", re.IGNORECASE),
    re.compile(r"\b[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b"),
    re.compile(r"\bsk-[A-Za-z0-9_-]{8,}\b"),
    re.compile(r"\b(?:dg|whsec|rk|pk|sk)_[A-Za-z0-9_-]{8,}\b"),
]


@dataclass
class SseEvent:
    event: str
    data: str


@dataclass
class SimpleResult:
    name: str
    ok: bool
    status: Optional[int]
    elapsed_ms: float
    body_bytes: int = 0
    note: str = ""
    metadata: Optional[Dict[str, Any]] = None


@dataclass
class StreamResult:
    ok: bool
    status: Optional[int]
    header_ms: Optional[float]
    first_event_ms: Optional[float]
    first_token_ms: Optional[float]
    full_ms: float
    events: int
    token_events: int
    token_chars: int
    done: bool
    note: str = ""
    metadata: Optional[Dict[str, Any]] = None


def monotonic_ms(start: float) -> float:
    return (time.perf_counter() - start) * 1000.0


def redact(text: object, known_secrets: Iterable[str] = ()) -> str:
    value = str(text)
    for secret in known_secrets:
        if secret:
            value = value.replace(secret, "[REDACTED]")
    for pattern in SECRET_PATTERNS:
        value = pattern.sub("[REDACTED]", value)
    return value


def env_status(value: Optional[str]) -> str:
    if value:
        return "set (masked)"
    return "missing"


def safe_base_for_display(base: str) -> str:
    parsed = parse.urlsplit(base)
    host = parsed.hostname or ""
    if parsed.port:
        host = f"{host}:{parsed.port}"
    path = parsed.path.rstrip("/")
    return parse.urlunsplit((parsed.scheme, host, path, "", ""))


def normalize_base(base: str) -> str:
    parsed = parse.urlsplit(base.strip())
    if parsed.scheme not in ("http", "https") or not parsed.netloc:
        raise ValueError("BLUEY_API_BASE must be an http(s) URL")
    return parse.urlunsplit(
        (parsed.scheme, parsed.netloc, parsed.path.rstrip("/"), "", "")
    )


def api_url(base: str, path: str) -> str:
    return f"{base.rstrip('/')}{path}"


def classify_ms(kind: str, value: Optional[float]) -> str:
    if value is None:
        return "n/a"
    warn, slow = THRESHOLDS_MS[kind]
    if value <= warn:
        return "ok"
    if value <= slow:
        return "watch"
    return "slow"


def fmt_ms(value: Optional[float]) -> str:
    if value is None:
        return "n/a"
    return f"{value:.1f} ms"


def read_error_note(exc: error.HTTPError, token: str) -> Tuple[int, str]:
    body = exc.read(4096)
    note = f"HTTP {exc.code} {exc.reason}; body={len(body)} bytes"
    content_type = exc.headers.get("content-type", "")
    if "json" in content_type.lower():
        try:
            parsed = json.loads(body.decode("utf-8", "replace"))
        except json.JSONDecodeError:
            return len(body), note
        reason = pick_error_reason(parsed)
        if reason:
            note = f"{note}; reason={redact(reason, [token])}"
    return len(body), note


def pick_error_reason(value: Any) -> Optional[str]:
    if not isinstance(value, dict):
        return None
    for key in ("reason", "error", "message", "code"):
        item = value.get(key)
        if isinstance(item, str) and item:
            return item[:160]
    return None


def auth_headers(token: Optional[str], accept: str = "application/json") -> Dict[str, str]:
    headers = {
        "Accept": accept,
        "Content-Type": "application/json",
        "User-Agent": "bluey-latency-eval/1.0",
    }
    if token:
        headers["Authorization"] = f"Bearer {token}"
    return headers


def make_payload(question: str, system: str, lane: str) -> Dict[str, str]:
    return {
        "request_id": f"latency-eval-{uuid.uuid4()}",
        "system": system,
        "user": question,
        "lane": lane,
    }


def measure_health(base: str, timeout: float) -> SimpleResult:
    url = api_url(base, "/health")
    req = request.Request(
        url,
        headers={
            "Accept": "application/json",
            "User-Agent": "bluey-latency-eval/1.0",
        },
        method="GET",
    )
    start = time.perf_counter()
    try:
        with request.urlopen(req, timeout=timeout) as resp:
            body = resp.read(4096)
            elapsed = monotonic_ms(start)
            return SimpleResult(
                name="health",
                ok=200 <= resp.status < 300,
                status=resp.status,
                elapsed_ms=elapsed,
                body_bytes=len(body),
            )
    except error.HTTPError as exc:
        body = exc.read(4096)
        return SimpleResult(
            name="health",
            ok=False,
            status=exc.code,
            elapsed_ms=monotonic_ms(start),
            body_bytes=len(body),
            note=f"HTTP {exc.code} {exc.reason}; body={len(body)} bytes",
        )
    except (error.URLError, TimeoutError, socket.timeout) as exc:
        return SimpleResult(
            name="health",
            ok=False,
            status=None,
            elapsed_ms=monotonic_ms(start),
            note=redact(exc),
        )


def measure_complete(
    base: str, token: str, payload: Dict[str, str], timeout: float
) -> SimpleResult:
    encoded = json.dumps(payload).encode("utf-8")
    req = request.Request(
        api_url(base, "/router/complete"),
        data=encoded,
        headers=auth_headers(token),
        method="POST",
    )
    start = time.perf_counter()
    try:
        with request.urlopen(req, timeout=timeout) as resp:
            body = resp.read()
            elapsed = monotonic_ms(start)
            metadata = metadata_from_complete_body(body)
            text_len = int(metadata.pop("text_len", 0))
            note = f"text_chars={text_len}"
            return SimpleResult(
                name="complete",
                ok=200 <= resp.status < 300 and text_len > 0,
                status=resp.status,
                elapsed_ms=elapsed,
                body_bytes=len(body),
                note=note,
                metadata=metadata,
            )
    except error.HTTPError as exc:
        body_len, note = read_error_note(exc, token)
        return SimpleResult(
            name="complete",
            ok=False,
            status=exc.code,
            elapsed_ms=monotonic_ms(start),
            body_bytes=body_len,
            note=note,
        )
    except (error.URLError, TimeoutError, socket.timeout) as exc:
        return SimpleResult(
            name="complete",
            ok=False,
            status=None,
            elapsed_ms=monotonic_ms(start),
            note=redact(exc, [token]),
        )


def metadata_from_complete_body(body: bytes) -> Dict[str, Any]:
    try:
        value = json.loads(body.decode("utf-8", "replace"))
    except json.JSONDecodeError:
        return {"text_len": 0}
    if not isinstance(value, dict):
        return {"text_len": 0}
    text = value.get("text")
    metadata: Dict[str, Any] = {"text_len": len(text) if isinstance(text, str) else 0}
    for key in ("provider", "model", "input_tokens", "output_tokens", "cost_cents"):
        if key in value and value[key] is not None:
            metadata[key] = value[key]
    return metadata


def response_lines(resp: Any) -> Iterable[bytes]:
    while True:
        line = resp.readline()
        if line == b"":
            break
        yield line


def iter_sse_events(lines: Iterable[bytes]) -> Iterable[SseEvent]:
    event = "message"
    data_lines: List[str] = []
    for raw in lines:
        line = raw.decode("utf-8", "replace").rstrip("\r\n")
        if not line:
            if data_lines or event != "message":
                yield SseEvent(event=event, data="\n".join(data_lines))
            event = "message"
            data_lines = []
            continue
        if line.startswith(":"):
            continue
        field, separator, value = line.partition(":")
        if not separator:
            continue
        if value.startswith(" "):
            value = value[1:]
        if field == "event":
            event = value or "message"
        elif field == "data":
            data_lines.append(value)
    if data_lines or event != "message":
        yield SseEvent(event=event, data="\n".join(data_lines))


def extract_delta_text(data: str) -> str:
    if data.strip() == "[DONE]":
        return ""
    try:
        value = json.loads(data)
    except json.JSONDecodeError:
        return ""
    if not isinstance(value, dict):
        return ""
    choices = value.get("choices")
    if isinstance(choices, list):
        chunks = []
        for choice in choices:
            if not isinstance(choice, dict):
                continue
            delta = choice.get("delta")
            if isinstance(delta, dict) and isinstance(delta.get("content"), str):
                chunks.append(delta["content"])
            elif isinstance(choice.get("text"), str):
                chunks.append(choice["text"])
        if chunks:
            return "".join(chunks)
    for key in ("delta", "content"):
        if isinstance(value.get(key), str):
            return value[key]
    return ""


def metadata_from_billing_event(data: str) -> Dict[str, Any]:
    try:
        value = json.loads(data)
    except json.JSONDecodeError:
        return {}
    if not isinstance(value, dict):
        return {}
    metadata: Dict[str, Any] = {}
    for key in ("provider", "model", "input_tokens", "output_tokens", "cost_cents"):
        if key in value and value[key] is not None:
            metadata[key] = value[key]
    text = value.get("text")
    if isinstance(text, str):
        metadata["billing_text_chars"] = len(text)
    return metadata


def measure_stream(
    base: str, token: str, payload: Dict[str, str], timeout: float
) -> StreamResult:
    encoded = json.dumps(payload).encode("utf-8")
    req = request.Request(
        api_url(base, "/router/complete/stream"),
        data=encoded,
        headers=auth_headers(token, accept="text/event-stream"),
        method="POST",
    )
    start = time.perf_counter()
    header_ms: Optional[float] = None
    first_event_ms: Optional[float] = None
    first_token_ms: Optional[float] = None
    events = 0
    token_events = 0
    token_chars = 0
    done = False
    metadata: Dict[str, Any] = {}
    try:
        with request.urlopen(req, timeout=timeout) as resp:
            header_ms = monotonic_ms(start)
            for event in iter_sse_events(response_lines(resp)):
                now_ms = monotonic_ms(start)
                events += 1
                if first_event_ms is None:
                    first_event_ms = now_ms
                if event.data.strip() == "[DONE]":
                    done = True
                    break
                if event.event == "billing":
                    metadata.update(metadata_from_billing_event(event.data))
                    continue
                delta = extract_delta_text(event.data)
                if delta:
                    token_events += 1
                    token_chars += len(delta)
                    if first_token_ms is None:
                        first_token_ms = now_ms
            full_ms = monotonic_ms(start)
            billing_text_chars = int(metadata.get("billing_text_chars", 0))
            ok = 200 <= resp.status < 300 and done and (
                token_chars > 0 or billing_text_chars > 0
            )
            note = ""
            if not done:
                note = "stream ended before [DONE]"
            elif token_chars == 0:
                note = "no content delta observed"
            return StreamResult(
                ok=ok,
                status=resp.status,
                header_ms=header_ms,
                first_event_ms=first_event_ms,
                first_token_ms=first_token_ms,
                full_ms=full_ms,
                events=events,
                token_events=token_events,
                token_chars=token_chars,
                done=done,
                note=note,
                metadata=metadata,
            )
    except error.HTTPError as exc:
        _, note = read_error_note(exc, token)
        return StreamResult(
            ok=False,
            status=exc.code,
            header_ms=header_ms,
            first_event_ms=first_event_ms,
            first_token_ms=first_token_ms,
            full_ms=monotonic_ms(start),
            events=events,
            token_events=token_events,
            token_chars=token_chars,
            done=done,
            note=note,
        )
    except (error.URLError, TimeoutError, socket.timeout) as exc:
        return StreamResult(
            ok=False,
            status=None,
            header_ms=header_ms,
            first_event_ms=first_event_ms,
            first_token_ms=first_token_ms,
            full_ms=monotonic_ms(start),
            events=events,
            token_events=token_events,
            token_chars=token_chars,
            done=done,
            note=redact(exc, [token]),
        )


def print_dry_run(args: argparse.Namespace, api_base: Optional[str], token: Optional[str]) -> None:
    print("Bluey latency eval dry run")
    print(f"BLUEY_API_BASE: {env_status(api_base)}")
    print(f"BLUEY_ACCESS_TOKEN: {env_status(token)}")
    missing = []
    if not api_base:
        missing.append("BLUEY_API_BASE")
    if not token:
        missing.append("BLUEY_ACCESS_TOKEN")
    if missing:
        print(f"Missing for full managed-answer eval: {', '.join(missing)}")
    else:
        print("Full managed-answer eval can run with the current environment.")
    print(f"Planned lane: {args.lane}")
    print(f"Planned question length: {len(args.question)} chars")
    print("Planned checks: GET /health, POST /router/complete/stream, POST /router/complete")


def print_simple_result(result: SimpleResult) -> None:
    status = result.status if result.status is not None else "n/a"
    grade = classify_ms(result.name, result.elapsed_ms)
    label = "OK" if result.ok else "FAIL"
    tail = f"; {result.note}" if result.note else ""
    metadata = format_metadata(result.metadata)
    if metadata:
        tail = f"{tail}; {metadata}"
    print(
        f"{result.name}: {label} status={status} total={fmt_ms(result.elapsed_ms)} "
        f"[{grade}] body={result.body_bytes} bytes{tail}"
    )


def print_stream_result(run_idx: int, result: StreamResult) -> None:
    status = result.status if result.status is not None else "n/a"
    label = "OK" if result.ok else "FAIL"
    metadata = format_metadata(result.metadata)
    tail = f"; {result.note}" if result.note else ""
    if metadata:
        tail = f"{tail}; {metadata}"
    print(
        f"stream[{run_idx}]: {label} status={status} "
        f"headers={fmt_ms(result.header_ms)} "
        f"first_event={fmt_ms(result.first_event_ms)} "
        f"[{classify_ms('first_event', result.first_event_ms)}] "
        f"first_token={fmt_ms(result.first_token_ms)} "
        f"[{classify_ms('first_token', result.first_token_ms)}] "
        f"full={fmt_ms(result.full_ms)} [{classify_ms('stream_full', result.full_ms)}] "
        f"events={result.events} token_events={result.token_events} "
        f"token_chars={result.token_chars} done={str(result.done).lower()}{tail}"
    )


def format_metadata(metadata: Optional[Dict[str, Any]]) -> str:
    if not metadata:
        return ""
    parts = []
    for key in (
        "provider",
        "model",
        "input_tokens",
        "output_tokens",
        "cost_cents",
        "billing_text_chars",
    ):
        if key in metadata:
            parts.append(f"{key}={metadata[key]}")
    return " ".join(parts)


def json_safe_simple(result: SimpleResult) -> Dict[str, Any]:
    return {
        "name": result.name,
        "ok": result.ok,
        "status": result.status,
        "elapsed_ms": round(result.elapsed_ms, 1),
        "threshold": classify_ms(result.name, result.elapsed_ms),
        "body_bytes": result.body_bytes,
        "note": result.note,
        "metadata": result.metadata or {},
    }


def json_safe_stream(result: StreamResult) -> Dict[str, Any]:
    return {
        "name": "stream",
        "ok": result.ok,
        "status": result.status,
        "header_ms": round(result.header_ms, 1) if result.header_ms is not None else None,
        "first_event_ms": (
            round(result.first_event_ms, 1) if result.first_event_ms is not None else None
        ),
        "first_token_ms": (
            round(result.first_token_ms, 1) if result.first_token_ms is not None else None
        ),
        "full_ms": round(result.full_ms, 1),
        "first_event_threshold": classify_ms("first_event", result.first_event_ms),
        "first_token_threshold": classify_ms("first_token", result.first_token_ms),
        "full_threshold": classify_ms("stream_full", result.full_ms),
        "events": result.events,
        "token_events": result.token_events,
        "token_chars": result.token_chars,
        "done": result.done,
        "note": result.note,
        "metadata": result.metadata or {},
    }


def run_self_test() -> int:
    sample_lines = [
        b'data: {"choices":[{"delta":{"content":"hel"}}]}\n',
        b"\n",
        b"event: billing\n",
        b'data: {"text":"hello","provider":"openai","model":"gpt-4o-mini"}\n',
        b"\n",
        b"data: [DONE]\n",
        b"\n",
    ]
    events = list(iter_sse_events(sample_lines))
    assert len(events) == 3, events
    assert events[0].event == "message"
    assert extract_delta_text(events[0].data) == "hel"
    assert events[1].event == "billing"
    billing = metadata_from_billing_event(events[1].data)
    assert billing["provider"] == "openai"
    assert billing["billing_text_chars"] == 5
    assert events[2].data == "[DONE]"
    assert "[REDACTED]" in redact("Authorization: Bearer abc.def.ghi")
    assert safe_base_for_display("https://user:pass@example.com:443/api?x=1") == (
        "https://example.com:443/api"
    )
    print("bluey-latency-eval self-test passed")
    return 0


def parse_args(argv: List[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Measure Bluey /health and managed answer path latency."
    )
    parser.add_argument(
        "--question",
        default=os.environ.get("BLUEY_EVAL_QUESTION", DEFAULT_QUESTION),
        help="Typed question to send. Not printed back by the harness.",
    )
    parser.add_argument(
        "--system",
        default=os.environ.get("BLUEY_EVAL_SYSTEM", DEFAULT_SYSTEM),
        help="System prompt to send. Not printed back by the harness.",
    )
    parser.add_argument(
        "--lane",
        default=os.environ.get("BLUEY_EVAL_LANE", "instant"),
        choices=("instant", "balanced", "deep", "vision"),
        help="Managed routing lane to evaluate.",
    )
    parser.add_argument(
        "--runs",
        type=int,
        default=int(os.environ.get("BLUEY_EVAL_RUNS", "1")),
        help="Number of authenticated stream/complete comparisons to run.",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=float(os.environ.get("BLUEY_EVAL_TIMEOUT", DEFAULT_TIMEOUT_SECONDS)),
        help="Per-request timeout in seconds.",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print env readiness and planned checks without making network calls; exits 0.",
    )
    parser.add_argument(
        "--health-only",
        action="store_true",
        help="Only check /health, even when BLUEY_ACCESS_TOKEN is set.",
    )
    parser.add_argument(
        "--skip-nonstream",
        action="store_true",
        help="Skip POST /router/complete comparison.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit machine-readable JSON. Secrets and response text are still omitted.",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Run parser/redaction self-tests without network calls.",
    )
    args = parser.parse_args(argv)
    if args.runs < 1:
        parser.error("--runs must be >= 1")
    if args.timeout <= 0:
        parser.error("--timeout must be > 0")
    return args


def main(argv: List[str]) -> int:
    args = parse_args(argv)
    if args.self_test:
        return run_self_test()

    api_base_env = os.environ.get("BLUEY_API_BASE")
    token = os.environ.get("BLUEY_ACCESS_TOKEN")

    if args.dry_run:
        print_dry_run(args, api_base_env, token)
        return 0

    if not api_base_env:
        print("BLUEY_API_BASE is required unless --dry-run is used.", file=sys.stderr)
        return 2

    try:
        api_base = normalize_base(api_base_env)
    except ValueError as exc:
        print(str(exc), file=sys.stderr)
        return 2

    results: Dict[str, Any] = {
        "api_base": safe_base_for_display(api_base),
        "token": env_status(token),
        "lane": args.lane,
        "question_chars": len(args.question),
        "health": None,
        "runs": [],
    }

    if not args.json:
        print("Bluey latency eval")
        print(f"API base: {safe_base_for_display(api_base)}")
        print(f"BLUEY_ACCESS_TOKEN: {env_status(token)}")
        print(f"Lane: {args.lane}; question_chars={len(args.question)}")

    exit_code = 0
    health = measure_health(api_base, args.timeout)
    results["health"] = json_safe_simple(health)
    if not args.json:
        print_simple_result(health)
    if not health.ok:
        exit_code = 1

    if args.health_only or not token:
        if not token and not args.health_only and not args.json:
            print("Managed answer checks skipped: BLUEY_ACCESS_TOKEN is missing.")
        if args.json:
            print(json.dumps(results, indent=2, sort_keys=True))
        return exit_code

    for idx in range(1, args.runs + 1):
        run_payload = make_payload(args.question, args.system, args.lane)
        stream_result = measure_stream(api_base, token, run_payload, args.timeout)
        run_entry: Dict[str, Any] = {"stream": json_safe_stream(stream_result)}
        if not args.json:
            print_stream_result(idx, stream_result)
        if not stream_result.ok:
            exit_code = 1

        if not args.skip_nonstream:
            complete_payload = make_payload(args.question, args.system, args.lane)
            complete_result = measure_complete(
                api_base, token, complete_payload, args.timeout
            )
            run_entry["complete"] = json_safe_simple(complete_result)
            if not args.json:
                print_simple_result(complete_result)
            if not complete_result.ok:
                exit_code = 1

        results["runs"].append(run_entry)

    if args.json:
        print(json.dumps(results, indent=2, sort_keys=True))

    return exit_code


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
