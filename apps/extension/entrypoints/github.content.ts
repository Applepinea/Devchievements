/**
 * GitHub content script.
 *
 * Its only job is to describe the page: URL, a few meta tags, and the text of a
 * couple of elements. All the reasoning happens in the service worker, so the rule
 * that decides what counts as activity is testable without a browser.
 */

const META_KEYS = [
  "octolytics-dimension-commit_date",
  "octolytics-dimension-release_date",
];

const TEXT_SELECTORS = ["relative-time[datetime]", "time-ago[datetime]"];

export default defineContentScript({
  matches: ["https://github.com/*"],
  runAt: "document_idle",
  main() {
    const meta: Record<string, string> = {};
    for (const key of META_KEYS) {
      const value = document.querySelector(`meta[name="${key}"]`)?.getAttribute("content");
      if (value) meta[key] = value;
    }

    const text: Record<string, string> = {};
    for (const selector of TEXT_SELECTORS) {
      const value = document.querySelector(selector)?.getAttribute("datetime");
      if (value) text[selector] = value;
    }

    chrome.runtime.sendMessage({ type: "devch:page", url: location.href, meta, text }).catch(() => {
      // The worker may be starting; a missed report must never break the page.
    });
  },
});
