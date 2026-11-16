import { useCallback, useEffect, useState } from "react";
import { getGallery, markViewed, type GalleryItem } from "../ipc";
import { dateTime } from "../format";

// The new marker must cover realtime, delayed, backfill and silent unlocks
// (spec 34.2), so it keys off viewed_at rather than off how the unlock arrived.
export default function Gallery() {
  const [items, setItems] = useState<GalleryItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(
    () =>
      getGallery().then(setItems, (e: unknown) =>
        setError(e instanceof Error ? e.message : String(e))),
    [],
  );

  useEffect(() => {
    void load();
  }, [load]);

  if (error) return <p className="error">{error}</p>;
  if (!items) return <p className="muted">Loading...</p>;

  const fresh = items.filter((i) => i.unlocked && !i.viewed);

  const reveal = async () => {
    await markViewed(fresh.map((i) => i.id));
    await load();
  };

  return (
    <>
      <div className="row spread">
        <h2>Achievements</h2>
        {fresh.length > 0 ? (
          <button onClick={() => void reveal()}>Mark {fresh.length} as seen</button>
        ) : null}
      </div>

      <ul className="grid">
        {items.map((item) => (
          <li key={item.id} className={item.unlocked ? "card unlocked" : "card"}>
            <div className="row spread">
              <strong>{item.unlocked || !item.hidden ? item.name : "Hidden"}</strong>
              {item.unlocked && !item.viewed ? <span className="badge">new</span> : null}
            </div>
            <p className="muted">
              {item.unlocked || !item.hidden ? item.description : "Unlock to reveal"}
            </p>
            <p className="tiny">
              {item.unlocked ? item.rarity + " · " + item.unlockSource + " · " + dateTime(item.unlockedAt) : item.rarity + " · locked"}
            </p>
          </li>
        ))}
      </ul>
    </>
  );
}
