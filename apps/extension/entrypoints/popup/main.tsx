/** Mounting entry for the popup. The component lives in App.tsx so this file stays a
 * two-line entry and cannot be mistaken for the UI itself. */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App.tsx";

const root = document.getElementById("root");
if (!root) throw new Error("#root is missing from the popup document");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
