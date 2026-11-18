import assert from "node:assert/strict";
import test from "node:test";

import { extract } from "./extract.ts";

// A merged pull request page, with the two facts GitHub actually publishes: a state
// badge and a relative time.
function page(url: string, facts: { meta?: Record<string, string>; text?: Record<string, string>; attr?: Record<string, string> }) {
  return {
    url,
    meta: (n: string) => facts.meta?.[n] ?? null,
    text: (s: string) => facts.text?.[s] ?? null,
    attribute: (s: string, _a: string) => facts.attr?.[s] ?? null,
  };
}

const iso = "2026-11-18T10:00:00Z";

test("a merged pull request page becomes a pull_request_merged event", () => {
  const result = extract(
    page("https://github.com/acme/widgets/pull/12", {
      attr: { ".State": "merged", 'a[data-hovercard-type="user"]': "/someone" },
      text: { "relative-time[datetime]": iso, "time-ago[datetime]": iso },
    }),
  );
  assert.equal(result.kind, "event");
  if (result.kind !== "event") return;
  assert.equal(result.event.eventType, "pull_request_merged");
  assert.equal(result.event.externalId, "pr:12");
  assert.equal(result.event.authorLogin, "someone");
});

test("an open pull request page becomes pull_request_opened", () => {
  const result = extract(
    page("https://github.com/acme/widgets/pull/13", {
      attr: { ".State": "open", 'a[data-hovercard-type="user"]': "/someone" },
      text: { "relative-time[datetime]": iso, "time-ago[datetime]": iso },
    }),
  );
  assert.equal(result.kind, "event");
  if (result.kind !== "event") return;
  assert.equal(result.event.eventType, "pull_request_opened");
});

test("a commit page without a full sha is ignored, not guessed", () => {
  const result = extract(page("https://github.com/acme/widgets/commit/abc1234", {}));
  assert.equal(result.kind, "ignored");
});
