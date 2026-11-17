import { useEffect, useState } from "react";
import {
  checkDependencies,
  completeOnboarding,
  getSettings,
  importHistory,
  installHooks,
  onboardingStatus,
  openDataFolder,
  openExternal,
  saveSettings,
  type Dependency,
  type DependencyReport,
  type OnboardingStatus,
  type SettingsView,
  uiReady,
} from "../ipc";
import { t, tf } from "../i18n";

// Spec 25: the first run explains what the companion needs and why. It is a
// sequence rather than one long form because each step has a precondition: without
// an identity, installing hooks would capture commits that are then refused.
type Step = "welcome" | "checks" | "identity" | "hooks" | "import" | "extension" | "done";

/**
 * Why the purpose text is mapped here instead of trusted as a key.
 *
 * The dependency description comes from Rust at runtime, and the i18n dictionary
 * only contains literal strings the source uses. Mapping the known ids keeps the
 * dictionary checkable while still translating what the user reads.
 */
function purposeText(dependency: Dependency): string {
  if (dependency.id === "webview2") return t("Draws the window and the notification popup.");
  if (dependency.id === "git") {
    return t("Records your commits. Without it the companion has nothing to watch.");
  }
  return dependency.purpose;
}

export default function Welcome({
  onFinish,
  initialStep = "welcome",
}: {
  onFinish: () => void;
  initialStep?: string;
}) {
  const [status, setStatus] = useState<OnboardingStatus | null>(null);
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const known: Step[] = ["welcome", "checks", "identity", "hooks", "import", "extension", "done"];
  const [step, setStepState] = useState<Step>(
    known.includes(initialStep as Step) ? (initialStep as Step) : "welcome",
  );
  // Declared before the readiness effect below: a hook argument is evaluated during
  // render, so referencing a later `const` is a temporal-dead-zone crash ("Cannot
  // access 'deps' before initialization") that leaves a blank window.
  const [deps, setDeps] = useState<DependencyReport | null>(null);

  // Report the current step so the smoke test can prove each one renders.
  //
  // The checks step waits for its probe: otherwise the window can claim it is ready
  // while still showing "Checking...", and the smoke test would pass on a panel that
  // never produced an answer.
  useEffect(() => {
    if (step === "checks" && deps === null) return;
    void uiReady("welcome-" + step);
  }, [step, deps]);
  const [repoPath, setRepoPath] = useState<string>("");
  const [notice, setNotice] = useState<string>("");
  const [importPath, setImportPath] = useState<string>("");
  const [importing, setImporting] = useState(false);
  useEffect(() => {
    Promise.all([onboardingStatus(), getSettings()]).then(([s, v]) => {
      setStatus(s);
      setSettings(v);
    });
    // Checked once, up front: the identity step is pointless advice if Git is
    // missing, and the hook step would fail with a confusing error instead of
    // saying what to install.
    checkDependencies().then(setDeps, () => setDeps(null));
  }, []);

  if (!status || !settings) return <p className="muted">{t("Loading...")}</p>;

  const setStep = (next: Step) => setStepState(next);

  const emails = settings.emails;

  const addSuggestion = (email: string) => {
    if (emails.includes(email)) return;
    setSettings({ ...settings, emails: [...emails, email] });
  };

  const saveIdentity = async () => {
    if (emails.length === 0) {
      setNotice(t("Add at least one email first."));
      return;
    }
    await saveSettings(settings);
    setStep("hooks");
  };

  const runInstall = async () => {
    if (!repoPath.trim()) {
      setNotice(t("Enter the path to a repository."));
      return;
    }
    try {
      const results = await installHooks(repoPath.trim());
      setNotice(
        tf("Installed: {hooks}. Commits in this repository are now captured.", {
          hooks: results.map((r) => r.hook).join(", "),
        }),
      );
      setImportPath(repoPath.trim());
      setStep("import");
    } catch (e) {
      setNotice(
        tf("Install failed: {error}", { error: e instanceof Error ? e.message : String(e) }),
      );
    }
  };

  const runImport = async () => {
    if (!importPath.trim()) {
      setNotice(t("Enter the path to a repository."));
      return;
    }
    setImporting(true);
    try {
      const report = await importHistory(importPath.trim());
      setNotice(
        tf("Imported {ingested} commits ({duplicates} already known).", {
          ingested: report.ingested,
          duplicates: report.duplicates,
        }),
      );
      setStep("extension");
    } catch (e) {
      setNotice(
        tf("Import failed: {error}", { error: e instanceof Error ? e.message : String(e) }),
      );
    } finally {
      setImporting(false);
    }
  };

  const finish = async () => {
    await completeOnboarding();
    onFinish();
  };

  return (
    <div className="wizard">
      <h1>Devchievements</h1>

      {step === "welcome" ? (
        <>
          <p className="muted">
            {t("This companion watches your local Git commits and unlocks achievements. Everything stays on this machine.")}
          </p>
          <ul className="list">
            <li className="row">{t("— it reads commit facts through a Git hook")}</li>
            <li className="row">{t("— it never uploads anything")}</li>
            <li className="row">
              {t("— it records nothing until you confirm your email")}
            </li>
          </ul>
          <button onClick={() => setStep("checks")}>{t("Get started")}</button>
        </>
      ) : null}

      {step === "checks" ? (
        <>
          <h2>{t("Before we start")}</h2>
          <p className="muted">
            {t("The companion needs two things from Windows. This checks them and tells you where to get anything that is missing.")}
          </p>
          {deps === null ? (
            <p className="muted">{t("Checking...")}</p>
          ) : (
            <ul className="list">
              {deps.dependencies.map((dependency) => (
                <li key={dependency.id} className="row spread">
                  <span>
                    <strong>{dependency.name}</strong>
                    <br />
                    <span className="tiny">{purposeText(dependency)}</span>
                  </span>
                  <span className={dependency.ok ? "muted" : "error"}>
                    {dependency.ok
                      ? t("Installed") + (dependency.version ? " " + dependency.version : "")
                      : t("Missing")}
                  </span>
                </li>
              ))}
            </ul>
          )}
          {deps && !deps.ready ? (
            <>
              <p className="muted">
                {t("Install what is missing, then open this window again. Nothing is installed for you: that is your decision, not ours.")}
              </p>
              <div className="row">
                {deps.dependencies
                  .filter((d) => !d.ok && d.downloadUrl)
                  .map((d) => (
                    <button
                      key={d.id}
                      className="ghost"
                      onClick={() => void openExternal(d.downloadUrl as string)}
                    >
                      {tf("Get {name}", { name: d.name })}
                    </button>
                  ))}
              </div>
            </>
          ) : null}
          <button onClick={() => setStep("identity")}>{t("Continue")}</button>
        </>
      ) : null}
      {step === "identity" ? (
        <>
          <h2>{t("Which commits are yours?")}</h2>
          <p className="muted">
            {t("Commits are matched by author email. Without one, the companion records nothing rather than attributing someone else's work to you.")}
          </p>
          {status.suggestedEmails.length > 0 ? (
            <>
              <p className="muted">{t("Found in your Git configuration:")}</p>
              <ul className="list">
                {status.suggestedEmails.map((email) => (
                  <li key={email} className="row spread">
                    <span className="mono">{email}</span>
                    <button
                      className="ghost"
                      onClick={() => addSuggestion(email)}
                      disabled={emails.includes(email)}
                    >
                      {emails.includes(email) ? t("Added") : t("Use this")}
                    </button>
                  </li>
                ))}
              </ul>
            </>
          ) : (
            <p className="muted">
              {t("Git has no user.email configured, so type the address you commit with.")}
            </p>
          )}
          <div className="row">
            <input
              value={emails.join(", ")}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  emails: e.target.value
                    .split(",")
                    .map((s) => s.trim())
                    .filter(Boolean),
                })
              }
              placeholder="you@example.com"
            />
          </div>
          <p className="notice">{notice}</p>
          <div className="row">
            <button onClick={() => void saveIdentity()}>{t("Continue")}</button>
          </div>
        </>
      ) : null}

      {step === "hooks" ? (
        <>
          <h2>{t("Install the Git hook")}</h2>
          <p className="muted">
            {t("Point at a repository to start capturing its commits. To add another one later, run this setup again from Settings.")}
          </p>
          {status.helperAvailable ? null : (
            <p className="error">
              {t("The hook helper is missing next to the app, so hooks cannot be installed. Reinstall the companion.")}
            </p>
          )}
          <div className="row">
            <input
              value={repoPath}
              onChange={(e) => setRepoPath(e.target.value)}
              placeholder={t("C:\\path\\to\\repo")}
            />
            <button
              onClick={() => void runInstall()}
              disabled={!status.helperAvailable}
            >
              {t("Install")}
            </button>
          </div>
          <p className="notice">{notice}</p>
          <button className="ghost" onClick={() => setStep("extension")}>
            {t("Skip for now")}
          </button>
        </>
      ) : null}

      {step === "import" ? (
        <>
          <h2>{t("Import past commits")}</h2>
          <p className="muted">
            {t("Optional. Past commits can be counted too, at their original dates. Imports are silent and can be repeated safely.")}
          </p>
          <div className="row">
            <input
              value={importPath}
              onChange={(e) => setImportPath(e.target.value)}
              placeholder={t("C:\\path\\to\\repo")}
            />
            <button onClick={() => void runImport()} disabled={importing}>
              {importing ? t("Importing...") : t("Import")}
            </button>
          </div>
          <p className="notice">{notice}</p>
          <div className="row">
            <button className="ghost" onClick={() => setStep("extension")}>
              {t("Skip")}
            </button>
          </div>
        </>
      ) : null}

      {step === "extension" ? (
        <>
          <h2>{t("Browser extension")}</h2>
          <p className="muted">
            {t("Optional. The companion already counts your local Git commits. The extension adds what only a browser can see: pull requests, releases and issues on GitHub.")}
          </p>
          <p className="muted">
            {t("It is not in a store yet, so it is installed by hand: open edge://extensions or chrome://extensions, turn on Developer mode, choose Load unpacked, and pick the extension folder that ships with this app.")}
          </p>
          <p className="muted">
            {t("Everything stays local either way: the extension reports to this app on this machine, and nothing is uploaded.")}
          </p>
          <div className="row">
            <button className="ghost" onClick={() => void openDataFolder()}>
              {t("Open the data folder")}
            </button>
            <button onClick={() => setStep("done")}>{t("Done")}</button>
          </div>
          <button className="ghost" onClick={() => void finish()}>
            {t("Skip for now")}
          </button>
        </>
      ) : null}
      {step === "done" ? (
        <>
          <h2>{t("Ready")}</h2>
          <p className="muted">{notice}</p>
          <p className="muted">
            {t("Make a commit and the first achievement will unlock.")}
          </p>
          <button onClick={() => void finish()}>{t("Open the dashboard")}</button>
        </>
      ) : null}
    </div>
  );
}
