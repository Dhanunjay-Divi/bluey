# Round 529 - Jobs healthcare resume parser and career suggestions

Date: 2026-07-16

Status: implementation complete; verified and ready for portal-only production deployment

## Objective

Fix a concrete Bluey Jobs onboarding import where a DOCX healthcare resume mixed
employer, title, and location values. Add compact, accessible suggestions to the
career fields where a user benefits from known choices without preventing custom
answers.

The owner-provided resume was used only as a local regression input. Its filename,
candidate identity, contact details, and document contents are not stored in the
repository or generated portal bundle.

## Root causes

The DOCX importer used Mammoth's raw-text output. Manual line breaks inside Word
paragraphs were flattened, so a degree and school or an employer and role could
lose the boundary the parser needed.

The employment heuristic also treated a company-looking line as a possible title
and did not reliably split these common resume shapes:

```text
Employer
Title, City, ST Month Year - Month Year
```

```text
Employer
Title, City, Country Month Year - Present
```

This could place the employer in the title field and combine the title with the
location field.

## Implementation

### Structured DOCX conversion

`jobs/portal/src/lib/documents.ts` now converts DOCX to HTML and then normalizes
the HTML into parser text while preserving:

- manual line breaks;
- paragraphs and list items;
- table cells;
- bullet boundaries;
- escaped characters.

The employment parser scores candidate line arrangements instead of assuming one
resume template. It separates employer, title, US location, international
location, and date range while retaining uncommon titles. Education parsing now
handles degree-field separators and a school/date line without turning the month
into part of the school name.

Affiliations and unrelated trailing sections are isolated from skills so they do
not become application keywords accidentally.

### Career suggestions

`jobs/portal/src/data/career-suggestions.ts` provides a reusable suggestion source
for:

- current and target locations;
- current and target roles;
- work-history employer, title, and location values;
- skills;
- certifications.

Imported profile values are merged with a curated baseline. Matches rank exact
prefixes before word-prefix and contained matches. Duplicate and already-selected
values are removed.

The onboarding fields use an accessible combobox pattern with Arrow Up/Down,
Enter, Escape, mouse selection, active-option state, and listbox semantics. A
suggestion changes a value only after explicit selection; free-form values remain
supported everywhere.

## Regression coverage

The committed fixture is fictional but preserves the reported resume structure.
It verifies:

- employer and title remain in their correct fields;
- `City, ST` and `City, Country` locations are separated;
- uncommon role names survive;
- degree, field, school, and completion year are recovered;
- Word tables, bullets, and manual line breaks survive conversion;
- affiliations do not pollute extracted skills;
- typeahead ranking, deduplication, and selected-value exclusion.

The exact owner-provided DOCX was also exercised locally through the production
import path. All five work-history entries, including US and international roles,
were separated into company, title, location, and date fields correctly. The
temporary private-file test was deleted immediately after verification.

## Verification

Automated:

- complete Jobs package suite: 248 tests passed;
  - automation: 130;
  - browser: 34;
  - runner: 40;
  - workflows: 23;
  - portal: 21;
- all five Jobs TypeScript packages typechecked;
- portal production build passed;
- generated Jobs output contains no source maps;
- repository scan found no owner-resume name or filename;
- `git diff --check` passed.

Visual and interaction QA:

- desktop current-location suggestions rendered below the input without changing
  the card dimensions;
- mobile `390x844` layout had zero horizontal overflow;
- keyboard selection inserted the highlighted location;
- role suggestions rendered inside the work-history editor;
- light-theme borders, spacing, focus state, and list contrast remained aligned;
- browser console contained no warnings or errors.

## Scope

This round changes only Jobs resume import, onboarding suggestions, tests, styles,
and the generated `/jobs/` portal bundle. It does not modify the main Bluey API,
Jobs API, native overlay, meeting/audio runtime, browser runner, billing, Coach,
workspaces, or signed native release artifacts.
