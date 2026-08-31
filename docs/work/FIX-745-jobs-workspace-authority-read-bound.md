# FIX-745: Jobs Workspace Authority Read Bound

**Severity:** P2 availability/scalability

**Status:** Implemented; focused evidence green; independent review pending

## Issue

Workspace projection opened multiple authority transactions per posting over an unbounded listing.
A permitted 10,000-job snapshot could trigger tens of thousands of database operations.

## Required Fix

- Use one coherent bounded or batched authority snapshot for workspace representation.
- Apply deterministic pagination/bounds without silently changing authorization semantics.
- Add a regression proving the maximum authority-read/workspace result contract.

## Evidence

`current_authority_list_fails_closed_above_its_bound_without_blocking_detail` passes. The complete
three-test workspace representation slice, server library check, and strict Clippy pass. Aggregate
gates, live PostgreSQL representation coverage, and post-fix independent review remain pending.
