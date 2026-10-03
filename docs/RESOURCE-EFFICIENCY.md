# Behavior-preserving resource refactor

This refactor optimizes the desktop agent's report analysis, pending sync queue,
and frontend startup while preserving existing features and data formats. The
initial validation baseline was the local Windows checkout at `b2d482e` plus its
existing edits. Release 5.0.18 ports the changes onto the current Windows, macOS,
and Linux branches, preserving platform capture and privacy implementations.

## Changes

- Category lookups use the shared policy registry directly for canonical names.
  Aliases and unknown values keep the original normalization and fallback rules.
- Repeated foreground visits use a prefix index of recent work destinations.
  Each intervening-work query now takes constant time rather than rescanning
  an observation range. The index adds two machine words per observation;
  grouping and chronological sorting remain in the analysis pipeline.
- Contiguous browsing episodes store start/end ranges instead of allocating
  one index per observation. Episode boundaries and duration thresholds stay
  the same, including midnight and sensor-grace behavior.
- Report examples retain the twelve longest observations with a bounded list
  of references. Selection takes O(n) time with O(12) working storage instead of
  cloning n records and sorting them in O(n log n). Equal durations retain their
  original input order. A duplicated candidate-record type is removed, and
  sample deduplication uses sets instead of maps containing unused booleans.
- SQLite rows are consumed as iterators in report and daily-history loading,
  avoiding an intermediate collection of raw narrative records.
- An idempotent partial index, `reports_pending_by_id`, indexes only rows with
  `synced = 0`. It serves the existing oldest-first sync query without changing
  its SQL predicate, selected columns, batch limit, or null handling. Bootstrap
  logs a warning and continues if index creation fails. The index is maintained
  by SQLite as reports are written and synchronized.
- The PDF renderer loads with jsPDF when PDF export is requested. Export layout
  and filenames remain unchanged; startup avoids parsing the PDF renderer.
- The direct `image` dependency enables PNG only. The screenshot path still
  resizes the same RGBA pixels with Lanczos3 and writes the same PNG format.
  The initial Windows validation removed 39 packages from its dependency graph. Transitive image
  dependencies used by the screenshot provider remain enabled as required.

AI models, prompts, inference settings, capture cadence, category policies,
focus thresholds, sync cadence, privacy settings, and product UI are preserved.

## Validation

- Baseline: 133 Rust tests and 57 renderer tests passed.
- Refactor: 138 Rust tests and all 57 renderer tests passed.
- Five added regression tests cover category aliases, exhaustive small work
  timelines, stable example selection, PNG round trips, and pending-query
  equivalence/index usage.
- Rust formatting and `cargo clippy --all-targets -- -D warnings` passed.
- The production Vite build passed. Entry JavaScript fell from 372,671 to
  360,569 bytes, a reduction of 12,102 bytes (3.25%). PDF code remains included
  in the installation as a separate module.
- Chromium exercised the production assets with synthetic data and mocked
  native IPC: PDF modules were absent from startup requests, loaded on demand,
  and generated a valid two-page PDF without page errors.
- Original and refactored focus/app algorithms produced identical serialized
  outputs across 1,600 deterministic comparisons, including privacy exclusions,
  gaps, zero-duration observations, and crossings of midnight.

## Local synthetic benchmarks

Release-mode Rust medians on this Windows machine; these measure individual
components, not the complete app or local model inference. Results vary by
hardware, history size, and foreground activity.

| Component and fixture | Before | After | Ratio |
| --- | ---: | ---: | ---: |
| Detours: 30,000 observations, 1,000 recurring destinations, no intervening work (stress case) | 3,568.53 ms | 95.24 ms | 37.47x |
| Detours: 30,000 mixed observations | 92.68 ms | 81.08 ms | 1.14x |
| Twelve longest examples: 100,000 activities | 54.08 ms | 0.37 ms | 145.38x |
| Pending query: 500,000 database rows, 500 pending | 12.87 ms | 0.29 ms | 43.65x |

The query benchmark uses local in-memory SQLite and verifies identical rows.
Query plans change from `SCAN reports` to
`SCAN reports USING INDEX reports_pending_by_id`.

Logs, baseline source backups, benchmark source, comparison JSON files, and the
scoped patch are saved under
`C:\Users\manue\codex_sessions\flowsight_resource_refactor_2026-10-03`.
Native validation used a development build with installer resources disabled;
an installer build and long-running CPU/RAM measurements are not included.
The bundled AI weights and runtime continue to account for most installed size
and inference memory, so the measurements above do not imply a comparable
whole-app reduction. Applying the source refactor does not update the currently
installed desktop app.

## Cross-platform release 5.0.18

The shared algorithms are applied to all three desktop repositories. macOS keeps
its Keychain encryption, Metal runtime, and Apple signing pipeline. Linux keeps
its portal capture, PNG decoding, model loading and package dependencies. Current
report temporal-coverage selection remains in place on every platform. The
optimization ports do not overwrite newer features or platform-specific code.

The benchmark table above was measured locally on Windows using synthetic
fixtures; it does not claim macOS/Linux timing or whole-app CPU/RAM reductions.
Release build and validation evidence is recorded in the 5.0.18 release session.
