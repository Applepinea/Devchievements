import assert from "node:assert/strict";
import test from "node:test";

import { extract } from "./extract.ts";

// The content script collects a fixed set of facts. This test feeds the extractor
// exactly that set -- nothing more -- so a selector the extractor needs but the script
// never collects shows up here instead of as "the extension does nothing on GitHub".
//
// That is how `.State` was found missing: real pull request pages were ignored because
// the state attribute was never collected and never forwarded.
const COLLECTED_TEXT = ["relative-time[datetime]", "time-ago[datetime]"];
const COLLECTED_ATTR: Record<string, string> = {
  'a[data-hovercard-type="user"]': "href",
  ".State": "data-state",
};

function page(url: string, collected: {
  meta?: Record<string, string>;
  text?: Record<string, string>;
  attributes?: Record<string, string>;
}) {
  return {
    url,
    meta: (n: string) => collected.meta?.[n] ?? null,
    text: (s: string) => (COLLECTED_TEXT.includes(s) ? (collected.text?.[s] ?? null) : null),
    attribute: (s: string, name: string) =>
      COLLECTED_ATTR[s] === name ? (collected.attributes?.[s] ?? null) : null,
  };
}

const iso = "2026-11-18T10:00:00Z";

test("a merged PR page is recognised from what the content script collects", () => {
  const result = extract(
    page("https://github.com/acme/widgets/pull/12", {
      text: { "relative-time[datetime]": iso },
      attributes: { 'a[data-hovercard-type="user"]': "/someone", ".State": "merged" },
    }),
  );
  assert.equal(result.kind, "event", "a real PR page must not be ignored");
  if (result.kind !== "event") return;
  assert.equal(result.event.eventType, "pull_request_merged");
  assert.equal(result.event.externalId, "pr:12");
});

test("an open PR page is pull_request_opened", () => {
  const result = extract(
    page("https://github.com/acme/widgets/pull/13", {
      text: { "relative-time[datetime]": iso },
      attributes: { ".State": "open" },
    }),
  );
  assert.equal(result.kind, "event");
  if (result.kind !== "event") return;
  assert.equal(result.event.eventType, "pull_request_opened");
});

test("a closed issue page is issue_closed", () => {
  const result = extract(
    page("https://github.com/acme/widgets/issues/21", {
      text: { "relative-time[datetime]": iso },
      attributes: { 'a[data-hovercard-type="user"]': "/someone", ".State": "closed" },
    }),
  );
  assert.equal(result.kind, "event");
  if (result.kind !== "event") return;
  assert.equal(result.event.eventType, "issue_closed");
});

test("a release page is recognised from the collected relative time", () => {
  const result = extract(
    page("https://github.com/acme/widgets/releases/tag/v1.0.0", {
      text: { "relative-time[datetime]": iso },
      attributes: { 'a[data-hovercard-type="user"]': "/someone" },
    }),
  );
  assert.equal(result.kind, "event");
  if (result.kind !== "event") return;
  assert.equal(result.event.eventType, "release_published");
  assert.equal(result.event.externalId, "v1.0.0");
});
