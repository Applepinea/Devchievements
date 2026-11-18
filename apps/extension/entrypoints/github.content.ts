/**
 * GitHub content script.
 *
 * Its only job is to describe the page: URL, a few meta tags, and the value of a couple
 * of attributes. All the reasoning happens in the service worker, so the rule that
 * decides what counts as activity is testable without a browser.
 *
 * # Why this runs more than once
 *
 * GitHub is a single-page app. Moving from /pulls to /pull/1 replaces the content
 * without reloading the document, so a script that only runs at `document_idle` reports
 * the landing page and then never speaks again. Measured on a real visit: a pull request
 * page was open in the browser and nothing reached the service worker at all.
 */

// Anything the extractor reads has to be collected here. `.State` was missing, so
// `data-state` was always null and a real pull request or issue could never be
// classified -- it fell through to "ignored" and the extension looked dead on
// exactly the pages it exists for. A contract test now drives the extractor with only
// what this file collects, so the two cannot drift apart silently.
const META_KEYS = [
  "octolytics-dimension-commit_date",
  "octolytics-dimension-release_date",
  "octolytics-dimension-user_login",
  "octolytics-dimension-pr_state",
  "octolytics-dimension-issue_state",
];

const TEXT_SELECTORS = ["relative-time[datetime]", "time-ago[datetime]"];

// Attribute values by selector, with the attribute each one needs.
const ATTRIBUTE_SELECTORS: Array<[string, string]> = [
  ['a[data-hovercard-type="user"]', "href"],
  [".State", "data-state"],
];

function report(): void {
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

  const attributes: Record<string, string> = {};
  for (const [selector, name] of ATTRIBUTE_SELECTORS) {
    const value = document.querySelector(selector)?.getAttribute(name);
    if (value) attributes[selector] = value;
  }

  chrome.runtime
    .sendMessage({ type: "devch:page", url: location.href, meta, text, attributes })
    .catch(() => {
      // The worker may be starting; a missed report must never break the page.
    });
}

export default defineContentScript({
  matches: ["https://github.com/*"],
  runAt: "document_idle",
  main(ctx) {
    let pending: number | undefined;

    // Debounced: one navigation produces many mutations, and each report costs a
    // message. The delay also lets GitHub finish painting, because the facts are read
    // from the DOM rather than from the URL alone.
    const schedule = () => {
      window.clearTimeout(pending);
      pending = window.setTimeout(report, 500);
    };

    report();
    ctx.addEventListener(window, "wxt:locationchange" as never, schedule);

    // The location event fires on the URL change; the observer covers the re-render
    // that follows it, and also pages changed without a history entry.
    const observer = new MutationObserver(schedule);
    observer.observe(document.body, { childList: true, subtree: true });

    ctx.onInvalidated(() => {
      observer.disconnect();
      window.clearTimeout(pending);
    });
  },
});
