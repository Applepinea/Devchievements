import { useEffect, useState } from "react";
import {
  completeOnboarding,
  getSettings,
  importHistory,
  installHooks,
  onboardingStatus,
  saveSettings,
  type OnboardingStatus,
  type SettingsView,
  uiReady,
} from "../ipc";

// Spec 25: the first run explains what the companion needs and why. It is a
// sequence rather than one long form because each step has a precondition: without
// an identity, installing hooks would capture commits that are then refused.
type Step = "welcome" | "identity" | "hooks" | "import" | "done";

export default function Welcome({
  onFinish,
  initialStep = "welcome",
}: {
  onFinish: () => void;
  initialStep?: string;
}) {
  const [status, setStatus] = useState<OnboardingStatus | null>(null);
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const known: Step[] = ["welcome", "identity", "hooks", "import", "done"];
  const [step, setStepState] = useState<Step>(
    known.includes(initialStep as Step) ? (initialStep as Step) : "welcome",
  );

  // Report the current step so the smoke test can prove each one renders.
  useEffect(() => {
    void uiReady("welcome-" + step);
  }, [step]);
  const [repoPath, setRepoPath] = useState<string>("");
  const [notice, setNotice] = useState<string>("");
  const [importPath, setImportPath] = useState<string>("");
  const [importing, setImporting] = useState(false);

  useEffect(() => {
    Promise.all([onboardingStatus(), getSettings()]).then(([s, v]) => {
      setStatus(s);
      setSettings(v);
    });
  }, []);

  if (!status || !settings) return <p className="muted">Loading...</p>;

  const setStep = (next: Step) => setStepState(next);

  const emails = settings.emails;

  const addSuggestion = (email: string) => {
    if (emails.includes(email)) return;
    setSettings({ ...settings, emails: [...emails, email] });
  };

  const saveIdentity = async () => {
    if (emails.length === 0) {
      setNotice("Add at least one email first.");
      return;
    }
    await saveSettings(settings);
    setStep("hooks");
  };

  const runInstall = async () => {
    if (!repoPath.trim()) {
      setNotice("Enter the path to a repository.");
      return;
    }
    try {
      const results = await installHooks(repoPath.trim());
      setNotice(
        "Installed: " + results.map((r) => r.hook).join(", ") +
          ". Commits in this repository are now captured.",
      );
      setImportPath(repoPath.trim());
      setStep("import");
    } catch (e) {
      setNotice("Install failed: " + (e instanceof Error ? e.message : String(e)));
    }
  };

  const runImport = async () => {
    if (!importPath.trim()) {
      setNotice("Enter the path to a repository.");
      return;
    }
    setImporting(true);
    try {
      const report = await importHistory(importPath.trim());
      setNotice(
        "Imported " + report.ingested + " commits (" + report.duplicates + " already known).",
      );
      setStep("done");
    } catch (e) {
      setNotice("Import failed: " + (e instanceof Error ? e.message : String(e)));
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
            This companion watches your local Git commits and unlocks achievements.
            Everything stays on this machine.
          </p>
          <ul className="list">
            <li className="row">— it reads commit facts through a Git hook</li>
            <li className="row">— it never uploads anything</li>
            <li className="row">— it records nothing until you confirm your email</li>
          </ul>
          <button onClick={() => setStep("identity")}>Get started</button>
        </>
      ) : null}

      {step === "identity" ? (
        <>
          <h2>Which commits are yours?</h2>
          <p className="muted">
            Commits are matched by author email. Without one, the companion records
            nothing rather than attributing someone else's work to you.
          </p>
          {status.suggestedEmails.length > 0 ? (
            <>
              <p className="muted">Found in your Git configuration:</p>
              <ul className="list">
                {status.suggestedEmails.map((email) => (
                  <li key={email} className="row spread">
                    <span className="mono">{email}</span>
                    <button
                      className="ghost"
                      onClick={() => addSuggestion(email)}
                      disabled={emails.includes(email)}
                    >
                      {emails.includes(email) ? "Added" : "Use this"}
                    </button>
                  </li>
                ))}
              </ul>
            </>
          ) : (
            <p className="muted">
              Git has no user.email configured, so type the address you commit with.
            </p>
          )}
          <div className="row">
            <input
              value={emails.join(", ")}
              onChange={(e) =>
                setSettings({
                  ...settings,
                  emails: e.target.value.split(",").map((s) => s.trim()).filter(Boolean),
                })
              }
              placeholder="you@example.com"
            />
          </div>
          <p className="notice">{notice}</p>
          <div className="row">
            <button onClick={() => void saveIdentity()}>Continue</button>
          </div>
        </>
      ) : null}

      {step === "hooks" ? (
        <>
          <h2>Install the Git hook</h2>
          <p className="muted">
            Point at a repository to start capturing its commits. You can add more
            repositories later from the Repositories page.
          </p>
          {status.helperAvailable ? null : (
            <p className="error">
              The hook helper is missing next to the app, so hooks cannot be
              installed. Reinstall the companion.
            </p>
          )}
          <div className="row">
            <input
              value={repoPath}
              onChange={(e) => setRepoPath(e.target.value)}
              placeholder="C:\path\to\repo"
            />
            <button
              onClick={() => void runInstall()}
              disabled={!status.helperAvailable}
            >
              Install
            </button>
          </div>
          <p className="notice">{notice}</p>
          <button className="ghost" onClick={() => void finish()}>
            Skip for now
          </button>
        </>
      ) : null}

      {step === "import" ? (
        <>
          <h2>Import past commits</h2>
          <p className="muted">
            Optional. Past commits can be counted too, at their original dates.
            Imports are silent and can be repeated safely.
          </p>
          <div className="row">
            <input
              value={importPath}
              onChange={(e) => setImportPath(e.target.value)}
              placeholder="C:\path\to\repo"
            />
            <button onClick={() => void runImport()} disabled={importing}>
              {importing ? "Importing..." : "Import"}
            </button>
          </div>
          <p className="notice">{notice}</p>
          <div className="row">
            <button className="ghost" onClick={() => setStep("done")}>
              Skip
            </button>
          </div>
        </>
      ) : null}

      {step === "done" ? (
        <>
          <h2>Ready</h2>
          <p className="muted">{notice}</p>
          <p className="muted">
            Make a commit and the first achievement will unlock.
          </p>
          <button onClick={() => void finish()}>Open the dashboard</button>
        </>
      ) : null}
    </div>
  );
}
