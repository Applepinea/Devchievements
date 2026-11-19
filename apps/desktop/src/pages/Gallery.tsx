import { useCallback, useEffect, useState } from "react";
import {
  getDailyRank,
  getGallery,
  getRepeatableAchievements,
  markViewed,
  type DailyRankView,
  type GalleryItem,
  type RepeatableAchievementItem,
} from "../ipc";
import { dateTime } from "../format";
import { t, tf } from "../i18n";

// The new marker must cover realtime, delayed, backfill and silent unlocks
// (spec 34.2), so it keys off viewed_at rather than off how the unlock arrived.
//
// Repeatable achievements are a separate list on purpose. Their triggers are not
// unlocks: the first one must not turn the card into a "done" goal, and the
// gallery's unviewed counter must keep meaning "permanent unlocks you haven't seen".
export default function Gallery() {
  const [items, setItems] = useState<GalleryItem[] | null>(null);
  const [repeatables, setRepeatables] = useState<RepeatableAchievementItem[] | null>(null);
  const [daily, setDaily] = useState<DailyRankView | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(
    () =>
      Promise.all([getGallery(), getRepeatableAchievements(), getDailyRank()]).then(
        ([permanent, repeatable, rank]) => {
          setItems(permanent);
          setRepeatables(repeatable);
          setDaily(rank);
        },
        (e: unknown) => setError(e instanceof Error ? e.message : String(e)),
      ),
    [],
  );

  useEffect(() => {
    void load();
  }, [load]);

  if (error) return <p className="error">{error}</p>;
  if (!items || !repeatables || !daily) {
    return <p className="muted">{t("Loading...")}</p>;
  }

  const fresh = items.filter((i) => i.unlocked && !i.viewed);

  const reveal = async () => {
    await markViewed(fresh.map((i) => i.id));
    await load();
  };

  return (
    <>
      <div className="row spread">
        <h2>{t("Achievements")}</h2>
        {fresh.length > 0 ? (
          <button onClick={() => void reveal()}>
            {tf("Mark {n} as seen", { n: fresh.length })}
          </button>
        ) : null}
      </div>

      <h3>{t("Permanent achievements")}</h3>
      <ul className="grid">
        {items.map((item) => (
          <li key={item.id} className={item.unlocked ? "card unlocked" : "card"}>
            <div className="row spread">
              <strong>
                {item.unlocked || !item.hidden ? item.name : t("Hidden")}
              </strong>
              {item.unlocked && !item.viewed ? (
                <span className="badge">{t("new")}</span>
              ) : null}
            </div>
            <p className="muted">
              {item.unlocked || !item.hidden ? item.description : t("Unlock to reveal")}
            </p>
            <p className="tiny">
              {item.unlocked
                ? item.rarity + " · " + item.unlockSource + " · " + dateTime(item.unlockedAt)
                : item.rarity + " · " + t("locked")}
            </p>
          </li>
        ))}
      </ul>

      <h3>{t("Repeatable achievements")}</h3>
      <ul className="grid">
        {repeatables.map((item) => (
          <li key={"repeatable:" + item.id} className="card unlocked">
            <div className="row spread">
              <strong>{item.name}</strong>
              <span className="badge">{item.rarity}</span>
            </div>
            <p className="muted">{item.description}</p>
            <p className="tiny">
              {item.triggerCount > 0
                ? tf("Completed {n} times", { n: item.triggerCount })
                : t("Not completed yet")}
            </p>
            {item.events.length > 0 ? (
              <details>
                <summary className="tiny">{t("Trigger history")}</summary>
                <ul className="list">
                  {item.events.map((event) => (
                    <li key={event.id} className="tiny">
                      {dateTime(event.unlockedAt)} · {event.repository} ·{" "}
                      <span className="mono">{event.triggeringEventId}</span>
                    </li>
                  ))}
                </ul>
              </details>
            ) : null}
          </li>
        ))}
      </ul>

      <h3>{t("Daily rank")}</h3>
      <div className="card unlocked">
        <div className="row spread">
          <strong>{daily.rank ? daily.rank : t("No rank yet")}</strong>
          <span className="tiny">{daily.localDate}</span>
        </div>
        <p className="muted">
          {tf("{n} commits today", { n: daily.commits })}
        </p>
        {daily.nextRank && daily.nextThreshold !== null ? (
          <p className="tiny">
            {tf("Next: {rank} at {n}", {
              rank: daily.nextRank,
              n: daily.nextThreshold,
            })}
          </p>
        ) : null}
        <p className="tiny">
          {t("Daily rank is tracked separately from achievements.")}
        </p>
      </div>
    </>
  );
}
