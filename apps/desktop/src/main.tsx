import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import Popup from "./pages/Popup";
import "./styles.css";

const root = document.getElementById("root");
if (!root) throw new Error("#root is missing from index.html");

// Notifications are a separate window with their own layout, selected by the hash
// the Rust side puts in the URL.
const isPopup = window.location.hash.startsWith("#popup");

createRoot(root).render(
  <StrictMode>{isPopup ? <Popup /> : <App />}</StrictMode>,
);
