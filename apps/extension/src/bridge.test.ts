/**
 * Tests for the native-messaging payload mapping.
 *
 * Run with: node --test (Node strips the types; no build step needed).
 *
 * The bridge used to map only two shapes, so a merged pull request arrived at the
 * desktop as a release. Nothing checked the mapping, and the failure looked like a
 * working connection: the host accepted the payload and stored the wrong event.
 */

import assert from "node:assert/strict";
import test from "node:test";

import { payloadFor } from "./bridge.ts";
import type { BrowserEvent } from "./events.ts";

function event(overrides: Partial<BrowserEvent>): BrowserEvent {
  return {
    eventId: "e1",
    eventType: "pull_request_merged",
    source: "content_script",
    canonicalRepositoryId: "github:acme/widgets",
    commitSha: null,
    externalId: "pr:7",
    occurredAt: 1_700_000_000_000,
    authorLogin: "someone",
    ...overrides,
  } as BrowserEvent;
}

test("a merged pull request is sent as a pull request, with a number", () => {
  const payload = payloadFor(event({}));
  assert.equal(payload.activity, "pull_request");
  assert.equal(payload.merged, true);
  assert.equal(payload.number, 7);
});

test("an open pull request is sent as unmerged", () => {
  const payload = payloadFor(event({ eventType: "pull_request_opened", externalId: "pr:9" }));
  assert.equal(payload.activity, "pull_request");
  assert.equal(payload.merged, false);
  assert.equal(payload.number, 9);
});

test("a release keeps its tag", () => {
  const payload = payloadFor(event({ eventType: "release_published", externalId: "v1.2.3" }));
  assert.equal(payload.activity, "release");
  assert.equal(payload.external_id, "v1.2.3");
});

test("an issue carries its number and closed state", () => {
  const opened = payloadFor(event({ eventType: "issue_opened", externalId: "issue:3" }));
  assert.equal(opened.activity, "issue");
  assert.equal(opened.closed, false);
  assert.equal(opened.number, 3);
  const closed = payloadFor(event({ eventType: "issue_closed", externalId: "issue:4" }));
  assert.equal(closed.closed, true);
});

test("a commit carries its sha and is the only commit-shaped payload", () => {
  const payload = payloadFor(
    event({ eventType: "commit_created", commitSha: "a".repeat(40), externalId: null }),
  );
  assert.equal(payload.activity, "commit");
  assert.equal(payload.commit_sha, "a".repeat(40));

  for (const type of [
    "release_published",
    "pull_request_opened",
    "pull_request_merged",
    "issue_opened",
    "issue_closed",
  ] as const) {
    assert.notEqual(payloadFor(event({ eventType: type })).activity, "commit");
  }
});
