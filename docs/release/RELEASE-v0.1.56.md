# Bluey 0.1.56

## Fixes

- If a managed streamed answer drops after partial text but the server already completed and cached the response, Bluey now recovers the cached final answer and finishes the overlay card.
- Algorithmic code prompts such as Sudoku solvers now route to the deeper code lane instead of the smaller simple-code path.

## Diagnostics

- Added managed stream recovery logs keyed by request id/ref so broken-answer reports can be traced without storing private prompt text in logs.
