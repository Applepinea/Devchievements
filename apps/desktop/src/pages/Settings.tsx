import { useCallback, useEffect, useState } from "react";
import {
  backupDatabase,
  clearData,
  getSettings,
  openDataFolder,
  saveSettings,
  type SettingsView,
  type LoginMapping,
} from "../ipc";

export default function Settings() {
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string>("");
  const [saving, setSaving] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [backupNotice, setBackupNotice] = useState<string>("");

  const onBackup = async () => {
    try {
      const report = await backupDatabase();
      setBackupNotice("Backup written: " + (report.bytes / 1024).toFixed(1) + " KB");
    } catch (e) {
      setBackupNotice("Backup failed: " + (e instanceof Error ? e.message : String(e)));
    }
  };

  const onClear = async () => {
    try {
      await clearData();
      setBackupNotice("Statistics deleted.");
      setConfirmClear(false);
      await load();
    } catch (e) {
      setBackupNotice("Delete failed: " + (e instanceof Error ? e.message : String(e)));
    }
  };

  const load = useCallback(
    () =>
      getSettings().then(setSettings, (e: unknown) =>
        setError(e instanceof Error ? e.message : String(e))),
    [],
  );

  useEffect(() => {
    void load();
  }, [load]);

  const save = async () => {
    if (!settings) return;
    setSaving(true);
    setNotice("");
    try {
      await saveSettings(settings);
      setNotice("Saved.");
    } catch (e) {
      setNotice("Could not save: " + (e instanceof Error ? e.message : String(e)));
    } finally {
      setSaving(false);
    }
  };

  if (error) return <p className="error">{error}</p>;
  if (!settings) return <p className="muted">Loading...</p>;

  const update = (patch: Partial<SettingsView>) =>
    setSettings({ ...settings, ...patch });

  return (
    <>
      <h2>Settings</h2>
      {settings.emails.length === 0 ? (
        <p className="notice">
          No identity yet. Add the email you commit with; until then the companion
          records nothing rather than guessing which commits are yours.
        </p>
      ) : null}

      <h3>Identity</h3>
      <p className="muted">
        Commits count only when the author email is one of these. At least one is
        required before statistics start.
      </p>
      <ul className="list">
        {settings.emails.map((email, index) => (
          <li key={index} className="row">
            <input
              value={email}
              onChange={(e) => {
                const next = [...settings.emails];
                next[index] = e.target.value;
                update({ emails: next });
              }}
            />
            <button className="ghost" onClick={() => update({
              emails: settings.emails.filter((_, i) => i !== index),
            })}>
              Remove
            </button>
          </li>
        ))}
      </ul>
      <button onClick={() => update({ emails: [...settings.emails, ""] })}>
        Add email
      </button>

      <h3>GitHub logins</h3>
      <p className="muted">
        The browser extension reports commits by GitHub account. A report only
        counts when its login is mapped here to one of the emails above;
        everything else is refused rather than assumed to be yours.
      </p>
      <ul className="list">
        {(settings.githubLogins ?? []).map((mapping, index) => (
          <li key={index} className="row">
            <input
              value={mapping.login}
              placeholder="github-login"
              onChange={(e) => {
                const next = [...settings.githubLogins];
                next[index] = { ...mapping, login: e.target.value };
                update({ githubLogins: next });
              }}
            />
            <span className="muted">→</span>
            <input
              value={mapping.emails.join(", ")}
              placeholder="you@example.com"
              onChange={(e) => {
                const next = [...settings.githubLogins];
                next[index] = {
                  ...mapping,
                  emails: e.target.value.split(",").map((s) => s.trim()).filter(Boolean),
                };
                update({ githubLogins: next });
              }}
            />
            <button
              className="ghost"
              onClick={() =>
                update({
                  githubLogins: settings.githubLogins.filter((_, i) => i !== index),
                })
              }
            >
              Remove
            </button>
          </li>
        ))}
      </ul>
      <button
        onClick={() =>
          update({
            githubLogins: [...(settings.githubLogins ?? []), { login: "", emails: [] }],
          })
        }
      >
        Add mapping
      </button>

      <h3>Privacy</h3>
      <label className="row">
        <input
          type="checkbox"
          checked={settings.privacy === "privacy"}
          onChange={(e) =>
            update({ privacy: e.target.checked ? "privacy" : "normal" })
          }
        />
        <span>Hide repository names and commit detail in notifications</span>
      </label>
      <p className="muted">
        Privacy mode hides context. It does not encrypt anything and does not detect
        screen sharing.
      </p>

      <h3>Notifications</h3>
      <label className="row">
        <input
          type="checkbox"
          checked={settings.notifications}
          onChange={(e) => update({ notifications: e.target.checked })}
        />
        <span>Show a popup when an achievement unlocks</span>
      </label>
      <p className="muted">
        Turning this off stops the interruption, not the counting: unlocks still
        happen and still appear in the gallery.
      </p>

      <h3>Sound</h3>
      <label className="row">
        <input
          type="checkbox"
          checked={settings.muted}
          onChange={(e) => update({ muted: e.target.checked })}
        />
        <span>Mute all sound</span>
      </label>
      <div className="row">
        <input
          type="range"
          min={0}
          max={100}
          value={settings.volume}
          disabled={settings.muted}
          onChange={(e) => update({ volume: Number(e.target.value) })}
          className="slider"
        />
        <span className="mono">{settings.volume}</span>
      </div>
      <p className="muted">
        Volume applies to this app only, by scaling the played samples rather than
        changing the system volume.
      </p>

      <h3>Run mode</h3>
      <p className="muted">
        <strong>{settings.runMode}</strong>
      </p>
      <p className="muted">
        Browser-based features (pull requests, releases, issues) need the extension;
        in Desktop Only they are listed as unavailable rather than hidden.
      </p>

      <h3>Timezone</h3>
      <input
        value={settings.timezone ?? ""}
        placeholder="Asia/Taipei"
        onChange={(e) => update({ timezone: e.target.value || null })}
      />
      <p className="muted">
        Decides which day a commit belongs to. Empty means the system zone.
      </p>

      <h3>Data</h3>
      <p className="muted">
        Database: {(settings.databaseBytes / 1024).toFixed(1)} KB
      </p>
      <p className="tiny">{settings.dataDir}</p>
      <div className="row">
        <button onClick={() => void openDataFolder()}>Open folder</button>
        <button onClick={() => void onBackup()}>Back up now</button>
      </div>
      <p className="notice">{backupNotice}</p>

      <div className="row">
        {confirmClear ? (
          <>
            <button className="danger" onClick={() => void onClear()}>
              Yes, delete everything
            </button>
            <button className="ghost" onClick={() => setConfirmClear(false)}>
              Cancel
            </button>
          </>
        ) : (
          <button className="ghost" onClick={() => setConfirmClear(true)}>
            Delete all statistics
          </button>
        )}
      </div>
      <p className="muted">
        Deleting removes the database and the spool. Settings and backups stay.
      </p>

      <div className="row">
        <button onClick={() => void save()} disabled={saving}>
          {saving ? "Saving..." : "Save"}
        </button>
        <span className="notice">{notice}</span>
      </div>
    </>
  );
}
