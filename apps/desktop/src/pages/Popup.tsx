import { useEffect, useState } from "react";

interface Payload {
  title: string;
  body: string;
  rarity: string;
  others: string[];
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
  }, []);

  if (!payload) return <div className="popup" />;

  return (
    <div className="popup">
      <div className="popup-icon">
        <span className="popup-monogram">{monogram(payload.title)}</span>
      </div>
      <div className="popup-text">
        <div className="popup-label">成就已解锁</div>
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
