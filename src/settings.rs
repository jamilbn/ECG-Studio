use std::path::PathBuf;

#[cfg(windows)]
const REGISTRY_SUBKEY: &str = r"Software\ECG Studio";

#[derive(Clone, Debug)]
pub struct UserSettings {
    pub clinic_name: String,
    pub physician_name: String,
    pub clinic_logo_path: Option<PathBuf>,
    pub live_device_value: String,
    pub filter_value: String,
    pub grid_theme_value: String,
    pub selected_leads_value: String,
    pub show_calibration: bool,
    pub language_code: String,
    pub last_file_directory: Option<PathBuf>,
    pub window: Option<WindowSettings>,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            clinic_name: String::new(),
            physician_name: String::new(),
            clinic_logo_path: None,
            live_device_value: String::new(),
            filter_value: String::new(),
            grid_theme_value: String::new(),
            selected_leads_value: String::new(),
            show_calibration: true,
            language_code: String::new(),
            last_file_directory: None,
            window: None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WindowSettings {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

#[cfg(windows)]
pub fn load_user_settings() -> UserSettings {
    windows_registry::load_user_settings()
}

#[cfg(not(windows))]
pub fn load_user_settings() -> UserSettings {
    UserSettings::default()
}

#[cfg(windows)]
pub fn save_user_settings(settings: &UserSettings) -> Result<(), String> {
    windows_registry::save_user_settings(settings)
}

#[cfg(not(windows))]
pub fn save_user_settings(_settings: &UserSettings) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
mod windows_registry {
    use super::{REGISTRY_SUBKEY, UserSettings, WindowSettings};
    use std::path::PathBuf;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
        RegCreateKeyExW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    };
    use windows::core::PCWSTR;

    pub fn load_user_settings() -> UserSettings {
        let Some(key) = RegistryKey::open_read() else {
            return UserSettings::default();
        };

        let clinic_logo_path = key
            .string_value("ClinicLogoPath")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
        let window = load_window_settings(&key);

        UserSettings {
            clinic_name: key.string_value("ClinicName").unwrap_or_default(),
            physician_name: key.string_value("PhysicianName").unwrap_or_default(),
            clinic_logo_path,
            live_device_value: key.string_value("LiveDeviceValue").unwrap_or_default(),
            filter_value: key.string_value("FilterValue").unwrap_or_default(),
            grid_theme_value: key.string_value("GridThemeValue").unwrap_or_default(),
            selected_leads_value: key.string_value("SelectedLeads").unwrap_or_default(),
            show_calibration: key
                .string_value("ShowCalibration")
                .as_deref()
                .map(|value| value != "0")
                .unwrap_or(true),
            language_code: key.string_value("LanguageCode").unwrap_or_default(),
            last_file_directory: key
                .string_value("LastFileDirectory")
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from),
            window,
        }
    }

    pub fn save_user_settings(settings: &UserSettings) -> Result<(), String> {
        let key = RegistryKey::create()?;
        key.set_string("ClinicName", settings.clinic_name.trim())?;
        key.set_string("PhysicianName", settings.physician_name.trim())?;
        key.set_string("LiveDeviceValue", settings.live_device_value.trim())?;
        key.set_string("FilterValue", settings.filter_value.trim())?;
        key.set_string("GridThemeValue", settings.grid_theme_value.trim())?;
        key.set_string("SelectedLeads", settings.selected_leads_value.trim())?;
        key.set_string("LanguageCode", settings.language_code.trim())?;
        key.set_string(
            "LastFileDirectory",
            settings
                .last_file_directory
                .as_ref()
                .and_then(|path| path.to_str())
                .unwrap_or(""),
        )?;
        key.set_string(
            "ShowCalibration",
            if settings.show_calibration { "1" } else { "0" },
        )?;
        key.set_string(
            "ClinicLogoPath",
            settings
                .clinic_logo_path
                .as_ref()
                .and_then(|path| path.to_str())
                .unwrap_or(""),
        )?;

        if let Some(window) = settings.window {
            key.set_string("WindowX", &window.x.to_string())?;
            key.set_string("WindowY", &window.y.to_string())?;
            key.set_string("WindowWidth", &window.width.to_string())?;
            key.set_string("WindowHeight", &window.height.to_string())?;
            key.set_string("WindowMaximized", if window.maximized { "1" } else { "0" })?;
        }

        Ok(())
    }

    fn load_window_settings(key: &RegistryKey) -> Option<WindowSettings> {
        let width = key.string_value("WindowWidth")?.parse::<u32>().ok()?;
        let height = key.string_value("WindowHeight")?.parse::<u32>().ok()?;
        if width < 640 || height < 480 {
            return None;
        }

        Some(WindowSettings {
            x: key.string_value("WindowX")?.parse::<i32>().ok()?,
            y: key.string_value("WindowY")?.parse::<i32>().ok()?,
            width,
            height,
            maximized: key
                .string_value("WindowMaximized")
                .as_deref()
                .is_some_and(|value| value == "1"),
        })
    }

    struct RegistryKey {
        handle: HKEY,
    }

    impl RegistryKey {
        fn open_read() -> Option<Self> {
            let mut handle = HKEY::default();
            let subkey = wide_null(REGISTRY_SUBKEY);
            let result = unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(subkey.as_ptr()),
                    Some(0),
                    KEY_READ,
                    &mut handle,
                )
            };

            if result == ERROR_SUCCESS {
                Some(Self { handle })
            } else {
                None
            }
        }

        fn create() -> Result<Self, String> {
            let mut handle = HKEY::default();
            let subkey = wide_null(REGISTRY_SUBKEY);
            let result = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(subkey.as_ptr()),
                    Some(0),
                    PCWSTR::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_WRITE,
                    None,
                    &mut handle,
                    None,
                )
            };

            if result == ERROR_SUCCESS {
                Ok(Self { handle })
            } else {
                Err(format!(
                    "Falha criando chave de configuração no Registro ({result:?})."
                ))
            }
        }

        fn string_value(&self, name: &str) -> Option<String> {
            let name = wide_null(name);
            let mut value_type = REG_SZ;
            let mut byte_len = 0_u32;
            let size_result = unsafe {
                RegQueryValueExW(
                    self.handle,
                    PCWSTR(name.as_ptr()),
                    None,
                    Some(&mut value_type),
                    None,
                    Some(&mut byte_len),
                )
            };

            if size_result == ERROR_FILE_NOT_FOUND || byte_len == 0 || value_type != REG_SZ {
                return None;
            }
            if size_result != ERROR_SUCCESS {
                return None;
            }

            let mut bytes = vec![0_u8; byte_len as usize];
            let read_result = unsafe {
                RegQueryValueExW(
                    self.handle,
                    PCWSTR(name.as_ptr()),
                    None,
                    Some(&mut value_type),
                    Some(bytes.as_mut_ptr()),
                    Some(&mut byte_len),
                )
            };

            if read_result != ERROR_SUCCESS || value_type != REG_SZ {
                return None;
            }

            let words = bytes[..byte_len as usize]
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                .take_while(|word| *word != 0)
                .collect::<Vec<_>>();
            String::from_utf16(&words).ok()
        }

        fn set_string(&self, name: &str, value: &str) -> Result<(), String> {
            let name = wide_null(name);
            let value = wide_null(value);
            let bytes =
                unsafe { std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2) };
            let result = unsafe {
                RegSetValueExW(
                    self.handle,
                    PCWSTR(name.as_ptr()),
                    Some(0),
                    REG_SZ,
                    Some(bytes),
                )
            };

            if result == ERROR_SUCCESS {
                Ok(())
            } else {
                Err(format!(
                    "Falha salvando configuração no Registro ({result:?})."
                ))
            }
        }
    }

    impl Drop for RegistryKey {
        fn drop(&mut self) {
            unsafe {
                let _ = RegCloseKey(self.handle);
            }
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}
