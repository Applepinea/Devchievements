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

export default function Popup() {
  const [payload] = useState<Payload | null>(readPayload);

  useEffect(() => {
    // The Rust side closes the window; this only marks it as drawn.
    document.documentElement.dataset.rendered = "true";
  }, []);

  if (!payload) return <div className="popup">Achievement unlocked</div>;

  return (
    <div className={"popup rarity-" + payload.rarity.toLowerCase()}>
      <div className="popup-badge">unlocked</div>
      <div className="popup-title">{payload.title}</div>
      <div className="popup-body">{payload.body}</div>
      {payload.others.length > 0 ? (
        <div className="popup-others">+{payload.others.length} more</div>
      ) : null}
    </div>
  );
}
