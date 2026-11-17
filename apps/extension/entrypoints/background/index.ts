/**
 * MV3 service worker.
 *
 * Responsibilities so far (Phase 3a):
 *  - turn reported page facts into events using the shared extractor;
 *  - persist them in IndexedDB with dedup by event id;
 *  - answer queries from the popup.
 *
 * Nothing here may live in module scope alone: the worker can be stopped between
 * messages (spec 17.7), which is why the IndexedDB handle is opened lazily.
 */

import { forward } from "../../src/bridge.ts";
import { extract } from "../../src/github/extract.ts";
import { IndexedDbEventStore } from "../../src/storage/idb.ts";
import type {
  ExtensionMessage,
  EventsResult,
  PageFactsResult,
  StatsResult,
} from "../../src/messages.ts";

const store = new IndexedDbEventStore();

async function handlePage(message: Extract<ExtensionMessage, { type: "devch:page" }>): Promise<PageFactsResult> {
  const extraction = extract({
    url: message.url,
    meta: (name: string) => message.meta[name] ?? null,
    text: (selector: string) => message.text[selector] ?? null,
  });

  if (extraction.kind === "ignored") {
    return { stored: false, reason: extraction.reason };
  }

  // The desktop companion judges and presents; this only reports what it saw.
  const bridged = await forward(extraction.event);
  if (bridged.delivered) {
    return { stored: true, event: extraction.event };
  }

  // Desktop side unreachable: keep a local copy so the dashboard is not blind, and
  // say why it was not forwarded. Losing the detection would be worse, and the
  // desktop side deduplicates by event id if it arrives later.
  const stored = await store.add(extraction.event);
  return {
    stored,
    event: extraction.event,
    reason: bridged.reason,
  };
}

async function handleEvents(): Promise<EventsResult> {
  return { events: await store.all() };
}

async function handleStats(): Promise<StatsResult> {
  const counts = await store.counts();
  return {
    events: counts.commit_created + counts.release_published,
    commits: counts.commit_created,
    releases: counts.release_published,
  };
}

export default defineBackground(() => {
  chrome.runtime.onMessage.addListener((message: ExtensionMessage, _sender, sendResponse) => {
    const work =
      message?.type === "devch:page"
        ? handlePage(message)
        : message?.type === "devch:stats"
          ? handleStats()
          : message?.type === "devch:events"
            ? handleEvents()
            : Promise.resolve({ stored: false, reason: "unknown message" });

    // Returning true keeps the response channel open for the async work.
    work.then(sendResponse, (error) => sendResponse({ stored: false, reason: String(error) }));
    return true;
  });
  console.info("[devchievements] service worker ready");
});
