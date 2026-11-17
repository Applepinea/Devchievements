# Performance optimization follow-up

This change set targets the measured and structurally evident costs identified in
the 2026-11-18 review. It keeps the existing event semantics, spool limits, history
import batching, and browser data model.

## Changes

- Count spool records with a reusable buffered reader instead of allocating each
  entire pending file as a `String`. The full queue still has to be scanned to
  enforce the current cross-process size and record limits; replacing that scan
  needs a crash-safe shared counter and is outside this change set.
- Increase the idle spool scan interval from about 50 ms to about 100 ms. The tray
  drain request still interrupts the wait in 10 ms slices. The existing 500 ms
  notification target remains the constraint; the effect of the new cadence needs
  a fresh end-to-end performance run.
- Remove the unused Git commit subject from history scan results and avoid building
  a second vector of references to all identity-matched commits. Import still uses
  one Git process and the existing bounded database batches.
- Use IndexedDB's time index for ordered event reads and add an event-type index so
  counts query the index instead of loading and sorting the full event history.

## Measurement status

The prior measurements remain the baseline, not evidence of the changes' results:
Hook p95 10.81 ms, ingestion p95 3.12 ms, idle CPU 2.40% of one core / 0.12% of the
whole machine under the 50 ms cadence, and notification p95 274–328 ms. No new
performance claim should be made until the Hook backlog, large history import,
extension event counts, and tray-only idle cases are measured again.
