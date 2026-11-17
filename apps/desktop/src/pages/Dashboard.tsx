import { useEffect, useState } from "react";
import { getDashboard, type DashboardView } from "../ipc";
import { bytes, channel, dateTime } from "../format";
import { t, tf } from "../i18n";

export default function Dashboard() {
  const [data, setData] = useState<DashboardView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getDashboard().then(setData, (e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)));
  }, []);

  if (error) return <p className="error">{error}</p>;
  if (!data) return <p className="muted">{t("Loading...")}</p>;

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
      <p className="muted">
        {bytes(data.storage.bytes)} · {tf("schema v{v}", { v: data.storage.schemaVersion })}
      </p>
      <p className="muted">{t("Oldest event:")} {dateTime(data.storage.oldestEventAt)}</p>
      <p className="tiny">{data.storage.path}</p>
      <p className="tiny">{tf("Companion {version}", { version: data.version })}</p>
    </>
  );
}
