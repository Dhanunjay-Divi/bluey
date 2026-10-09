#!/usr/bin/env python3
"""Operator-only model availability probe; never prints provider credentials."""
import json
import os
import pathlib
import shlex
import socket
import ssl
import sys
import urllib.error
import urllib.request


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def main():
    if os.geteuid() != 0 or socket.gethostname() != "bluey-brain" or sys.argv[1:] != ["--approved-existing-provider-keys"]:
        raise RuntimeError("operator source host confirmation required")
    selected = {}
    allowed = {"OPENAI_API_KEY", "OPENAI_API_KEYS", "ANTHROPIC_API_KEY", "ANTHROPIC_API_KEYS"}
    for line in pathlib.Path("/etc/bluey-api/bluey-api.env").read_text().splitlines():
        name, separator, value = line.partition("=")
        if separator and name in allowed:
            parsed = shlex.split(value, comments=True)
            if len(parsed) == 1:
                selected[name] = parsed[0].split(",")[0].strip()
    http = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect(),
                                      urllib.request.HTTPSHandler(context=ssl.create_default_context()))
    for provider, model, url, headers, key in [
        ("anthropic", "claude-haiku-5-5", "https://api.anthropic.com/v1/models/claude-haiku-5-5",
         {"anthropic-version": "2023-06-01"}, selected.get("ANTHROPIC_API_KEYS") or selected.get("ANTHROPIC_API_KEY")),
        ("openai", "gpt-6-sol", "https://api.openai.com/v1/models/gpt-6-sol", {},
         selected.get("OPENAI_API_KEYS") or selected.get("OPENAI_API_KEY")),
    ]:
        if not key:
            print(f"{provider} {model}: no configured credential")
            continue
        headers["x-api-key" if provider == "anthropic" else "Authorization"] = (
            key if provider == "anthropic" else "Bearer " + key)
        try:
            with http.open(urllib.request.Request(url, headers=headers), timeout=15) as response:
                raw = response.read(1024 * 1024 + 1)
                if len(raw) > 1024 * 1024:
                    raise RuntimeError("oversized model metadata")
                value = json.loads(raw)
                print(f"{provider} {model}: HTTP {response.status}; exact_id={value.get('id') == model}")
        except urllib.error.HTTPError as error:
            print(f"{provider} {model}: HTTP {error.code}; unavailable to probe")
            error.close()
    print("Metadata GET only; no generation, transcription or payment call")


if __name__ == "__main__":
    try:
        main()
    except Exception as error:
        print("Model probe failed: " + type(error).__name__, file=sys.stderr)
        sys.exit(1)
