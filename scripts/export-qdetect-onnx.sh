#!/usr/bin/env bash
# Export the question-detection classifier to ONNX (int8) for the meeting build.
#
# The daemon's two-stage question detector (PLAN-CONTEXT-WARMUP SET 1) runs
# shahrukhx01/question-vs-statement-classifier via ort. No hosted ONNX export of
# that model exists, so we export it ourselves ONCE here and ship the ~11MB
# int8 file inside the tarball (dist/, never the git repo — >1MB rule). The
# daemon falls back to regex-only when the model file is absent, so a build
# without this step still works.
#
# Output: dist/models/qdetect-en/{model_int8.onnx,tokenizer.json}
# Requires: `uv` (fetches an isolated python; nothing global is touched).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

OUT="dist/models/qdetect-en"
if [[ -f "$OUT/model_int8.onnx" && -f "$OUT/tokenizer.json" ]]; then
  echo "export-qdetect: $OUT already present — skipping (delete to re-export)"
  exit 0
fi

command -v uv >/dev/null || { echo "export-qdetect: needs uv (https://docs.astral.sh/uv)"; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
export UV_PYTHON_INSTALL_DIR="$WORK/py"
# python 3.12: broadest ML wheel coverage (torch/onnx conflict on 3.13, checked).
uv venv --python 3.12 "$WORK/venv" >/dev/null
# Pinned to the battle-tested LEGACY exporter combo: torch>=2.9 defaults to the
# dynamo exporter, whose opset version-conversion produced a graph that fails
# onnx shape inference during quantization (verified 2026-07: "Inferred shape
# and existing shape differ in dimension 0"). torch 2.6 + dynamo=False is the
# classic path that quantize_dynamic supports reliably.
uv pip install --python "$WORK/venv/bin/python" --quiet \
  'torch==2.6.*' 'transformers==4.*' onnx onnxruntime

mkdir -p "$OUT"
"$WORK/venv/bin/python" - "$OUT" <<'PY'
import sys, tempfile, os
out = sys.argv[1]
tmp = tempfile.mkdtemp()

# 1. Export fp32 ONNX via plain torch.onnx (dynamic batch/sequence axes).
import torch
from transformers import AutoTokenizer, AutoModelForSequenceClassification

MODEL = "shahrukhx01/question-vs-statement-classifier"
tok = AutoTokenizer.from_pretrained(MODEL)
pt = AutoModelForSequenceClassification.from_pretrained(MODEL)
pt.eval()

sample = tok("is this a question", return_tensors="pt")
fp32_path = os.path.join(tmp, "model.onnx")
torch.onnx.export(
    pt,
    (sample["input_ids"], sample["attention_mask"], sample["token_type_ids"]),
    fp32_path,
    input_names=["input_ids", "attention_mask", "token_type_ids"],
    output_names=["logits"],
    dynamic_axes={
        "input_ids": {0: "batch", 1: "seq"},
        "attention_mask": {0: "batch", 1: "seq"},
        "token_type_ids": {0: "batch", 1: "seq"},
        "logits": {0: "batch"},
    },
    opset_version=17,
    dynamo=False,  # legacy TorchScript exporter — see pin rationale above
)

# 2. Dynamic int8 quantization (weights-only — robust for BERT classifiers).
from onnxruntime.quantization import quantize_dynamic, QuantType
quantize_dynamic(
    fp32_path,
    os.path.join(out, "model_int8.onnx"),
    weight_type=QuantType.QInt8,
)
# tokenizer.json for the Rust `tokenizers` crate.
tok.save_pretrained(tmp)
import shutil
shutil.copy(os.path.join(tmp, "tokenizer.json"), os.path.join(out, "tokenizer.json"))

# 3. PARITY CHECK: int8 ONNX must agree with the PyTorch model on labeled lines.
import onnxruntime as ort_rt
from transformers import AutoTokenizer, AutoModelForSequenceClassification
import torch

tok = AutoTokenizer.from_pretrained("shahrukhx01/question-vs-statement-classifier")
pt = AutoModelForSequenceClassification.from_pretrained(
    "shahrukhx01/question-vs-statement-classifier"
)
pt.eval()
sess = ort_rt.InferenceSession(os.path.join(out, "model_int8.onnx"))

cases = [
    ("what's the p99 latency on the new endpoint", 1),
    ("wait is this thread safe", 1),
    ("i pushed the fix it's in review", 0),
    ("nothing blocking on my end", 0),
    ("who owns the auth service now", 1),
    ("the build is green we're good to merge", 0),
]
agree = 0
for text, _ in cases:
    enc = tok(text, return_tensors="np")
    feeds = {k: v for k, v in enc.items() if k in {i.name for i in sess.get_inputs()}}
    onnx_logits = sess.run(None, feeds)[0][0]
    with torch.no_grad():
        pt_logits = pt(**tok(text, return_tensors="pt")).logits[0].numpy()
    if (onnx_logits.argmax() == pt_logits.argmax()):
        agree += 1
print(f"parity: {agree}/{len(cases)} predictions agree (int8 vs pytorch)")
assert agree == len(cases), "int8 quantization changed predictions — do not ship"
PY

ls -lh "$OUT"
echo "export-qdetect: done — package-airdrop.sh ships $OUT inside the tarball"
