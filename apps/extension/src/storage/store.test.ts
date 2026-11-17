import assert from "node:assert/strict";
import test from "node:test";

import { MemoryEventStore } from "./store.ts";
import type { BrowserEvent } from "../events.ts";

function event(overrides: Partial<BrowserEvent> = {}): BrowserEvent {
  return {
    eventId: "browser:github:o/r#abc1234",
    eventType: "commit_created",
    source: "browser-github",
    canonicalRepositoryId: "github:o/r",
    commitSha: "abc1234",
    externalId: null,
    occurredAt: 1,
    authorLogin: "someone",
    localDate: "2026-01-01",
    payload: "{}",
    ...overrides,
  };
}

test("an event is stored once", async () => {
  const store = new MemoryEventStore();
  assert.equal(await store.add(event()), true);
  assert.equal((await store.all()).length, 1);
});

test("the same event id is not stored twice", async () => {
  const store = new MemoryEventStore();
  await store.add(event());
  assert.equal(await store.add(event()), false);
  assert.equal((await store.all()).length, 1);
});

test("different activity of the same shape is stored separately", async () => {
  const store = new MemoryEventStore();
  await store.add(event({ eventId: "a", commitSha: "aaa1111" }));
  await store.add(event({ eventId: "b", commitSha: "bbb2222" }));
  assert.equal((await store.all()).length, 2);
});

test("events come back in time order, not insertion order", async () => {
  const store = new MemoryEventStore();
  await store.add(event({ eventId: "late", occurredAt: 200 }));
  await store.add(event({ eventId: "early", occurredAt: 100 }));
  const all = await store.all();
  assert.deepEqual(all.map((e) => e.eventId), ["early", "late"]);
});

test("counts separate commits from releases", async () => {
  const store = new MemoryEventStore();
  await store.add(event({ eventId: "c1" }));
  await store.add(event({ eventId: "c2", commitSha: "def5678" }));
  await store.add(
    event({ eventId: "r1", eventType: "release_published", commitSha: null, externalId: "v1" }),
  );
  const counts = await store.counts();
  assert.equal(counts.commit_created, 2);
  assert.equal(counts.release_published, 1);
});
