/**
 * The browser-side event model.
 *
 * # Why these fields and not more
 *
 * The desktop engine works on events with a repository, a commit id, three
 * timestamps and a frozen local date (spec 5, 12). The browser must produce the
 * same shape or the two modes cannot share rules. Everything here is derivable
 * from data the page itself publishes; nothing is guessed.
 */

/** Where an event came from. */
export type EventSource = "browser-github";

/** What kind of activity was observed. */
export type EventType = "commit_created" | "release_published";

/** A normalised GitHub repository identifier. */
export interface RepositoryId {
  owner: string;
  name: string;
}

/** One observed piece of activity. */
export interface BrowserEvent {
  eventId: string;
  eventType: EventType;
  source: EventSource;
  canonicalRepositoryId: string;
  commitSha: string | null;
  externalId: string | null;
  occurredAt: number;
  /**
   * GitHub login of whoever produced the activity, when the page states it.
   *
   * Needed because the identity gate works by attribution (spec 7): without it a
   * commit made by somebody else, on a page the user happened to open, would be
   * counted as the user's.
   */
  authorLogin: string | null;
  localDate: string;
  payload: string;
}

/**
 * The canonical repository id for a GitHub repository.
 *
 * Lowercased because GitHub treats owner and name case-insensitively; without
 * normalising, "Foo/Bar" and "foo/bar" would be two repositories.
 */
export function canonicalRepositoryId(repo: RepositoryId): string {
  return "github:" + repo.owner.toLowerCase() + "/" + repo.name.toLowerCase();
}

/**
 * The dedup key.
 *
 * Spec 11.2 requires one dedup rule across capture paths. For GitHub activity the
 * identity of a commit is the repository plus the commit id; a release is
 * identified by its tag. Reusing the same rule here means a commit seen through the
 * browser and later through the hook is one event, not two.
 */
export function dedupKey(event: Pick<BrowserEvent, "canonicalRepositoryId" | "commitSha" | "externalId">): string {
  if (event.commitSha) return event.canonicalRepositoryId + "#" + event.commitSha;
  return event.canonicalRepositoryId + "#release:" + (event.externalId ?? "unknown");
}

/**
 * The event id, derived from the dedup key.
 *
 * Same activity means same id, so a replayed content script cannot double-count.
 */
export function eventId(event: Pick<BrowserEvent, "canonicalRepositoryId" | "commitSha" | "externalId">): string {
  return "browser:" + dedupKey(event);
}

/**
 * The local date of an instant, in UTC.
 *
 * The browser cannot read the user's IANA zone through this API, and inventing one
 * would put events on the wrong day. UTC is recorded explicitly rather than
 * silently; the desktop side owns the timezone and can correct it there.
 */
export function localDateUtc(at: number): string {
  return new Date(at).toISOString().slice(0, 10);
}
