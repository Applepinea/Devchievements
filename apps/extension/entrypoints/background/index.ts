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
    // The content script records one value per selector, and each selector maps to
    // exactly one attribute. Hardcoding "href" here dropped `data-state`, which is
    // what tells a merged pull request from an open one.
    attribute: (selector: string, _name: string) => message.attributes?.[selector] ?? null,
  });

  if (extraction.kind === "ignored") {
    // Logged, because "the extension does nothing" and "the page is not activity"
    // look identical from outside. This is the line that says which.
    console.info("[devchievements] ignored:", extraction.reason, message.url);
    return { stored: false, reason: extraction.reason };
  }

  // Record locally AND forward.
  //
  // The local copy is a cache of what this browser saw, so the dashboard is not
  // blind when the desktop app is doing the judging. It is not a second judgement:
  // no achievement is unlocked here.
  const stored = await store.add(extraction.event);

  // The desktop companion owns judgement and presentation.
  const bridged = await forward(extraction.event);
  console.info(
    "[devchievements] event",
    extraction.event.eventType,
    extraction.event.eventId,
    bridged.delivered ? "delivered" : "not delivered: " + bridged.reason,
  );

  return {
    stored,
    event: extraction.event,
    reason: bridged.delivered ? undefined : bridged.reason,
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
