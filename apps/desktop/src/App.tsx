import { useCallback, useEffect, useState } from "react";
import {
  getSettings,
  getStatus,
  saveSettings,
  uiReady,
  type SettingsView,
  type StatusView,
} from "./ipc";

// Settings is the first real page (spec 34.1). It is deliberately the smallest one
// that still exercises the whole chain: window -> React -> IPC -> SQLite -> back,
// so a failure here is a failure of the plumbing rather than of a large view.

type LoadState =
  | { kind: "loading" }
  | { kind: "ready"; settings: SettingsView; status: StatusView }
  | { kind: "failed"; message: string };

export default function App() {
  const [state, setState] = useState<LoadState>({ kind: "loading" });
  const [notice, setNotice] = useState<string>("");
  const [saving, setSaving] = useState(false);

  const load = useCallback(async () => {
    try {
      const [settings, status] = await Promise.all([getSettings(), getStatus()]);
      setState({ kind: "ready", settings, status });
      // Tells the Rust side the UI actually rendered inside the webview. Without
      // this the only evidence of a working UI would be a window that exists.
      await uiReady("settings");
    } catch (e) {
      setState({ kind: "failed", message: e instanceof Error ? e.message : String(e) });
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (state.kind === "loading") {
    return <main className="shell"><p className="muted">Loading…</p></main>;
  }

  if (state.kind === "failed") {
    return (
      <main className="shell">
        <h1>Settings</h1>
        <p className="error">Could not load settings: {state.message}</p>
        <button onClick={() => void load()}>Retry</button>
      </main>
    );
  }

  const { settings, status } = state;

  const update = (patch: Partial<SettingsView>) =>
    setState({ kind: "ready", settings: { ...settings, ...patch }, status });

  const onSave = async () => {
    setSaving(true);
    setNotice("");
    try {
      await saveSettings(settings);
      const fresh = await getStatus();
      setState({ kind: "ready", settings, status: fresh });
      setNotice("Saved.");
    } catch (e) {
      setNotice("Could not save: " + (e instanceof Error ? e.message : String(e)));
    } finally {
      setSaving(false);
    }
  };

  return (
    <main className="shell">
      <header className="head">
        <h1>Devchievements</h1>
        <p className="muted">Achievements for Developers</p>
      </header>

      <section>
        <h2>Identity</h2>
        <p className="muted">
          Commits are only counted when the author email is one of these. At least one
          is required before statistics start.
        </p>
        <ul className="list">
          {settings.emails.map((email, index) => (
            <li key={email + index} className="row">
              <input
                value={email}
                onChange={(e) => {
                  const next = [...settings.emails];
                  next[index] = e.target.value;
                  update({ emails: next });
                }}
              />
              <button
                className="ghost"
                onClick={() =>
                  update({ emails: settings.emails.filter((_, i) => i !== index) })
                }
              >
                Remove
              </button>
            </li>
          ))}
        </ul>
        <button onClick={() => update({ emails: [...settings.emails, ""] })}>
          Add email
        </button>
      </section>

      <section>
        <h2>Privacy</h2>
        <label className="row">
          <input
            type="checkbox"
            checked={settings.privacy === "privacy"}
            onChange={(e) =>
              update({ privacy: e.target.checked ? "privacy" : "normal" })
            }
          />
          <span>
            Hide repository names and commit detail in notifications
          </span>
        </label>
        <p className="muted">
          Privacy mode hides context. It does not encrypt anything and does not detect
          screen sharing.
        </p>
      </section>

      <section>
        <h2>Timezone</h2>
        <input
          value={settings.timezone ?? ""}
          placeholder="Asia/Taipei"
          onChange={(e) => update({ timezone: e.target.value || null })}
        />
        <p className="muted">
          Used to decide which day a commit belongs to. Leave empty to use the system
          zone.
        </p>
      </section>

      <section>
        <h2>Status</h2>
        <dl className="stats">
          <div><dt>Events</dt><dd>{status.events}</dd></div>
          <div><dt>Repositories</dt><dd>{status.repositories}</dd></div>
          <div><dt>Achievements</dt><dd>{status.unlockedAchievements}</dd></div>
          <div><dt>Pending notifications</dt><dd>{status.pendingNotifications}</dd></div>
          <div><dt>Never shown</dt><dd>{status.notificationsNeverShown}</dd></div>
          <div><dt>Presentation</dt><dd>{status.channel.replace(/_/g, " ")}</dd></div>
          <div><dt>System notifications</dt><dd>{status.toastReady ? "available" : "unavailable"}</dd></div>
          <div><dt>Sounds</dt><dd>{status.sounds.length}</dd></div>
        </dl>
        <p className="muted">Data lives in {settings.dataDir}</p>
      </section>

      <footer className="foot">
        <button onClick={() => void onSave()} disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </button>
        {notice ? <span className="notice">{notice}</span> : null}
      </footer>
    </main>
  );
}
