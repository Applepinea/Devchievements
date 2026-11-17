import { useEffect, useState } from "react";
import { getActivity, type ActivityDay } from "../ipc";
import { t, tf } from "../i18n";

export default function Activity() {
  const [days, setDays] = useState<ActivityDay[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getActivity(60).then(setDays, (e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)));
  }, []);

  if (error) return <p className="error">{error}</p>;
  if (!days) return <p className="muted">{t("Loading...")}</p>;

  const peak = days.reduce((max, d) => Math.max(max, d.commits), 0);

  return (
    <>
      <h2>{t("Activity")}</h2>
      <p className="muted">
        {t("Ordered newest first. Bars are scaled to the busiest day.")}
      </p>
      {days.length === 0 ? (
        <p className="muted">{t("Nothing recorded yet.")}</p>
      ) : (
        <ul className="list">
          {days.map((day) => (
            <li key={day.localDate} className="row spread">
              <span className="mono">{day.localDate}</span>
              <span className="bar-track">
                <span
                  className="bar"
                  style={{ width: peak > 0 ? (day.commits / peak) * 100 + "%" : "0%" }}
                />
              </span>
              <span className="muted mono">
                {day.commits}{day.rank ? " · " + day.rank : ""}
              </span>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}
