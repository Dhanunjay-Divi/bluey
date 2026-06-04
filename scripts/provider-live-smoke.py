#!/usr/bin/env python3
"""Live provider smoke tests for Bluey-managed upstream accounts.

The script reads provider keys from environment variables, keeps secrets in
memory only, and prints only pass/fail metadata. It intentionally does not
write keys to repo config, temp files, curl command lines, or logs.
"""

from __future__ import annotations

import argparse
import base64
import json
import os
import re
import ssl
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass


OPENAI_TEXT_MODEL = os.environ.get("BLUEY_SMOKE_OPENAI_TEXT_MODEL", "gpt-4o-mini")
OPENAI_VISION_MODEL = os.environ.get("BLUEY_SMOKE_OPENAI_VISION_MODEL", "gpt-4o")
OPENAI_EMBED_MODEL = os.environ.get("BLUEY_SMOKE_OPENAI_EMBED_MODEL", "text-embedding-3-small")
OPENAI_STT_MODEL = os.environ.get("BLUEY_SMOKE_OPENAI_STT_MODEL", "gpt-4o-mini-transcribe")
ANTHROPIC_MODEL = os.environ.get("BLUEY_SMOKE_ANTHROPIC_MODEL", "claude-3-5-sonnet-latest")
DEEPGRAM_MODEL = os.environ.get("BLUEY_SMOKE_DEEPGRAM_MODEL", "nova-3")
GEMINI_MODEL = os.environ.get("BLUEY_SMOKE_GEMINI_MODEL", "gemini-2.5-flash")
TIMEOUT_SECONDS = float(os.environ.get("BLUEY_PROVIDER_SMOKE_TIMEOUT", "30"))

# 1x1 transparent PNG.
TINY_PNG_DATA_URL = (
    "data:image/png;base64,"
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMB"
    "/6Xn3foAAAAASUVORK5CYII="
)


@dataclass
class SmokeResult:
    provider: str
    check: str
    status: str
    latency_ms: int | None = None
    detail: str = ""


def first_env(names: list[str]) -> str | None:
    for name in names:
        value = os.environ.get(name)
        if value and value.strip():
            return value.strip()
    return None


def first_key(raw: str | None) -> str | None:
    if not raw:
        return None
    for candidate in raw.split(","):
        key = candidate.strip()
        if key:
            return key
    return None


def configured_secrets() -> list[str]:
    names = [
        "OPENAI_API_KEY",
        "OPENAI_API_KEYS",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_API_KEYS",
        "DEEPGRAM_API_KEY",
        "DEEPGRAM_API_KEYS",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
    ]
    secrets: list[str] = []
    for name in names:
        raw = os.environ.get(name)
        if raw:
            secrets.extend([part.strip() for part in raw.split(",") if part.strip()])
    return secrets


SECRET_PATTERNS = [
    re.compile(r"sk-[A-Za-z0-9_\-]{12,}"),
    re.compile(r"sk-ant-[A-Za-z0-9_\-]{12,}"),
    re.compile(r"[A-Fa-f0-9]{32,}"),
    re.compile(r"AIza[A-Za-z0-9_\-]{20,}"),
]


def redact(text: str) -> str:
    out = text
    for secret in configured_secrets():
        out = out.replace(secret, "<redacted>")
    for pattern in SECRET_PATTERNS:
        out = pattern.sub("<redacted>", out)
    return out


def request_json(
    url: str,
    headers: dict[str, str],
    payload: dict,
    *,
    timeout: float = TIMEOUT_SECONDS,
) -> tuple[int, bytes]:
    data = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        url,
        data=data,
        headers={**headers, "Content-Type": "application/json"},
        method="POST",
    )
    with urllib.request.urlopen(
        request,
        timeout=timeout,
        context=ssl.create_default_context(),
    ) as response:
        return response.status, response.read()


def request_bytes(
    url: str,
    headers: dict[str, str],
    body: bytes,
    *,
    timeout: float = TIMEOUT_SECONDS,
) -> tuple[int, bytes]:
    request = urllib.request.Request(url, data=body, headers=headers, method="POST")
    with urllib.request.urlopen(
        request,
        timeout=timeout,
        context=ssl.create_default_context(),
    ) as response:
        return response.status, response.read()


def multipart_request(
    url: str,
    headers: dict[str, str],
    fields: dict[str, str],
    files: dict[str, tuple[str, str, bytes]],
    *,
    timeout: float = TIMEOUT_SECONDS,
) -> tuple[int, bytes]:
    boundary = f"bluey-smoke-{int(time.time() * 1000)}"
    chunks: list[bytes] = []
    for name, value in fields.items():
        chunks.append(f"--{boundary}\r\n".encode("utf-8"))
        chunks.append(f'Content-Disposition: form-data; name="{name}"\r\n\r\n'.encode("utf-8"))
        chunks.append(value.encode("utf-8"))
        chunks.append(b"\r\n")
    for name, (filename, content_type, content) in files.items():
        chunks.append(f"--{boundary}\r\n".encode("utf-8"))
        chunks.append(
            (
                f'Content-Disposition: form-data; name="{name}"; '
                f'filename="{filename}"\r\n'
                f"Content-Type: {content_type}\r\n\r\n"
            ).encode("utf-8")
        )
        chunks.append(content)
        chunks.append(b"\r\n")
    chunks.append(f"--{boundary}--\r\n".encode("utf-8"))
    body = b"".join(chunks)
    return request_bytes(
        url,
        {**headers, "Content-Type": f"multipart/form-data; boundary={boundary}"},
        body,
        timeout=timeout,
    )


def linear16_silence(seconds: float = 1.0, sample_rate: int = 16_000) -> bytes:
    samples = int(seconds * sample_rate)
    return b"\x00\x00" * samples


def wav_silence(seconds: float = 1.0, sample_rate: int = 16_000) -> bytes:
    pcm = linear16_silence(seconds, sample_rate)
    byte_rate = sample_rate * 2
    data_size = len(pcm)
    riff_size = 36 + data_size
    return b"".join(
        [
            b"RIFF",
            riff_size.to_bytes(4, "little"),
            b"WAVEfmt ",
            (16).to_bytes(4, "little"),
            (1).to_bytes(2, "little"),
            (1).to_bytes(2, "little"),
            sample_rate.to_bytes(4, "little"),
            byte_rate.to_bytes(4, "little"),
            (2).to_bytes(2, "little"),
            (16).to_bytes(2, "little"),
            b"data",
            data_size.to_bytes(4, "little"),
            pcm,
        ]
    )


def run_check(provider: str, check: str, fn) -> SmokeResult:
    start = time.monotonic()
    try:
        status, body = fn()
        latency_ms = int((time.monotonic() - start) * 1000)
        if 200 <= status < 300:
            return SmokeResult(provider, check, "PASS", latency_ms)
        detail = redact(body.decode("utf-8", errors="replace"))[:280]
        return SmokeResult(provider, check, "FAIL", latency_ms, f"HTTP {status}: {detail}")
    except urllib.error.HTTPError as exc:
        latency_ms = int((time.monotonic() - start) * 1000)
        body = exc.read().decode("utf-8", errors="replace")
        return SmokeResult(
            provider,
            check,
            "FAIL",
            latency_ms,
            f"HTTP {exc.code}: {redact(body)[:280]}",
        )
    except Exception as exc:  # noqa: BLE001 - smoke script needs concise failure detail.
        latency_ms = int((time.monotonic() - start) * 1000)
        return SmokeResult(provider, check, "FAIL", latency_ms, redact(str(exc))[:280])


def skipped(provider: str, check: str, why: str) -> SmokeResult:
    return SmokeResult(provider, check, "SKIP", None, why)


def openai_checks(key: str | None) -> list[SmokeResult]:
    if not key:
        return [
            skipped("openai", "text", "OPENAI_API_KEY(S) missing"),
            skipped("openai", "vision", "OPENAI_API_KEY(S) missing"),
            skipped("openai", "embeddings", "OPENAI_API_KEY(S) missing"),
            skipped("openai", "transcription", "OPENAI_API_KEY(S) missing"),
        ]
    headers = {"Authorization": f"Bearer {key}"}
    return [
        run_check(
            "openai",
            f"text:{OPENAI_TEXT_MODEL}",
            lambda: request_json(
                "https://api.openai.com/v1/chat/completions",
                headers,
                {
                    "model": OPENAI_TEXT_MODEL,
                    "messages": [
                        {
                            "role": "user",
                            "content": "Return exactly BLUEY_OPENAI_OK and nothing else.",
                        }
                    ],
                    "max_tokens": 12,
                    "temperature": 0,
                },
            ),
        ),
        run_check(
            "openai",
            f"vision:{OPENAI_VISION_MODEL}",
            lambda: request_json(
                "https://api.openai.com/v1/chat/completions",
                headers,
                {
                    "model": OPENAI_VISION_MODEL,
                    "messages": [
                        {
                            "role": "user",
                            "content": [
                                {
                                    "type": "text",
                                    "text": "This is a provider smoke. Return exactly BLUEY_VISION_OK.",
                                },
                                {"type": "image_url", "image_url": {"url": TINY_PNG_DATA_URL}},
                            ],
                        }
                    ],
                    "max_tokens": 12,
                    "temperature": 0,
                },
            ),
        ),
        run_check(
            "openai",
            f"embeddings:{OPENAI_EMBED_MODEL}",
            lambda: request_json(
                "https://api.openai.com/v1/embeddings",
                headers,
                {"model": OPENAI_EMBED_MODEL, "input": "bluey provider smoke"},
            ),
        ),
        run_check(
            "openai",
            f"stt:{OPENAI_STT_MODEL}",
            lambda: multipart_request(
                "https://api.openai.com/v1/audio/transcriptions",
                headers,
                {"model": OPENAI_STT_MODEL},
                {"file": ("bluey-smoke.wav", "audio/wav", wav_silence())},
            ),
        ),
    ]


def anthropic_checks(key: str | None) -> list[SmokeResult]:
    if not key:
        return [skipped("anthropic", "text", "ANTHROPIC_API_KEY(S) missing")]
    return [
        run_check(
            "anthropic",
            f"text:{ANTHROPIC_MODEL}",
            lambda: request_json(
                "https://api.anthropic.com/v1/messages",
                {
                    "x-api-key": key,
                    "anthropic-version": "2023-06-01",
                },
                {
                    "model": ANTHROPIC_MODEL,
                    "max_tokens": 16,
                    "temperature": 0,
                    "messages": [
                        {
                            "role": "user",
                            "content": "Return exactly BLUEY_ANTHROPIC_OK and nothing else.",
                        }
                    ],
                },
            ),
        )
    ]


def deepgram_checks(key: str | None) -> list[SmokeResult]:
    if not key:
        return [skipped("deepgram", "stt", "DEEPGRAM_API_KEY(S) missing")]
    query = f"model={DEEPGRAM_MODEL}&language=en&encoding=linear16&sample_rate=16000&channels=1"
    return [
        run_check(
            "deepgram",
            f"stt:{DEEPGRAM_MODEL}",
            lambda: request_bytes(
                f"https://api.deepgram.com/v1/listen?{query}",
                {
                    "Authorization": f"Token {key}",
                    "Content-Type": "audio/raw",
                },
                linear16_silence(),
            ),
        )
    ]


def gemini_checks(key: str | None) -> list[SmokeResult]:
    if not key:
        return [skipped("gemini", "text", "GEMINI_API_KEY/GOOGLE_API_KEY missing")]
    return [
        run_check(
            "gemini",
            f"text:{GEMINI_MODEL}",
            lambda: request_json(
                f"https://generativelanguage.googleapis.com/v1beta/models/{GEMINI_MODEL}:generateContent",
                {"x-goog-api-key": key},
                {
                    "contents": [
                        {
                            "parts": [
                                {
                                    "text": "Return exactly BLUEY_GEMINI_OK and nothing else.",
                                }
                            ]
                        }
                    ],
                    "generationConfig": {
                        "temperature": 0,
                        "maxOutputTokens": 12,
                    },
                },
            ),
        )
    ]


def print_results(results: list[SmokeResult]) -> None:
    width_provider = max(len(result.provider) for result in results)
    width_status = max(len(result.status) for result in results)
    for result in results:
        latency = "" if result.latency_ms is None else f" {result.latency_ms}ms"
        detail = "" if not result.detail else f" — {result.detail}"
        print(
            f"{result.status:<{width_status}} "
            f"{result.provider:<{width_provider}} "
            f"{result.check}{latency}{detail}"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description="Run live provider smoke tests without printing keys.")
    parser.add_argument(
        "--strict",
        action="store_true",
        default=os.environ.get("BLUEY_PROVIDER_SMOKE_STRICT") == "1",
        help="Fail if OpenAI, Anthropic, or Deepgram keys are missing.",
    )
    parser.add_argument(
        "--skip-gemini",
        action="store_true",
        help="Do not run the optional direct Gemini smoke.",
    )
    args = parser.parse_args()

    openai_key = first_key(first_env(["OPENAI_API_KEYS", "OPENAI_API_KEY"]))
    anthropic_key = first_key(first_env(["ANTHROPIC_API_KEYS", "ANTHROPIC_API_KEY"]))
    deepgram_key = first_key(first_env(["DEEPGRAM_API_KEYS", "DEEPGRAM_API_KEY"]))
    gemini_key = first_key(first_env(["GEMINI_API_KEY", "GOOGLE_API_KEY"]))

    results: list[SmokeResult] = []
    results.extend(openai_checks(openai_key))
    results.extend(anthropic_checks(anthropic_key))
    results.extend(deepgram_checks(deepgram_key))
    if not args.skip_gemini:
        results.extend(gemini_checks(gemini_key))

    print_results(results)

    failed = [result for result in results if result.status == "FAIL"]
    skipped_required = []
    if args.strict:
        required = {"openai", "anthropic", "deepgram"}
        skipped_required = [
            result
            for result in results
            if result.status == "SKIP" and result.provider in required
        ]
    if failed or skipped_required:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
