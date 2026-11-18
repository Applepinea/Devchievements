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
---

## 托盘扫描周期：实测后保留 50 ms

本轮把 drain 的等待从 5×10 ms 改成 10×10 ms（100 ms）。同一棵代码树、同一套夹具、
120 次真实提交，两种周期各测一遍：

| 周期 | commit→通知 p95 | 空闲 CPU（单核） | 空闲 CPU（整机） |
| --- | --- | --- | --- |
| 100 ms | **437 ms** | 1.823% | 0.091% |
| 50 ms | **277 ms** | 3.906% | 0.195% |

两者都达标（预算 500 ms、空闲 < 1%），所以这是取舍而不是缺陷：

- 100 ms 把弹窗延迟推到预算的 **87%**，而且这是 20 核机器上的数字；
- 50 ms 多花的是整机 **0.195%**，约每秒 2 ms，对续航没有可感影响。

结论：**保留 50 ms**。用户能感觉到 160 ms 的差别，感觉不到 0.1% 的 CPU。
要把空闲压到 0.01% 的正确做法是 spool 目录的文件系统通知，而不是继续拉长轮询——
后者只是拿延迟换数字。
