import { Suspense, lazy, useEffect, useState } from "react";
import { bootStatus, pageMs, uiLanguage, uiReady, uiTiming } from "./ipc";
import { setLanguage, t } from "./i18n";
import Dashboard from "./pages/Dashboard";
import { uiError } from "./main";

// Pages are split so opening the dashboard does not pay for parsing the four pages the
// user is not looking at. Dashboard stays eager: it is the page that opens by default,
// and a chunk for it would add a round trip to the first screen it is meant to show.
const Gallery = lazy(() => import("./pages/Gallery"));
const Activity = lazy(() => import("./pages/Activity"));
const Repositories = lazy(() => import("./pages/Repositories"));
const About = lazy(() => import("./pages/About"));
const Settings = lazy(() => import("./pages/Settings"));
// Loaded only when the wizard is actually shown, so a returning user never pays for it.
const Welcome = lazy(() => import("./pages/Welcome"));

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

/// The neutral placeholder shown while a page chunk is still loading.
///
/// Fixed height on purpose: a skeleton that resizes once the real page arrives is a
/// second layout shift, which is worse than the wait it is hiding.
function PageSkeleton() {
  return (
    <div className="shell-skeleton" aria-busy="true">
      <div className="skeleton-line short" />
      <div className="skeleton-line" />
      <div className="skeleton-line" />
      <div className="skeleton-line" />
    </div>
  );
}

export default function App() {
  const [onboarded, setOnboarded] = useState<boolean | null>(null);
  const [languageReady, setLanguageReady] = useState(false);
  const [current, setCurrent] = useState<PageId>(initialPage);

  // The page reports its own stages so a slow first screen can be attributed to the
  // webview, to script, to IPC or to SQLite instead of being guessed at.
  useEffect(() => {
    void uiTiming("script_start", pageMs());
  }, []);

  // One small call answers both questions the shell needs. The full settings document
  // and the wizard's Git probing are no longer on the first-screen path.
  useEffect(() => {
    bootStatus().then(
      (boot) => {
        const lang = setLanguage(boot.language);
        // Recorded server-side so "the UI is in the wrong language" can be answered
        // from the log, and so an end-to-end test can assert what actually rendered
        // rather than what the settings file asked for.
        void uiLanguage(lang, t("Dashboard"));
        void uiTiming("boot_resolved", pageMs());
        setLanguageReady(true);
        setOnboarded(boot.onboardingCompleted);
      },
      // Without a status the app must still open; English is the source language.
      () => {
        void uiTiming("boot_failed", pageMs());
        setLanguageReady(true);
        setOnboarded(true);
      },
    );
  }, []);

  // Tell the Rust side which view rendered. The window smoke test waits for this,
  // because a window showing a blank page looks identical to a working one.
  useEffect(() => {
    if (onboarded === null || !languageReady) return;
    // The wizard reports its own steps, and knows when one of them is still
    // loading. Announcing "welcome" from here marked the window ready before the
    // first-run dependency check had answered, so a smoke run could exit with the
    // panel still saying "Checking...".
    if (!onboarded) return;
    void uiReady(current);
    void uiTiming("shell_ready", pageMs());
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
    // The shell paints immediately: the navigation and the frame appear at once, and
    // only the page area is a placeholder. Waiting on two IPC calls before drawing
    // anything was the largest avoidable part of the first-screen delay.
    return (
      <div className="app">
        <nav>
          <div className="brand">Devchievements</div>
        </nav>
        <main className="shell">
          <PageSkeleton />
        </main>
      </div>
    );
  }
  if (!onboarded) {
    return (
      <Suspense fallback={<PageSkeleton />}>
        <Welcome initialStep={requestedStep() ?? "welcome"} onFinish={() => setOnboarded(true)} />
      </Suspense>
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
        <Suspense fallback={<PageSkeleton />}>
          <Page />
        </Suspense>
      </main>
    </div>
  );
}
