// Dependency-free markdown renderer for streamed agent answers.
//
// Agents stream ONE markdown string (prose with **bold**, `inline code`,
// ```fenced blocks, lists, tables). Rendering it as raw text leaks the literal
// `##` / `**` / `|` syntax — the exact thing the user flagged. This turns it
// into the chat-app experience (ChatGPT/Claude-style): formatted prose, fenced
// code in its own card, real tables.
//
// Ported from the proven interview overlay renderer (crates/cue-overlay-tauri/
// ui/index.html), but re-expressed as REACT NODES rather than innerHTML — no
// `dangerouslySetInnerHTML`, no XSS surface, strict-TS clean. Self-contained on
// purpose: the overlay is screen-capture-invisible (hard to iterate on) and we
// want no external lib / network / version risk. Scope = the markdown agents
// actually emit, nothing more.
//
// Streaming contract (the key trick Claude/ChatGPT UIs use): while text is
// still streaming we render PLAIN text, so a half-open ``` fence never flickers
// raw markers. Only once `done` is true do we render the full rich markup.

import { Fragment, type ReactNode } from "react";

// --- inline: bold / italic / inline-code / links -> react nodes -------------
// We tokenize a single line of already-block-stripped text. Order matters:
// inline code is captured first (so * inside code isn't italicized), then
// bold, then italic, then links.
function renderInline(text: string, keyPrefix: string): ReactNode[] {
  const out: ReactNode[] = [];
  // One alternation pass; each match is one styled token, gaps are plain text.
  // The link group captures ANY scheme (`scheme:rest`) — the scheme is then
  // validated below. Agents emit file:// links (to local repo files) as well as
  // http/https/mailto; matching only `https?:` left file:// links rendering as
  // raw `[text](file://…)`, which is exactly the leak the user flagged.
  // `__bold__` is guarded to word boundaries (CommonMark forbids intraword `__`
  // emphasis): the opener must be at start-of-string or after a non-word char,
  // and the closer must be followed by a non-word char or end. Without this,
  // identifiers like `mcp__perplexity__perplexity_ask`, dunder function names,
  // and `FOO__BAR` env vars had their middle segment turned into spurious bold
  // with the `__` markers eaten — the run-on/mangling the user flagged.
  const re =
    /`([^`]+)`|\*\*([^*]+)\*\*|(?:^|(?<=\W))__([^_]+)__(?=\W|$)|(?:^|(?<=[^*]))\*([^*\n]+)\*|\[([^\]]+)\]\(([a-zA-Z][\w+.-]*:[^)\s]+)\)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let n = 0;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) out.push(text.slice(last, m.index));
    const k = `${keyPrefix}-i${n++}`;
    if (m[1] !== undefined) {
      out.push(
        <code key={k} className="md-code">
          {m[1]}
        </code>,
      );
    } else if (m[2] !== undefined) {
      out.push(<strong key={k}>{m[2]}</strong>);
    } else if (m[3] !== undefined) {
      out.push(<strong key={k}>{m[3]}</strong>);
    } else if (m[4] !== undefined) {
      out.push(<em key={k}>{m[4]}</em>);
    } else if (m[5] !== undefined && m[6] !== undefined) {
      out.push(renderLink(m[5], m[6], k));
    }
    last = re.lastIndex;
  }
  if (last < text.length) out.push(text.slice(last));
  return out;
}

// Render a markdown link with a positive scheme allowlist (never a blocklist).
//   http/https/mailto -> a real, openable anchor.
//   file:             -> styled but NON-clickable text. Agents reference local
//                        repo files via file://; a bare file:// href won't open
//                        from the sandboxed Tauri webview anyway, and making it
//                        clickable is a deliberate security surface (local-file
//                        access from model output) we don't open without an
//                        explicit opener capability. So we show it readable, not
//                        raw, not navigable.
//   anything else (javascript:, data:, vbscript:, …) -> plain label text only.
function renderLink(label: string, href: string, key: string): ReactNode {
  const scheme = (href.match(/^([a-zA-Z][\w+.-]*):/)?.[1] ?? "").toLowerCase();
  if (scheme === "http" || scheme === "https" || scheme === "mailto") {
    return (
      <a key={key} href={href} target="_blank" rel="noopener noreferrer">
        {label}
      </a>
    );
  }
  if (scheme === "file") {
    return (
      <span key={key} className="md-filelink" title={href}>
        {label}
      </span>
    );
  }
  // Disallowed / unknown scheme — render the label as inert text.
  return <span key={key}>{label}</span>;
}

// A markdown table row "| a | b |" -> cells. Returns null if not a table row.
function tableCells(line: string): string[] | null {
  const t = line.trim();
  if (!t.startsWith("|")) return null;
  return t
    .replace(/^\|/, "")
    .replace(/\|$/, "")
    .split("|")
    .map((c) => c.trim());
}

// A separator row "| --- | :--: |" — marks the header/body boundary.
function isTableSeparator(line: string): boolean {
  const cells = tableCells(line);
  return (
    !!cells && cells.length > 0 && cells.every((c) => /^:?-{1,}:?$/.test(c))
  );
}

/** Render a markdown string as React nodes (block-level). */
export function Markdown({ source }: { source: string }): ReactNode {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const blocks: ReactNode[] = [];
  let i = 0;
  let key = 0;
  const nextKey = () => `b${key++}`;

  while (i < lines.length) {
    const line = lines[i];

    // fenced code block -> code card. Render as a code card ONLY when the
    // CLOSING fence has arrived (the 2026 streaming pattern): a still-open
    // fence mid-stream would otherwise turn ordinary prose after a lone ```
    // into a spurious code block that never closes — the "code block where
    // there's no code" the user flagged. Until the closing ``` lands we treat
    // the opener as literal text so it just reads as ``` on its own line.
    const fence = line.match(/^\s*```\s*([\w+#.-]*)\s*$/);
    if (fence) {
      let j = i + 1;
      while (j < lines.length && !/^\s*```\s*$/.test(lines[j])) j++;
      const closed = j < lines.length;
      if (closed) {
        const lang = fence[1] || "";
        const buf = lines.slice(i + 1, j);
        blocks.push(
          <CodeCard key={nextKey()} lang={lang} code={buf.join("\n")} />,
        );
        i = j + 1; // past the closing fence
        continue;
      }
      // Unclosed fence (still streaming): fall through and render this line as
      // ordinary text; the code card materializes once the closer arrives.
    }

    // heading (#, ##, ###)
    const h = line.match(/^(#{1,3})\s+(.*)$/);
    if (h) {
      const lvl = h[1].length;
      const inner = renderInline(h[2], nextKey());
      blocks.push(
        lvl === 1 ? (
          <h1 key={nextKey()} className="md-h1">
            {inner}
          </h1>
        ) : lvl === 2 ? (
          <h2 key={nextKey()} className="md-h2">
            {inner}
          </h2>
        ) : (
          <h3 key={nextKey()} className="md-h3">
            {inner}
          </h3>
        ),
      );
      i++;
      continue;
    }

    // horizontal rule
    if (/^\s*(---|\*\*\*|___)\s*$/.test(line)) {
      blocks.push(<hr key={nextKey()} className="md-hr" />);
      i++;
      continue;
    }

    // table: a row followed by a separator row
    if (
      tableCells(line) &&
      i + 1 < lines.length &&
      isTableSeparator(lines[i + 1])
    ) {
      const header = tableCells(line)!;
      i += 2; // skip header + separator
      const rows: string[][] = [];
      while (i < lines.length && tableCells(lines[i])) {
        rows.push(tableCells(lines[i])!);
        i++;
      }
      const tk = nextKey();
      blocks.push(
        <table key={tk} className="md-table">
          <thead>
            <tr>
              {header.map((c, ci) => (
                <th key={ci}>{renderInline(c, `${tk}-th${ci}`)}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r, ri) => (
              <tr key={ri}>
                {r.map((c, ci) => (
                  <td key={ci}>{renderInline(c, `${tk}-r${ri}c${ci}`)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>,
      );
      continue;
    }

    // blockquote
    if (/^\s*>\s?/.test(line)) {
      blocks.push(
        <blockquote key={nextKey()} className="md-quote">
          {renderInline(line.replace(/^\s*>\s?/, ""), nextKey())}
        </blockquote>,
      );
      i++;
      continue;
    }

    // unordered list (gather consecutive items)
    if (/^\s*[-*+]\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*[-*+]\s+(.*)$/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*[-*+]\s+/, ""));
        i++;
      }
      const lk = nextKey();
      blocks.push(
        <ul key={lk} className="md-ul">
          {items.map((it, ii) => (
            <li key={ii}>{renderInline(it, `${lk}-${ii}`)}</li>
          ))}
        </ul>,
      );
      continue;
    }

    // ordered list
    if (/^\s*\d+\.\s+/.test(line)) {
      const items: string[] = [];
      while (i < lines.length && /^\s*\d+\.\s+(.*)$/.test(lines[i])) {
        items.push(lines[i].replace(/^\s*\d+\.\s+/, ""));
        i++;
      }
      const lk = nextKey();
      blocks.push(
        <ol key={lk} className="md-ol">
          {items.map((it, ii) => (
            <li key={ii}>{renderInline(it, `${lk}-${ii}`)}</li>
          ))}
        </ol>,
      );
      continue;
    }

    // blank line — skip
    if (/^\s*$/.test(line)) {
      i++;
      continue;
    }

    // paragraph (gather consecutive non-special lines, join with <br>)
    const para: string[] = [line];
    i++;
    while (
      i < lines.length &&
      !/^\s*$/.test(lines[i]) &&
      !/^\s*```/.test(lines[i]) &&
      !/^(#{1,3})\s/.test(lines[i]) &&
      !/^\s*[-*+]\s/.test(lines[i]) &&
      !/^\s*\d+\.\s/.test(lines[i]) &&
      !/^\s*>/.test(lines[i])
    ) {
      para.push(lines[i]);
      i++;
    }
    const pk = nextKey();
    blocks.push(
      <p key={pk} className="md-p">
        {para.map((l, li) => (
          <Fragment key={li}>
            {li > 0 && <br />}
            {renderInline(l, `${pk}-${li}`)}
          </Fragment>
        ))}
      </p>,
    );
  }

  return <>{blocks}</>;
}

// A fenced code block as its own card with a language label + copy button.
function CodeCard({ lang, code }: { lang: string; code: string }) {
  const copy = () => {
    void navigator.clipboard?.writeText(code).catch(() => {});
  };
  return (
    <div className="md-codecard">
      <div className="md-cc-head">
        <span className="md-cc-lang">{lang || "code"}</span>
        <button
          type="button"
          className="md-cc-copy"
          onClick={copy}
          aria-label="Copy code"
        >
          ⧉ Copy
        </button>
      </div>
      <pre className="md-pre">
        <code>{code}</code>
      </pre>
    </div>
  );
}
