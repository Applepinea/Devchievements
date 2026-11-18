import { useEffect, useState } from "react";
import {
  getDashboard,
  getStorageHealth,
  pageMs,
  uiTiming,
  type DashboardView,
  type StorageHealth,
} from "../ipc";
import { bytes, channel, dateTime } from "../format";
import { t, tf } from "../i18n";

export default function Dashboard() {
  const [data, setData] = useState<DashboardView | null>(null);
  const [storage, setStorage] = useState<StorageHealth | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getDashboard().then(
      (view) => {
        setData(view);
        void uiTiming("dashboard_data", pageMs());
      },
      (e: unknown) => setError(e instanceof Error ? e.message : String(e)),
    );
  }, []);

  // The storage block is diagnostics: the path, the size, the schema version. None of
  // it belongs on the first screen, so it is requested separately and rendered when it
  // arrives instead of holding the numbers back.
  useEffect(() => {
    getStorageHealth().then(setStorage, () => setStorage(null));
  }, []);

  if (error) return <p className="error">{error}</p>;
  if (!data) {
    return (
      <div className="shell-skeleton" aria-busy="true">
        <div className="skeleton-line short" />
        <div className="skeleton-line" />
        <div className="skeleton-line" />
        <div className="skeleton-line" />
      </div>
    );
  }

  const s = data.statistics;
  return (
    <>
      <h2>{t("Dashboard")}</h2>
      <dl className="stats">
        <div><dt>{t("Commits")}</dt><dd>{s.commits}</dd></div>
        <div><dt>{t("Repositories")}</dt><dd>{s.repositories}</dd></div>
        <div><dt>{t("Achievements")}</dt><dd>{s.achievements}</dd></div>
        <div><dt>{t("New achievements")}</dt><dd>{s.unviewedAchievements}</dd></div>
        <div><dt>{t("Active days")}</dt><dd>{s.activeDays}</dd></div>
        <div><dt>{t("Longest streak")}</dt><dd>{tf("{n} d", { n: s.longestStreak })}</dd></div>
      </dl>

      <h3>{t("Last two weeks")}</h3>
      {data.recent.length === 0 ? (
        <p className="muted">
          {t("No activity yet. Make a commit with the hooks installed.")}
        </p>
      ) : (
        <ul className="list">
          {data.recent.map((day) => (
            <li key={day.localDate} className="row spread">
              <span>{day.localDate}</span>
              <span className="muted">
                {tf("{n} commits", { n: day.commits })}{day.rank ? " · " + day.rank : ""}
              </span>
            </li>
          ))}
        </ul>
      )}

      <h3>{t("Presentation")}</h3>
      <p className="muted">
        {t("Currently using")} <strong>{channel(data.channel)}</strong>.
      </p>
      <p className="muted">
        {t("System notifications:")}{" "}
        {data.systemNotifications ? t("available") : t("unavailable")}.
      </p>

      <h3>{t("Storage")}</h3>
      {storage ? (
        <>
          <p className="muted">
            {bytes(storage.bytes)} · {tf("schema v{v}", { v: storage.schemaVersion })}
          </p>
          <p className="muted">{t("Oldest event:")} {dateTime(storage.oldestEventAt)}</p>
          <p className="tiny">{storage.path}</p>
        </>
      ) : (
        <div className="shell-skeleton" aria-busy="true">
          <div className="skeleton-line" />
        </div>
      )}
      <p className="tiny">{tf("Companion {version}", { version: data.version })}</p>
    </>
  );
}
