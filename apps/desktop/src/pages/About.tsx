import { useEffect, useState } from "react";
import { backupDatabase, getAbout, type AboutView } from "../ipc";
import { bytes, channel, dateTime } from "../format";
import { t, tf } from "../i18n";

export default function About() {
  const [data, setData] = useState<AboutView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string>("");

  useEffect(() => {
    getAbout().then(setData, (e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)));
  }, []);

  const backup = async () => {
    try {
      const report = await backupDatabase();
      setNotice(tf("Backup written: {size}", { size: bytes(report.bytes) }));
    } catch (e) {
      setNotice(
        tf("Backup failed: {error}", { error: e instanceof Error ? e.message : String(e) }),
      );
    }
  };

  if (error) return <p className="error">{error}</p>;
  if (!data) return <p className="muted">{t("Loading...")}</p>;

  const cap = data.capabilities;
  const lat = data.outbox.latency;

  return (
    <>
      <h2>{t("About & Diagnostics")}</h2>

      <h3>{t("Version")}</h3>
      <p className="muted">{tf("Companion {version}", { version: data.version })}</p>

      <h3>{t("Privacy")}</h3>
      <p className="muted">{data.privacyDisclaimer}</p>

      <h3>{t("Capabilities")}</h3>
      <dl className="stats">
        <div><dt>{t("Desktop popup")}</dt><dd>{channel(cap.desktopPopup)}</dd></div>
        <div><dt>{t("System notification")}</dt><dd>{channel(cap.systemNotification)}</dd></div>
        <div><dt>{t("Tray")}</dt><dd>{channel(cap.tray)}</dd></div>
        <div><dt>{t("Do not disturb")}</dt><dd>{channel(cap.doNotDisturbDetection)}</dd></div>
        <div><dt>{t("Multi monitor")}</dt><dd>{channel(cap.multiMonitor)}</dd></div>
        <div><dt>{t("DPI scaling")}</dt><dd>{channel(cap.dpiScaling)}</dd></div>
      </dl>
      <p className="muted">
        {t("Presentation channel:")} {channel(data.channel)}
      </p>

      <h3>{t("Notifications")}</h3>
      <dl className="stats">
        <div><dt>{t("Total")}</dt><dd>{data.outbox.total}</dd></div>
        <div><dt>{t("Pending")}</dt><dd>{data.outbox.pending}</dd></div>
        <div><dt>{t("p50 latency")}</dt><dd>{lat.p50Ms} ms</dd></div>
        <div><dt>{t("p99 latency")}</dt><dd>{lat.p99Ms} ms</dd></div>
        <div><dt>{t("Samples")}</dt><dd>{lat.samples}</dd></div>
        <div><dt>{t("Never shown")}</dt><dd>{data.outbox.reasons.length}</dd></div>
      </dl>
      {data.outbox.reasons.length === 0 ? (
        <p className="muted">{t("No delivery failures recorded.")}</p>
      ) : (
        <ul className="list">
          {data.outbox.reasons.map((r) => (
            <li key={r.reason} className="row spread">
              <span className="mono">{r.reason}</span>
              <span>{r.count}</span>
            </li>
          ))}
        </ul>
      )}

      <h3>{t("Data")}</h3>
      <p className="muted">
        {bytes(data.storage.bytes)} · {tf("schema v{v}", { v: data.storage.schemaVersion })}
      </p>
      <p className="muted">{t("Oldest event:")} {dateTime(data.storage.oldestEventAt)}</p>
      <p className="tiny">{data.storage.path}</p>
      <button onClick={() => void backup()}>{t("Back up now")}</button>
      <p className="notice">{notice}</p>

      <h3>{t("Needs the browser extension")}</h3>
      <p className="muted">
        {t("These work only in Full Experience. Desktop Only lists them as unavailable rather than pretending they work.")}
      </p>
      <ul className="list">
        {data.needsExtension.map((feature) => (
          <li key={feature} className="row spread">
            <span>{feature}</span>
            <span className="muted">{t("needs extension")}</span>
          </li>
        ))}
      </ul>

      <h3>{t("Sounds")}</h3>
      <p className="muted">{data.sounds.join(", ")}</p>
    </>
  );
}
