/**
 * GitHub content script.
 *
 * Phase 0 scope: prove the script is injected on the right pages and can talk to
 * the service worker. No DOM scraping, no page-world injection, no event capture
 * yet — those require the verifiable-activity design from spec 27.2, which is a
 * Phase 3 deliverable and must not be improvised now.
 */

export default defineContentScript({
  matches: ["https://github.com/*"],
  run_at: "document_idle",
  main() {
    console.info("[devchievements] content script active on", location.pathname);
  },
});
