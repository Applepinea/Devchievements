// Small formatting helpers, so every page renders numbers the same way.

export function bytes(value: number): string {
  if (value < 1024) return value + " B";
  if (value < 1024 * 1024) return (value / 1024).toFixed(1) + " KB";
  return (value / (1024 * 1024)).toFixed(1) + " MB";
}

export function dateTime(ms: number | null | undefined): string {
  // A missing value must never reach the user as "Invalid Date": that is what the
  // dashboard showed when the wire format and the TypeScript types disagreed, and it
  // looked like a crash rather than a missing field.
  if (ms === null || ms === undefined || !Number.isFinite(ms)) return "-";
  const date = new Date(ms);
  if (Number.isNaN(date.getTime())) return "-";
  return date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export function channel(name: string): string {
  return name.replace(/_/g, " ");
}
