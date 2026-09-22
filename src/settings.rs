#[cfg(any(not(windows), test))]
use std::fs;
#[cfg(any(not(windows), test))]
use std::path::Path;
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
    load_user_settings_from_path(&user_settings_path())
}

#[cfg(windows)]
pub fn save_user_settings(settings: &UserSettings) -> Result<(), String> {
    windows_registry::save_user_settings(settings)
}

#[cfg(not(windows))]
pub fn save_user_settings(settings: &UserSettings) -> Result<(), String> {
    save_user_settings_to_path(&user_settings_path(), settings)
}

#[cfg(any(not(windows), test))]
fn load_user_settings_from_path(path: &Path) -> UserSettings {
    let Ok(text) = fs::read_to_string(path) else {
        return UserSettings::default();
    };
    decode_user_settings(&text)
}

#[cfg(any(not(windows), test))]
fn save_user_settings_to_path(path: &Path, settings: &UserSettings) -> Result<(), String> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory).map_err(|error| {
            format!(
                "Falha criando pasta de configuração {}: {error}",
                directory.display()
            )
        })?;
    }

    fs::write(path, encode_user_settings(settings))
        .map_err(|error| format!("Falha salvando configuração em {}: {error}", path.display()))
}

#[cfg(any(not(windows), test))]
fn encode_user_settings(settings: &UserSettings) -> String {
    let mut text = String::new();
    push_setting(&mut text, "ClinicName", settings.clinic_name.trim());
    push_setting(&mut text, "PhysicianName", settings.physician_name.trim());
    push_setting(
        &mut text,
        "LiveDeviceValue",
        settings.live_device_value.trim(),
    );
    push_setting(&mut text, "FilterValue", settings.filter_value.trim());
    push_setting(
        &mut text,
        "GridThemeValue",
        settings.grid_theme_value.trim(),
    );
    push_setting(
        &mut text,
        "SelectedLeads",
        settings.selected_leads_value.trim(),
    );
    push_setting(&mut text, "LanguageCode", settings.language_code.trim());
    push_setting(
        &mut text,
        "LastFileDirectory",
        path_setting(settings.last_file_directory.as_deref()),
    );
    push_setting(
        &mut text,
        "ShowCalibration",
        if settings.show_calibration { "1" } else { "0" },
    );
    push_setting(
        &mut text,
        "ClinicLogoPath",
        path_setting(settings.clinic_logo_path.as_deref()),
    );

    if let Some(window) = settings.window {
        push_setting(&mut text, "WindowX", &window.x.to_string());
        push_setting(&mut text, "WindowY", &window.y.to_string());
        push_setting(&mut text, "WindowWidth", &window.width.to_string());
        push_setting(&mut text, "WindowHeight", &window.height.to_string());
        push_setting(
            &mut text,
            "WindowMaximized",
            if window.maximized { "1" } else { "0" },
        );
    }

    text
}

#[cfg(any(not(windows), test))]
fn decode_user_settings(text: &str) -> UserSettings {
    let mut settings = UserSettings::default();
    let mut window_x = None;
    let mut window_y = None;
    let mut window_width = None;
    let mut window_height = None;
    let mut window_maximized = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "ClinicName" => settings.clinic_name = value.to_owned(),
            "PhysicianName" => settings.physician_name = value.to_owned(),
            "LiveDeviceValue" => settings.live_device_value = value.to_owned(),
            "FilterValue" => settings.filter_value = value.to_owned(),
            "GridThemeValue" => settings.grid_theme_value = value.to_owned(),
            "SelectedLeads" => settings.selected_leads_value = value.to_owned(),
            "LanguageCode" => settings.language_code = value.to_owned(),
            "LastFileDirectory" => {
                settings.last_file_directory = nonempty_path(value);
            }
            "ShowCalibration" => settings.show_calibration = value != "0",
            "ClinicLogoPath" => settings.clinic_logo_path = nonempty_path(value),
            "WindowX" => window_x = value.parse().ok(),
            "WindowY" => window_y = value.parse().ok(),
            "WindowWidth" => window_width = value.parse().ok(),
            "WindowHeight" => window_height = value.parse().ok(),
            "WindowMaximized" => window_maximized = value == "1",
            _ => {}
        }
    }

    if let (Some(x), Some(y), Some(width), Some(height)) =
        (window_x, window_y, window_width, window_height)
    {
        if width >= 640 && height >= 480 {
            settings.window = Some(WindowSettings {
                x,
                y,
                width,
                height,
                maximized: window_maximized,
            });
        }
    }

    settings
}

#[cfg(any(not(windows), test))]
fn push_setting(text: &mut String, key: &str, value: &str) {
    text.push_str(key);
    text.push('=');
    text.push_str(value);
    text.push('\n');
}

#[cfg(any(not(windows), test))]
fn path_setting(path: Option<&Path>) -> &str {
    path.and_then(Path::to_str).unwrap_or("")
}

#[cfg(any(not(windows), test))]
fn nonempty_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

#[cfg(not(windows))]
fn user_settings_path() -> PathBuf {
    user_config_dir().join("settings")
}

#[cfg(not(windows))]
fn user_config_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        home_dir().join("Library/Application Support/ECG Studio")
    }
    #[cfg(not(target_os = "macos"))]
    {
        xdg_config_home().join("ecg-studio")
    }
}

#[cfg(all(not(windows), not(target_os = "macos")))]
fn xdg_config_home() -> PathBuf {
    match std::env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => home_dir().join(".config"),
    }
}

#[cfg(not(windows))]
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
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

#[cfg(test)]
mod tests {
    use super::{
        UserSettings, WindowSettings, decode_user_settings, encode_user_settings,
        load_user_settings_from_path, save_user_settings_to_path,
    };
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sample_settings() -> UserSettings {
        UserSettings {
            clinic_name: "Clinica Central".to_owned(),
            physician_name: "Dr. Silva".to_owned(),
            clinic_logo_path: Some(PathBuf::from("/home/user/logo.png")),
            live_device_value: "CONTEC ECG90A".to_owned(),
            filter_value: "diagnostic".to_owned(),
            grid_theme_value: "technical_gray".to_owned(),
            selected_leads_value: "I,II,V1".to_owned(),
            show_calibration: false,
            language_code: "pt-BR".to_owned(),
            last_file_directory: Some(PathBuf::from("/home/user/ecg files")),
            window: Some(WindowSettings {
                x: 40,
                y: 80,
                width: 1280,
                height: 720,
                maximized: true,
            }),
        }
    }

    #[test]
    fn round_trips_settings_text() {
        let encoded = encode_user_settings(&sample_settings());
        let decoded = decode_user_settings(&encoded);

        assert_eq!(decoded.clinic_name, "Clinica Central");
        assert_eq!(decoded.physician_name, "Dr. Silva");
        assert_eq!(
            decoded.clinic_logo_path.as_deref(),
            Some(std::path::Path::new("/home/user/logo.png"))
        );
        assert_eq!(decoded.live_device_value, "CONTEC ECG90A");
        assert_eq!(decoded.filter_value, "diagnostic");
        assert_eq!(decoded.grid_theme_value, "technical_gray");
        assert_eq!(decoded.selected_leads_value, "I,II,V1");
        assert!(!decoded.show_calibration);
        assert_eq!(decoded.language_code, "pt-BR");
        assert_eq!(
            decoded.last_file_directory.as_deref(),
            Some(std::path::Path::new("/home/user/ecg files"))
        );
        let window = decoded.window.expect("window should round-trip");
        assert_eq!(window.x, 40);
        assert_eq!(window.y, 80);
        assert_eq!(window.width, 1280);
        assert_eq!(window.height, 720);
        assert!(window.maximized);
    }

    #[test]
    fn keeps_equals_inside_values() {
        let mut settings = UserSettings::default();
        settings.clinic_name = "A=B Clinic".to_owned();
        let decoded = decode_user_settings(&encode_user_settings(&settings));
        assert_eq!(decoded.clinic_name, "A=B Clinic");
    }

    #[test]
    fn ignores_unknown_keys_and_comments() {
        let decoded = decode_user_settings(
            "# comment\nClinicName=Demo\nUnknown=1\nShowCalibration=0\nWindowWidth=100\n",
        );
        assert_eq!(decoded.clinic_name, "Demo");
        assert!(!decoded.show_calibration);
        assert!(decoded.window.is_none());
    }

    #[test]
    fn round_trips_settings_file() {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("ecg-studio-settings-{millis}.txt"));
        save_user_settings_to_path(&path, &sample_settings()).expect("settings should save");
        let loaded = load_user_settings_from_path(&path);
        let _ = std::fs::remove_file(&path);

        assert_eq!(loaded.clinic_name, "Clinica Central");
        assert_eq!(
            loaded.last_file_directory.as_deref(),
            Some(std::path::Path::new("/home/user/ecg files"))
        );
        assert_eq!(loaded.window.map(|window| window.width), Some(1280));
    }
}
