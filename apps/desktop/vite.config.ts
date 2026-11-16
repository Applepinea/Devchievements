import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Why these settings:
// - the bundle is loaded from the Tauri asset protocol, so every URL must be
//   relative (base: './'), otherwise the webview resolves paths against the
//   protocol root and nothing loads;
// - target is the WebView2 runtime's Chromium, so no legacy transpilation.
export default defineConfig({
  plugins: [react()],
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "chrome120",
    sourcemap: false,
  },
});
