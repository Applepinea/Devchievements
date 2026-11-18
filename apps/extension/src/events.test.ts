import assert from "node:assert/strict";
import test from "node:test";

import { dedupKey, eventId } from "./events.ts";

// A pull request used to produce an id ending in "#release:pr:1", which reads as a
// release and would collide with a release sharing that external id. The activity type
// is part of the identity, so it belongs in the key.
test("the dedup key names the activity type", () => {
  const pr = dedupKey({
    canonicalRepositoryId: "github:acme/widgets",
    commitSha: null,
    externalId: "pr:1",
    eventType: "pull_request_opened",
  });
  assert.equal(pr, "github:acme/widgets#pull_request_opened:pr:1");
  assert.ok(!pr.includes("release"), "a pull request must not look like a release");
});

test("a commit is identified by its sha, not by a type prefix", () => {
  const key = dedupKey({
    canonicalRepositoryId: "github:acme/widgets",
    commitSha: "a".repeat(40),
    externalId: null,
    eventType: "commit_created",
  });
  assert.equal(key, "github:acme/widgets#" + "a".repeat(40));
});

test("two different activities with the same external id do not collide", () => {
  const base = { canonicalRepositoryId: "github:acme/widgets", commitSha: null, externalId: "x:1" };
  const release = dedupKey({ ...base, eventType: "release_published" });
  const issue = dedupKey({ ...base, eventType: "issue_opened" });
  assert.notEqual(release, issue);
});

test("the same activity always produces the same id", () => {
  const event = {
    canonicalRepositoryId: "github:acme/widgets",
    commitSha: null,
    externalId: "pr:7",
    eventType: "pull_request_merged" as const,
  };
  assert.equal(eventId(event), eventId(event));
});
