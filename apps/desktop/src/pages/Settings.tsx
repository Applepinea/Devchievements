import { useCallback, useEffect, useState } from "react";
import { getSettings, saveSettings, type SettingsView } from "../ipc";

export default function Settings() {
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string>("");
  const [saving, setSaving] = useState(false);

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
      <p className="tiny">{settings.dataDir}</p>

      <div className="row">
        <button onClick={() => void save()} disabled={saving}>
          {saving ? "Saving..." : "Save"}
        </button>
        <span className="notice">{notice}</span>
      </div>
    </>
  );
}
