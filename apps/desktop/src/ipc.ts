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

export interface RepeatableTrigger {
  id: number;
  triggeringEventId: string;
  unlockedAt: number;
  ruleVersion: number;
  repository: string;
}

export interface RepeatableAchievementItem {
  id: string;
  name: string;
  description: string;
  rarity: string;
  triggerCount: number;
  events: RepeatableTrigger[];
}

export interface DailyRankView {
  localDate: string;
  commits: number;
  rank: string | null;
  nextThreshold: number | null;
  nextRank: string | null;
}

export interface RepositoryEntry {
  alias: string;
  commits: number;
  /** Merged pull requests the browser extension observed (spec 2.3). */
  pullRequests: number;
  /** Releases the browser extension observed. */
  releases: number;
  firstSeenAt: number;
  updatedAt: number;
}

/** Browser reports parked because the GitHub account is not linked (spec 7). */
export interface UnlinkedLogin {
  login: string;
  reports: number;
  lastReceivedAt: number;
}

export const getUnlinkedGithubLogins = () =>
  invoke<UnlinkedLogin[]>("get_unlinked_github_logins");

/**
 * Link a GitHub account to the configured email and replay what it sent.
 *
 * Returns how many parked reports were replayed, which is the only honest way for
 * the UI to say "3 reports are being counted now" instead of "saved".
 */
export const linkGithubLogin = (login: string) =>
  invoke<number>("link_github_login", { login });

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

/** One GitHub login mapped to the emails it commits with (spec 7). */
export interface LoginMapping {
  login: string;
  emails: string[];
}

/** Popup appearance (spec 21). */
export interface PopupStyle {
  width: number;
  height: number | null;
  scalePercent: number;
  iconSize: number;
  cornerRadius: number;
  screenMargin: number;
  corner: "top_left" | "top_right" | "bottom_left" | "bottom_right";
  opacityPercent: number;
  displaySeconds: number;
  animationMs: number;
  backgroundBlur: number;
}

/** How a repository selector treats its list (spec 15). */
export type RepositorySelection =
  | { mode: "all" }
  | { mode: "only"; ids: string[] }
  | { mode: "except"; ids: string[] };

/** User event filters (spec 15). */
export interface FilterSet {
  repositories: RepositorySelection;
  author_emails: string[];
  include_merges: boolean;
  include_imported: boolean;
  allowed_confidence: string[];
  time_range: { from: number; to: number } | null;
  event_types: string[];
}

export interface SettingsView {
  emails: string[];
  privacy: "normal" | "privacy";
  timezone: string | null;
  dataDir: string;
  notifications: boolean;
  volume: number;
  muted: boolean;
  runMode: string;
  /** Whether a normal launch stays in the tray instead of opening the window. */
  startHidden: boolean;
  /** Whether Windows starts the companion at sign-in. */
  startWithWindows: boolean;
  /** UI language setting: system, en or zh-CN. */
  language: string;
  databaseBytes: number;
  githubLogins: LoginMapping[];
  filters: FilterSet;
  popup: PopupStyle;
  /**
   * What had to be dropped when the settings file was read.
   *
   * `null` means the file parsed cleanly. A string means fields the user DID write
   * were ignored, so the page shows a banner instead of silently displaying defaults.
   */
  loadWarning: string | null;
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

/**
 * Record one start-up stage. Fire-and-forget on purpose: measuring must never be
 * able to delay the thing being measured.
 */
export const uiTiming = (stage: string, sincePageMs: number) =>
  invoke<void>("ui_timing", { stage, sincePageMs });

/**
 * Milliseconds since this document started loading. `performance.now()` counts from
 * navigation start, which is the closest a page can get to "window created".
 */
export const pageMs = () => performance.now();

/** What the first screen needs: language and whether setup is done. */
export interface BootView {
  language: string;
  onboardingCompleted: boolean;
}

export const bootStatus = () => invoke<BootView>("boot_status");

export const getSettings = () => invoke<SettingsView>("get_settings");
export const saveSettings = (settings: SettingsView) =>
  invoke<void>("save_settings", { settings });
export const getStatus = () => invoke<StatusView>("get_status");
export const getDashboard = () => invoke<DashboardView>("get_dashboard");
/** Diagnostics block, fetched after the cards are on screen. */
export const getStorageHealth = () => invoke<StorageHealth>("get_storage_health");
export const getGallery = () => invoke<GalleryItem[]>("get_gallery");
export const getRepeatableAchievements = () =>
  invoke<RepeatableAchievementItem[]>("get_repeatable_achievements");
export const getDailyRank = () => invoke<DailyRankView>("get_daily_rank");
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
export interface OnboardingStatus {
  completed: boolean;
  suggestedEmails: string[];
  gitHasIdentity: boolean;
  helperAvailable: boolean;
}

export interface HookInstallResult {
  hook: string;
  outcome: string;
}

export const onboardingStatus = () =>
  invoke<OnboardingStatus>("onboarding_status");
export const installHooks = (repoPath: string) =>
  invoke<HookInstallResult[]>("install_hooks", { repoPath });
/** Clears the setup flag so the wizard runs again; settings and data are untouched. */
export const resetOnboarding = () => invoke<void>("reset_onboarding");
export const completeOnboarding = () => invoke<void>("complete_onboarding");
export const showPreview = () => invoke<void>("show_popup_preview");
export const openDataFolder = () => invoke<void>("open_data_folder");
export const clearData = () => invoke<void>("clear_data");
export interface StoredAsset {
  name: string;
  format: string;
  bytes: number;
}

export const listAssets = () => invoke<StoredAsset[]>("list_assets");
export const importAsset = (path: string) => invoke<StoredAsset>("import_asset", { path });
export const removeAsset = (name: string) => invoke<void>("remove_asset", { name });
export const importSound = (path: string) => invoke<StoredAsset>("import_sound", { path });
export const availableSounds = () => invoke<string[]>("available_sounds");

/** What a sound preview played (spec 19). */
export interface SoundPreview {
  /** The sound id that was chosen. */
  sound: string;
  /** Which rule chose it: BuiltIn, RarityCustom, GlobalCustom, ... */
  source: string;
  /** Whether anything reached the audio device. */
  played: boolean;
  /** Why nothing was played, when nothing was. */
  reason: string | null;
}

/**
 * Play the sound the current settings select for one slot.
 *
 * Read-only on the settings side: it plays what a real notification would play, so the
 * user can hear the difference between slots before a commit happens.
 */
export const previewSound = (slot: string) =>
  invoke<SoundPreview>("preview_sound", { slot });

/** The slots the page offers, so the list is not duplicated in two languages. */
export const previewSoundSlots = () => invoke<string[]>("preview_sound_slots");
export const uiReady = (view: string) => invoke<void>("ui_ready", { view });

/** One prerequisite, as the Rust preflight reports it. */
export interface Dependency {
  id: string;
  name: string;
  ok: boolean;
  purpose: string;
  version: string | null;
  downloadUrl: string | null;
}

export interface DependencyReport {
  dependencies: Dependency[];
  ready: boolean;
}

/** Checks WebView2 and Git without installing anything (spec 25). */
export const checkDependencies = () => invoke<DependencyReport>("check_dependencies");

/**
 * Writes a diagnostics document into the data directory.
 *
 * `raw` keeps real repository names and paths. Spec 26.2 allows that only as an
 * explicit choice, so it is a parameter rather than a setting.
 */
export const exportDiagnostics = (raw: boolean) =>
  invoke<{ path: string; raw: boolean }>("export_diagnostics", { raw });

/** Opens a URL in the default browser. Only http(s) is honoured, in Rust. */
export const openExternal = (url: string) => invoke<void>("open_external", { url });

/**
 * Reports the language the window actually rendered in, for logs and tests.
 *
 * `sample` is a translated word: it proves the dictionary was consulted, which the
 * setting alone cannot.
 */
export const uiLanguage = (lang: string, sample: string) =>
  invoke<void>("ui_language", { lang, sample });

/** One row of the theme list (spec 23). */
export interface ThemeSummary {
  name: string;
  /** The synthesised default: it cannot be renamed or deleted. */
  builtin: boolean;
}

export interface ThemeListView {
  themes: ThemeSummary[];
  /**
   * Which theme's style is in effect.
   *
   * `null` means the stored style matches no theme — a slider was moved — so the page
   * says "modified" instead of naming a theme that is not actually in effect.
   */
  active: string | null;
}

export const listThemes = () => invoke<ThemeListView>("list_themes");
export const applyTheme = (name: string) => invoke<void>("apply_theme", { name });
/** Returns the stored name, so the page can say which entry appeared. */
export const importTheme = (path: string) => invoke<string>("import_theme", { path });
/** Returns the path written. */
export const exportTheme = (path: string) => invoke<string>("export_theme", { path });
export const resetTheme = () => invoke<void>("reset_theme");
export const removeTheme = (name: string) => invoke<void>("remove_theme", { name });

/** A user achievement rule (spec 22). Flat on the wire: see the builder crate. */
export type RuleExpr =
  | { op: "condition"; metric: UserMetric; at_least: number }
  | { op: "all"; children: RuleExpr[] }
  | { op: "any"; children: RuleExpr[] };

export type UserMetric =
  | "commits"
  | "pull_requests_merged"
  | "releases"
  | "streak_days";

export interface UserAchievement {
  id: string;
  name: string;
  description: string;
  rule: RuleExpr;
  version: number;
}

/**
 * Store a user achievement definition.
 *
 * The definition travels as JSON text so the page cannot hand Rust a pre-parsed rule
 * that skipped `definition::validate`; the same entry point the CLI uses is the one
 * the form uses.
 */
export const addAchievement = (definition: unknown) =>
  invoke<string>("add_achievement", { definition: JSON.stringify(definition) });

/** Remove one user achievement. */
export const removeAchievement = (id: string) => invoke<void>("remove_achievement", { id });

/** The stored user achievements. */
export const listAchievements = () => invoke<UserAchievement[]>("list_user_achievements");

/** Built-in achievement ids and names, for the "this name is taken" message. */
export interface BuiltinAchievementSummary {
  id: string;
  name: string;
  description: string;
  rarity: string;
  hidden: boolean;
}

export const listBuiltinAchievements = () =>
  invoke<BuiltinAchievementSummary[]>("list_builtin_achievements");
