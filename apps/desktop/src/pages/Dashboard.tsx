import { useEffect, useState } from "react";
import { getDashboard, type DashboardView } from "../ipc";
import { bytes, channel, dateTime } from "../format";

export default function Dashboard() {
  const [data, setData] = useState<DashboardView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getDashboard().then(setData, (e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)));
  }, []);

  if (error) return <p className="error">{error}</p>;
  if (!data) return <p className="muted">Loading...</p>;

  const s = data.statistics;
  return (
    <>
      <h2>Dashboard</h2>
      <dl className="stats">
        <div><dt>Commits</dt><dd>{s.commits}</dd></div>
        <div><dt>Repositories</dt><dd>{s.repositories}</dd></div>
        <div><dt>Achievements</dt><dd>{s.achievements}</dd></div>
        <div><dt>New achievements</dt><dd>{s.unviewedAchievements}</dd></div>
        <div><dt>Active days</dt><dd>{s.activeDays}</dd></div>
        <div><dt>Longest streak</dt><dd>{s.longestStreak} d</dd></div>
      </dl>

      <h3>Last two weeks</h3>
      {data.recent.length === 0 ? (
        <p className="muted">No activity yet. Make a commit with the hooks installed.</p>
      ) : (
        <ul className="list">
          {data.recent.map((day) => (
            <li key={day.localDate} className="row spread">
              <span>{day.localDate}</span>
              <span className="muted">
                {day.commits} commits{day.rank ? " · " + day.rank : ""}
              </span>
            </li>
          ))}
        </ul>
      )}

      <h3>Presentation</h3>
      <p className="muted">
        Currently using <strong>{channel(data.channel)}</strong>.
      </p>
      <p className="muted">
        System notifications: {data.systemNotifications ? "available" : "unavailable"}.
      </p>

      <h3>Storage</h3>
      <p className="muted">
        {bytes(data.storage.bytes)} · schema v{data.storage.schemaVersion}
      </p>
      <p className="muted">Oldest event: {dateTime(data.storage.oldestEventAt)}</p>
      <p className="tiny">{data.storage.path}</p>
      <p className="tiny">Companion {data.version}</p>
    </>
  );
}
