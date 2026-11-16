import { useEffect, useState } from "react";
import { uiReady } from "./ipc";
import Dashboard from "./pages/Dashboard";
import Gallery from "./pages/Gallery";
import Activity from "./pages/Activity";
import Repositories from "./pages/Repositories";
import About from "./pages/About";
import Settings from "./pages/Settings";

const PAGES = [
  { id: "dashboard", label: "Dashboard", component: Dashboard },
  { id: "gallery", label: "Achievements", component: Gallery },
  { id: "activity", label: "Activity", component: Activity },
  { id: "repositories", label: "Repositories", component: Repositories },
  { id: "settings", label: "Settings", component: Settings },
  { id: "about", label: "About", component: About },
] as const;

type PageId = (typeof PAGES)[number]["id"];

export default function App() {
  // The hash selects the initial page, so a smoke test can open any page directly
  // instead of driving the nav with synthetic clicks.
  const initial = (): PageId => {
    const raw = window.location.hash.replace(/^#/, "");
    const found = PAGES.find((p) => p.id === raw);
    return found ? found.id : "dashboard";
  };
  const [current, setCurrent] = useState<PageId>(initial);

  // Tell the Rust side which page rendered. The window smoke test waits for this,
  // because a window showing a blank page is otherwise indistinguishable from a
  // working one.
  useEffect(() => {
    void uiReady(current);
  }, [current]);

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
