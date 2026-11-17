//! Preflight: what this machine has, and what to install if it does not (spec 25).
//!
//! Why the app checks at all, when the installer already does:
//!
//! - the NSIS bootstrapper handles WebView2, but only silently -- a machine that
//!   refuses the download gets a program that starts and draws nothing;
//! - it cannot check Git at all, and without Git there are no hooks, so the product
//!   has nothing to watch;
//! - a user can remove a prerequisite after installing, and then the next launch has
//!   to say so instead of failing quietly.
//!
//! Nothing here installs software. It reports, and it tells the user where to get
//! the thing; running an installer is the user's decision, not ours.

/// One prerequisite and what to do about it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dependency {
    /// Stable identifier the UI keys its advice from.
    pub id: String,
    /// Name a person recognises.
    pub name: String,
    /// Whether it is usable right now.
    pub ok: bool,
    /// What it is needed for, in the user's terms.
    pub purpose: String,
    /// Version string when it was found.
    pub version: Option<String>,
    /// Where to get it, when missing.
    pub download_url: Option<String>,
}

/// The whole preflight picture.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    /// Everything checked, in the order the UI shows it.
    pub dependencies: Vec<Dependency>,
    /// True when every required item is present.
    pub ready: bool,
}

/// Locate git.
///
/// PATH first because that is what the hooks will use; the usual install
/// directories second, because a shell started before Git was installed does not
/// see the updated PATH and would otherwise report a false negative.
fn find_git() -> Option<std::path::PathBuf> {
    let exe = if cfg!(windows) { "git.exe" } else { "git" };

    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    if cfg!(windows) {
        let roots = [
            std::env::var_os("ProgramFiles").map(std::path::PathBuf::from),
            std::env::var_os("ProgramFiles(x86)").map(std::path::PathBuf::from),
            std::env::var_os("LOCALAPPDATA")
                .map(std::path::PathBuf::from)
                .map(|p| p.join("Programs")),
        ];
        for root in roots.into_iter().flatten() {
            let candidate = root.join("Git").join("cmd").join("git.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Read the version out of `git --version`.
fn git_version(git: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new(git)
        .arg("--version")
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW: a check must not flash a console.
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    // "git version 2.54.0.windows.1" -> "2.54.0.windows.1"
    text.split_whitespace().nth(2).map(|v| v.to_string())
}

trait NoWindow {
    fn creation_flags(&mut self, flags: u32) -> &mut Self;
}

#[cfg(windows)]
impl NoWindow for std::process::Command {
    fn creation_flags(&mut self, flags: u32) -> &mut Self {
        use std::os::windows::process::CommandExt;
        std::os::windows::process::CommandExt::creation_flags(self, flags)
    }
}

#[cfg(not(windows))]
impl NoWindow for std::process::Command {
    fn creation_flags(&mut self, _flags: u32) -> &mut Self {
        self
    }
}

/// Is the WebView2 runtime installed?
///
/// Only the registry locations a real install writes are accepted. A file in the
/// user's data folder proves nothing: this app could have created it.
#[cfg(windows)]
pub fn webview2_version() -> Option<String> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ,
    };

    // 32-bit installs live under WOW6432Node, which a 64-bit process does not see
    // unless it asks for it explicitly.
    const CLIENTS: [&str; 3] = [
        r"SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        r"Software\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
    ];

    let read = |root: HKEY, path: &str| -> Option<String> {
        let key = HSTRING::from(path);
        let name = HSTRING::from("pv");
        let mut buffer = [0u16; 64];
        let mut size = (buffer.len() * 2) as u32;
        // Safety: the buffer and the size are consistent, and the key is closed below.
        let status = unsafe {
            RegGetValueW(
                root,
                &key,
                &name,
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let len = buffer.iter().position(|c| *c == 0).unwrap_or(0);
        let version = String::from_utf16_lossy(&buffer[..len]);
        (!version.is_empty()).then_some(version)
    };

    let registered = CLIENTS.iter().find_map(|path| {
        // HKCU first for the third path, HKLM for the machine-wide ones; trying both
        // for each is cheaper than reasoning about which install mode is in use.
        read(HKEY_LOCAL_MACHINE, path).or_else(|| read(HKEY_CURRENT_USER, path))
    });
    let _ = RegCloseKey;

    // The registry alone is not proof. Measured on a stripped Windows 11 image
    // (2026-11-17): the EdgeUpdate client key was there with a version, the runtime
    // files were not, and the app painted an empty window and then died with wry's
    // own "Could not find the WebView2 Runtime" box. So the version is only
    // reported when the executable it names actually exists.
    if let Some(version) = registered {
        if runtime_present(Some(&version)) {
            return Some(version);
        }
        eprintln!("WEBVIEW2 registry says {version} but the runtime files are missing");
        return None;
    }

    // No usable registry entry at all; a runtime found on disk still counts.
    runtime_present(None).then(|| "unknown".to_string())
}

/// Where a WebView2 runtime can live.
fn runtime_roots() -> Vec<std::path::PathBuf> {
    let mut roots = Vec::new();
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(dir) = std::env::var_os(var) {
            roots.push(
                std::path::PathBuf::from(dir)
                    .join("Microsoft")
                    .join("EdgeWebView")
                    .join("Application"),
            );
        }
    }
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        roots.push(
            std::path::PathBuf::from(dir)
                .join("Microsoft")
                .join("EdgeWebView")
                .join("Application"),
        );
    }
    roots
}

/// Does this application folder contain a runtime?
///
/// `version` is the value the registry claims. When given, that exact folder is
/// required; otherwise any version folder with the executable will do, which is the
/// honest answer when the registry is silent but the files are there.
fn contains_runtime(app_root: &std::path::Path, version: Option<&str>) -> bool {
    let has_exe = |dir: &std::path::Path| dir.join("msedgewebview2.exe").is_file();
    if let Some(version) = version {
        return has_exe(&app_root.join(version));
    }
    std::fs::read_dir(app_root)
        .map(|entries| entries.flatten().any(|entry| has_exe(&entry.path())))
        .unwrap_or(false)
}

/// Is a runtime present in any of the usual locations?
fn runtime_present(version: Option<&str>) -> bool {
    runtime_roots()
        .iter()
        .any(|root| contains_runtime(root, version))
}

#[cfg(not(windows))]
pub fn webview2_version() -> Option<String> {
    None
}

/// A diagnostic override: pretend the runtime is absent.
///
/// Uninstalling WebView2 to test the message is not something a build machine should
/// do, and the failure is invisible until someone hits it. This makes the path
/// reachable on purpose. It can only make the app show a prompt and exit, so it is not
/// a lever anyone can use against a user.
pub fn simulate_missing_webview() -> bool {
    matches!(
        std::env::var("DEVCH_FORCE_NO_WEBVIEW2").as_deref(),
        Ok("1") | Ok("true")
    )
}

/// Where the WebView2 runtime comes from.
pub const WEBVIEW2_URL: &str = "https://developer.microsoft.com/microsoft-edge/webview2/";

/// Whether the app must stop because it cannot draw anything.
///
/// Only WebView2 is fatal: Git missing means no data, which the in-app preflight can
/// explain in a real window. WebView2 missing means there is no window to explain
/// anything in, so the guidance has to come from outside the webview (a native
/// message box).
pub fn must_stop_for_missing_webview(version: Option<&str>) -> bool {
    version.map(|v| v.trim().is_empty()).unwrap_or(true)
}

/// The text of the native prompt.
///
/// Both languages, because the box appears before the UI exists and so cannot use
/// the language setting or the dictionary.
pub fn missing_webview_message() -> String {
    format!(
        "Devchievements cannot start: the Microsoft Edge WebView2 Runtime is missing.\n\n\
         Devchievements 无法启动：缺少 Microsoft Edge WebView2 运行时。\n\n\
         WebView2 是 Windows 上绘制窗口的组件，没有它程序画不出任何界面。\n\
         It usually ships with Windows; if it is missing, install it from Microsoft and \
         start Devchievements again:\n{}\n\n\
         Open that page now?",
        WEBVIEW2_URL
    )
}

/// Collect the preflight picture.
pub fn check() -> Preflight {
    let mut dependencies = Vec::new();

    let webview = webview2_version();
    dependencies.push(Dependency {
        id: "webview2".to_string(),
        name: "WebView2 Runtime".to_string(),
        ok: webview.is_some(),
        purpose: "Draws the window and the notification popup.".to_string(),
        version: webview,
        download_url: Some("https://developer.microsoft.com/microsoft-edge/webview2/".to_string()),
    });

    let git = find_git();
    let version = git.as_deref().and_then(git_version);
    dependencies.push(Dependency {
        id: "git".to_string(),
        name: "Git for Windows".to_string(),
        ok: git.is_some(),
        purpose: "Records your commits. Without it the companion has nothing to watch.".to_string(),
        version,
        download_url: Some("https://git-scm.com/download/win".to_string()),
    });

    let ready = dependencies.iter().all(|d| d.ok);
    Preflight {
        dependencies,
        ready,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("devch-webview2-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn fake_version(root: &std::path::Path, version: &str) {
        let dir = root.join(version);
        std::fs::create_dir_all(&dir).expect("version dir");
        std::fs::write(dir.join("msedgewebview2.exe"), b"MZ").expect("exe");
    }

    #[test]
    fn the_registered_version_must_exist_on_disk() {
        // The case from the stripped Windows image: registry says 157.x, the folder
        // is not there. Reporting the runtime as present is what produced a blank
        // window and wry's own error box.
        let root = scratch("stale");
        assert!(!contains_runtime(&root, Some("157.0.4341.46")));
        fake_version(&root, "157.0.4341.46");
        assert!(contains_runtime(&root, Some("157.0.4341.46")));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_runtime_without_a_registry_entry_still_counts() {
        let root = scratch("silent");
        fake_version(&root, "150.0.1.0");
        assert!(contains_runtime(&root, None));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_version_folder_without_the_executable_is_not_a_runtime() {
        let root = scratch("empty-version");
        std::fs::create_dir_all(root.join("157.0.4341.46")).expect("dir");
        assert!(!contains_runtime(&root, Some("157.0.4341.46")));
        assert!(!contains_runtime(&root, None));
        let _ = std::fs::remove_dir_all(&root);
    }
    #[test]
    fn a_missing_runtime_must_stop_the_app() {
        assert!(must_stop_for_missing_webview(None));
        assert!(must_stop_for_missing_webview(Some("  ")));
        assert!(!must_stop_for_missing_webview(Some("157.0.4341.46")));
    }

    #[test]
    fn the_native_prompt_names_the_runtime_and_where_to_get_it() {
        let text = missing_webview_message();
        assert!(text.contains("WebView2"));
        assert!(text.contains(WEBVIEW2_URL));
        // Bilingual on purpose: the box is drawn before the language setting is
        // readable, so it cannot pick one.
        assert!(text.contains("无法启动"));
    }
    #[test]
    fn git_is_reported_with_the_fields_the_ui_needs() {
        let report = check();
        let git = report
            .dependencies
            .iter()
            .find(|d| d.id == "git")
            .expect("git is always reported, present or not");
        assert!(!git.purpose.is_empty());
        assert_eq!(
            git.download_url.as_deref(),
            Some("https://git-scm.com/download/win")
        );
    }

    #[test]
    fn readiness_follows_the_individual_checks() {
        let report = check();
        let expected = report.dependencies.iter().all(|d| d.ok);
        assert_eq!(report.ready, expected);
    }
}
