import { useEffect, useState } from "react";
import { onboardingStatus, uiReady } from "./ipc";
import Dashboard from "./pages/Dashboard";
import Gallery from "./pages/Gallery";
import Activity from "./pages/Activity";
import Repositories from "./pages/Repositories";
import About from "./pages/About";
import Settings from "./pages/Settings";
import Welcome from "./pages/Welcome";

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
  const [current, setCurrent] = useState<PageId>(initialPage);

  // Spec 25: the wizard runs before the pages, and only once.
  useEffect(() => {
    onboardingStatus().then(
      (s) => setOnboarded(s.completed),
      // If the status cannot be read the app must still open; the wizard is
      // helpful, not mandatory.
      () => setOnboarded(true),
    );
  }, []);

  // Tell the Rust side which view rendered. The window smoke test waits for this,
  // because a window showing a blank page looks identical to a working one.
  useEffect(() => {
    // Wait for the status before reporting: while it is unknown the app does not
    // yet know whether this is a first run, and reporting early would claim the
    // dashboard rendered on a machine that is about to show the wizard.
    if (onboarded === null) return;
    void uiReady(onboarded === false ? "welcome" : current);
  }, [onboarded, current]);

  if (onboarded === null) return <p className="muted">Starting...</p>;
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
            {p.label}
          </button>
        ))}
      </nav>
      <main className="shell">
        <Page />
      </main>
    </div>
  );
}
