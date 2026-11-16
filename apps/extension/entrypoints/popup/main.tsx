import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

/**
 * Phase 0 popup.
 *
 * Its ONLY job is to report whether the wasm core loaded under the MV3 CSP.
 * Deliberately not a product UI: showing fake achievements here would violate the
 * "no static mock data presented as real" rule in AGENTS.md 10.
 */

type WasmStatus =
  | { state: "loading" }
  | { state: "ok"; bytes: number; instantiateMs: number }
  | { state: "error"; message: string };

function App() {
  const [status, setStatus] = useState<WasmStatus>({ state: "loading" });

  useEffect(() => {
    let cancelled = false;

    async function probe() {
      try {
        const url = chrome.runtime.getURL("/wasm/devchievements_core.wasm");
        const t0 = performance.now();
        const res = await fetch(url);
        if (!res.ok) throw new Error(`fetch failed: ${res.status}`);
        const bytes = await res.arrayBuffer();
        // This line is the real test: MV3 without 'wasm-unsafe-eval' throws here.
        const { instance } = await WebAssembly.instantiate(bytes, {});
        const ms = performance.now() - t0;
        if (!cancelled) {
          setStatus({ state: "ok", bytes: bytes.byteLength, instantiateMs: ms });
        }
        void instance;
      } catch (e) {
        if (!cancelled) {
          setStatus({ state: "error", message: e instanceof Error ? e.message : String(e) });
        }
      }
    }

    void probe();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <main style={{ fontFamily: "system-ui, sans-serif", padding: 16, minWidth: 320 }}>
      <h1 style={{ fontSize: 16, margin: "0 0 8px" }}>Devchievements</h1>
      <p style={{ fontSize: 12, color: "#666", margin: "0 0 16px" }}>
        Phase 0 — technical validation build. No achievements yet.
      </p>
      <section style={{ fontSize: 13 }}>
        <strong>WASM core</strong>
        <div style={{ marginTop: 6 }}>
          {status.state === "loading" && "loading…"}
          {status.state === "ok" &&
            `loaded ${status.bytes} bytes, instantiated in ${status.instantiateMs.toFixed(1)} ms`}
          {status.state === "error" && `failed: ${status.message}`}
        </div>
      </section>
    </main>
  );
}

const container = document.getElementById("root");
if (container) {
  createRoot(container).render(
    <StrictMode>
      <App />
    </StrictMode>
  );
}
