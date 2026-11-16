import { useEffect, useState } from "react";
import { backupDatabase, getAbout, type AboutView } from "../ipc";
import { bytes, channel, dateTime } from "../format";

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
      setNotice("Backup written: " + bytes(report.bytes));
    } catch (e) {
      setNotice("Backup failed: " + (e instanceof Error ? e.message : String(e)));
    }
  };

  if (error) return <p className="error">{error}</p>;
  if (!data) return <p className="muted">Loading...</p>;

  const cap = data.capabilities;
  const lat = data.outbox.latency;

  return (
    <>
      <h2>About & Diagnostics</h2>

      <h3>Version</h3>
      <p className="muted">Companion {data.version}</p>

      <h3>Privacy</h3>
      <p className="muted">{data.privacyDisclaimer}</p>

      <h3>Capabilities</h3>
      <dl className="stats">
        <div><dt>Desktop popup</dt><dd>{channel(cap.desktopPopup)}</dd></div>
        <div><dt>System notification</dt><dd>{channel(cap.systemNotification)}</dd></div>
        <div><dt>Tray</dt><dd>{channel(cap.tray)}</dd></div>
        <div><dt>Do not disturb</dt><dd>{channel(cap.doNotDisturbDetection)}</dd></div>
        <div><dt>Multi monitor</dt><dd>{channel(cap.multiMonitor)}</dd></div>
        <div><dt>DPI scaling</dt><dd>{channel(cap.dpiScaling)}</dd></div>
      </dl>
      <p className="muted">Presentation channel: {channel(data.channel)}</p>

      <h3>Notifications</h3>
      <dl className="stats">
        <div><dt>Total</dt><dd>{data.outbox.total}</dd></div>
        <div><dt>Pending</dt><dd>{data.outbox.pending}</dd></div>
        <div><dt>p50 latency</dt><dd>{lat.p50Ms} ms</dd></div>
        <div><dt>p99 latency</dt><dd>{lat.p99Ms} ms</dd></div>
        <div><dt>Samples</dt><dd>{lat.samples}</dd></div>
        <div><dt>Never shown</dt><dd>{data.outbox.reasons.length}</dd></div>
      </dl>
      {data.outbox.reasons.length === 0 ? (
        <p className="muted">No delivery failures recorded.</p>
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

      <h3>Data</h3>
      <p className="muted">
        {bytes(data.storage.bytes)} · schema v{data.storage.schemaVersion}
      </p>
      <p className="muted">Oldest event: {dateTime(data.storage.oldestEventAt)}</p>
      <p className="tiny">{data.storage.path}</p>
      <button onClick={() => void backup()}>Back up now</button>
      <p className="notice">{notice}</p>

      <h3>Needs the browser extension</h3>
      <p className="muted">
        These work only in Full Experience. Desktop Only lists them as unavailable
        rather than pretending they work.
      </p>
      <ul className="list">
        {data.needsExtension.map((feature) => (
          <li key={feature} className="row spread">
            <span>{feature}</span>
            <span className="muted">needs extension</span>
          </li>
        ))}
      </ul>

      <h3>Sounds</h3>
      <p className="muted">{data.sounds.join(", ")}</p>
    </>
  );
}
