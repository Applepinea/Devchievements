// Typed access to the Rust commands.
//
// Why not @tauri-apps/api: this window only needs `invoke`, and pulling the SDK in
// would add a dependency for one function. The internals object is stable in Tauri 2
// and is what the SDK itself calls.

export interface Statistics {
  events: number;
  commits: number;
  repositories: number;
  achievements: number;
  unviewedAchievements: number;
  longestStreak: number;
  activeDays: number;
}

export interface ActivityDay {
  localDate: string;
  commits: number;
  rank: string | null;
}

export interface StorageHealth {
  path: string;
  bytes: number;
  schemaVersion: number;
  oldestEventAt: number | null;
}

export interface DashboardView {
  statistics: Statistics;
  recent: ActivityDay[];
  storage: StorageHealth;
  channel: string;
  systemNotifications: boolean;
  version: string;
}

export interface GalleryItem {
  id: string;
  name: string;
  description: string;
  rarity: string;
  hidden: boolean;
  unlocked: boolean;
  unlockedAt: number | null;
  unlockSource: string | null;
  viewed: boolean;
}

export interface RepositoryEntry {
  alias: string;
  commits: number;
  firstSeenAt: number;
  updatedAt: number;
}

export interface LatencyBuckets {
  under100ms: number;
  under1s: number;
  under10s: number;
  under1min: number;
  over1min: number;
}

export interface OutboxHealth {
  total: number;
  pending: number;
  oldestPendingMs: number | null;
  reasons: { reason: string; count: number }[];
  latency: {
    samples: number;
    p50Ms: number;
    p90Ms: number;
    p99Ms: number;
    maxMs: number;
    buckets: LatencyBuckets;
  };
}

export interface Capabilities {
  desktopPopup: string;
  systemNotification: string;
  tray: string;
  doNotDisturbDetection: string;
  multiMonitor: string;
  dpiScaling: string;
  wasm: string;
}

export interface AboutView {
  version: string;
  storage: StorageHealth;
  capabilities: Capabilities;
  channel: string;
  privacyDisclaimer: string;
  sounds: string[];
  outbox: OutboxHealth;
  needsExtension: string[];
}

export interface SettingsView {
  emails: string[];
  privacy: "normal" | "privacy";
  timezone: string | null;
  dataDir: string;
}

export interface StatusView {
  events: number;
  repositories: number;
  unlockedAchievements: number;
  notificationsNeverShown: number;
  pendingNotifications: number;
  channel: "desktop_popup" | "system_notification" | "in_app_only";
  toastReady: boolean;
  sounds: string[];
}

interface Internals {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
}

function internals(): Internals {
  const w = window as unknown as { __TAURI_INTERNALS__?: Internals };
  const api = w.__TAURI_INTERNALS__;
  if (!api) {
    throw new Error("Tauri IPC is unavailable: this page must run inside the app");
  }
  return api;
}

export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  return internals().invoke<T>(command, args);
}

export const getSettings = () => invoke<SettingsView>("get_settings");
export const saveSettings = (settings: SettingsView) =>
  invoke<void>("save_settings", { settings });
export const getStatus = () => invoke<StatusView>("get_status");
export const getDashboard = () => invoke<DashboardView>("get_dashboard");
export const getGallery = () => invoke<GalleryItem[]>("get_gallery");
export const markViewed = (ids: string[]) =>
  invoke<number>("mark_achievements_viewed", { ids });
export const getActivity = (limit?: number) =>
  invoke<ActivityDay[]>("get_activity", { limit });
export const getRepositories = () => invoke<RepositoryEntry[]>("get_repositories");
export const getAbout = () => invoke<AboutView>("get_about");
export const backupDatabase = () =>
  invoke<{ path: string; bytes: number }>("backup_database");
export const restoreDatabase = (path: string) => invoke<void>("restore_database", { path });
export interface ImportReport {
  scanned: number;
  matched: number;
  ingested: number;
  duplicates: number;
  refused: number;
  batches: number;
  cancelled: boolean;
}

export const importHistory = (path: string) =>
  invoke<ImportReport>("import_history", { path });
export const uiReady = (view: string) => invoke<void>("ui_ready", { view });
