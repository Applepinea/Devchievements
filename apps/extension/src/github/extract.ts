/**
 * Turning a GitHub page into a verifiable event (spec 27.2).
 *
 * # Why an adapter instead of touching the DOM here
 *
 * The rule that decides whether a page is verifiable activity must be testable
 * without a browser, and the DOM is the one thing a unit test cannot provide. So the
 * page is described by a tiny interface, the content script implements it over
 * document, and the tests implement it over fixed data.
 *
 * # What counts as verification
 *
 * Only facts the page itself publishes: the URL identifies the repository, and the
 * commit id or release tag comes from the URL or from the page metadata. Nothing is
 * inferred from text that merely looks like a commit.
 */

import {
  canonicalRepositoryId,
  eventId,
  localDateUtc,
  type BrowserEvent,
  type RepositoryId,
} from "../events.ts";

/** What the extractor is allowed to look at. */
export interface PageFacts {
  /** Full page URL. */
  url: string;
  /** Content of a meta tag by name, or null. */
  meta(name: string): string | null;
  /** Text of the first element matching a selector, or null. */
  text(selector: string): string | null;
  /** Value of the FIRST element matching a selector, by attribute. */
  attribute(selector: string, name: string): string | null;
}

/** Result of looking at a page. */
export type Extraction =
  | { kind: "event"; event: BrowserEvent }
  | { kind: "ignored"; reason: string };

/** Parse owner and name out of a GitHub URL path. */
export function repositoryFromPath(pathname: string): RepositoryId | null {
  const parts = pathname.split("/").filter(Boolean);
  if (parts.length < 2) return null;
  const owner = parts[0];
  const name = parts[1];
  if (!owner || !name) return null;
  // Reserved top-level paths are not repositories.
  const reserved = new Set([
    "settings",
    "notifications",
    "explore",
    "marketplace",
    "pulls",
    "issues",
    "search",
    "topics",
    "sponsors",
    "login",
    "join",
    "about",
  ]);
  if (reserved.has(owner.toLowerCase())) return null;
  return { owner, name: name.replace(/\.git$/, "") };
}

/** A commit page: /owner/repo/commit/<sha>. */
function commitEvent(facts: PageFacts, repo: RepositoryId, pathname: string): Extraction {
  const match = pathname.match(/^\/[^/]+\/[^/]+\/commit\/([0-9a-f]{40})\/?$/i);
  if (!match || !match[1]) {
    // A short id cannot deduplicate against the hook path, which records the full id,
    // so accepting one would count the same commit twice.
    return { kind: "ignored", reason: "not a commit page with a full commit id" };
  }
  const sha = match[1].toLowerCase();

  // The page states when the commit was made; without it the day would be invented.
  const stamp =
    facts.meta("octolytics-dimension-commit_date") ??
    facts.text("relative-time[datetime]") ??
    facts.text("time-ago[datetime]");
  const occurredAt = stamp ? Date.parse(stamp) : Number.NaN;
  if (Number.isNaN(occurredAt)) {
    return { kind: "ignored", reason: "commit page without a machine-readable date" };
  }

  // Who made it. GitHub renders the author in the commit header; when the page
  // does not state it, the login is left null and the desktop side refuses to
  // attribute the commit rather than assuming it is the user's.
  const authorLogin =
    facts.attribute('a[data-hovercard-type="user"]', "href")?.replace(/^\//, "") ??
    facts.meta("octolytics-dimension-user_login");

  const base = {
    canonicalRepositoryId: canonicalRepositoryId(repo),
    commitSha: sha,
    externalId: null,
  };

  return {
    kind: "event",
    event: {
      eventId: eventId(base),
      eventType: "commit_created",
      source: "browser-github",
      canonicalRepositoryId: base.canonicalRepositoryId,
      commitSha: sha,
      externalId: null,
      occurredAt,
      authorLogin,
      localDate: localDateUtc(occurredAt),
      payload: JSON.stringify({ via: "content-script", page: "commit" }),
    },
  };
}

/** A release page: /owner/repo/releases/tag/<tag>. */
function releaseEvent(facts: PageFacts, repo: RepositoryId, pathname: string): Extraction {
  const match = pathname.match(/^\/[^/]+\/[^/]+\/releases\/tag\/(.+?)\/?$/);
  if (!match || !match[1]) return { kind: "ignored", reason: "not a release page" };
  const tag = decodeURIComponent(match[1]);

  const stamp = facts.text("relative-time[datetime]") ?? facts.meta("octolytics-dimension-release_date");
  const occurredAt = stamp ? Date.parse(stamp) : Number.NaN;
  if (Number.isNaN(occurredAt)) {
    return { kind: "ignored", reason: "release page without a machine-readable date" };
  }

  // Same rule as commits: attribute only when the page says who did it.
  const authorLogin =
    facts.attribute('a[data-hovercard-type="user"]', "href")?.replace(/^\//, "") ??
    facts.meta("octolytics-dimension-user_login");

  const base = {
    canonicalRepositoryId: canonicalRepositoryId(repo),
    commitSha: null,
    externalId: tag,
  };

  return {
    kind: "event",
    event: {
      eventId: eventId(base),
      eventType: "release_published",
      source: "browser-github",
      canonicalRepositoryId: base.canonicalRepositoryId,
      commitSha: null,
      externalId: tag,
      occurredAt,
      authorLogin,
      localDate: localDateUtc(occurredAt),
      payload: JSON.stringify({ via: "content-script", page: "release" }),
    },
  };
}

/** Decide whether this page is activity worth recording. */
export function extract(facts: PageFacts): Extraction {
  let url: URL;
  try {
    url = new URL(facts.url);
  } catch {
    return { kind: "ignored", reason: "unparseable url" };
  }
  if (url.hostname !== "github.com") return { kind: "ignored", reason: "not github.com" };

  const repo = repositoryFromPath(url.pathname);
  if (!repo) return { kind: "ignored", reason: "not a repository path" };

  if (/\/commit\//.test(url.pathname)) return commitEvent(facts, repo, url.pathname);
  if (/\/releases\/tag\//.test(url.pathname)) return releaseEvent(facts, repo, url.pathname);
  return { kind: "ignored", reason: "page type not implemented" };
}
