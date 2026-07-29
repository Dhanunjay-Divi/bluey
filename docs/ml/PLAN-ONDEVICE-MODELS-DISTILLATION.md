# Complete Plan — Two on-device models, both trained under the $300 Google credit

> Constraint honored: **Bluey is local-first & private. We never collect, store, or train on real
> user meeting data.** Both models are trained 100% by **distillation from a frontier LLM +
> synthetic error simulation.** No real audio harvesting. The only "real" audio that ever exists
> is the three founders' own consented smoke-test recordings — used to *sanity check*, never to train.

---

## The two models (what / does / where it plugs in)

### Model A — Intent + Addressee classifier  (Task 1)
- **What:** MiniLM-L6-H384 fine-tuned → ONNX **int8** (~22M params, ~11 MB). Drop-in successor to
  today's `crates/cue-daemon/src/qdetect.rs` model (`dist/models/qdetect-en/model_int8.onnx`).
- **Does:** per finalized transcript segment → outputs (a) is this a question/request that needs
  answering, and (b) is it plausibly directed at the Bluey user. Handles declarative questions,
  tag questions, buried/embedded questions, disfluent ASR, AND the name-position logic
  (initial/medial/final vocative) + mention-vs-address hard negatives.
- **Where:** exactly where qdetect sits now — stage-2 after the regex prefilter. It is the trigger
  that decides "wake the agent to answer this."

### Model B — Lexical Speaker Error Corrector (LSEC/AG-LSEC)  (Task 2)
- **What:** small RoBERTa/MiniLM-class encoder fine-tuned → ONNX **int8**.
- **Does:** takes the diarizer's word→speaker labels and fixes words assigned to the wrong speaker
  using language plausibility around turn boundaries. 25–40% relative WDER reduction (LSEC); +15–25%
  more with acoustic grounding (AG-LSEC).
- **Where:** post-processor AFTER Sortformer diarization, BEFORE transcript reaches overlay/agent.
  Pipeline: `diarizer proposes → LSEC cleans → clean speaker-attributed transcript`.

### How they compound
Cleaner speaker labels (B) → more reliable "who asked this" → better addressee decision (A) →
the agent answers the *right question* from the *right person*. B is literally the "extra context
assurance that the same person spoke" idea, realized as a model.

### Model B is a CONFIDENCE-FUSION model, not a hierarchy (core design)
Neither the acoustic diarizer nor the lexical model is sovereign. For every word / turn boundary,
BOTH emit a probability distribution over speakers **plus a confidence**, and a learned fusion layer
combines them into one decision + one output confidence:
- **audio strong, text weak** → keep audio (don't let a weak lexical hunch override a certain voice ID)
- **audio borderline, text strong** → language breaks the tie (reply-logic / 1st-vs-2nd-person / backchannel)
- **both agree** → high-confidence label
- **both unsure** → emit a **low-confidence** label so downstream can treat it as uncertain (ask /
  lower the agent's certainty) instead of confidently guessing wrong.

Fusion is **learned, not hardcoded** — this is exactly AG-LSEC: the diarizer's per-speaker confidence
scores are fed IN as input features to the lexical transformer, which learns when to trust which
side (trust audio when its margin is wide; trust text when audio is split). Model B outputs a
**fused speaker label AND its own confidence**, which feeds Model A's addressee decision.

**Invariant:** Model B only moves speaker tags — it NEVER changes the words. It cannot hallucinate
text; it re-attributes existing text. (Matches the "don't create new words" rule.)

Rollout: **v1 = lexical-only** (text signal + its own confidence; ships first, pure-synthetic, no
audio input). **v2 = full fusion (AG-LSEC)** — add the acoustic confidence feature; needs a little
paired audio for that feature, so it is v2. Both are score/confidence producers so v1→v2 is additive.

---

## What VoxConverse actually gives us (and what it does NOT)

We have **448 files (232 test + 216 dev) WAV + RTTM** in `~/Downloads/voxconverse_*`.
- An RTTM line = `SPEAKER <file> 1 <start> <dur> <NA> <NA> spkNN` → **who spoke when. NO WORDS.**
- VoxConverse is audio-only diarization ground truth (YouTube/debate audio), **not meetings, no transcripts.**

Therefore:
- **Task 1: VoxConverse is useless** (no text/meaning). Task 1 = 100% frontier-generated text.
- **Task 2: VoxConverse can't train LSEC directly** (LSEC needs words on turns). Its ONLY role is
  **turn-timing calibration** — mine the RTTMs for realistic turn-length distributions, switch
  frequency, and overlap rate, so our *synthetic* transcripts have realistic turn rhythm.
  (Optional. If it's fiddly, skip it and let the LLM produce natural turns.)

Net: **both models are trained on frontier-synthetic data.** VoxConverse = a small realism knob for B.

---

## DATA PLAN (pure distillation)

### Task 1 dataset — ~10k train + ~1k held-out eval
Frontier LLM generates over a forced grid (guarantees coverage, kills template repetition):
`{9 question forms} × {name position: initial / medial / final / long-address-then-ask / no-name}
 × {addressee: directed-at-user / directed-at-other / mention-only / ambient}
 × {domain: eng / sales / standup / design / ops / legal}
 × {ASR-noise: clean / disfluent / phonetic-error} × {language: en (+ es/hi later)}`

Class balance (the whole ballgame):
- ~3.5k positive questions across forms (over-weight declarative/embedded/disfluent)
- ~2.5k positive addressee cases across all name positions incl. long-address-then-ask
- **~2.5k hard negatives**: name-MENTION ("Sashreek deployed it yesterday") + pronoun statements — ~1:1 with addressee positives
- ~1.5k neutral statements / narration / trailing thoughts
- **~1k held-out eval**: generated SEPARATELY (different seed/prompt), hand-verified by you. Never trained on.

### Task 2 dataset — ~15k simulated (target ~10–20k)
1. Frontier LLM generates ~2–3k realistic multi-speaker meeting transcripts (words + speaker per turn).
2. **Error-simulation script** injects diarizer-style errors → yields ~15k word-window training pairs:
   - boundary flips (last/first words of a turn swapped to neighbor speaker) — the dominant real error
   - turn split / turn merge, occasional whole-turn mislabel
   - (optional) calibrate turn lengths/switch-rate from VoxConverse RTTMs
3. Label = the corrected (true) speaker sequence. Model learns to undo the corruption from words alone.
4. Held-out eval: ~1k simulated, separate seed. (Optional AG boost: add first-pass acoustic score
   feature later — needs a little paired audio; defer to v2.)

---

## COMPUTE PLAN on the $300 Google credit

### Where the money goes (two line items)
1. **Frontier LLM generation/labeling** (the teacher) — via **batch API** for the discount.
2. **GPU fine-tuning** of the two students — on **Vertex AI / Colab Enterprise** with the credit.

### Cost estimate (deliberately conservative)
| Item | Volume | Est. cost |
|---|---|---|
| Task 1 generation+labels (batch) | ~11k short samples | $15–40 |
| Task 2 transcript generation (batch) | ~2.5k transcripts (longer) | $20–50 |
| Task 2 error-sim | script, local | $0 |
| GPU fine-tune A (MiniLM, few epochs, T4/L4) | a few runs | $5–15 |
| GPU fine-tune B (RoBERTa-small, curriculum) | a few runs | $15–40 |
| Active-learning re-label rounds | uncertain-only | $10–30 |
| Buffer for re-runs / mistakes | — | $50 |
| **Total** | | **~$120–225 of $300** |
Comfortable headroom. Both models fit under $300 with room for iteration.

### Optimization techniques we use (to keep it lean + cheap + high quality)
- **Batch API for all generation** — ~50% cheaper than sync, and generation is not latency-sensitive.
- **Grid/stratified sampling** instead of "generate 10k random" — forces coverage, avoids paying for
  duplicates; dedup by normalized-text hash (already in `generate_llm_dataset.py`).
- **Active learning (M-RARU style)** — train → find low-confidence/disagreement → frontier-verify
  ONLY those → add. Cuts labeling up to ~70%. This is why 10k *curated* beats 50k random.
- **Distillation, not from-scratch** — student learns from teacher labels (+ optionally soft
  logits / rationale) → far fewer examples for the same accuracy.
- **Parameter-efficient fine-tune (LoRA/prefix)** for <1k-per-class regimes early, full fine-tune
  once data is ample — cheaper GPU, less overfit.
- **Mixed precision (fp16) + small batch on T4/L4**, few epochs (3–4) — these tasks converge fast.
- **int8 dynamic quantization** on export (already in `finetune_and_export.py`) — ~4× smaller, CPU-fast.
- **Spot/preemptible GPUs** on Vertex for the training runs — cheapest tier; jobs are short.
- **Parity gate** (int8 vs fp32 agreement) before shipping — already the pattern in
  `scripts/export-qdetect-onnx.sh`. Keep it.

### "Are these the leanest, highest-quality models for our purpose?" — yes, and why
- **A: MiniLM-L6 int8** — smaller & faster than DistilBERT, near-identical accuracy on narrow
  classification, ~11 MB fits the one-binary distribution constraint. ModernBERT only as an
  *offline teacher* if we want an even stronger label source — never shipped.
- **B: small RoBERTa-class int8** — this is literally the architecture the LSEC papers use;
  matching it de-risks the approach. Ships as another small ONNX next to A.
- Neither needs a GPU at runtime — both run int8 on CPU inside the daemon, on-device, private.

---

## EXECUTION ORDER
1. **Task 1 first** (it's the drop-in upgrade with immediate product value; reuses the existing
   `qdetect.rs` load path + export script).
   a. Write the grid generator (extend `generate_llm_dataset.py`) → batch-generate ~11k.
   b. Fine-tune MiniLM-L6 → int8 ONNX (adapt `finetune_and_export.py`, swap DistilBERT→MiniLM).
   c. Parity-gate, eval on held-out, wire into `qdetect.rs` behind a flag.
2. **Task 2 second** (post-processor; bigger new surface).
   a. Transcript generator (batch) + error-simulation script.
   b. Fine-tune RoBERTa-small → int8 ONNX.
   c. Wire as post-diarization pass; add the addressee cross-signal into Model A.
3. Active-learning round on whichever model is weakest on eval. Ship.

---

## Privacy story (bonus — good for YC too)
"We train entirely on synthetic, distilled data. We never see, store, or train on a single user's
meeting. The product is local-first; the models are too." That's a moat, not a compromise.

---

## Sources
- LSEC (Interspeech 2023) — text-only training via simulated speaker errors — https://arxiv.org/html/2306.09313
- AG-LSEC (Interspeech 2024) — acoustic grounding, 25–40% WDER — https://arxiv.org/abs/2406.17266
- Amazon Science — AG-LSEC — https://www.amazon.science/publications/ag-lsec-audio-grounded-lexical-speaker-error-correction
- Knowledge distillation in automated annotation (500→25k curve) — https://arxiv.org/pdf/2406.17633
- AugGPT — GPT data augmentation (12–18% F1) — https://arxiv.org/pdf/2302.13007
- ActiveLLM — LLM-based active learning — https://arxiv.org/pdf/2405.10808
- VoxConverse — audio-only diarization ground truth (RTTM, no transcripts) — https://www.robots.ox.ac.uk/~vgg/data/voxconverse/
