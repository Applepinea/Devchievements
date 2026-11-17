import { useEffect, useState, type CSSProperties } from "react";
import { invoke } from "../ipc";
import { t } from "../i18n";

interface Payload {
  title: string;
  body: string;
  rarity: string;
  others: string[];
  /** Custom icon for this achievement, resolved by the app. */
  iconUrl?: string | null;
  /** Appearance values, filled by the app from the saved style. */
  style?: PopupRender | null;
}

interface PopupRender {
  radius: number;
  iconSize: number;
  opacity: number;
  animationMs: number;
  blur: number;
}

// The payload travels in the URL fragment so the popup can draw on its first frame.
// A popup that appears empty and then fills in reads as a glitch.
function readPayload(): Payload | null {
  const hash = window.location.hash.replace(/^#/, "");
  const query = hash.includes("?") ? hash.slice(hash.indexOf("?") + 1) : "";
  const params = new URLSearchParams(query);
  const raw = params.get("data");
  if (!raw) return null;
  try {
    return JSON.parse(decodeURIComponent(raw)) as Payload;
  } catch {
    return null;
  }
}

// A monogram instead of an icon: the product has no artwork per achievement, and
// inventing one would be inventing data. The letter is derived from the name.
function monogram(title: string): string {
  const first = title.trim().charAt(0);
  return first ? first.toUpperCase() : "?";
}

export default function Popup() {
  const [payload] = useState<Payload | null>(readPayload);

  useEffect(() => {
    document.documentElement.dataset.rendered = "true";
    // Spec 44.4 ends the latency budget at the first rendered frame, and only the
    // page knows when that happened. Two frames: the first confirms the commit,
    // the second that a frame containing it has been painted.
    let cancelled = false;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (cancelled) return;
        // A failure here costs a measurement, never the popup itself.
        invoke("popup_painted").catch(() => undefined);
      });
    });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!payload) return <div className="popup" />;

  // Inline custom properties rather than a stylesheet rewrite: the values change per
  // notification, and a class-based approach would need one class per combination.
  const style = payload.style;
  const cssVars = style
    ? ({
        "--popup-radius": style.radius + "px",
        "--popup-icon": style.iconSize + "px",
        "--popup-opacity": String(style.opacity / 100),
        "--popup-anim": style.animationMs + "ms",
        "--popup-blur": style.blur + "px",
      } as CSSProperties)
    : undefined;

  return (
    <div className="popup" style={cssVars}>
      <div className="popup-icon">
        {payload.iconUrl ? (
          // The image is decorative: the achievement name carries the meaning, so
          // a failed load degrades silently rather than showing a broken icon.
          <img
            className="popup-image"
            src={payload.iconUrl}
            alt=""
            onError={(e) => {
              (e.currentTarget as HTMLImageElement).style.display = "none";
            }}
          />
        ) : (
          <span className="popup-monogram">{monogram(payload.title)}</span>
        )}
      </div>
      <div className="popup-text">
        <div className="popup-label">{t("Achievement unlocked")}</div>
        <div className="popup-title">{payload.title}</div>
        <div className="popup-body">
          {payload.others.length > 0
            ? payload.body + " (+" + payload.others.length + ")"
            : payload.body}
        </div>
      </div>
    </div>
  );
}
