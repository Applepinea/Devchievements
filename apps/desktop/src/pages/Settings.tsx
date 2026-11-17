import { useCallback, useEffect, useState } from "react";
import {
  backupDatabase,
  clearData,
  getSettings,
  openDataFolder,
  saveSettings,
  type SettingsView,
  type LoginMapping,
  type FilterSet,
  type StoredAsset,
  importAsset,
  listAssets,
  removeAsset,
  showPreview,
} from "../ipc";

/** Add an excluded repository id. */
function AddExclusion({ onAdd }: { onAdd: (id: string) => void }) {
  const [value, setValue] = useState("");
  const commit = () => {
    const trimmed = value.trim();
    if (!trimmed) return;
    onAdd(trimmed);
    setValue("");
  };
  return (
    <div className="row">
      <input
        value={value}
        placeholder="github:owner/repo"
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
        }}
      />
      <button onClick={commit}>Exclude</button>
    </div>
  );
}

/**
 * Ask the OS for a file path.
 *
 * The dialog plugin's JS API is a separate package; calling the command it exposes
 * keeps this file free of another dependency.
 */
async function openFileDialog(): Promise<string | null> {
  const w = window as unknown as {
    __TAURI_INTERNALS__?: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> };
  };
  const api = w.__TAURI_INTERNALS__;
  if (!api) return null;
  try {
    const picked = await api.invoke<{ path?: string } | null>("plugin:dialog|open", {
      options: {
        multiple: false,
        filters: [{ name: "Image or sound", extensions: ["png", "jpg", "jpeg", "webp", "svg", "wav", "mp3", "ogg"] }],
      },
    });
    // The plugin returns a string on some versions and { path } on others.
    if (typeof picked === "string") return picked;
    return picked?.path ?? null;
  } catch {
    return null;
  }
}

export default function Settings() {
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string>("");
  const [saving, setSaving] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);
  const [backupNotice, setBackupNotice] = useState<string>("");
  const [previewNotice, setPreviewNotice] = useState<string>("");
  const [assetNotice, setAssetNotice] = useState<string>("");
  const [assets, setAssets] = useState<StoredAsset[]>([]);

  const refreshAssets = useCallback(async () => {
    try {
      setAssets(await listAssets());
    } catch {
      // The list is cosmetic; a failure here must not break the page.
    }
  }, []);

  const pickAsset = async () => {
    // The dialog picks a path; validation happens in Rust.
    const chosen = await openFileDialog();
    if (!chosen) return;
    try {
      await importAsset(chosen);
      setAssetNotice("Imported.");
      await refreshAssets();
    } catch (e) {
      setAssetNotice("Refused: " + (e instanceof Error ? e.message : String(e)));
    }
  };

  const dropAsset = async (name: string) => {
    try {
      await removeAsset(name);
      setAssetNotice("Removed.");
      await refreshAssets();
    } catch (e) {
      setAssetNotice("Could not remove: " + (e instanceof Error ? e.message : String(e)));
    }
  };

  const preview = async () => {
    if (!settings) return;
    // Save first: a preview that shows unsaved values would be a lie about what
    // the next notification looks like.
    await saveSettings(settings);
    try {
      await showPreview();
      setPreviewNotice("Preview opened.");
    } catch (e) {
      setPreviewNotice("Preview failed: " + (e instanceof Error ? e.message : String(e)));
    }
  };

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
    void refreshAssets();
  }, [load, refreshAssets]);

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

      <h3>Popup appearance</h3>
      <p className="muted">
        The notification window. Values outside the allowed range are clamped when
        saved, so the popup can never be placed off-screen.
      </p>

      <div className="row">
        <label className="field">
          <span>Width</span>
          <input
            type="range" min={280} max={600}
            value={settings.popup.width}
            onChange={(e) =>
              update({ popup: { ...settings.popup, width: Number(e.target.value) } })
            }
          />
          <span className="mono">{settings.popup.width}</span>
        </label>
        <label className="field">
          <span>Scale</span>
          <input
            type="range" min={70} max={150}
            value={settings.popup.scalePercent}
            onChange={(e) =>
              update({ popup: { ...settings.popup, scalePercent: Number(e.target.value) } })
            }
          />
          <span className="mono">{settings.popup.scalePercent}%</span>
        </label>
      </div>

      <div className="row">
        <label className="field">
          <span>Corner</span>
          <select
            value={settings.popup.corner}
            onChange={(e) =>
              update({ popup: { ...settings.popup, corner: e.target.value as typeof settings.popup.corner } })
            }
          >
            <option value="top_left">top left</option>
            <option value="top_right">top right</option>
            <option value="bottom_left">bottom left</option>
            <option value="bottom_right">bottom right</option>
          </select>
        </label>
        <label className="field">
          <span>Margin</span>
          <input
            type="range" min={0} max={100}
            value={settings.popup.screenMargin}
            onChange={(e) =>
              update({ popup: { ...settings.popup, screenMargin: Number(e.target.value) } })
            }
          />
          <span className="mono">{settings.popup.screenMargin}</span>
        </label>
      </div>

      <div className="row">
        <button onClick={() => void preview()}>Preview popup</button>
        <span className="notice">{previewNotice}</span>
      </div>

      <h3>Custom assets</h3>
      <p className="muted">
        Images and sounds you import. Each file is checked against its actual bytes
        before it is stored: a renamed document is refused, and an SVG that could
        run code is rejected rather than sanitised.
      </p>
      <div className="row">
        <button onClick={() => void pickAsset()}>Import file…</button>
        <span className="notice">{assetNotice}</span>
      </div>
      {assets.length === 0 ? (
        <p className="tiny">No custom assets yet.</p>
      ) : (
        <ul className="list">
          {assets.map((asset) => (
            <li key={asset.name} className="row spread">
              <span className="mono">{asset.name}</span>
              <span className="muted">
                {asset.format} · {(asset.bytes / 1024).toFixed(1)} KB
              </span>
              <button className="ghost" onClick={() => void dropAsset(asset.name)}>
                Remove
              </button>
            </li>
          ))}
        </ul>
      )}

      <h3>Event filters</h3>
      <p className="muted">
        These affect your dashboard and ordinary statistics only. Built-in
        achievements keep counting everything: excluding a repository here never
        takes an achievement away.
      </p>

      <label className="row">
        <input
          type="checkbox"
          checked={settings.filters.include_merges}
          onChange={(e) => update({ filters: { ...settings.filters, include_merges: e.target.checked } })}
        />
        <span>Count merge commits</span>
      </label>

      <label className="row">
        <input
          type="checkbox"
          checked={settings.filters.include_imported}
          onChange={(e) => update({ filters: { ...settings.filters, include_imported: e.target.checked } })}
        />
        <span>Count imported history</span>
      </label>

      <p className="muted">Excluded repositories:</p>
      {settings.filters.repositories.mode === "all" ? (
        <p className="tiny">Nothing excluded.</p>
      ) : (
        <ul className="list">
          {settings.filters.repositories.ids.map((id, index) => (
            <li key={id} className="row spread">
              <span className="mono">{id}</span>
              <button className="ghost" onClick={() => {
                const ids = settings.filters.repositories.mode === "all"
                  ? []
                  : settings.filters.repositories.ids.filter((_, i) => i !== index);
                update({
                  filters: {
                    ...settings.filters,
                    repositories: ids.length ? { mode: "except", ids } : { mode: "all" },
                  },
                });
              }}>Remove</button>
            </li>
          ))}
        </ul>
      )}
      <AddExclusion
        onAdd={(id) => {
          const current =
            settings.filters.repositories.mode === "all" ? [] : settings.filters.repositories.ids;
          if (current.includes(id)) return;
          update({ filters: { ...settings.filters, repositories: { mode: "except", ids: [...current, id] } } });
        }}
      />

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
