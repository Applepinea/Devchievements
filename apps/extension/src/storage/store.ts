/**
 * Persistent storage for browser-side events (spec 36.1: IndexedDB and a standalone
 * outbox).
 *
 * # Why an interface with two implementations
 *
 * IndexedDB only exists in a browser, so a unit test cannot exercise it. The rules
 * that matter -- dedup by event id, ordering, counts -- live behind this interface
 * and are tested against the memory store; the IndexedDB store is then verified end
 * to end in a real browser by scripts/e2e/extension-load.mjs.
 */

import type { BrowserEvent, EventType } from "../events.ts";

/** What the rest of the extension is allowed to do with stored events. */
export interface EventStore {
  /**
   * Record an event.
   *
   * Returns false when the event was already known. Dedup is by event id, which is
   * derived from the activity itself, so replaying the same page cannot double-count.
   */
  add(event: BrowserEvent): Promise<boolean>;
  /** Every stored event, oldest first. */
  all(): Promise<BrowserEvent[]>;
  /** How many events of each type are stored. */
  counts(): Promise<Record<EventType, number>>;
}

/** In-memory implementation, used by tests and as a fallback when IndexedDB fails. */
export class MemoryEventStore implements EventStore {
  private readonly events = new Map<string, BrowserEvent>();

  async add(event: BrowserEvent): Promise<boolean> {
    if (this.events.has(event.eventId)) return false;
    this.events.set(event.eventId, event);
    return true;
  }

  async all(): Promise<BrowserEvent[]> {
    return [...this.events.values()].sort((a, b) => a.occurredAt - b.occurredAt);
  }

  async counts(): Promise<Record<EventType, number>> {
    const counts: Record<EventType, number> = {
      commit_created: 0,
      release_published: 0,
    };
    for (const event of this.events.values()) counts[event.eventType] += 1;
    return counts;
  }
}
