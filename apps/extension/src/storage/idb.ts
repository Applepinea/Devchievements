/**
 * IndexedDB storage for browser-side events.
 *
 * # Why the store is opened lazily and memoised
 *
 * An MV3 service worker can be stopped between messages (spec 17.7 / 27.4). Opening
 * the database once per message is cheap and removes a whole class of "the
 * connection died with the worker" bugs; caching the promise makes it cheap enough.
 */

import type { BrowserEvent, EventType } from "../events.ts";
import type { EventStore } from "./store.ts";

const DB_NAME = "devchievements";
const DB_VERSION = 1;
const STORE = "events";

let opening: Promise<IDBDatabase> | null = null;

function open(): Promise<IDBDatabase> {
  if (opening) return opening;
  opening = new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE)) {
        const store = db.createObjectStore(STORE, { keyPath: "eventId" });
        store.createIndex("by_occurred_at", "occurredAt");
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  return opening;
}

export class IndexedDbEventStore implements EventStore {
  async add(event: BrowserEvent): Promise<boolean> {
    const db = await open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, "readwrite");
      const store = tx.objectStore(STORE);
      const req = store.add(event);
      // A duplicate key is the normal "already recorded" case, not a failure.
      req.onerror = (err) => {
        err.preventDefault();
        err.stopPropagation();
        resolve(false);
      };
      req.onsuccess = () => resolve(true);
      tx.onabort = () => reject(tx.error);
    });
  }

  async all(): Promise<BrowserEvent[]> {
    const db = await open();
    return new Promise((resolve, reject) => {
      const tx = db.transaction(STORE, "readonly");
      const req = tx.objectStore(STORE).getAll();
      req.onsuccess = () =>
        resolve(
          (req.result as BrowserEvent[]).sort((a, b) => a.occurredAt - b.occurredAt),
        );
      req.onerror = () => reject(req.error);
    });
  }

  async counts(): Promise<Record<EventType, number>> {
    const counts: Record<EventType, number> = {
      commit_created: 0,
      release_published: 0,
      pull_request_opened: 0,
      pull_request_merged: 0,
      issue_opened: 0,
      issue_closed: 0,
    };
    for (const event of await this.all()) counts[event.eventType] += 1;
    return counts;
  }
}
