// Typed access to the Rust commands.
//
// Why not @tauri-apps/api: this window only needs `invoke`, and pulling the SDK in
// would add a dependency for one function. The internals object is stable in Tauri 2
// and is what the SDK itself calls.

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
export const uiReady = (view: string) => invoke<void>("ui_ready", { view });
