import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import Popup from "./pages/Popup";
import { invoke } from "./ipc";
import "./styles.css";

// A renderer exception otherwise leaves a blank window with no explanation: the
// webview console is not something a user can open, and the Rust side only knows
// that no view was ever reported ready. Reporting the message turns "the UI does
// not load" into a line in companion.log.
function report(message: string) {
  invoke("ui_error", { message }).catch(() => undefined);
}
window.addEventListener("error", (event) => {
  report(event.error instanceof Error ? event.error.stack ?? event.error.message : event.message);
});
window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason;
  report(reason instanceof Error ? reason.stack ?? reason.message : String(reason));
});

const root = document.getElementById("root");
if (!root) throw new Error("#root is missing from index.html");

// Notifications are a separate window with their own layout, selected by the hash
// the Rust side puts in the URL.
const isPopup = window.location.hash.startsWith("#popup");

createRoot(root).render(
  <StrictMode>{isPopup ? <Popup /> : <App />}</StrictMode>,
);
