import { useEffect, useState } from "react";
import { getSettings, onboardingStatus, uiLanguage, uiReady } from "./ipc";
import { currentLanguage, setLanguage, t } from "./i18n";
import Dashboard from "./pages/Dashboard";
import Gallery from "./pages/Gallery";
import Activity from "./pages/Activity";
import Repositories from "./pages/Repositories";
import About from "./pages/About";
import Settings from "./pages/Settings";
import Welcome from "./pages/Welcome";
import { uiError } from "./main";

// Labels are resolved at render time, so switching the language is a reload rather
// than a state machine. Page ids stay English: they are the URL contract used by the
// smoke tests, not text a user reads.
const PAGES = [
  { id: "dashboard", label: "Dashboard", component: Dashboard },
  { id: "gallery", label: "Achievements", component: Gallery },
  { id: "activity", label: "Activity", component: Activity },
  { id: "repositories", label: "Repositories", component: Repositories },
  { id: "settings", label: "Settings", component: Settings },
  { id: "about", label: "About", component: About },
] as const;

type PageId = (typeof PAGES)[number]["id"];

// The hash selects the initial page, so a smoke test can open any page directly
// instead of driving the nav with synthetic clicks.
function initialPage(): PageId {
  const raw = window.location.hash.replace(/^#/, "").split("?")[0] ?? "";
  const found = PAGES.find((p) => p.id === raw);
  return found ? found.id : "dashboard";
}

/// A wizard step requested in the URL, used by the smoke test.
function requestedStep(): string | null {
  const raw = window.location.hash.replace(/^#/, "");
  const query = raw.includes("?") ? raw.slice(raw.indexOf("?") + 1) : "";
  return new URLSearchParams(query).get("step");
}

export default function App() {
  const [onboarded, setOnboarded] = useState<boolean | null>(null);
  const [languageReady, setLanguageReady] = useState(false);
  const [current, setCurrent] = useState<PageId>(initialPage);

  // Language first: every string below is produced after this resolves, so the
  // window never paints English and then switch.
  useEffect(() => {
    getSettings().then(
      (settings) => {
        const lang = setLanguage(settings.language);
        // Recorded server-side so "the UI is in the wrong language" can be answered
        // from the log, and so an end-to-end test can assert what actually rendered
        // rather than what the settings file asked for.
        void uiLanguage(lang, t("Dashboard"));
        setLanguageReady(true);
      },
      // Without settings the app must still open; English is the source language.
      () => setLanguageReady(true),
    );
  }, []);

  // Spec 25: the wizard runs before the pages, and only once.
  useEffect(() => {
    onboardingStatus().then(
      (s) => setOnboarded(s.completed),
      // If the status cannot be read the app must still open; the wizard is
      // helpful, not mandatory. This branch is also how a registration bug hid for
      // weeks: when `onboarding_status` was missing from the app's command list but
      // present in the smoke-test list, the product opened straight to an empty
      // dashboard while every test passed. The command lists are one macro now, and
      // scripts/e2e/wizard.mjs drives a real launch, not only the smoke builder.
      () => setOnboarded(true),
    );
  }, []);

  // Tell the Rust side which view rendered. The window smoke test waits for this,
  // because a window showing a blank page looks identical to a working one.
  useEffect(() => {
    // Wait for the status before reporting: while it is unknown the app does not
    // yet know whether this is a first run, and reporting early would claim the
    // dashboard rendered on a machine that is about to show the wizard.
    if (onboarded === null || !languageReady) return;
    // The wizard reports its own steps, and knows when one of them is still
    // loading. Announcing "welcome" from here marked the window ready before the
    // first-run dependency check had answered, so a smoke run could exit with the
    // panel still saying "Checking...".
    if (!onboarded) return;
    void uiReady(current);
  }, [onboarded, current, languageReady]);

  // After the page has had time to load its data, look for placeholder text on
  // screen and report it through the same channel as an exception.
  //
  // This exists because the dashboard printed `undefined d` and `Invalid Date` for a
  // released build and no test noticed: the values were wrong on the wire, and every
  // assertion checked the data source rather than what a person would read.
  useEffect(() => {
    if (onboarded !== true) return;
    const timer = window.setTimeout(() => {
      const text = document.body.innerText;
      const found = ["undefined", "NaN", "Invalid Date", "[object Object]"].filter((bad) =>
        text.includes(bad),
      );
      if (found.length > 0) {
        void uiError(`UI PLACEHOLDER ${found.join(",")} on page ${current}`);
      } else {
        void uiError(`UI PLACEHOLDER none on page ${current}`);
      }
    }, 2500);
    return () => window.clearTimeout(timer);
  }, [onboarded, current]);

  if (onboarded === null || !languageReady) {
    return <p className="muted">{t("Starting...")}</p>;
  }
  if (!onboarded) {
    return (
      <Welcome
        initialStep={requestedStep() ?? "welcome"}
        onFinish={() => setOnboarded(true)}
      />
    );
  }

  const page = PAGES.find((p) => p.id === current) ?? PAGES[0];
  const Page = page.component;

  return (
    <div className="app">
      <nav>
        <div className="brand">Devchievements</div>
        {PAGES.map((p) => (
          <button
            key={p.id}
            className={p.id === current ? "nav active" : "nav"}
            onClick={() => setCurrent(p.id)}
          >
            {t(p.label)}
          </button>
        ))}
      </nav>
      <main className="shell">
        <Page />
      </main>
    </div>
  );
}
