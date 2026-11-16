/**
 * MV3 service worker.
 *
 * Phase 0 scope: prove the WASM module loads under the extension's CSP and that
 * the service worker can survive suspension by storing its state in
 * storage.session rather than module scope (spec 27.4 / 17.7).
 *
 * NOT implemented here (Phase 3+): GitHub activity recognition, event delivery,
 * outbox, offer/claim/ack. Those depend on checkpoints A and B.
 */

export default defineBackground(() => {
  console.info("[devchievements] background service worker started");

  // Demonstrates the MV3 lifecycle constraint: nothing durable may live only in
  // module scope, because the worker can be killed at any moment.
  void chrome.storage.session.set({ backgroundStartedAt: Date.now() }).catch(() => {
    // storage.session is unavailable before Chrome 102; failing here is not fatal.
  });
});
