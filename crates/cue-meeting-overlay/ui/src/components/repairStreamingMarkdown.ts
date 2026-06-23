// Streaming-markdown repair — the trick every production AI chat UI uses.
//
// The problem: an agent answer streams in token-by-token. If we hand the raw
// cumulative text to a markdown parser mid-stream, the live TAIL is usually
// half-written — an open ``` fence with no close yet, a `**bold` with no
// closer, a `[label](file://par` link still being typed. Rendered as-is those
// flash on screen as ugly raw syntax until the answer finishes. (That's the
// "raw ## and ** everywhere" the user saw.)
//
// The fix (Streamdown/`remend`'s "termination" approach, verified across
// ChatGPT/Claude/Perplexity/Vercel AI SDK): before parsing, scan the cumulative
// string and append the MISSING closers to a COPY, so the parser always sees
// well-formed markdown. Formatting then appears live, every frame, with no raw
// markers ever reaching the screen. The original text is never mutated.
//
// Order matters and is the whole correctness story: we establish code-fence
// context FIRST, because markers inside an open fence are literal code and must
// NOT be "balanced" (doing so corrupts the code block). Only OUTSIDE a fence do
// we balance inline emphasis / inline code, then neutralize a trailing
// incomplete link/image.
//
// Constraints honored: pure function, strict TS (no `any`), zero dependencies,
// NO regex lookbehind (kept portable to older WebKit). Scope = the markdown our
// agents actually emit; this is intentionally a small, well-understood subset,
// not a general markdown repairer.

/** Append whatever closers the trailing unterminated construct needs, on a
 *  copy. The returned string parses as well-formed markdown for our renderer. */
export function repairStreamingMarkdown(src: string): string {
  if (!src) return src;

  // ---- 1. Code-fence context (HIGHEST priority) --------------------------
  // Walk lines tracking the single currently-open fence (if any). A fence opens
  // with >=3 backticks or tildes (up to 3 leading spaces) and closes with a run
  // of the SAME char that is at least as long. We only care about the state at
  // end-of-input: is a fence still open, and where did its body start.
  const lines = src.split("\n");
  let fenceChar: "`" | "~" | null = null;
  let fenceLen = 0;
  const fenceRe = /^[ \t]{0,3}(`{3,}|~{3,})/;
  for (const line of lines) {
    const m = line.match(fenceRe);
    if (!m) continue;
    const run = m[1];
    const ch = run[0] as "`" | "~";
    if (fenceChar === null) {
      // opening fence
      fenceChar = ch;
      fenceLen = run.length;
    } else if (ch === fenceChar && run.length >= fenceLen) {
      // closing fence (same char, >= length)
      fenceChar = null;
      fenceLen = 0;
    }
    // a fence-looking line of the other char while open is just code — ignore.
  }

  let out = src;

  if (fenceChar !== null) {
    // A fence is open at EOF: everything after it is code. Append a synthetic
    // closing fence so our renderer's fenced-code branch fires NOW (renders a
    // code card immediately) instead of leaking the opening ``` into a
    // paragraph. Do NOT touch any inline markers — they're literal code.
    const closer = fenceChar.repeat(fenceLen);
    // Ensure the closer is on its own line.
    out = out.endsWith("\n") ? `${out}${closer}` : `${out}\n${closer}`;
    return out;
  }

  // ---- 2. Trailing incomplete link / image (before inline balancing) -----
  // A half-typed `[label](partial` or `![alt](partial`, or a dangling `[label`
  // / `![alt` with no `]( ` yet, must NOT flash as a broken/clickable link.
  // Render it as plain text by stripping the markdown link scaffolding from the
  // tail only. Completed links earlier in the string are untouched.
  out = neutralizeTrailingLink(out);

  // ---- 3. Balance trailing inline markers on the tail --------------------
  // Outside any fence, an odd count of an inline marker means the tail is
  // mid-emphasis; append the matching closer so it renders styled instead of
  // showing the raw marker. We strip complete inline-code spans first so a
  // backtick-protected `*` is never counted.
  out = balanceInline(out);

  return out;
}

// Remove a trailing, still-incomplete markdown link or image so it renders as
// plain text rather than a broken link. Only the very end of the string is
// considered — anything fully formed (`[x](y)`) is left alone.
function neutralizeTrailingLink(src: string): string {
  // Find the last unmatched '[' that begins the trailing region.
  const lastOpen = src.lastIndexOf("[");
  if (lastOpen === -1) return src;

  const tail = src.slice(lastOpen);
  // If the tail contains a complete `](...)`, the link is closed → leave it.
  if (/\]\([^)]*\)/.test(tail)) return src;

  // Otherwise the trailing `[...` (optionally with `(` started, optionally an
  // image `![`) is incomplete. Drop the scaffolding, keep the human text.
  const imageBang = lastOpen > 0 && src[lastOpen - 1] === "!" ? 1 : 0;
  const head = src.slice(0, lastOpen - imageBang);
  // tail forms: "[label", "[label]", "[label](partial", "[label](" → keep label
  const label = tail
    .replace(/^\[/, "")
    .replace(/\]\([^)]*$/, "")
    .replace(/\]$/, "");
  return head + label;
}

// Append closers for an odd number of trailing emphasis / inline-code markers.
function balanceInline(src: string): string {
  // Mask out complete inline-code spans (`code`) so markers inside them aren't
  // counted. Replace each complete span with spaces of equal length to keep the
  // odd/even backtick accounting correct for the UNclosed trailing span.
  const masked = src.replace(/`[^`\n]*`/g, (m) => " ".repeat(m.length));

  let out = src;

  // Inline code: an odd number of remaining backticks means an open span.
  const backticks = (masked.match(/`/g) || []).length;
  if (backticks % 2 === 1) {
    out += "`";
    return out; // inside inline code now; don't also balance emphasis on it
  }

  // Bold `**`: count complete `**` pairs; an odd count means an open bold.
  const boldMarkers = (masked.match(/\*\*/g) || []).length;
  if (boldMarkers % 2 === 1) {
    // If the buffer ends in a lone `*` (first half of a streaming `**`), append
    // only one `*` so we never produce `***`.
    out += out.endsWith("*") && !out.endsWith("**") ? "*" : "**";
    return out;
  }

  // Italic single `*`: count single stars that are NOT part of `**` and are
  // emphasis-shaped (have non-space content after the opener). We approximate
  // by removing `**` pairs first, then counting bare `*`.
  const singleStarBuf = masked.replace(/\*\*/g, "");
  const singleStars = (singleStarBuf.match(/\*/g) || []).length;
  if (singleStars % 2 === 1 && !endsWithListBullet(src)) {
    out += "*";
    return out;
  }

  // Strikethrough `~~`.
  const strike = (masked.match(/~~/g) || []).length;
  if (strike % 2 === 1) {
    out += "~~";
    return out;
  }

  return out;
}

// True when the final line is a bare list bullet ("- ", "* ", "+ ") — so a
// trailing "* " must never be auto-closed as italic.
function endsWithListBullet(src: string): boolean {
  const lastLine = src.slice(src.lastIndexOf("\n") + 1);
  return /^\s*[-*+]\s*$/.test(lastLine);
}
