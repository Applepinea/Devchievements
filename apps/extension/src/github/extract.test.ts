/**
 * Tests for page extraction.
 *
 * Run with: node --test (Node strips the types; no build step needed).
 */

import assert from "node:assert/strict";
import test from "node:test";

import { extract, repositoryFromPath, type PageFacts } from "./extract.ts";

function facts(url: string, opts: { meta?: Record<string, string>; text?: Record<string, string> } = {}): PageFacts {
  return {
    url,
    meta: (name) => opts.meta?.[name] ?? null,
    text: (selector) => opts.text?.[selector] ?? null,
  };
}

test("a commit page becomes a commit event", () => {
  const result = extract(
    facts("https://github.com/Applepinea/Devchievements/commit/abcdef0123456789abcdef0123456789abcdef01", {
      meta: { "octolytics-dimension-commit_date": "2026-11-17T10:00:00Z" },
    }),
  );
  assert.equal(result.kind, "event");
  if (result.kind !== "event") return;
  assert.equal(result.event.eventType, "commit_created");
  assert.equal(result.event.canonicalRepositoryId, "github:applepinea/devchievements");
  assert.equal(result.event.commitSha, "abcdef0123456789abcdef0123456789abcdef01");
  assert.equal(result.event.localDate, "2026-11-17");
  assert.equal(result.event.occurredAt, Date.parse("2026-11-17T10:00:00Z"));
});

test("the same commit always yields the same event id", () => {
  const page = facts("https://github.com/o/r/commit/abcdef0123456789abcdef0123456789abcdef01", {
    meta: { "octolytics-dimension-commit_date": "2026-01-01T00:00:00Z" },
  });
  const first = extract(page);
  const second = extract(page);
  assert.equal(first.kind, "event");
  assert.equal(second.kind, "event");
  if (first.kind !== "event" || second.kind !== "event") return;
  assert.equal(first.event.eventId, second.event.eventId);
});

test("owner and repository case are normalised", () => {
  const result = extract(
    facts("https://github.com/ApplePinea/DevChievements/commit/abcdef0123456789abcdef0123456789abcdef01", {
      meta: { "octolytics-dimension-commit_date": "2026-01-01T00:00:00Z" },
    }),
  );
  assert.equal(result.kind, "event");
  if (result.kind !== "event") return;
  assert.equal(result.event.canonicalRepositoryId, "github:applepinea/devchievements");
});

test("a commit page without a machine-readable date is ignored, not invented", () => {
  const result = extract(facts("https://github.com/o/r/commit/abcdef0123456789abcdef0123456789abcdef01"));
  assert.equal(result.kind, "ignored");
  if (result.kind !== "ignored") return;
  assert.match(result.reason, /machine-readable date/);
});

test("a release tag page becomes a release event", () => {
  const result = extract(
    facts("https://github.com/o/r/releases/tag/v1.2.3", {
      text: { "relative-time[datetime]": "2026-03-04T05:06:07Z" },
    }),
  );
  assert.equal(result.kind, "event");
  if (result.kind !== "event") return;
  assert.equal(result.event.eventType, "release_published");
  assert.equal(result.event.externalId, "v1.2.3");
  assert.equal(result.event.commitSha, null);
});

test("two releases of the same repository are distinct events", () => {
  const one = extract(
    facts("https://github.com/o/r/releases/tag/v1", {
      text: { "relative-time[datetime]": "2026-01-01T00:00:00Z" },
    }),
  );
  const two = extract(
    facts("https://github.com/o/r/releases/tag/v2", {
      text: { "relative-time[datetime]": "2026-01-02T00:00:00Z" },
    }),
  );
  assert.equal(one.kind, "event");
  assert.equal(two.kind, "event");
  if (one.kind !== "event" || two.kind !== "event") return;
  assert.notEqual(one.event.eventId, two.event.eventId);
});

test("pages that are not activity are ignored", () => {
  const cases = [
    "https://github.com/o/r",
    "https://github.com/o/r/pulls",
    "https://github.com/notifications",
    "https://example.com/o/r/commit/abcdef0123456789abcdef0123456789abcdef01",
  ];
  for (const url of cases) {
    assert.equal(extract(facts(url)).kind, "ignored", url);
  }
});

test("reserved top-level paths are not repositories", () => {
  assert.equal(repositoryFromPath("/settings/profile"), null);
  assert.equal(repositoryFromPath("/notifications"), null);
  assert.deepEqual(repositoryFromPath("/owner/name"), { owner: "owner", name: "name" });
});

test("a repository name ending in .git is normalised", () => {
  assert.deepEqual(repositoryFromPath("/o/r.git"), { owner: "o", name: "r" });
});
