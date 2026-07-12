# Bluey Positioning And Marketing Plan

Updated: 2026-07-12

## Recommended Position

Bluey should own:

> Live context for engineering meetings.

Bluey is not just a meeting recorder and not just a desktop chatbot. It is the
bridge between live conversation and the work context people normally scramble
to find: browser pages, code, docs, prior decisions, attached files, screenshots,
repo/project memory, and coding-agent context where explicitly authorized.

## One-Line Pitch

Bluey is a consent-first desktop assistant that listens when you turn it on,
reads approved context, and answers from your meeting, screen, files, and
project memory.

## Better Hero Direction

Use language like:

- "Bring your work context into every meeting."
- "Ask from the conversation, screen, files, and prior decisions."
- "A private AI copilot for live engineering work."
- "Stay present while Bluey keeps the context ready."

Avoid leading with:

- "Stay unseen."
- "Disguise."
- "Hidden assistant."
- Anything that sounds like bypassing proctoring, monitoring, workplace policy,
  or consent.

## Primary ICP

Start with engineering and technical work:

- Staff engineers in design reviews.
- Engineering managers in planning and incident calls.
- Technical founders in customer/support calls.
- Support engineers debugging with customers.
- Implementation consultants onboarding customers.
- Developer advocates or solutions engineers doing live demos.

Why this ICP:

- They have high context load.
- They already use coding agents and docs.
- The value is not just transcript quality; it is live retrieval and answer
  grounding from work artifacts.
- They are more likely to pay for managed routing, session memory, and source
  cards than generic note-taking users.

## Competitive Frame

### Against Meeting Note Tools

Otter, Fireflies, Fathom, Granola, Read AI, and similar tools help remember
meetings. Bluey should help users answer during the meeting.

Position:

- Notes are after-the-fact.
- Bluey is live context while the call is happening.

### Against Coding Agents

Codex, Cursor, Claude Code, Kiro, and Copilot know code, but they are not the
live meeting layer.

Position:

- Coding agents live in repos/editors.
- Bluey sits in the conversation and brings approved project context forward.

### Against Enterprise Copilots

Microsoft Copilot and Gemini are strong inside their ecosystems, but Bluey can
be cross-app, terminal-first, and context-source transparent.

Position:

- Bluey shows what it used and what was missing.
- Bluey can work across local files, pages, transcripts, and session memory.

## Website/SEO/GEO Work To Add

Mirror the Pinky search/AI-discovery pattern:

- `/llms.txt`
- `robots.txt` and `sitemap.xml`
- Canonical URLs
- `Organization`, `SoftwareApplication`, and `Product` JSON-LD
- FAQPage JSON-LD
- Plain-text, crawlable feature pages
- Search submission pack for Google Search Console and Bing Webmaster Tools
- UTM links for Product Hunt, X, LinkedIn, Hacker News, Reddit, founder email,
  and technical-community outreach

Recommended feature pages:

- `/ai-meeting-context-copilot`
- `/engineering-meeting-copilot`
- `/ai-copilot-for-design-reviews`
- `/screen-context-ai-assistant`
- `/meeting-memory-and-project-context`
- `/auto-model-router`
- `/private-desktop-ai-overlay`
- `/bluey-vs-ai-meeting-notetakers`
- `/bluey-vs-coding-agents`
- `/context-coverage`

## Content Themes

1. "What did we decide last time?"
2. "What code/doc/ticket is this person talking about?"
3. "Answer from the current screen plus the last 10 minutes."
4. "Attach the doc once; ask from it throughout the call."
5. "Know when Bluey is missing context."
6. "One answer card, right model, source-aware."

## Trust/Safety Copy

Use:

- Visible, user-controlled listening and context attachment.
- Best-effort capture exclusion where OS-supported.
- Cloud sync off by default and controlled separately from sign-in.
- Transient raw-audio processing; transcript text can be saved.
- No customer-content training in the current product.
- Export/delete controls with honest retention limits.
- Source cards and missing-context prompts.

Avoid:

- Undetectable.
- Untrackable.
- Bypass.
- Cheat.
- Proctoring.
- Hidden from security tools.
- Universal invisibility.

## Launch Order

1. Close AI cost reservation and trial abuse P1s.
2. Clean stale docs and owner docs.
3. Add `/llms.txt`, sitemap, JSON-LD, FAQ, and feature pages.
4. Run clean-Mac paid alpha smoke.
5. Invite a small engineering-heavy alpha cohort.
6. Watch provider spend, conversion, support tickets, refunds, and answer
   quality daily.
7. Only then do broader public/community marketing.
