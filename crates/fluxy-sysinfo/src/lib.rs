//! Who is this machine, in words a user recognises.
//!
//! A pairing list that shows only "windows / android" is not enough once there
//! are several peers: two Windows laptops read the same. This module answers
//! the three questions a user actually asks when picking a device - what brand
//! is it, what model, and which OS version - and returns empty strings when it
//! cannot tell, so the UI degrades to the old text instead of guessing.

/// Everything this platform can tell us, already trimmed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemInfo {
    /// "LENOVO", "HP", "OPPO" - empty when unknown.
    pub manufacturer: String,
    /// "ThinkPad X1 Carbon", "Find X9" - empty when unknown.
    pub model: String,
    /// "Windows 11", "Debian GNU/Linux", "Android 16" - empty when unknown.
    pub os_name: String,
    /// The version that goes in parentheses after the name.
    pub os_version: String,
}

impl SystemInfo {
    /// The single line a peer list shows.
    pub fn summary(&self) -> String {
        let model = self.model.trim();
        let os = self.display_os();
        match (model.is_empty(), os.is_empty()) {
            (false, false) => format!("{model} · {os}"),
            (false, true) => model.to_string(),
            (true, false) => os,
            (true, true) => std::env::consts::OS.to_string(),
        }
    }

    /// "Windows 11 (24H2)" style text, or empty.
    pub fn display_os(&self) -> String {
        match (
            self.os_name.trim().is_empty(),
            self.os_version.trim().is_empty(),
        ) {
            (false, false) => format!("{} ({})", self.os_name.trim(), self.os_version.trim()),
            (false, true) => self.os_name.trim().to_string(),
            (true, false) => self.os_version.trim().to_string(),
            (true, true) => String::new(),
        }
    }
}

/// Read this device's brand, model and OS.
pub fn detect() -> SystemInfo {
    platform::detect()
}

/// The same values, read once per process.
///
/// Reading touches the registry (Windows), sysfs (Linux) or system properties
/// (Android); the answer cannot change while the app runs, and both the mDNS
/// advertisement and the handshake ask for it.
pub fn cached() -> &'static SystemInfo {
    static INFO: std::sync::OnceLock<SystemInfo> = std::sync::OnceLock::new();
    INFO.get_or_init(detect)
}

fn clean(value: impl AsRef<str>) -> String {
    let text = value.as_ref();
    // Firmware and drivers pad these with NULs and stray whitespace.
    text.trim_matches(char::from(0)).trim().to_string()
}

#[cfg(windows)]
mod platform {
    use super::{SystemInfo, clean};
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ};

    fn read(root: &RegKey, path: &str, name: &str) -> Option<String> {
        root.open_subkey_with_flags(path, KEY_READ)
            .ok()
            .and_then(|key| key.get_value::<String, _>(name).ok())
            .map(clean)
            .filter(|value| !value.is_empty())
    }

    pub fn detect() -> SystemInfo {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let bios = r"HARDWARE\DESCRIPTION\System\BIOS";
        let manufacturer = read(&hklm, bios, "SystemManufacturer")
            .or_else(|| read(&hklm, bios, "BaseBoardManufacturer"))
            .unwrap_or_default();
        let model = read(&hklm, bios, "SystemProductName").unwrap_or_default();

        let version_key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let product = read(&hklm, version_key, "ProductName").unwrap_or_default();
        let edition = read(&hklm, version_key, "DisplayVersion").unwrap_or_default();
        let build = read(&hklm, version_key, "CurrentBuild").unwrap_or_default();

        // Windows 11 still reports "Windows 10" in ProductName; the build number
        // is what separates them (22000 and up).
        let is_win11 = build
            .parse::<u32>()
            .map(|build| build >= 22000)
            .unwrap_or(false);
        let os_name = if product.is_empty() {
            String::new()
        } else if is_win11 {
            product.replace("Windows 10", "Windows 11")
        } else {
            product
        };

        SystemInfo {
            manufacturer,
            model,
            os_name,
            os_version: edition,
        }
    }
}

#[cfg(target_os = "android")]
mod platform {
    use super::{SystemInfo, clean};
    use std::ffi::{CStr, CString};
    use std::os::raw::{c_char, c_int};

    unsafe extern "C" {
        fn __system_property_get(name: *const c_char, value: *mut c_char) -> c_int;
    }

    fn property(name: &str) -> String {
        let Ok(key) = CString::new(name) else {
            return String::new();
        };
        let mut buffer = [0u8; 128];
        // SAFETY: the key is NUL-terminated and the buffer is owned by us; the
        // call writes at most PROP_VALUE_MAX bytes into it and NUL-terminates.
        let len = unsafe { __system_property_get(key.as_ptr(), buffer.as_mut_ptr().cast()) };
        if len <= 0 {
            return String::new();
        }
        let text = unsafe { CStr::from_ptr(buffer.as_ptr().cast()) }
            .to_string_lossy()
            .into_owned();
        clean(text)
    }

    pub fn detect() -> SystemInfo {
        let release = property("ro.build.version.release");
        let sdk = property("ro.build.version.sdk");
        SystemInfo {
            manufacturer: property("ro.product.manufacturer"),
            model: property("ro.product.model"),
            // "Android 16" reads better than a bare "16".
            os_name: if release.is_empty() {
                String::new()
            } else {
                format!("Android {release}")
            },
            os_version: if sdk.is_empty() {
                String::new()
            } else {
                format!("API {sdk}")
            },
        }
    }
}

#[cfg(all(unix, not(target_os = "android"), not(target_os = "macos")))]
mod platform {
    use super::{SystemInfo, clean};

    fn read_file(path: &str) -> String {
        std::fs::read_to_string(path).map(clean).unwrap_or_default()
    }

    pub fn detect() -> SystemInfo {
        let manufacturer = read_file("/sys/class/dmi/id/sys_vendor");
        let model = read_file("/sys/class/dmi/id/product_name");
        let (os_name, os_version) = std::fs::read_to_string("/etc/os-release")
            .ok()
            .map(|text| {
                let mut name = String::new();
                let mut version = String::new();
                for line in text.lines() {
                    if let Some(value) = line.strip_prefix("NAME=") {
                        name = clean(value.trim_matches('"'));
                    } else if let Some(value) = line.strip_prefix("VERSION_ID=") {
                        version = clean(value.trim_matches('"'));
                    }
                }
                (name, version)
            })
            .unwrap_or_default();
        SystemInfo {
            manufacturer,
            model,
            os_name,
            os_version,
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use super::{SystemInfo, clean};
    use std::process::Command;

    fn run(program: &str, args: &[&str]) -> String {
        Command::new(program)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| clean(String::from_utf8_lossy(&output.stdout)))
            .unwrap_or_default()
    }

    pub fn detect() -> SystemInfo {
        SystemInfo {
            // Apple hardware is Apple hardware; the interesting part is the model.
            manufacturer: "Apple".to_string(),
            model: run("sysctl", &["-n", "hw.model"]),
            os_name: run("sw_vers", &["-productName"]),
            os_version: run("sw_vers", &["-productVersion"]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SystemInfo;

    #[test]
    fn summary_prefers_model_and_os() {
        let info = SystemInfo {
            manufacturer: "LENOVO".into(),
            model: "ThinkPad X1".into(),
            os_name: "Windows 11".into(),
            os_version: "24H2".into(),
        };
        assert_eq!(info.summary(), "ThinkPad X1 · Windows 11 (24H2)");
    }

    #[test]
    fn summary_falls_back_when_fields_are_missing() {
        assert_eq!(SystemInfo::default().summary(), std::env::consts::OS);
        assert_eq!(
            SystemInfo {
                model: "Find X9".into(),
                ..Default::default()
            }
            .summary(),
            "Find X9"
        );
        assert_eq!(
            SystemInfo {
                os_name: "Debian GNU/Linux".into(),
                os_version: "13".into(),
                ..Default::default()
            }
            .summary(),
            "Debian GNU/Linux (13)"
        );
    }

    #[test]
    fn display_os_never_prints_empty_parentheses() {
        assert_eq!(
            SystemInfo {
                os_name: "Android 16".into(),
                ..Default::default()
            }
            .display_os(),
            "Android 16"
        );
        assert_eq!(SystemInfo::default().display_os(), "");
    }
}
