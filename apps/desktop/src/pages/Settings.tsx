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
  importSound,
  availableSounds,
  previewSound,
  previewSoundSlots,
  listAssets,
  removeAsset,
  showPreview,
  resetOnboarding,
  exportDiagnostics,
  getUnlinkedGithubLogins,
  linkGithubLogin,
  listThemes,
  applyTheme,
  importTheme,
  exportTheme,
  resetTheme,
  removeTheme,
  type ThemeSummary,
  type UnlinkedLogin,
} from "../ipc";
import { t, tf } from "../i18n";

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
      <button onClick={commit}>{t("Exclude")}</button>
    </div>
  );
}

/**
 * Ask the OS for a file path.
 *
 * The dialog plugin's JS API is a separate package; calling the command it exposes
 * keeps this file free of another dependency.
 */
async function openFileDialog(extensions: string[] = ["png", "jpg", "jpeg", "webp", "svg", "wav", "mp3", "ogg"]): Promise<string | null> {
  const w = window as unknown as {
    __TAURI_INTERNALS__?: { invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T> };
  };
  const api = w.__TAURI_INTERNALS__;
  if (!api) return null;
  try {
    const picked = await api.invoke<{ path?: string } | null>("plugin:dialog|open", {
      options: {
        multiple: false,
        filters: [{ name: "Supported files", extensions }],
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
  const [soundNotice, setSoundNotice] = useState<string>("");
  const [diagnosticsRaw, setDiagnosticsRaw] = useState(false);
  const [diagnosticsNotice, setDiagnosticsNotice] = useState<string>("");
  // Reports the identity gate refused. They are real activity; only the account is
  // not linked yet, so this list is a to-do rather than an error log.
  const [waiting, setWaiting] = useState<UnlinkedLogin[]>([]);
  const [linkNotice, setLinkNotice] = useState<string>("");

  // Theme packages (spec 23). The list is kept next to the settings so the section
  // renders without a second round trip, and it is refreshed after every action
  // because apply/import/delete all change what "active" means.
  const [themes, setThemes] = useState<ThemeSummary[]>([]);
  const [activeTheme, setActiveTheme] = useState<string | null>(null);
  const [themeNotice, setThemeNotice] = useState("");

  const refreshThemes = useCallback(async () => {
    try {
      const view = await listThemes();
      setThemes(view.themes);
      setActiveTheme(view.active);
    } catch {
      // Cosmetic: a theme list that cannot be read must not break the page.
    }
  }, []);

  // `success` is already-translated text: passing a key here would hide the literal
  // from scripts/check-i18n.mjs, which only sees literal t() calls.
  const runThemeAction = useCallback(
    async (action: () => Promise<unknown>, success: string) => {
      setThemeNotice("");
      try {
        await action();
        setThemeNotice(success);
      } catch (e) {
        setThemeNotice(tf("Failed: {error}", { error: e instanceof Error ? e.message : String(e) }));
      }
      await refreshThemes();
      // Applying or resetting a theme writes the popup style through the settings
      // document, so the sliders below have to show the new values.
      try {
        setSettings(await getSettings());
      } catch {
        /* the list is already correct; the slider block will catch up on reload */
      }
    },
    [refreshThemes],
  );

  const importThemeFile = useCallback(async () => {
    const picked = await openFileDialog(["json"]);
    if (!picked) return;
    await runThemeAction(() => importTheme(picked), t("Theme imported"));
  }, [runThemeAction]);

  const exportThemeFile = useCallback(async () => {
    // No save dialog here: the native dialog only produces a path for open, and
    // inventing one silently would write somewhere the user did not choose.
    const target = window.prompt(t("Path to write the theme package to"));
    if (!target) return;
    await runThemeAction(() => exportTheme(target), t("Theme exported"));
  }, [runThemeAction]);

  // Sound preview (spec 19). The slot list comes from Rust so the page cannot offer a
  // slot the selector does not understand.
  const [previewSlots, setPreviewSlots] = useState<string[]>([]);
  const [soundPreviewNotice, setSoundPreviewNotice] = useState("");

  const playPreview = useCallback(async (slot: string) => {
    setSoundPreviewNotice("");
    try {
      const report = await previewSound(slot);
      if (report.played) {
        setSoundPreviewNotice(tf("Playing {slot}: {sound} ({source})", { slot, sound: report.sound, source: report.source }));
      } else {
        // Not playing is a legitimate outcome: muted, volume zero, or a missing file.
        // Saying WHY is the difference between a useful preview and a dead button.
        setSoundPreviewNotice(
          tf("{slot} selects {sound}, but nothing was played: {reason}", {
            slot,
            sound: report.sound,
            reason: report.reason ?? "unknown",
          }),
        );
      }
    } catch (e) {
      setSoundPreviewNotice(tf("Preview failed: {error}", { error: e instanceof Error ? e.message : String(e) }));
    }
  }, []);

  const refreshWaiting = useCallback(async () => {
    try {
      setWaiting(await getUnlinkedGithubLogins());
    } catch {
      // Cosmetic: a failure here must not break the page.
    }
  }, []);

  const linkAccount = async (login: string) => {
    setLinkNotice("");
    try {
      const replayed = await linkGithubLogin(login);
      // "Replayed", not "counted": an opened pull request and a merged one both
      // replay, but only the merged one moves a statistic.
      setLinkNotice(
        tf("Linked {login}. {n} waiting report(s) were replayed.", {
          login,
          n: replayed,
        }),
      );
      await Promise.all([load(), refreshWaiting()]);
    } catch (e) {
      setLinkNotice(
        tf("Could not link {login}: {error}", {
          login,
          error: e instanceof Error ? e.message : String(e),
        }),
      );
    }
  };

  const pickSound = async () => {
    const chosen = await openFileDialog(["wav"]);
    if (!chosen) return;
    try {
      await importSound(chosen);
      setSoundNotice(t("Imported."));
      await refreshSounds();
    } catch (e) {
      setSoundNotice(tf("Refused: {error}", { error: e instanceof Error ? e.message : String(e) }));
    }
  };
  const [assets, setAssets] = useState<StoredAsset[]>([]);
  const [sounds, setSounds] = useState<string[]>([]);

  const refreshSounds = useCallback(async () => {
    try {
      setSounds(await availableSounds());
    } catch {
      // Cosmetic; a failure must not break the page.
    }
  }, []);

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
      setAssetNotice(t("Imported."));
      await refreshAssets();
    } catch (e) {
      setAssetNotice(tf("Refused: {error}", { error: e instanceof Error ? e.message : String(e) }));
    }
  };

  const dropAsset = async (name: string) => {
    try {
      await removeAsset(name);
      setAssetNotice(t("Removed."));
      await refreshAssets();
    } catch (e) {
      setAssetNotice(
        tf("Could not remove: {error}", { error: e instanceof Error ? e.message : String(e) }),
      );
    }
  };

  const preview = async () => {
    if (!settings) return;
    // Save first: a preview that shows unsaved values would be a lie about what
    // the next notification looks like.
    await saveSettings(settings);
    try {
      await showPreview();
      setPreviewNotice(t("Preview opened."));
    } catch (e) {
      setPreviewNotice(
        tf("Preview failed: {error}", { error: e instanceof Error ? e.message : String(e) }),
      );
    }
  };

  const onBackup = async () => {
    try {
      const report = await backupDatabase();
      setBackupNotice(
        tf("Backup written: {size}", { size: (report.bytes / 1024).toFixed(1) + " KB" }),
      );
    } catch (e) {
      setBackupNotice(
        tf("Backup failed: {error}", { error: e instanceof Error ? e.message : String(e) }),
      );
    }
  };

  const onClear = async () => {
    try {
      await clearData();
      setBackupNotice(t("Statistics deleted."));
      setConfirmClear(false);
      await load();
    } catch (e) {
      setBackupNotice(
        tf("Delete failed: {error}", { error: e instanceof Error ? e.message : String(e) }),
      );
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
    void refreshSounds();
    void refreshWaiting();
    void refreshThemes();
    previewSoundSlots().then(setPreviewSlots, () => setPreviewSlots([]));
  }, [load, refreshAssets, refreshSounds, refreshWaiting, refreshThemes]);

  const save = async () => {
    if (!settings) return;
    setSaving(true);
    setNotice("");
    try {
      await saveSettings(settings);
      setNotice(t("Saved."));
    } catch (e) {
      setNotice(tf("Could not save: {error}", { error: e instanceof Error ? e.message : String(e) }));
    } finally {
      setSaving(false);
    }
  };

  if (error) return <p className="error">{error}</p>;
  if (!settings) return <p className="muted">{t("Loading...")}</p>;

  const update = (patch: Partial<SettingsView>) =>
    setSettings({ ...settings, ...patch });

  return (
    <>
      <h2>{t("Settings")}</h2>
      {settings.emails.length === 0 ? (
        <p className="notice">
          {t("No identity yet. Add the email you commit with; until then the companion records nothing rather than guessing which commits are yours.")}
        </p>
      ) : null}

      {settings.loadWarning ? (
        <p className="error" data-testid="settings-load-warning">
          {t("Some settings could not be read and were reset to their defaults.")}{" "}
          {settings.loadWarning}
        </p>
      ) : null}

      <h3>{t("Language")}</h3>
      <div className="row">
        <select
          value={settings.language}
          onChange={async (e) => {
            const next = { ...settings, language: e.target.value };
            setSettings(next);
            // Saved and reloaded immediately: the strings on screen are produced by
            // the shell before this page renders, so a language that only took effect
            // at the next launch would look like a broken switch. Saving first also
            // means other edits on this page are not silently dropped by the reload.
            await saveSettings(next);
            window.location.reload();
          }}
        >
          <option value="system">{t("Follow the system")}</option>
          <option value="en">English</option>
          <option value="zh-CN">简体中文</option>
        </select>
      </div>
      <p className="muted">
        {t("Applies immediately: the window reloads. The popup follows the same choice.")}
      </p>

      <h3>{t("Identity")}</h3>
      <p className="muted">
        {t("Commits count only when the author email is one of these. At least one is required before statistics start.")}
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
              {t("Remove")}
            </button>
          </li>
        ))}
      </ul>
      <button onClick={() => update({ emails: [...settings.emails, ""] })}>
        {t("Add email")}
      </button>

      <h3>{t("GitHub logins")}</h3>
      <p className="muted">
        {t("The browser extension reports commits by GitHub account. A report only counts when its login is mapped here to one of the emails above; everything else is refused rather than assumed to be yours.")}
      </p>
      {waiting.length > 0 ? (
        <div className="notice">
          <strong>{t("Waiting for you")}</strong>
          <p className="muted">
            {t("These GitHub accounts sent reports that were refused because they are not linked. Linking one also counts what it already sent.")}
          </p>
          <ul className="list">
            {waiting.map((entry) => (
              <li key={entry.login} className="row spread">
                <span className="mono">{entry.login}</span>
                <span className="muted">
                  {tf("{n} report(s)", { n: entry.reports })}
                </span>
                <button onClick={() => void linkAccount(entry.login)}>
                  {t("Link to my email")}
                </button>
              </li>
            ))}
          </ul>
          <p className="notice">{linkNotice}</p>
        </div>
      ) : null}
      <ul className="list">
        {(settings.githubLogins ?? []).map((mapping: LoginMapping, index: number) => (
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
                  githubLogins: settings.githubLogins.filter((_, i: number) => i !== index),
                })
              }
            >
              {t("Remove")}
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
        {t("Add mapping")}
      </button>

      <h3>{t("Themes")}</h3>
      <p className="muted">
        {t("A theme is a saved popup appearance. Importing one is validated: a package that contains code or a stylesheet is refused, and ranges are clamped.")}
      </p>
      <p className="muted">
        {activeTheme
          ? tf("Active theme: {name}", { name: activeTheme })
          : t("Popup appearance has been edited, so no theme is active.")}
      </p>
      <ul className="events" data-testid="theme-list">
        {themes.map((theme) => (
          <li key={theme.name}>
            <span className="kind">{theme.name}</span>
            <span className="muted">{theme.builtin ? t("built in") : t("imported")}</span>
            <button
              disabled={theme.name === activeTheme}
              onClick={() => void runThemeAction(() => applyTheme(theme.name), t("Theme applied"))}
            >
              {t("Apply")}
            </button>
            {theme.builtin ? null : (
              <button
                onClick={() => void runThemeAction(() => removeTheme(theme.name), t("Theme deleted"))}
              >
                {t("Delete")}
              </button>
            )}
          </li>
        ))}
      </ul>
      <div className="row">
        <button onClick={() => void importThemeFile()}>{t("Import theme…")}</button>
        <button onClick={() => void exportThemeFile()}>{t("Export theme…")}</button>
        <button onClick={() => void runThemeAction(resetTheme, t("Default theme restored"))}>
          {t("Restore default")}
        </button>
      </div>
      {themeNotice ? <p className="muted">{themeNotice}</p> : null}

      <h3>{t("Popup appearance")}</h3>
      <p className="muted">
        {t("The notification window. Values outside the allowed range are clamped when saved, so the popup can never be placed off-screen.")}
      </p>

      <div className="row">
        <label className="field">
          <span>{t("Width")}</span>
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
          <span>{t("Scale")}</span>
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
          <span>{t("Corner")}</span>
          <select
            value={settings.popup.corner}
            onChange={(e) =>
              update({ popup: { ...settings.popup, corner: e.target.value as typeof settings.popup.corner } })
            }
          >
            <option value="top_left">{t("top left")}</option>
            <option value="top_right">{t("top right")}</option>
            <option value="bottom_left">{t("bottom left")}</option>
            <option value="bottom_right">{t("bottom right")}</option>
          </select>
        </label>
        <label className="field">
          <span>{t("Margin")}</span>
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
        <label className="field">
          <span>{t("Display seconds")}</span>
          <input
            type="range" min={1} max={30}
            value={settings.popup.displaySeconds}
            onChange={(e) =>
              update({ popup: { ...settings.popup, displaySeconds: Number(e.target.value) } })
            }
          />
          <span className="mono">{settings.popup.displaySeconds}s</span>
        </label>
        <label className="field">
          <span>{t("Corner radius")}</span>
          <input
            type="range" min={0} max={32}
            value={settings.popup.cornerRadius}
            onChange={(e) =>
              update({ popup: { ...settings.popup, cornerRadius: Number(e.target.value) } })
            }
          />
          <span className="mono">{settings.popup.cornerRadius}px</span>
        </label>
      </div>

      <div className="row">
        <label className="field">
          <span>{t("Icon size")}</span>
          <input
            type="range" min={32} max={160}
            value={settings.popup.iconSize}
            onChange={(e) =>
              update({ popup: { ...settings.popup, iconSize: Number(e.target.value) } })
            }
          />
          <span className="mono">{settings.popup.iconSize}px</span>
        </label>
        <label className="field">
          <span>{t("Opacity")}</span>
          <input
            type="range" min={20} max={100}
            value={settings.popup.opacityPercent}
            onChange={(e) =>
              update({ popup: { ...settings.popup, opacityPercent: Number(e.target.value) } })
            }
          />
          <span className="mono">{settings.popup.opacityPercent}%</span>
        </label>
      </div>

      <div className="row">
        <label className="field">
          <span>{t("Animation")}</span>
          <input
            type="range" min={0} max={1000} step={20}
            value={settings.popup.animationMs}
            onChange={(e) =>
              update({ popup: { ...settings.popup, animationMs: Number(e.target.value) } })
            }
          />
          <span className="mono">{settings.popup.animationMs}ms</span>
        </label>
        <label className="field">
          <span>{t("Background blur")}</span>
          <input
            type="range" min={0} max={24}
            value={settings.popup.backgroundBlur}
            onChange={(e) =>
              update({ popup: { ...settings.popup, backgroundBlur: Number(e.target.value) } })
            }
          />
          <span className="mono">{settings.popup.backgroundBlur}px</span>
        </label>
      </div>

      <div className="row">
        <button onClick={() => void preview()}>{t("Preview popup")}</button>
        <span className="notice">{previewNotice}</span>
      </div>

      <h3>{t("Sounds")}</h3>
      <p className="muted">
        {t("Available sounds. Import a WAV to replace one: a file named after a slot (for example commitment) becomes selectable, and an override pointing at it is what actually changes playback.")}
      </p>
      <div className="row">
        <button onClick={() => void pickSound()}>{t("Import sound…")}</button>
        <span className="notice">{soundNotice}</span>
      </div>
      <p className="tiny">{sounds.join(" · ") || t("No sounds found.")}</p>

      <p className="muted">
        {t("Preview the sound a real notification would play for each slot. It uses your saved overrides and volume, so what you hear is what you get.")}
      </p>
      <div className="row" data-testid="sound-preview">
        {previewSlots.map((slot) => (
          <button key={slot} onClick={() => void playPreview(slot)}>
            {slot}
          </button>
        ))}
      </div>
      <p className="notice" data-testid="sound-preview-notice">{soundPreviewNotice}</p>

      <h3>{t("Custom assets")}</h3>
      <p className="muted">
        {t("Images and sounds you import. Each file is checked against its actual bytes before it is stored: a renamed document is refused, and an SVG that could run code is rejected rather than sanitised.")}
      </p>
      <div className="row">
        <button onClick={() => void pickAsset()}>{t("Import file…")}</button>
        <span className="notice">{assetNotice}</span>
      </div>
      {assets.length === 0 ? (
        <p className="tiny">{t("No custom assets yet.")}</p>
      ) : (
        <ul className="list">
          {assets.map((asset: StoredAsset) => (
            <li key={asset.name} className="row spread">
              <span className="mono">{asset.name}</span>
              <span className="muted">
                {asset.format} · {(asset.bytes / 1024).toFixed(1)} KB
              </span>
              <button className="ghost" onClick={() => void dropAsset(asset.name)}>
                {t("Remove")}
              </button>
            </li>
          ))}
        </ul>
      )}

      <h3>{t("Event filters")}</h3>
      <p className="muted">
        {t("These affect your dashboard and ordinary statistics only. Built-in achievements keep counting everything: excluding a repository here never takes an achievement away.")}
      </p>

      <label className="row">
        <input
          type="checkbox"
          checked={settings.filters.include_merges}
          onChange={(e) => update({ filters: { ...settings.filters, include_merges: e.target.checked } })}
        />
        <span>{t("Count merge commits")}</span>
      </label>

      <label className="row">
        <input
          type="checkbox"
          checked={settings.filters.include_imported}
          onChange={(e) => update({ filters: { ...settings.filters, include_imported: e.target.checked } })}
        />
        <span>{t("Count imported history")}</span>
      </label>

      <p className="muted">{t("Excluded repositories:")}</p>
      {settings.filters.repositories.mode === "all" ? (
        <p className="tiny">{t("Nothing excluded.")}</p>
      ) : (
        <ul className="list">
          {settings.filters.repositories.ids.map((id: string, index: number) => (
            <li key={id} className="row spread">
              <span className="mono">{id}</span>
              <button className="ghost" onClick={() => {
                const ids = settings.filters.repositories.mode === "all"
                  ? []
                  : settings.filters.repositories.ids.filter((_, i: number) => i !== index);
                update({
                  filters: {
                    ...settings.filters,
                    repositories: ids.length ? { mode: "except", ids } : { mode: "all" },
                  },
                });
              }}>{t("Remove")}</button>
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

      <h3>{t("Privacy")}</h3>
      <label className="row">
        <input
          type="checkbox"
          checked={settings.privacy === "privacy"}
          onChange={(e) =>
            update({ privacy: e.target.checked ? "privacy" : "normal" })
          }
        />
        <span>{t("Hide repository names and commit detail in notifications")}</span>
      </label>
      <p className="muted">
        {t("Privacy mode hides context. It does not encrypt anything and does not detect screen sharing.")}
      </p>

      <h3>{t("Notifications")}</h3>
      <label className="row">
        <input
          type="checkbox"
          checked={settings.notifications}
          onChange={(e) => update({ notifications: e.target.checked })}
        />
        <span>{t("Show a popup when an achievement unlocks")}</span>
      </label>
      <p className="muted">
        {t("Turning this off stops the interruption, not the counting: unlocks still happen and still appear in the gallery.")}
      </p>

      <h3>{t("Sound")}</h3>
      <label className="row">
        <input
          type="checkbox"
          checked={settings.muted}
          onChange={(e) => update({ muted: e.target.checked })}
        />
        <span>{t("Mute all sound")}</span>
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
        {t("Volume applies to this app only, by scaling the played samples rather than changing the system volume.")}
      </p>

      <h3>{t("Run mode")}</h3>
      <p className="muted">
        <strong>{settings.runMode}</strong>
      </p>
      <p className="muted">
        {t("Browser-based features (pull requests, releases, issues) need the extension; in Desktop Only they are listed as unavailable rather than hidden.")}
      </p>

      <label className="row">
        <input
          type="checkbox"
          checked={settings.startHidden}
          onChange={(e) => update({ startHidden: e.target.checked })}
        />
        {t("Start hidden in the tray")}
      </label>
      <p className="muted">
        {t("On: a launch only puts the icon in the tray and the app keeps counting. Off: the window opens too, which is what a launcher behaves like. Either way the first run shows the setup wizard, and the tray menu can always open the window. Takes effect on the next launch.")}
      </p>

      <label className="row">
        <input
          type="checkbox"
          checked={settings.startWithWindows}
          onChange={(e) => update({ startWithWindows: e.target.checked })}
        />
        {t("Start with Windows")}
      </label>
      <p className="muted">
        {t("Off by default. Turning it on registers the app in")}
        <span className="mono"> HKCU\...\CurrentVersion\Run</span>
        {t(", which signing out and back in follows; uninstalling removes the same entry, so nothing is left behind. Combined with start hidden this is the usual companion setup: running from login, silent until it has something to say.")}
      </p>

      <h3>{t("Timezone")}</h3>
      <input
        value={settings.timezone ?? ""}
        placeholder="Asia/Taipei"
        onChange={(e) => update({ timezone: e.target.value || null })}
      />
      <p className="muted">
        {t("Decides which day a commit belongs to. Empty means the system zone.")}
      </p>

      <h3>{t("Data")}</h3>
      <p className="muted">
        {tf("Database: {size} KB", { size: (settings.databaseBytes / 1024).toFixed(1) })}
      </p>
      <p className="tiny">{settings.dataDir}</p>
      <div className="row">
        <button onClick={() => void openDataFolder()}>{t("Open folder")}</button>
        <button onClick={() => void onBackup()}>{t("Back up now")}</button>
      </div>

      <h3>{t("Diagnostics")}</h3>
      <p className="muted">
        {t("Writes a JSON report next to the database: counters, outbox health, capabilities and settings. Attach it to a bug report. The redacted form replaces repository names with stable aliases and masks the data path.")}
      </p>
      <label className="row">
        <input
          type="checkbox"
          checked={diagnosticsRaw}
          onChange={(e) => setDiagnosticsRaw(e.target.checked)}
        />
        {t("Include real repository names and paths")}
      </label>
      {diagnosticsRaw ? (
        <p className="error">
          {t("The raw form is not safe to share publicly: it names your repositories.")}
        </p>
      ) : null}
      <div className="row">
        <button
          onClick={async () => {
            try {
              const report = await exportDiagnostics(diagnosticsRaw);
              setDiagnosticsNotice(tf("Saved to {path}", { path: report.path }));
            } catch (e) {
              setDiagnosticsNotice(
                tf("Export failed: {error}", {
                  error: e instanceof Error ? e.message : String(e),
                }),
              );
            }
          }}
        >
          {t("Export diagnostics")}
        </button>
        <span className="notice">{diagnosticsNotice}</span>
      </div>

      <h3>{t("Setup")}</h3>
      <p className="muted">
        {t("Runs the first-run wizard again: confirm which email is yours and install the Git hooks. Nothing is deleted — history, achievements and these settings stay as they are.")}
      </p>
      <div className="row">
        <button
          onClick={async () => {
            await resetOnboarding();
            // Reload instead of navigating: the wizard is rendered by the shell, not
            // by a page, so the shell has to re-read the flag it decided on.
            window.location.reload();
          }}
        >
          {t("Re-run setup")}
        </button>
      </div>
      <p className="notice">{backupNotice}</p>

      <div className="row">
        {confirmClear ? (
          <>
            <button className="danger" onClick={() => void onClear()}>
              {t("Yes, delete everything")}
            </button>
            <button className="ghost" onClick={() => setConfirmClear(false)}>
              {t("Cancel")}
            </button>
          </>
        ) : (
          <button className="ghost" onClick={() => setConfirmClear(true)}>
            {t("Delete all statistics")}
          </button>
        )}
      </div>
      <p className="muted">
        {t("Deleting removes the database and the spool. Settings and backups stay.")}
      </p>

      <div className="row">
        <button onClick={() => void save()} disabled={saving}>
          {saving ? t("Saving...") : t("Save")}
        </button>
        <span className="notice">{notice}</span>
      </div>
    </>
  );
}
