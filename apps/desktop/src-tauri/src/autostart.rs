//! Start-with-Windows support (spec 40.1, installation and upgrade experience).
//!
//! Why the Run key and not a Startup-folder shortcut: the installer's uninstaller
//! already deletes `HKCU\...\Run\Devchievements` (tauri's NSIS template does it with
//! `${PRODUCTNAME}`), so writing that same value is the only autostart mechanism that
//! uninstalling also cleans up. A shortcut in shell:startup would be left behind.
//!
//! The setting is the source of truth. At startup this module only makes the registry
//! agree with it: refresh the path when enabled (an app that moved must still start),
//! and remove the value when disabled, but only if it points at this executable --
//! leaving a value written by some other install alone is the safer of the two.

#[cfg(windows)]
mod registry {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
        HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ,
    };

    /// Must match the value name the uninstaller removes.
    pub const VALUE_NAME: &str = "Devchievements";
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    /// Open the Run key, or explain why it could not be opened.
    ///
    /// The status code travels with the failure: "cannot open the Run key" alone does
    /// not distinguish ERROR_ACCESS_DENIED (a policy blocks it) from
    /// ERROR_FILE_NOT_FOUND (the key is missing), and those need different answers.
    fn open(key: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Result<HKEY, String> {
        let path = HSTRING::from(RUN_KEY);
        let mut handle = HKEY::default();
        // Safety: the path is a valid wide string and the handle outlives the caller's
        // use because every caller closes it.
        let status = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, &path, None, key, &mut handle) };
        if status == ERROR_SUCCESS {
            Ok(handle)
        } else {
            Err(format!("RegOpenKeyEx({RUN_KEY}) failed: {status:?}"))
        }
    }

    /// The current value, if any.
    pub fn read() -> Option<String> {
        let handle = open(KEY_QUERY_VALUE).ok()?;
        let name = HSTRING::from(VALUE_NAME);
        let mut kind = Default::default();
        let mut bytes: u32 = 0;
        // Size first, then the value: the string length is not known up front.
        let sized = unsafe {
            RegQueryValueExW(handle, &name, None, Some(&mut kind), None, Some(&mut bytes))
        };
        if sized != ERROR_SUCCESS || bytes == 0 {
            unsafe {
                let _ = RegCloseKey(handle);
            }
            return None;
        }
        let mut buffer = vec![0u8; bytes as usize];
        let ok = unsafe {
            RegQueryValueExW(
                handle,
                &name,
                None,
                Some(&mut kind),
                Some(buffer.as_mut_ptr()),
                Some(&mut bytes),
            )
        };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        if ok != ERROR_SUCCESS {
            return None;
        }
        // REG_SZ is UTF-16; the trailing NUL is dropped by from_utf16.
        let units: Vec<u16> = buffer
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        Some(
            String::from_utf16_lossy(&units)
                .trim_end_matches('\0')
                .to_string(),
        )
    }

    pub fn write(command: &str) -> Result<(), String> {
        let handle = open(KEY_SET_VALUE)?;
        let name = HSTRING::from(VALUE_NAME);
        let data: Vec<u16> = command.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
        let status = unsafe { RegSetValueExW(handle, &name, None, REG_SZ, Some(bytes)) };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("RegSetValueEx failed: {status:?}"))
        }
    }

    pub fn remove() -> Result<(), String> {
        let handle = open(KEY_SET_VALUE)?;
        let name = HSTRING::from(VALUE_NAME);
        let status = unsafe { RegDeleteValueW(handle, &name) };
        unsafe {
            let _ = RegCloseKey(handle);
        }
        // Removing a value that is not there is the desired end state, not a failure.
        if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(format!("RegDeleteValue failed: {status:?}"))
        }
    }
}

/// The command line stored in the Run key: the executable, quoted, nothing else.
/// A launch at sign-in should behave exactly like a double-click.
fn command_for(exe: &std::path::Path) -> String {
    format!("\"{}\"", exe.display())
}

/// Whether the stored command refers to this executable.
fn points_at(command: &str, exe: &std::path::Path) -> bool {
    command
        .trim()
        .trim_matches('"')
        .eq_ignore_ascii_case(&exe.to_string_lossy())
}

fn current_exe() -> std::path::PathBuf {
    std::env::current_exe().unwrap_or_default()
}

/// Turn the registry entry on or off, as the setting asks.
pub fn apply(enabled: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        if enabled {
            registry::write(&command_for(&current_exe()))
        } else {
            registry::remove()
        }
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err("start with Windows is only implemented on Windows".to_string())
    }
}

/// Whether the registry currently starts this app at sign-in.
pub fn is_enabled() -> bool {
    #[cfg(windows)]
    {
        registry::read()
            .map(|command| points_at(&command, &current_exe()))
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Make the registry agree with the setting, once at startup.
pub fn reconcile(enabled: bool) {
    #[cfg(windows)]
    {
        let exe = current_exe();
        let stored = registry::read();
        match (enabled, stored) {
            // Refresh on every start: an app that moved must still start at sign-in.
            (true, Some(command)) if points_at(&command, &exe) => {
                if let Err(e) = registry::write(&command_for(&exe)) {
                    println!("AUTOSTART refresh failed: {e}");
                }
            }
            (true, _) => {
                if let Err(e) = registry::write(&command_for(&exe)) {
                    println!("AUTOSTART apply failed: {e}");
                }
            }
            (false, Some(command)) if points_at(&command, &exe) => {
                if let Err(e) = registry::remove() {
                    println!("AUTOSTART remove failed: {e}");
                }
            }
            // Disabled, and either nothing is stored or it belongs to another install.
            (false, _) => {}
        }
        println!("AUTOSTART setting={enabled} enabled={}", is_enabled());
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_command_quotes_the_executable() {
        let exe = std::path::Path::new(r"C:\Program Files\Devchievements\app.exe");
        assert_eq!(
            command_for(exe),
            "\"C:\\Program Files\\Devchievements\\app.exe\""
        );
    }

    #[test]
    fn a_command_without_quotes_still_matches() {
        let exe = std::path::Path::new(r"C:\app\dev.exe");
        assert!(points_at(r"C:\app\dev.exe", exe));
    }

    #[test]
    fn a_command_for_another_copy_is_not_ours() {
        // The point of the check: disabling autostart must not delete an entry that
        // belongs to a different installation of the same product.
        let exe = std::path::Path::new(r"C:\app\dev.exe");
        assert!(!points_at("\"C:\\other\\dev.exe\"", exe));
    }

    #[test]
    fn windows_paths_compare_without_case() {
        let exe = std::path::Path::new(r"C:\App\Dev.exe");
        assert!(points_at("\"c:\\app\\dev.exe\"", exe));
    }
}
