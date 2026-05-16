#!/bin/bash
# Downloads the whisper.cpp tiny.en quantized model on first run.
# Model: ggml-tiny.en-q5_1.bin (~75 MB)
# Destination: ~/.cache/bluey/whisper/tiny.en-q5_1.bin
set -euo pipefail

MODEL_DIR="${HOME}/.cache/bluey/whisper"
MODEL_FILE="${MODEL_DIR}/tiny.en-q5_1.bin"
MODEL_URL="https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en-q5_1.bin"

if [ -f "${MODEL_FILE}" ]; then
    echo "Model already exists: ${MODEL_FILE}"
    exit 0
fi

echo "Downloading whisper.cpp tiny.en-q5_1 model..."
mkdir -p "${MODEL_DIR}"

if command -v curl &>/dev/null; then
    curl -L --progress-bar -o "${MODEL_FILE}" "${MODEL_URL}"
elif command -v wget &>/dev/null; then
    wget --show-progress -O "${MODEL_FILE}" "${MODEL_URL}"
else
    echo "Error: neither curl nor wget found" >&2
    exit 1
fi

echo "Model downloaded: ${MODEL_FILE}"
