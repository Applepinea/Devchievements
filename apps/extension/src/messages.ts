/**
 * Message shapes shared by the content script, the service worker and the UI.
 *
 * One definition, imported by every side, so a field rename cannot silently break
 * the channel.
 */

import type { BrowserEvent } from "./events.ts";

/** What the content script observed on a page. */
export interface PageFactsMessage {
  type: "devch:page";
  url: string;
  meta: Record<string, string>;
  text: Record<string, string>;
}

/** Response to a page report. */
export interface PageFactsResult {
  stored: boolean;
  reason?: string;
  event?: BrowserEvent;
}

/** Ask the service worker what it holds. */
export interface StatsMessage {
  type: "devch:stats";
}

/** What the service worker reports. */
export interface StatsResult {
  events: number;
  commits: number;
  releases: number;
}

export type ExtensionMessage = PageFactsMessage | StatsMessage;
