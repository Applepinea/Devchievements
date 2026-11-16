// Small formatting helpers, so every page renders numbers the same way.

export function bytes(value: number): string {
  if (value < 1024) return value + " B";
  if (value < 1024 * 1024) return (value / 1024).toFixed(1) + " KB";
  return (value / (1024 * 1024)).toFixed(1) + " MB";
}

export function dateTime(ms: number | null): string {
  if (ms === null) return "-";
  return new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export function channel(name: string): string {
  return name.replace(/_/g, " ");
}
