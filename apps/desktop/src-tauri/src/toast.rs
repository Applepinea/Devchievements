//! Windows toast notifications for an unpackaged app (spec 16.4, 16.6, 28).
//!
//! # The problem this module exists to solve
//!
//! An unpackaged Win32 process has no package identity, so the toast APIs refuse it
//! with 0x80073D54 ("this process has no package identifier"). That is not a sandbox
//! quirk; it is how every non-MSIX Windows app behaves.
//!
//! # What actually grants the capability
//!
//! Two pieces, and BOTH were verified here by removing them one at a time:
//!
//! 1. a Start Menu shortcut whose System.AppUserModel.ID property is our AUMID -- this
//!    is what gives the process an identity the notification platform trusts;
//! 2. a registry key under HKCU\Software\Classes\AppUserModelId supplying the display
//!    name shown in Settings.
//!
//! Measured results (see docs/development/phase-2a-progress.md):
//!
//! - registry key only                      -> 0x80073D54
//! - SetCurrentProcessExplicitAppUserModelID only -> 0x80073D54
//! - key + shortcut + explicit process id    -> toast shown
//!
//! # Failure policy
//!
//! A toast is a side effect. If it fails, ingestion continues: the achievement is
//! already committed and the gallery shows it regardless (spec 17.9). Nothing here
//! panics, and every entry point returns a Result the caller logs.

/// The identity Windows attributes our toasts to.
pub const APP_USER_MODEL_ID: &str = "Devchievements.Companion";

/// The display name shown to the user.
pub const DISPLAY_NAME: &str = "Devchievements";

/// Why a toast could not be prepared or shown.
#[derive(Debug)]
pub enum ToastError {
    /// The WinRT call failed.
    WinRt(String),
    /// The AppUserModelID could not be registered.
    Registration(String),
    /// The toast payload could not be built.
    Payload(String),
}

impl std::fmt::Display for ToastError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToastError::WinRt(m) => write!(f, "toast winrt error: {m}"),
            ToastError::Registration(m) => write!(f, "toast registration error: {m}"),
            ToastError::Payload(m) => write!(f, "toast payload error: {m}"),
        }
    }
}

impl std::error::Error for ToastError {}

/// Path of the Start Menu shortcut that registers the AppUserModelID.
pub fn shortcut_path() -> Option<std::path::PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    let mut path = std::path::PathBuf::from(appdata);
    path.push("Microsoft");
    path.push("Windows");
    path.push("Start Menu");
    path.push("Programs");
    path.push("Devchievements.lnk");
    Some(path)
}

/// Whether the registration shortcut is already present.
pub fn is_registered() -> bool {
    shortcut_path().is_some_and(|p| p.exists())
}

/// Whether Windows will accept our toasts, as observed without changing anything.
///
/// The app only OBSERVES. Registration is an install-time action (see
/// scripts/install-companion.ps1 and ADR-0015): an application that silently writes
/// to the Start Menu and to HKCU on every launch is indistinguishable from something
/// suspicious, and a normal user account may not be permitted to do it at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationState {
    /// Everything the notification platform needs is present.
    Ready,
    /// The registration shortcut is missing from the Start Menu.
    ShortcutMissing {
        /// Where it was expected.
        path: std::path::PathBuf,
    },
    /// APPDATA is unavailable, so the shortcut location cannot even be computed.
    NoAppData,
}

impl RegistrationState {
    /// Whether toasts can be attempted.
    pub fn is_ready(&self) -> bool {
        matches!(self, RegistrationState::Ready)
    }

    /// A one-line explanation for logs and the diagnostics view.
    pub fn describe(&self) -> String {
        match self {
            RegistrationState::Ready => "registered".to_string(),
            RegistrationState::ShortcutMissing { path } => {
                format!(
                    "shortcut missing: {} (run scripts/install-companion.ps1)",
                    path.display()
                )
            }
            RegistrationState::NoAppData => {
                "APPDATA is not set; cannot locate the Start Menu".to_string()
            }
        }
    }
}

/// Inspect registration without modifying anything.
pub fn registration_state() -> RegistrationState {
    match shortcut_path() {
        Some(path) if path.exists() => RegistrationState::Ready,
        Some(path) => RegistrationState::ShortcutMissing { path },
        None => RegistrationState::NoAppData,
    }
}

/// Ask the shell to notice that a Start Menu shortcut appeared.
///
/// Without this, the notification platform can keep using its cached app list until
/// the next sign-in, so a freshly installed shortcut appears to do nothing. The
/// installer calls this immediately after placing the shortcut.
#[cfg(windows)]
pub fn notify_shell() {
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};

    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

#[cfg(not(windows))]
pub fn notify_shell() {}

/// Claim the AppUserModelID for this process.
///
/// The registry key supplies only display metadata; the process must also advertise
/// the identity it is using, otherwise CreateToastNotifierWithId rejects it.
#[cfg(windows)]
fn set_process_identity() {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let _ = unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_USER_MODEL_ID)) };
}

#[cfg(not(windows))]
fn set_process_identity() {}

/// Create the registration shortcut at an explicit path.
///
/// Install-time only. The installer places the result in the Start Menu; the app never
/// does, because writing there is an installation concern (see ADR-0015).
///
/// Needs ShellLink + IPropertyStore, both COM. The PROPVARIANT is built by hand: the
/// AUMID must be VT_LPWSTR backed by CoTaskMemAlloc, and PropVariantClear frees the
/// original after the property store has copied it.
#[cfg(windows)]
pub fn create_shortcut(lnk: &std::path::Path, target: &std::path::Path) -> Result<(), ToastError> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    // PKEY_AppUserModel_ID = {9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}, pid 5.
    const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0x9F4C2855_9F79_4B39_A8D0_E1D42DE1D5F3),
        pid: 5,
    };

    unsafe {
        // RPC_E_CHANGED_MODE only means the caller already lives in an MTA, where COM
        // still works, so the result is deliberately ignored.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| ToastError::Registration(format!("CoCreateInstance(ShellLink): {e}")))?;

        let target_wide: Vec<u16> = target
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        link.SetPath(PCWSTR(target_wide.as_ptr()))
            .map_err(|e| ToastError::Registration(format!("IShellLink::SetPath: {e}")))?;

        let store: IPropertyStore = link
            .cast()
            .map_err(|e| ToastError::Registration(format!("cast IPropertyStore: {e}")))?;

        let wide: Vec<u16> = APP_USER_MODEL_ID.encode_utf16().collect();
        let mut pv: PROPVARIANT = std::mem::zeroed();
        {
            let bytes = (wide.len() + 1) * std::mem::size_of::<u16>();
            let raw = CoTaskMemAlloc(bytes) as *mut u16;
            if raw.is_null() {
                return Err(ToastError::Registration("CoTaskMemAlloc failed".into()));
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), raw, wide.len());
            *raw.add(wide.len()) = 0;

            let inner = &mut *pv.Anonymous.Anonymous;
            inner.vt = VT_LPWSTR;
            inner.Anonymous.pwszVal = windows::core::PWSTR(raw);
        }

        let set = store.SetValue(&PKEY_APP_USER_MODEL_ID, &pv);
        let _ = PropVariantClear(&mut pv);
        set.map_err(|e| ToastError::Registration(format!("SetValue(AppUserModel.ID): {e}")))?;

        store
            .Commit()
            .map_err(|e| ToastError::Registration(format!("IPropertyStore::Commit: {e}")))?;

        let persist: IPersistFile = link
            .cast()
            .map_err(|e| ToastError::Registration(format!("cast IPersistFile: {e}")))?;
        let lnk_wide: Vec<u16> = lnk
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        persist
            .Save(PCWSTR(lnk_wide.as_ptr()), false)
            .map_err(|e| ToastError::Registration(format!("IPersistFile::Save: {e}")))?;
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn create_shortcut(
    _lnk: &std::path::Path,
    _target: &std::path::Path,
) -> Result<(), ToastError> {
    Err(ToastError::Registration(
        "shortcuts are Windows-only".into(),
    ))
}

/// Show a toast.
///
/// tag is attached to the notification so it can be found again in the Action Center
/// history; the delivery layer passes the notification id.
pub fn show(title: &str, body: &str, tag: &str) -> Result<(), ToastError> {
    match registration_state() {
        RegistrationState::Ready => {}
        other => return Err(ToastError::Registration(other.describe())),
    }
    set_process_identity();
    show_registered(title, body, tag)
}

/// Show a toast as an explicit AppUserModelID.
///
/// Exists for diagnostics: probing a known-good identifier separates a broken
/// registration of ours from a process that cannot use toasts at all.
pub fn show_with_aumid(aumid: &str, title: &str, body: &str) -> Result<(), ToastError> {
    use windows::core::HSTRING;
    use windows::UI::Notifications::{
        ToastNotification, ToastNotificationManager, ToastTemplateType,
    };

    let xml = ToastNotificationManager::GetTemplateContent(ToastTemplateType::ToastText02)
        .map_err(|e| ToastError::WinRt(format!("GetTemplateContent: {e}")))?;
    let texts = xml
        .GetElementsByTagName(&HSTRING::from("text"))
        .map_err(|e| ToastError::WinRt(format!("GetElementsByTagName: {e}")))?;
    let title_node = xml
        .CreateTextNode(&HSTRING::from(title))
        .map_err(|e| ToastError::Payload(format!("title: {e}")))?;
    texts
        .Item(0)
        .and_then(|n| n.AppendChild(&title_node))
        .map_err(|e| ToastError::Payload(format!("append title: {e}")))?;
    let body_node = xml
        .CreateTextNode(&HSTRING::from(body))
        .map_err(|e| ToastError::Payload(format!("body: {e}")))?;
    texts
        .Item(1)
        .and_then(|n| n.AppendChild(&body_node))
        .map_err(|e| ToastError::Payload(format!("append body: {e}")))?;

    let toast = ToastNotification::CreateToastNotification(&xml)
        .map_err(|e| ToastError::WinRt(format!("CreateToastNotification: {e}")))?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(aumid))
        .map_err(|e| ToastError::WinRt(format!("CreateToastNotifierWithId({aumid}): {e}")))?;
    notifier
        .Show(&toast)
        .map_err(|e| ToastError::WinRt(format!("Show: {e}")))?;
    Ok(())
}

#[cfg(not(windows))]
pub fn show_with_aumid(_aumid: &str, _title: &str, _body: &str) -> Result<(), ToastError> {
    Err(ToastError::Registration("toasts are Windows-only".into()))
}

/// Diagnose the toast path step by step.
///
/// Prints each step's result because the API reports one opaque HRESULT for very
/// different failures, and guessing which step failed has already cost time.
pub fn selftest() -> Vec<String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::UI::Notifications::{ToastNotificationManager, ToastTemplateType};
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let mut out = Vec::new();

    // Apartment test. PowerShell 5.1 runs STA and its toasts work here; this process
    // does not initialise COM on the CLI path, so WinRT picks MTA. If the notification
    // factory cares about the apartment, this line changes the result below.
    let hr = unsafe {
        windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        )
    };
    out.push(format!("CoInitializeEx(STA): {hr:?}"));

    let wide: Vec<u16> = APP_USER_MODEL_ID
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let set = unsafe { SetCurrentProcessExplicitAppUserModelID(PCWSTR(wide.as_ptr())) };
    out.push(match set {
        Ok(()) => "SetCurrentProcessExplicitAppUserModelID: ok".to_string(),
        Err(e) => format!("SetCurrentProcessExplicitAppUserModelID: {e}"),
    });

    out.push(format!(
        "shortcut present: {}",
        shortcut_path().map(|p| p.exists()).unwrap_or(false)
    ));

    match ToastNotificationManager::GetTemplateContent(ToastTemplateType::ToastText02) {
        Ok(_) => out.push("GetTemplateContent: ok".to_string()),
        Err(e) => out.push(format!("GetTemplateContent: {e}")),
    }

    // The no-argument overload uses the process AUMID and is documented for
    // unpackaged apps that set one.
    match ToastNotificationManager::CreateToastNotifier() {
        Ok(_) => out.push("CreateToastNotifier(): ok".to_string()),
        Err(e) => out.push(format!("CreateToastNotifier(): {e}")),
    }

    match ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_USER_MODEL_ID)) {
        Ok(_) => out.push("CreateToastNotifierWithId(ours): ok".to_string()),
        Err(e) => out.push(format!("CreateToastNotifierWithId(ours): {e}")),
    }

    // What identity Windows associates with this process right now.
    match unsafe { windows::Win32::UI::Shell::GetCurrentProcessExplicitAppUserModelID() } {
        Ok(id) => out.push(format!("process AUMID: {:?}", unsafe { id.to_string() })),
        Err(e) => out.push(format!("process AUMID: none ({e})")),
    }
    out
}

#[cfg(not(windows))]
pub fn selftest() -> Vec<String> {
    vec!["toasts are Windows-only".to_string()]
}

#[cfg(windows)]
fn show_registered(title: &str, body: &str, tag: &str) -> Result<(), ToastError> {
    use windows::core::HSTRING;
    use windows::UI::Notifications::{
        ToastNotification, ToastNotificationManager, ToastTemplateType,
    };

    let xml = ToastNotificationManager::GetTemplateContent(ToastTemplateType::ToastText02)
        .map_err(|e| ToastError::WinRt(format!("GetTemplateContent: {e}")))?;

    let texts = xml
        .GetElementsByTagName(&HSTRING::from("text"))
        .map_err(|e| ToastError::WinRt(format!("GetElementsByTagName: {e}")))?;

    // Line 0 is the bold title, line 1 the body: the ToastText02 layout.
    let title_node = xml
        .CreateTextNode(&HSTRING::from(title))
        .map_err(|e| ToastError::Payload(format!("CreateTextNode(title): {e}")))?;
    texts
        .Item(0)
        .and_then(|n| n.AppendChild(&title_node))
        .map_err(|e| ToastError::Payload(format!("AppendChild(title): {e}")))?;

    let body_node = xml
        .CreateTextNode(&HSTRING::from(body))
        .map_err(|e| ToastError::Payload(format!("CreateTextNode(body): {e}")))?;
    texts
        .Item(1)
        .and_then(|n| n.AppendChild(&body_node))
        .map_err(|e| ToastError::Payload(format!("AppendChild(body): {e}")))?;

    let toast = ToastNotification::CreateToastNotification(&xml)
        .map_err(|e| ToastError::WinRt(format!("CreateToastNotification: {e}")))?;
    toast
        .SetTag(&HSTRING::from(tag))
        .map_err(|e| ToastError::WinRt(format!("SetTag: {e}")))?;
    toast
        .SetGroup(&HSTRING::from("devchievements"))
        .map_err(|e| ToastError::WinRt(format!("SetGroup: {e}")))?;

    let notifier =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_USER_MODEL_ID))
            .map_err(|e| ToastError::WinRt(format!("CreateToastNotifierWithId: {e}")))?;

    notifier
        .Show(&toast)
        .map_err(|e| ToastError::WinRt(format!("Show: {e}")))?;
    Ok(())
}

#[cfg(not(windows))]
fn show_registered(_title: &str, _body: &str, _tag: &str) -> Result<(), ToastError> {
    Err(ToastError::WinRt("toasts are Windows-only".into()))
}
