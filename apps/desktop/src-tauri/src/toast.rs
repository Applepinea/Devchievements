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

/// Register the AppUserModelID, creating the shortcut on first use.
pub fn ensure_registered() -> Result<(), ToastError> {
    register_identity()
}

#[cfg(windows)]
fn register_identity() -> Result<(), ToastError> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = format!("Software\\Classes\\AppUserModelId\\{APP_USER_MODEL_ID}");

    // Best effort: the key supplies only the display name shown in Settings. If it is
    // already correct we skip the write entirely, and a denied write must not stop the
    // shortcut from being created -- the shortcut is what actually grants the identity.
    let already_correct = hkcu
        .open_subkey_with_flags(&path, winreg::enums::KEY_READ)
        .ok()
        .and_then(|k| k.get_value::<String, _>("DisplayName").ok())
        .is_some_and(|v| v == DISPLAY_NAME);

    if !already_correct {
        match hkcu.create_subkey(&path) {
            Ok((key, _)) => {
                let _ = key.set_value("DisplayName", &DISPLAY_NAME);
            }
            Err(e) => {
                // Recorded, not fatal: see the comment above.
                eprintln!("toast: cannot write AppUserModelId key: {e}");
            }
        }
    }

    set_process_identity();

    let shortcut =
        shortcut_path().ok_or_else(|| ToastError::Registration("APPDATA is not set".into()))?;
    if shortcut.exists() {
        // Already installed; rewriting it on every launch would churn the Start Menu.
        return Ok(());
    }

    let target = std::env::current_exe()
        .map_err(|e| ToastError::Registration(format!("locate own executable: {e}")))?;
    create_shortcut(&shortcut, &target)
}

#[cfg(not(windows))]
fn register_identity() -> Result<(), ToastError> {
    Err(ToastError::Registration("toast registration is Windows-only".into()))
}

/// Claim the AppUserModelID for this process.
#[cfg(windows)]
fn set_process_identity() {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    let _ = unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_USER_MODEL_ID)) };
}

#[cfg(not(windows))]
fn set_process_identity() {}

/// Create the Start Menu shortcut and stamp it with our AppUserModelID.
///
/// This needs ShellLink + IPropertyStore, both COM. The PROPVARIANT is built by hand
/// because the AUMID must be VT_LPWSTR and storage allocated with CoTaskMemAlloc: the
/// property store copies the value, and PropVariantClear frees the original.
/// Create the registration shortcut at an arbitrary path.
///
/// Public because installing the shortcut can legitimately happen outside the app
/// (an installer, or a test that is not allowed to write to the Start Menu).
#[cfg(windows)]
pub fn create_shortcut(lnk: &std::path::Path, target: &std::path::Path) -> Result<(), ToastError> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::{Interface, PCWSTR};
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemAlloc, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::Foundation::PROPERTYKEY;
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
fn create_shortcut(_lnk: &std::path::Path, _target: &std::path::Path) -> Result<(), ToastError> {
    Err(ToastError::Registration("shortcuts are Windows-only".into()))
}

/// Show a toast.
///
/// tag is attached to the notification so it can be found again in the Action Center
/// history; the delivery layer passes the notification id.
pub fn show(title: &str, body: &str, tag: &str) -> Result<(), ToastError> {
    ensure_registered()?;
    show_registered(title, body, tag)
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
