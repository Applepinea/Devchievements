/**
 * The popup: the browser-only dashboard (spec 36.1).
 *
 * Everything shown here comes from the service worker, which reads IndexedDB. There
 * are no placeholder numbers: an empty state means the store is genuinely empty.
 */

import { useCallback, useEffect, useState } from "react";

import type { BrowserEvent } from "../../src/events.ts";
import type { EventsResult, StatsResult } from "../../src/messages.ts";

interface Stats {
  events: number;
  commits: number;
  releases: number;
}

function send<T>(message: unknown): Promise<T> {
  return new Promise((resolve, reject) => {
    chrome.runtime.sendMessage(message, (response: T) => {
      const error = chrome.runtime.lastError;
      if (error) reject(new Error(error.message));
      else resolve(response);
    });
  });
}

export default function App() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [events, setEvents] = useState<BrowserEvent[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const s = await send<StatsResult>({ type: "devch:stats" });
      const e = await send<EventsResult>({ type: "devch:events" });
      setStats({ events: s.events, commits: s.commits, releases: s.releases });
      setEvents(e.events);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (error) {
    return (
      <main className="popup-shell">
        <h1>Devchievements</h1>
        <p className="error">Could not read the store: {error}</p>
      </main>
    );
  }

  if (!stats || !events) {
    return (
      <main className="popup-shell">
        <h1>Devchievements</h1>
        <p className="muted">Loading...</p>
      </main>
    );
  }

  return (
    <main className="popup-shell">
      <h1>Devchievements</h1>

      <dl className="stats">
        <div>
          <dt>Commits</dt>
          <dd data-testid="commits">{stats.commits}</dd>
        </div>
        <div>
          <dt>Releases</dt>
          <dd data-testid="releases">{stats.releases}</dd>
        </div>
        <div>
          <dt>Total</dt>
          <dd data-testid="total">{stats.events}</dd>
        </div>
      </dl>

      <h2>Recent activity</h2>
      {events.length === 0 ? (
        <p className="muted" data-testid="empty">
          Nothing recorded yet. Open a commit page on GitHub with the extension enabled.
        </p>
      ) : (
        <ul className="events" data-testid="events">
          {events.slice(-10).reverse().map((event) => (
            <li key={event.eventId}>
              <span className="kind">
                {event.eventType === "commit_created" ? "commit" : "release"}
              </span>
              <span className="repo">{event.canonicalRepositoryId.replace("github:", "")}</span>
              <span className="id">{event.commitSha ?? event.externalId}</span>
              <span className="date">{event.localDate}</span>
            </li>
          ))}
        </ul>
      )}
    </main>
  );
}
