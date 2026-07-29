# Deep Research — How questions surface in meetings, and how to tell WHO one is directed at

> Purpose: before we build/train anything, define the **full problem space** the model must
> cover. Two axes: (1) every way a *question* can surface in real meeting speech, and (2) every
> position a *name / address term* can appear so we can infer *who the question is directed at*.
> Then: how much data actually gives quality output (grounded, not assumed).
>
> This supersedes the intuition in `sample_transcript.txt`. All claims here are web-grounded;
> sources at the bottom.

---

## 0. What the model actually has to decide

Two coupled decisions, NOT one:

1. **Is this utterance something my agent should answer?** (a question / a request for info)
2. **Is it directed at ME (the Bluey user)?** — inferred from *where and how a name/address term appears*.

Both must be robust to ASR output: **no reliable punctuation, no capitalization, no `?`,
disfluencies, self-corrections, and phonetic errors.** That single fact kills every
punctuation-based shortcut and is why this is a real ML problem, not a regex.

---

## AXIS 1 — Every way a QUESTION surfaces in spoken conversation

Linguistics recognizes a small set of *forms* but a large set of *surface realizations*. In
speech, form and intonation diverge constantly — the hard cases are questions that **don't look
like questions on the page**.

### 1.1 Canonical forms (the easy 20%)
- **Wh-questions** — begin with what / why / when / where / who / whose / which / how.
  ("how long has this been broken")
- **Polar / yes-no via inversion** — aux-subject inversion. ("did we ship the hotfix", "is the mutex thread-safe")

### 1.2 Declarative questions / rising declaratives (the hard 80%)
A **rising declarative** has the *syntax of a statement* but the *function of a question* — in
text it's indistinguishable from a statement because the only cue was intonation, which ASR
throws away. ("the migration is backwards compatible" = could be a claim OR "…right?").
→ This is our `declarative_confirmation` category. **Highest-value, hardest class.**

### 1.3 Tag questions
Statement + interrogative fragment tag: "…right", "…correct", "…yeah", "…isn't it",
"…makes sense". Far more common in spoken than written. ("the tests are passing on CI right")

### 1.4 Embedded / buried questions
The question sits inside a long narrated preamble: "yeah so the routing is fine but when we pass
data it hangs, I think it's the MCP connector, like is it actually pulling live context or just
grabbing a stale state". The model must find the interrogative buried mid-turn.

### 1.5 Echo / clarification questions
Repeating back for confirmation: "the *staging* cluster?" — a fragment that's a question only
in context.

### 1.6 Alternative ("or") questions
"do we chunk it or fail out", "should we switch to worker threads or optimize the query" —
two options joined by *or*, no leading aux.

### 1.7 Disfluent / hedged questions
"so um did we actually ship that", "wait like is this thread safe or whatever", "I mean do we
even need the flag" — filler words (um, uh, like, I mean, you know, so, wait) wrap the question.

### 1.8 ASR-corrupted questions
Phonetic errors + self-correction inside the transcript: "did u check whether the cash is
empty i mean whether the cache is empty". The model must survive homophone errors and mid-
utterance restarts.

### 1.9 Imperative-as-request (borderline — decide policy)
"walk me through the deploy", "remind me what the timeout is" — imperative surface, but it *is*
a request the agent should answer. **Policy decision needed:** treat as answerable (label 1) or
separate `request` class. (This is the seam between the binary product and the multi-intent product.)

### 1.10 NON-questions that look like questions (hard negatives — critical)
- **Rhetorical / discourse markers:** "you know what I mean", "right?" as a filler, "guess what" —
  no real answer wanted.
- **Reported/embedded clauses:** "I asked him whether the cache was warm" — contains *whether*
  but is a *statement about a past question*, not a live one.

> **Coverage requirement:** the dataset must contain all of 1.1–1.10, with 1.2 (declarative),
> 1.4 (embedded), 1.7 (disfluent) over-represented because they're where naive models fail.

---

## AXIS 2 — WHO is the question directed at? (name / address-term position)

This is the axis your `sample_transcript.txt` intuition nailed and the current data barely
covers. Linguistics term: **vocative / noun of direct address.** A vocative can sit in **initial,
medial, or final** position, and the position changes the parse.

### 2.1 The three canonical positions
| Position | Example | Signal |
|---|---|---|
| **Initial** | "Sashreek, did you check the DB" | name → then question. Strongest, cleanest cue. |
| **Medial** | "we need your call, Sashreek, before we merge" | name embedded, commas gone in ASR → hard |
| **Final** | "so who's owning the rollback, Sashreek" | question → then name. Very common in meetings |

### 2.2 The pattern you specifically called out — "address a lot, then ask, ending in *you*?"
Speaker addresses a person, talks *at length*, then lands the question, sometimes with the
addressee marker (the name OR a turn-final "…you?") at the very end. e.g.:
"Sashreek — so we were looking at the vertex payload and the schema's huge, and I know you were
in that code last week, so did you already add the chunking, or is that still on you?"
→ The addressee cue is **far** from the interrogative core. Requires whole-utterance context,
not a local window. This is the marquee hard case; it must be a named category with many examples.

### 2.3 Distinguishing addressee-name from mentioned-name (THE central hard negative)
Same token "Sashreek", two totally different meanings:
- **Addressee (label toward "directed at me"):** "Sashreek, did you deploy it" — 2nd person, *you*.
- **Third-person mention (NOT directed at me):** "Sashreek deployed it yesterday" — he/she/they, past tense.
The discriminating lexical cues (grounded in addressee-detection research):
- **2nd-person pronouns** (you / your / you'd) co-occurring with the name → addressee.
- **3rd-person framing** (he/she/they, "Sashreek's PR", past-tense narration) → mention.
- **Utterance-initial cues:** command verbs & "you" → directed; "I"/"it"/"yeah"/"okay" → not.
→ These are your `hard_negative_name` and `hard_negative_pronoun` classes. They are the entire
reason a dumb name-match fails, and they must be **~1:1 with the positive addressee examples**,
not a small slice.

### 2.4 Context window matters (grounded)
Addressee-detection research: extending context from the current utterance to a **~10-second
window** raised addressee accuracy from 59.5% → 67.2% mAP; beyond ~15s it *degrades* (irrelevant
history). → Implication for us: feed the model **the current utterance + a short prior-turn
window**, not the isolated sentence, and not the whole transcript.

### 2.5 No-name-at-all cases
Often nobody says a name — addressee is carried purely by "you", gaze, or turn-taking. Text-only,
we can only use the pronoun/verb cues. The model should output *lower confidence* here, and this
is exactly where the **diarization/speaker-context signal (Task 2)** adds assurance: "same person
who owns this code is likely the addressee." That's the cross-signal you intuited.

---

## The label space this implies

Positive (agent should consider answering) requires **BOTH**: (a) it's a question/request AND
(b) plausibly directed at the user. That's a 2-dimensional label, best modeled as:

- **Dim A — is_question** {none, wh, polar, declarative, tag, embedded, alternative, echo, imperative-request}
- **Dim B — addressee** {directed_at_user, directed_at_other_named, ambient/unclear, third-person-mention}

Product label = `answerable_by_me = is_question(Dim A ≠ none) AND addressee(Dim B ∈ {directed_at_user, ambient})`.

Keeping the two dims as **auxiliary heads / metadata** (like your `category` field already does)
is what makes the model debuggable and lets you flip the product policy without relabeling.

---

## AXIS 3 — How much data actually gives quality output (grounded, not assumed)

Web-grounded numbers for fine-tuning a small BERT-class model on a narrow classification task:

- **Floor:** a distilled classifier hits ~60% accuracy at **~500 synthetic examples (~50/class)**;
  performance keeps rising up to ~25k. So 500 is a *toy*, not a product.
- **Few-shot reality:** below ~500/class, BERT fine-tuning overfits and fails to generalize —
  this is exactly why our current 87-line set is inadequate.
- **Synthetic augmentation payoff:** GPT-generated synthetic data lifts BERT F1 by **12–18%**
  precisely when real samples are under ~500/class.
- **Rule of thumb for multi-class:** 3–5 classes → 100+ synthetic/class beats prompting;
  **10+ classes or nuanced outputs → 300+/class.** Our nuanced hard-negative-heavy task sits at
  the high end.
- **Active learning:** becomes impactful **above ~1k samples** and cuts labeling by **up to 70%**
  — so we generate broad, then only human/frontier-verify the *uncertain* ones.
- **Diminishing returns / risk:** more synthetic diversity helps until it introduces off-task
  "error-island" samples; needs dedup + regularization + a *held-out REAL* eval set.

### Target dataset size (recommendation)
Given the nuance and the heavy hard-negative requirement:

| Bucket | Target count | Notes |
|---|---|---|
| Positive — questions × 9 forms (1.1–1.9) | ~2,000 | over-weight declarative/embedded/disfluent |
| Positive — addressee positions (2.1–2.5) | ~1,500 | each position + the "long address then ask" case |
| **Hard negatives — name-mention & pronoun (2.3)** | ~2,000 | ~1:1 with positives — the whole ballgame |
| Neutral statements / narration / trailing thoughts | ~1,500 | |
| Multilingual + heavy ASR-noise variants | ~1,000 | robustness |
| **Held-out REAL eval** (from actual meeting transcripts) | ~300–500 | frontier-labeled, never trained on — the honest scoreboard |
| **Total** | **~8k train + ~0.5k real eval** | ≈ 300+/effective-class, above the active-learning threshold |

Cost on the $300 Google credit: frontier synthetic generation + labeling of ~8k at this scale is
well within **$50–150**; GPU distillation/training runs another **$20–50**. Comfortable headroom.

### Generation strategy (frontier synthetic + active learning — the chosen path)
1. **Frontier LLM generates** across a grid: {9 question forms} × {3 name positions + no-name} ×
   {domains beyond eng: sales, standup, design, legal} × {ASR-noise on/off} × {language}. Grid
   sampling forces coverage instead of the template repetition in the current set.
2. **Dedup** (normalized-text hash — already in `generate_llm_dataset.py`).
3. **Active learning loop:** train small model → find low-confidence / disagreement examples →
   frontier-verify ONLY those → add. Repeat. (M-RARU style; up to 70% fewer labels.)
4. **Hold out a REAL eval set** from the 11 users' actual transcripts (privacy-scrubbed,
   frontier-labeled). Never train on it. This is the only number that matters.

---

## What this changes vs. the current `small test` build
- Current: 87 examples, English, eng-domain, binary, template-ish. **Verdict: a proof-of-concept,
  not trainable to product quality.** Right instinct, wrong scale.
- Keep: the **category taxonomy** (it maps cleanly onto Axis-1 forms + the hard negatives) and the
  **DistilBERT→ONNX int8 export path** — but ship on **MiniLM-L6 int8** per the model decision.
- Add: Axis-2 addressee positions as first-class categories (esp. 2.2 long-address-then-ask and
  2.3 mention-vs-address), the ~8k grid-generated set, and a real held-out eval.

---

## Sources
- Question–response system in American English conversation — https://scispace.com/pdf/an-overview-of-the-question-response-system-in-american-3gg6uel5l7.pdf
- Rising declarative — https://en.wikipedia.org/wiki/Rising_declarative
- Echo question — https://en.wikipedia.org/wiki/Echo_question
- Tag questions in discourse — http://www.rusnauka.com/PNR_2006/DN2006/Philologia/4_mihaylenko%20valeriy%20.doc.htm
- Universal Dependencies — vocative relation — https://universaldependencies.org/u/dep/vocative.html
- Noun of address / vocative expression — https://en.wikipedia.org/wiki/Vocative_expression
- Towards automatic addressee identification in multi-party dialogues — https://aclanthology.org/W04-2317.pdf
- Addressee detection for spoken dialogue systems (initial-position lexical cues) — https://www.ncbi.nlm.nih.gov/pmc/articles/PMC7249173/
- LLM benchmark for addressee recognition in multi-party dialogue — https://arxiv.org/pdf/2501.16643
- Selective Attention System — device-addressed speech detection (context window) — https://arxiv.org/pdf/2604.08412
- Fine-tuning LLMs with limited data: survey — https://arxiv.org/pdf/2411.09539
- Knowledge distillation in automated annotation (500→25k curve) — https://arxiv.org/pdf/2406.17633
- AugGPT: GPT for text data augmentation (12–18% F1) — https://arxiv.org/pdf/2302.13007
- ActiveLLM: LLM-based active learning, few-shot — https://arxiv.org/pdf/2405.10808
