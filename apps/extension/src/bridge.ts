/**
 * The bridge to the desktop companion (spec 18).
 *
 * # The rule this file encodes
 *
 * The extension only DETECTS. Judgement and presentation belong to the desktop app,
 * which owns the rules and the single database (spec 18.2). So a page report is
 * forwarded to the Native Messaging host, and only if that is unavailable does the
 * extension keep a local copy -- losing a detection is worse than storing one twice,
 * because the desktop side deduplicates anyway.
 *
 * # Why the fallback is not "Browser Only mode"
 *
 * Storing locally keeps the dashboard honest about what was seen. It does not unlock
 * achievements, because doing that in the browser would mean a second rule set.
 */

import type { BrowserEvent } from "./events.ts";

/** The host name registered with the browser. */
export const NATIVE_HOST = "com.devchievements.host";

/** What the host replies. */
interface HostAck {
  stored?: boolean;
  reason?: string;
}

/** Result of trying to forward one event. */
export type BridgeOutcome =
  | { delivered: true }
  | { delivered: false; reason: string };

/**
 * Translate one observed event into the host's wire shape.
 *
 * This used to read `eventType === "commit_created" ? "commit" : "release"`,
 * which sent every pull request, issue and release as a `release` with whatever
 * `externalId` happened to be. Measured on 2026-11-18: a merged pull request
 * arrived as `ReleasePublished` with tag `pr:5` -- the wrong event, and it would
 * have unlocked a release achievement. The six types are mapped explicitly now, and
 * the fields the host validates (`number`, `merged`, `closed`) are always present so
 * a payload is never judged on a field the bridge forgot to send.
 */
export function payloadFor(event: BrowserEvent): Record<string, unknown> {
  const base = {
    kind: "browser_github",
    event_id: event.eventId,
    repository_id: event.canonicalRepositoryId,
    occurred_ms: event.occurredAt,
    // Who made it. The desktop side refuses to attribute activity without this
    // rather than counting somebody else's work (spec 7).
    author_login: event.authorLogin,
  };

  // `externalId` carries "pr:<n>" / "issue:<n>" / the tag / the sha, depending on
  // the type; the host wants the bare number for pull requests and issues.
  const numeric = () => {
    const match = /(\d+)\s*$/.exec(event.externalId ?? "");
    return match ? Number(match[1]) : undefined;
  };

  switch (event.eventType) {
    case "commit_created":
      return { ...base, activity: "commit", commit_sha: event.commitSha };
    case "release_published":
      return { ...base, activity: "release", external_id: event.externalId };
    case "pull_request_opened":
      return { ...base, activity: "pull_request", number: numeric(), merged: false };
    case "pull_request_merged":
      return { ...base, activity: "pull_request", number: numeric(), merged: true };
    case "issue_opened":
      return { ...base, activity: "issue", number: numeric(), closed: false };
    case "issue_closed":
      return { ...base, activity: "issue", number: numeric(), closed: true };
  }
}

/**
 * Forward one event to the desktop companion.
 *
 * Never throws: a missing host is an expected state (the app may not be installed),
 * and the caller must be able to fall back without a try/catch around every call.
 */
export function forward(event: BrowserEvent): Promise<BridgeOutcome> {
  return new Promise((resolve) => {
    let port: chrome.runtime.Port;
    try {
      port = chrome.runtime.connectNative(NATIVE_HOST);
    } catch (error) {
      resolve({ delivered: false, reason: String(error) });
      return;
    }

    let settled = false;
    const finish = (outcome: BridgeOutcome) => {
      if (settled) return;
      settled = true;
      try {
        port.disconnect();
      } catch {
        // Already gone; nothing to do.
      }
      resolve(outcome);
    };

    // connectNative does not report failure synchronously: the port dies instead.
    port.onDisconnect.addListener(() => {
      const message = chrome.runtime.lastError?.message ?? "host closed the port";
      finish({ delivered: false, reason: message });
    });

    port.onMessage.addListener((reply: HostAck) => {
      if (reply?.stored) {
        finish({ delivered: true });
      } else {
        finish({ delivered: false, reason: reply?.reason ?? "host refused the report" });
      }
    });

    port.postMessage({
      type: "devch:github_event",
      payload: payloadFor(event),
    });
  });
}
