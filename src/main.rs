#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod analysis;
mod domain;
mod exporting;
mod i18n;
mod live;
mod platform_open;
mod preview;
mod printing;
mod processing;
mod readers;
mod settings;

use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use domain::{DocumentKind, EcgDocument, LeadData, NORMAL_LEAD_SAMPLE_SECONDS};
use i18n::{Language, LanguageSelection, Texts};
use live::{LiveMessage, LiveSampleFrame, LiveSession};
use preview::{ClinicLogo, GridTheme, LogoBitmap, PageOrientation, RenderOptions};
use processing::FilterMode;
use settings::{UserSettings, WindowSettings};
use slint::{
    CloseRequestResponse, ComponentHandle, PhysicalPosition, PhysicalSize, Timer, TimerMode,
};

slint::include_modules!();

const PREVIEW_REFRESH_DELAY: Duration = Duration::from_millis(150);
const LIVE_DISPLAY_SECONDS: f64 = 30.0;
const LIVE_PREVIEW_SECONDS: f64 = 13.0;
const LIVE_MEASUREMENT_INTERVAL: Duration = Duration::from_secs(1);
const ABOUT_WINDOW_WIDTH: u32 = 340;
const ABOUT_WINDOW_HEIGHT: u32 = 220;
const SETTINGS_WINDOW_WIDTH: u32 = 440;
const SETTINGS_WINDOW_HEIGHT: u32 = 390;
const LIVE_LEAD_NAMES: [&str; live::LIVE_ECG_LEAD_COUNT] = [
    "I", "II", "III", "aVR", "aVL", "aVF", "V1", "V2", "V3", "V4", "V5", "V6",
];

#[derive(Clone, Copy)]
enum LiveDevice {
    Contec8000G,
    Ecg90A,
}

impl LiveDevice {
    fn from_ui(value: &str) -> Self {
        if matches!(value, "CONTEC ECG90A" | "ECG90A") {
            Self::Ecg90A
        } else {
            Self::Contec8000G
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Contec8000G => "CONTEC 8000G",
            Self::Ecg90A => "CONTEC ECG90A",
        }
    }

    fn sample_interval_seconds(self) -> f64 {
        match self {
            Self::Contec8000G => live::live_sample_interval_seconds(),
            Self::Ecg90A => live::ecg90a_sample_interval_seconds(),
        }
    }

    fn capture_stem(self) -> &'static str {
        match self {
            Self::Contec8000G => "contec8000g-live",
            Self::Ecg90A => "ecg90a-live",
        }
    }
}

struct AppState {
    background_tx: Option<Sender<BackgroundMessage>>,
    document: Option<EcgDocument>,
    clinic_logo: Option<ClinicLogo>,
    clinic_logo_path: Option<PathBuf>,
    live_session: Option<LiveSession>,
    live_rx: Option<Receiver<LiveMessage>>,
    live_recording: bool,
    live_stop_requested: bool,
    live_sample_count: usize,
    recorded_live_document: Option<EcgDocument>,
    pending_open_path: Option<PathBuf>,
    last_file_directory: Option<PathBuf>,
    document_request_id: u64,
    preview_request_id: u64,
    preview_in_flight: bool,
    preview_refresh_pending: bool,
    live_static_key: String,
    live_measurement_lines: Vec<String>,
    live_measurements_at: Option<Instant>,
    language_selection: LanguageSelection,
    language: Language,
}

impl AppState {
    fn new(language_selection: LanguageSelection, language: Language) -> Self {
        Self {
            background_tx: None,
            document: None,
            clinic_logo: None,
            clinic_logo_path: None,
            live_session: None,
            live_rx: None,
            live_recording: false,
            live_stop_requested: false,
            live_sample_count: 0,
            recorded_live_document: None,
            pending_open_path: None,
            last_file_directory: None,
            document_request_id: 0,
            preview_request_id: 0,
            preview_in_flight: false,
            preview_refresh_pending: false,
            live_static_key: String::new(),
            live_measurement_lines: Vec::new(),
            live_measurements_at: None,
            language_selection,
            language,
        }
    }
}

enum RenderedPreview {
    Page {
        svg: String,
        status: String,
    },
    Live {
        static_svg: Option<String>,
        static_key: String,
        signals_svg: String,
        measurement_lines: Option<Vec<String>>,
        status: String,
    },
}

struct LivePreviewPlan {
    reuse_static: bool,
    static_key: String,
    refresh_measurements: bool,
    cached_measurement_lines: Vec<String>,
}

enum BackgroundMessage {
    DocumentLoaded {
        request_id: u64,
        result: Result<EcgDocument, String>,
    },
    PreviewRendered {
        request_id: u64,
        preview: RenderedPreview,
    },
    ExportFinished {
        status: String,
    },
}

fn main() -> Result<(), slint::PlatformError> {
    // No Wayland o KWin ignora o PNG da janela e busca ecg-studio.desktop por este id.
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        slint::BackendSelector::new().select()?;
        slint::set_xdg_app_id("ecg-studio")?;
    }

    let ui = AppWindow::new()?;
    let about_window = Rc::new(RefCell::new(None::<AboutWindow>));
    let settings_window = Rc::new(RefCell::new(None::<SettingsWindow>));
    let (background_tx, background_rx) = mpsc::channel();
    let saved_settings = settings::load_user_settings();
    let language_selection = LanguageSelection::from_code(&saved_settings.language_code);
    let language = i18n::resolve_language(language_selection);
    let state = Rc::new(RefCell::new(AppState::new(language_selection, language)));
    state.borrow_mut().background_tx = Some(background_tx);

    apply_texts(&ui, current_texts(&state));
    ui.set_preview_image(preview::render_empty(
        PageOrientation::Landscape,
        GridTheme::LightSalmon,
        current_texts(&state).page,
    ));
    apply_saved_settings(&ui, &state, saved_settings);
    fill_default_exam_date(&ui);
    install_birth_date_formatter(&ui);
    install_auto_preview_refresh(&ui, &state);
    install_settings_persistence(&ui, &state);
    let _background_timer = install_background_message_drain(&ui, &state, background_rx);
    let _open_file_handler = platform_open::install_open_file_handler();
    let _platform_open_timer = install_platform_open_drain(&ui, &state);

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_open_file(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            let last_directory = last_file_directory(&state);
            if let Some(path) = choose_ecg_file(current_texts(&state), last_directory.as_deref()) {
                start_open_document(&ui, &state, path);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_choose_logo(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            if let Some(path) = choose_logo_file(current_texts(&state)) {
                match load_clinic_logo(path.clone(), current_texts(&state)) {
                    Ok(logo) => {
                        ui.set_clinic_logo_name(logo.display_name.clone().into());
                        {
                            let mut state = state.borrow_mut();
                            state.clinic_logo = Some(logo);
                            state.clinic_logo_path = Some(path);
                        }
                        refresh_preview(&ui, &state);
                    }
                    Err(error) => {
                        let texts = current_texts(&state);
                        ui.set_status_text(
                            format!("{}{error}", texts.status.load_logo_failed_prefix).into(),
                        )
                    }
                }
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_clear_logo(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };
            {
                let mut state = state.borrow_mut();
                state.clinic_logo = None;
                state.clinic_logo_path = None;
            }
            ui.set_clinic_logo_name(current_texts(&state).ui.no_logo.into());
            refresh_preview(&ui, &state);
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_export_ecg(move || {
            if let Some(ui) = ui_weak.upgrade() {
                export_current_ecg(&ui, &state);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_start_live(move || {
            if let Some(ui) = ui_weak.upgrade() {
                start_live_capture(&ui, &state);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_start_recording(move || {
            if let Some(ui) = ui_weak.upgrade() {
                start_live_recording(&ui, &state);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_stop_live(move || {
            if let Some(ui) = ui_weak.upgrade() {
                stop_live_capture(&ui, &state);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        ui.on_print_ecg(move || {
            if let Some(ui) = ui_weak.upgrade() {
                print_current_ecg(&ui, &state);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let about_window = Rc::clone(&about_window);
        let state = Rc::clone(&state);
        ui.on_show_about(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };

            if about_window.borrow().is_none() {
                match AboutWindow::new() {
                    Ok(window) => *about_window.borrow_mut() = Some(window),
                    Err(error) => {
                        let texts = current_texts(&state);
                        ui.set_status_text(
                            format!("{}{error}", texts.status.open_about_failed_prefix).into(),
                        );
                        return;
                    }
                }
            }

            if let Some(window) = about_window.borrow().as_ref() {
                apply_about_window_texts(window, current_texts(&state));
                window
                    .window()
                    .set_size(PhysicalSize::new(ABOUT_WINDOW_WIDTH, ABOUT_WINDOW_HEIGHT));
                center_about_window(&ui, window);
                if let Err(error) = window.show() {
                    let texts = current_texts(&state);
                    ui.set_status_text(
                        format!("{}{error}", texts.status.open_about_failed_prefix).into(),
                    );
                    return;
                }
                center_about_window(&ui, window);
            }
        });
    }

    {
        let ui_weak = ui.as_weak();
        let settings_window = Rc::clone(&settings_window);
        let state = Rc::clone(&state);
        ui.on_show_settings(move || {
            let Some(ui) = ui_weak.upgrade() else {
                return;
            };

            if settings_window.borrow().is_none() {
                match SettingsWindow::new() {
                    Ok(window) => {
                        install_settings_window_callbacks(&ui, &state, &window);
                        *settings_window.borrow_mut() = Some(window);
                    }
                    Err(error) => {
                        let texts = current_texts(&state);
                        ui.set_status_text(
                            format!("{}{error}", texts.status.open_settings_failed_prefix).into(),
                        );
                        return;
                    }
                }
            }

            if let Some(window) = settings_window.borrow().as_ref() {
                apply_settings_window_texts(window, current_texts(&state));
                window.set_language_value(state.borrow().language_selection.label().into());
                window.window().set_size(PhysicalSize::new(
                    SETTINGS_WINDOW_WIDTH,
                    SETTINGS_WINDOW_HEIGHT,
                ));
                center_settings_window(&ui, window);
                if let Err(error) = window.show() {
                    let texts = current_texts(&state);
                    ui.set_status_text(
                        format!("{}{error}", texts.status.open_settings_failed_prefix).into(),
                    );
                    return;
                }
                center_settings_window(&ui, window);
            }
        });
    }

    let live_timer = Timer::default();
    {
        let ui_weak = ui.as_weak();
        let state = Rc::clone(&state);
        live_timer.start(TimerMode::Repeated, Duration::from_millis(250), move || {
            if let Some(ui) = ui_weak.upgrade() {
                drain_live_messages(&ui, &state);
            }
        });
    }

    if let Some(path) = initial_open_path_from_args(std::env::args_os()) {
        start_open_document(&ui, &state, path);
    }

    let result = ui.run();
    save_current_settings(&ui, &state);
    result
}

fn center_about_window(ui: &AppWindow, about: &AboutWindow) {
    let owner_window = ui.window();
    let owner_position = owner_window.position();
    let owner_size = owner_window.size();
    if owner_size.width == 0 || owner_size.height == 0 {
        return;
    }

    let about_window = about.window();
    let about_size = about_window.size();
    let about_width = if about_size.width == 0 {
        ABOUT_WINDOW_WIDTH
    } else {
        about_size.width
    };
    let about_height = if about_size.height == 0 {
        ABOUT_WINDOW_HEIGHT
    } else {
        about_size.height
    };

    let x = owner_position.x + (owner_size.width as i32 - about_width as i32) / 2;
    let y = owner_position.y + (owner_size.height as i32 - about_height as i32) / 2;
    about_window.set_position(PhysicalPosition::new(x, y));
}

fn center_settings_window(ui: &AppWindow, settings: &SettingsWindow) {
    let owner_window = ui.window();
    let owner_position = owner_window.position();
    let owner_size = owner_window.size();
    if owner_size.width == 0 || owner_size.height == 0 {
        return;
    }

    let settings_window = settings.window();
    let settings_size = settings_window.size();
    let settings_width = if settings_size.width == 0 {
        SETTINGS_WINDOW_WIDTH
    } else {
        settings_size.width
    };
    let settings_height = if settings_size.height == 0 {
        SETTINGS_WINDOW_HEIGHT
    } else {
        settings_size.height
    };

    let x = owner_position.x + (owner_size.width as i32 - settings_width as i32) / 2;
    let y = owner_position.y + (owner_size.height as i32 - settings_height as i32) / 2;
    settings_window.set_position(PhysicalPosition::new(x, y));
}

fn current_texts(state: &Rc<RefCell<AppState>>) -> &'static Texts {
    i18n::texts(state.borrow().language)
}

fn apply_texts(ui: &AppWindow, texts: &Texts) {
    let filter_key = filter_key_from_ui(ui);
    let orientation_key = orientation_key_from_ui(ui);
    let grid_theme_key = grid_theme_key_from_ui(ui);

    ui.set_text_live_capture(texts.ui.live_capture.into());
    ui.set_text_device(texts.ui.device.into());
    ui.set_text_recording(texts.ui.recording.into());
    ui.set_text_record(texts.ui.record.into());
    ui.set_text_capturing(texts.ui.capturing.into());
    ui.set_text_start_live(texts.ui.start_live.into());
    ui.set_text_stop(texts.ui.stop.into());
    ui.set_text_header(texts.ui.header.into());
    ui.set_text_clinic(texts.ui.clinic.into());
    ui.set_text_physician(texts.ui.physician.into());
    ui.set_text_physician_placeholder(texts.ui.physician_placeholder.into());
    ui.set_text_logo(texts.ui.logo.into());
    ui.set_text_choose(texts.ui.choose.into());
    ui.set_text_remove(texts.ui.remove.into());
    ui.set_text_patient(texts.ui.patient.into());
    ui.set_text_patient_placeholder(texts.ui.patient_placeholder.into());
    ui.set_text_birth_date(texts.ui.birth_date.into());
    ui.set_text_birth_date_placeholder(texts.ui.birth_date_placeholder.into());
    ui.set_text_exam_date(texts.ui.exam_date.into());
    ui.set_text_exam_date_placeholder(texts.ui.exam_date_placeholder.into());
    ui.set_text_view(texts.ui.view.into());
    ui.set_text_filter(texts.ui.filter.into());
    ui.set_text_orientation(texts.ui.orientation.into());
    ui.set_text_grid(texts.ui.grid.into());
    ui.set_text_printed_leads(texts.ui.printed_leads.into());
    ui.set_text_calibration_pulse(texts.ui.calibration_pulse.into());
    ui.set_text_save_label(texts.ui.save_label.into());
    ui.set_text_open(texts.ui.open.into());
    ui.set_text_save(texts.ui.save.into());
    ui.set_text_print(texts.ui.print.into());
    ui.set_text_about(texts.ui.about.into());
    ui.set_text_settings(texts.ui.settings.into());
    ui.set_filter_none_text(texts.ui.filter_none.into());
    ui.set_filter_baseline_text(texts.ui.filter_baseline.into());
    ui.set_filter_low_pass_text(texts.ui.filter_low_pass.into());
    ui.set_filter_diagnostic_text(texts.ui.filter_diagnostic.into());
    ui.set_filter_monitor_text(texts.ui.filter_monitor.into());
    ui.set_orientation_landscape_text(texts.ui.orientation_landscape.into());
    ui.set_orientation_portrait_text(texts.ui.orientation_portrait.into());
    ui.set_grid_light_salmon_text(texts.ui.grid_light_salmon.into());
    ui.set_grid_technical_gray_text(texts.ui.grid_technical_gray.into());

    apply_filter_key(ui, texts, &filter_key);
    apply_orientation_key(ui, texts, &orientation_key);
    apply_grid_theme_key(ui, texts, &grid_theme_key);
}

fn filter_key_from_ui(ui: &AppWindow) -> String {
    filter_key_from_value(ui.get_filter_value().as_str()).to_owned()
}

fn orientation_key_from_ui(ui: &AppWindow) -> String {
    orientation_key_from_value(ui.get_orientation_value().as_str()).to_owned()
}

fn grid_theme_key_from_ui(ui: &AppWindow) -> String {
    grid_theme_key_from_value(ui.get_grid_theme_value().as_str()).to_owned()
}

fn filter_key_from_value(value: &str) -> &'static str {
    match value.trim() {
        "none" => return "none",
        "baseline" => return "baseline",
        "low_pass_40" => return "low_pass_40",
        "diagnostic" => return "diagnostic",
        "monitor" => return "monitor",
        "Diagnostico" => return "diagnostic",
        _ => {}
    }

    for texts in i18n::all_texts() {
        let ui = texts.ui;
        if value == ui.filter_baseline {
            return "baseline";
        }
        if value == ui.filter_low_pass {
            return "low_pass_40";
        }
        if value == ui.filter_diagnostic {
            return "diagnostic";
        }
        if value == ui.filter_monitor {
            return "monitor";
        }
        if value == ui.filter_none {
            return "none";
        }
    }
    "none"
}

fn orientation_key_from_value(value: &str) -> &'static str {
    match value.trim() {
        "portrait" => return "portrait",
        "landscape" => return "landscape",
        _ => {}
    }

    for texts in i18n::all_texts() {
        let ui = texts.ui;
        if value == ui.orientation_portrait {
            return "portrait";
        }
        if value == ui.orientation_landscape {
            return "landscape";
        }
    }
    "landscape"
}

fn grid_theme_key_from_value(value: &str) -> &'static str {
    match value.trim() {
        "light_salmon" | "lightSalmon" => return "light_salmon",
        "technical_gray" => return "technical_gray",
        _ => {}
    }

    for texts in i18n::all_texts() {
        let ui = texts.ui;
        if value == ui.grid_technical_gray {
            return "technical_gray";
        }
        if value == ui.grid_light_salmon {
            return "light_salmon";
        }
    }
    "light_salmon"
}

fn apply_filter_key(ui: &AppWindow, texts: &Texts, key: &str) {
    let label = match FilterMode::from_key(key) {
        FilterMode::Baseline => texts.ui.filter_baseline,
        FilterMode::LowPass40Hz => texts.ui.filter_low_pass,
        FilterMode::Diagnostic => texts.ui.filter_diagnostic,
        FilterMode::Monitor => texts.ui.filter_monitor,
        FilterMode::None => texts.ui.filter_none,
    };
    ui.set_filter_value(label.into());
}

fn apply_orientation_key(ui: &AppWindow, texts: &Texts, key: &str) {
    let label = match PageOrientation::from_key(key) {
        PageOrientation::Portrait => texts.ui.orientation_portrait,
        PageOrientation::Landscape => texts.ui.orientation_landscape,
    };
    ui.set_orientation_value(label.into());
}

fn apply_grid_theme_key(ui: &AppWindow, texts: &Texts, key: &str) {
    let label = match GridTheme::from_key(key) {
        GridTheme::LightSalmon => texts.ui.grid_light_salmon,
        GridTheme::TechnicalGray => texts.ui.grid_technical_gray,
    };
    ui.set_grid_theme_value(label.into());
}

fn is_known_no_logo_label(value: &str) -> bool {
    i18n::all_texts()
        .iter()
        .any(|texts| value == texts.ui.no_logo)
}

fn is_known_no_file_loaded_label(value: &str) -> bool {
    i18n::all_texts()
        .iter()
        .any(|texts| value == texts.ui.no_file_loaded)
}

fn apply_about_window_texts(window: &AboutWindow, texts: &Texts) {
    window.set_about_title(texts.ui.about_title.into());
    window.set_version_text(texts.ui.version.into());
    window.set_made_with_text(texts.ui.made_with.into());
}

fn apply_settings_window_texts(window: &SettingsWindow, texts: &Texts) {
    window.set_settings_title(texts.ui.settings_title.into());
    window.set_language_label(texts.ui.language.into());
    window.set_save_text(texts.ui.save.into());
    window.set_cancel_text(texts.ui.cancel.into());
}

fn install_settings_window_callbacks(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    settings_window: &SettingsWindow,
) {
    let ui_weak = ui.as_weak();
    let window_weak = settings_window.as_weak();
    let state = Rc::clone(state);
    settings_window.on_save_settings(move |language| {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let selection = LanguageSelection::from_label(language.as_str());
        {
            let mut state = state.borrow_mut();
            state.language_selection = selection;
            state.language = i18n::resolve_language(selection);
        }
        let texts = current_texts(&state);
        apply_texts(&ui, texts);
        if ui.get_clinic_name().trim().is_empty() {
            ui.set_clinic_name(texts.ui.clinic_default.into());
        }
        if ui.get_clinic_logo_name().trim().is_empty()
            || is_known_no_logo_label(&ui.get_clinic_logo_name())
        {
            ui.set_clinic_logo_name(texts.ui.no_logo.into());
        }
        if ui.get_file_name().trim().is_empty()
            || is_known_no_file_loaded_label(&ui.get_file_name())
        {
            ui.set_file_name(texts.ui.no_file_loaded.into());
        }
        refresh_preview(&ui, &state);
        save_current_settings(&ui, &state);
        ui.set_status_text(texts.status.settings_saved.into());
        if let Some(window) = window_weak.upgrade() {
            window.hide().ok();
        }
    });

    let window_weak = settings_window.as_weak();
    settings_window.on_cancel_settings(move || {
        if let Some(window) = window_weak.upgrade() {
            window.hide().ok();
        }
    });
}

fn apply_saved_settings(ui: &AppWindow, state: &Rc<RefCell<AppState>>, saved: UserSettings) {
    let texts = current_texts(state);
    ui.set_clinic_logo_name(texts.ui.no_logo.into());
    ui.set_file_name(texts.ui.no_file_loaded.into());
    ui.set_status_text(texts.ui.empty_status.into());
    ui.set_live_sample_text(texts.samples(0).into());
    ui.set_recorded_sample_text(texts.recorded_samples(0).into());
    if !saved.clinic_name.trim().is_empty() {
        ui.set_clinic_name(saved.clinic_name.into());
    } else {
        ui.set_clinic_name(texts.ui.clinic_default.into());
    }
    if !saved.physician_name.trim().is_empty() {
        ui.set_physician_name(saved.physician_name.into());
    }
    if !saved.live_device_value.trim().is_empty() {
        ui.set_live_device_value(saved_live_device_label(&saved.live_device_value).into());
    }
    apply_filter_key(ui, texts, filter_key_from_value(&saved.filter_value));
    if !saved.grid_theme_value.trim().is_empty() {
        apply_grid_theme_key(
            ui,
            texts,
            grid_theme_key_from_value(&saved.grid_theme_value),
        );
    }
    if !saved.selected_leads_value.trim().is_empty() {
        apply_selected_leads_value(ui, &saved.selected_leads_value);
    }
    ui.set_show_calibration(saved.show_calibration);
    if let Some(window) = saved.window {
        ui.window()
            .set_size(PhysicalSize::new(window.width, window.height));
        ui.window()
            .set_position(PhysicalPosition::new(window.x, window.y));
        if window.maximized {
            ui.window().set_maximized(true);
        }
    }
    if let Some(path) = saved
        .last_file_directory
        .filter(|path| !path.as_os_str().is_empty() && path.is_dir())
    {
        state.borrow_mut().last_file_directory = Some(path);
    }
    if let Some(path) = saved.clinic_logo_path {
        match load_clinic_logo(path.clone(), texts) {
            Ok(logo) => {
                ui.set_clinic_logo_name(logo.display_name.clone().into());
                let mut state = state.borrow_mut();
                state.clinic_logo = Some(logo);
                state.clinic_logo_path = Some(path);
            }
            Err(error) => ui.set_status_text(
                format!("{}{error}", texts.status.saved_logo_load_failed_prefix).into(),
            ),
        }
    }
}

fn install_settings_persistence(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let ui_weak = ui.as_weak();
    let state = Rc::clone(state);
    ui.window().on_close_requested(move || {
        if let Some(ui) = ui_weak.upgrade() {
            save_current_settings(&ui, &state);
        }
        CloseRequestResponse::HideWindow
    });
}

fn install_background_message_drain(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    background_rx: Receiver<BackgroundMessage>,
) -> Timer {
    let timer = Timer::default();
    let ui_weak = ui.as_weak();
    let state = Rc::clone(state);
    timer.start(TimerMode::Repeated, Duration::from_millis(50), move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };

        while let Ok(message) = background_rx.try_recv() {
            match message {
                BackgroundMessage::DocumentLoaded { request_id, result } => {
                    finish_open_document(&ui, &state, request_id, result);
                }
                BackgroundMessage::PreviewRendered {
                    request_id,
                    preview,
                } => {
                    finish_preview_refresh(&ui, &state, request_id, preview);
                }
                BackgroundMessage::ExportFinished { status } => {
                    ui.set_status_text(status.into());
                }
            }
        }
    });
    timer
}

fn install_platform_open_drain(ui: &AppWindow, state: &Rc<RefCell<AppState>>) -> Timer {
    let timer = Timer::default();
    let ui_weak = ui.as_weak();
    let state = Rc::clone(state);
    timer.start(TimerMode::Repeated, Duration::from_millis(150), move || {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        if let Some(path) = platform_open::take_open_file_paths().into_iter().next() {
            start_open_document(&ui, &state, path);
        }
    });
    timer
}

fn install_auto_preview_refresh(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let refresh_timer = Rc::new(Timer::default());
    let ui_weak = ui.as_weak();
    let state = Rc::clone(state);
    ui.on_preview_inputs_changed(move || {
        let ui_weak = ui_weak.clone();
        let state = Rc::clone(&state);
        refresh_timer.start(TimerMode::SingleShot, PREVIEW_REFRESH_DELAY, move || {
            if let Some(ui) = ui_weak.upgrade() {
                refresh_preview(&ui, &state);
            }
        });
    });
}

fn install_birth_date_formatter(ui: &AppWindow) {
    let ui_weak = ui.as_weak();
    ui.on_format_birth_date(move |value| {
        let Some(ui) = ui_weak.upgrade() else {
            return;
        };
        let current = value.to_string();
        let formatted = format_birth_date_input(&current);
        if formatted != ui.get_patient_birth_date().as_str() {
            ui.set_patient_birth_date(formatted.into());
        }
    });
}

fn save_current_settings(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let window = ui.window();
    let position = window.position();
    let size = window.size();
    let (clinic_logo_path, language_code, last_file_directory) = {
        let state = state.borrow();
        (
            state.clinic_logo_path.clone(),
            state.language_selection.code().to_owned(),
            state.last_file_directory.clone(),
        )
    };
    let settings = UserSettings {
        clinic_name: ui.get_clinic_name().trim().to_owned(),
        physician_name: ui.get_physician_name().trim().to_owned(),
        clinic_logo_path,
        live_device_value: ui.get_live_device_value().trim().to_owned(),
        filter_value: filter_key_from_ui(ui),
        grid_theme_value: grid_theme_key_from_ui(ui),
        selected_leads_value: selected_leads_value_from_ui(ui),
        show_calibration: ui.get_show_calibration(),
        language_code,
        last_file_directory,
        window: Some(WindowSettings {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
            maximized: window.is_maximized(),
        }),
    };

    if let Err(error) = settings::save_user_settings(&settings) {
        ui.set_status_text(error.into());
    }
}

fn choose_ecg_file(texts: &Texts, last_directory: Option<&Path>) -> Option<PathBuf> {
    file_dialog_with_last_directory(last_directory)
        .set_title(texts.status.open_ecg_title)
        .add_filter("ECG", &["xml", "aecg", "hl7", "c8k", "ecg", "dcm", "dicom"])
        .add_filter(texts.status.all_files_filter, &["*"])
        .pick_file()
}

fn file_dialog_with_last_directory(last_directory: Option<&Path>) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new();
    if let Some(directory) = last_directory.filter(|path| path.is_dir()) {
        dialog = dialog.set_directory(directory);
    }
    dialog
}

fn last_file_directory(state: &Rc<RefCell<AppState>>) -> Option<PathBuf> {
    state
        .borrow()
        .last_file_directory
        .clone()
        .filter(|path| path.is_dir())
}

fn remember_file_directory(ui: &AppWindow, state: &Rc<RefCell<AppState>>, path: &Path) {
    let Some(directory) = path
        .parent()
        .filter(|directory| !directory.as_os_str().is_empty() && directory.is_dir())
        .map(Path::to_path_buf)
    else {
        return;
    };

    state.borrow_mut().last_file_directory = Some(directory);
    save_current_settings(ui, state);
}

fn initial_open_path_from_args<I, S>(args: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    args.into_iter()
        .skip(1)
        .filter_map(|arg| open_path_from_arg(arg.into()))
        .next()
}

fn open_path_from_arg(arg: OsString) -> Option<PathBuf> {
    if arg.is_empty() || is_launcher_argument(&arg) {
        return None;
    }
    path_from_file_uri(&arg).or_else(|| Some(PathBuf::from(arg)))
}

fn is_launcher_argument(arg: &OsStr) -> bool {
    let value = arg.to_string_lossy();
    value == "--" || value.starts_with("-psn_")
}

fn path_from_file_uri(arg: &OsStr) -> Option<PathBuf> {
    let uri = arg.to_str()?.strip_prefix("file://")?;
    let uri_path = uri.strip_prefix("localhost").unwrap_or(uri);
    if uri_path.is_empty() {
        return None;
    }
    let decoded = percent_decode(uri_path)?;
    file_uri_path_to_path_buf(decoded)
}

#[cfg(windows)]
fn file_uri_path_to_path_buf(mut path: String) -> Option<PathBuf> {
    if path.starts_with('/') && path.as_bytes().get(2) == Some(&b':') {
        path.remove(0);
    } else if !path.starts_with('/') {
        path = format!("//{path}");
    }
    Some(PathBuf::from(path.replace('/', "\\")))
}

#[cfg(not(windows))]
fn file_uri_path_to_path_buf(path: String) -> Option<PathBuf> {
    if path.starts_with('/') {
        Some(PathBuf::from(path))
    } else {
        None
    }
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn choose_logo_file(texts: &Texts) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(texts.status.choose_logo_title)
        .add_filter(texts.status.image_filter, &["png", "jpg", "jpeg", "bmp"])
        .add_filter(texts.status.all_files_filter, &["*"])
        .pick_file()
}

fn load_clinic_logo(path: PathBuf, texts: &Texts) -> Result<ClinicLogo, String> {
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|error| format!("{} ({error})", texts.status.invalid_image_format_prefix))?
        .to_rgba8();
    let (width, height) = decoded.dimensions();
    let mime_type = logo_mime_type(&path, texts)?;
    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("logotipo")
        .to_owned();

    Ok(ClinicLogo::from_bytes(
        display_name,
        mime_type,
        &bytes,
        Some(LogoBitmap {
            width,
            height,
            rgba: decoded.into_raw(),
        }),
    ))
}

fn logo_mime_type(path: &std::path::Path, texts: &Texts) -> Result<&'static str, String> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => Ok("image/png"),
        Some("jpg") | Some("jpeg") => Ok("image/jpeg"),
        Some("bmp") => Ok("image/bmp"),
        _ => Err(texts.status.unsupported_logo_format.to_owned()),
    }
}

fn start_open_document(ui: &AppWindow, state: &Rc<RefCell<AppState>>, path: PathBuf) {
    let texts = current_texts(state);
    remember_file_directory(ui, state, &path);
    if state.borrow().live_session.is_some() {
        state.borrow_mut().pending_open_path = Some(path);
        stop_live_capture(ui, state);
        ui.set_status_text(texts.status.stopping_live.into());
        return;
    }

    let (request_id, background_tx) = {
        let mut state = state.borrow_mut();
        state.document_request_id = state.document_request_id.wrapping_add(1);
        let Some(background_tx) = state.background_tx.clone() else {
            ui.set_status_text(texts.status.background_queue_unavailable.into());
            return;
        };
        (state.document_request_id, background_tx)
    };

    let display_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("(sem nome)")
        .to_owned();
    ui.set_status_text(format!("{}{display_name}...", texts.status.loading_ecg_prefix).into());

    thread::spawn(move || {
        let result = readers::read_ecg(&path).map_err(|error| error.to_string());
        let _ = background_tx.send(BackgroundMessage::DocumentLoaded { request_id, result });
    });
}

fn finish_open_document(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    request_id: u64,
    result: Result<EcgDocument, String>,
) {
    let texts = current_texts(state);
    if state.borrow().document_request_id != request_id {
        return;
    }

    match result {
        Ok(mut document) => {
            if document.clinic_name.trim().is_empty()
                || matches!(document.clinic_name.as_str(), "Clinica" | "Clínica")
            {
                document.clinic_name = ui.get_clinic_name().trim().to_owned();
            } else {
                ui.set_clinic_name(document.clinic_name.clone().into());
            }
            if document.physician_name.trim().is_empty() {
                document.physician_name = ui.get_physician_name().trim().to_owned();
            } else {
                ui.set_physician_name(document.physician_name.clone().into());
            }
            ui.set_patient_name(document.patient_name.clone().into());
            ui.set_patient_birth_date(document.patient_birth_date.clone().into());
            document.exam_date = exam_date_for_ui(&document.exam_date);
            ui.set_exam_date(document.exam_date.clone().into());
            ui.set_file_name(document.display_file_name().into());
            ui.set_live_running(false);
            ui.set_live_recording(false);
            ui.set_live_sample_text(texts.samples(0).into());
            ui.set_recorded_sample_text(texts.recorded_samples(0).into());
            ui.set_status_text(status_for_document(&document, texts).into());
            {
                let mut state = state.borrow_mut();
                state.document = Some(document);
                state.live_recording = false;
                state.live_stop_requested = false;
                state.live_sample_count = 0;
                state.recorded_live_document = None;
                state.pending_open_path = None;
                invalidate_preview_jobs(&mut state);
            }
            refresh_preview(ui, state);
        }
        Err(error) => {
            ui.set_status_text(format!("{}{error}", texts.status.error_prefix).into());
        }
    }
}

fn invalidate_preview_jobs(state: &mut AppState) {
    state.preview_request_id = state.preview_request_id.wrapping_add(1);
    state.preview_in_flight = false;
    state.preview_refresh_pending = false;
    state.live_static_key.clear();
    state.live_measurement_lines.clear();
    state.live_measurements_at = None;
}

fn refresh_preview(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let fallback_state = Rc::clone(state);
    let texts = current_texts(state);
    let (document, clinic_logo, request_id, background_tx) = {
        let mut state = state.borrow_mut();
        let Some(document) = state.document.clone() else {
            invalidate_preview_jobs(&mut state);
            let orientation = PageOrientation::from_key(&orientation_key_from_ui(ui));
            let grid_theme = GridTheme::from_key(&grid_theme_key_from_ui(ui));
            ui.set_preview_image(preview::render_empty(orientation, grid_theme, texts.page));
            ui.set_trace_visible(false);
            ui.set_status_text(texts.ui.empty_status.into());
            return;
        };
        if state.preview_in_flight {
            state.preview_refresh_pending = true;
            return;
        }
        let Some(background_tx) = state.background_tx.clone() else {
            state.preview_in_flight = false;
            state.preview_refresh_pending = false;
            drop(state);
            refresh_preview_sync(ui, &fallback_state);
            return;
        };
        state.preview_request_id = state.preview_request_id.wrapping_add(1);
        state.preview_in_flight = true;
        state.preview_refresh_pending = false;
        (
            document,
            state.clinic_logo.clone(),
            state.preview_request_id,
            background_tx,
        )
    };

    let mut document = document;
    sync_document_from_ui(ui, &mut document);
    let filter = FilterMode::from_key(&filter_key_from_ui(ui));
    let selected_leads = selected_leads_from_ui(ui);
    let options = current_render_options(ui, clinic_logo, texts);
    let live_plan = live_preview_plan(state, &document, &selected_leads, &options);
    let texts = *texts;

    thread::spawn(move || {
        let preview = render_preview_image(
            &document,
            filter,
            &selected_leads,
            &options,
            &texts,
            live_plan,
        );
        let _ = background_tx.send(BackgroundMessage::PreviewRendered {
            request_id,
            preview,
        });
    });
}

fn finish_preview_refresh(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    request_id: u64,
    preview: RenderedPreview,
) {
    let should_refresh_again = {
        let mut state = state.borrow_mut();
        if state.preview_request_id != request_id {
            return;
        }
        state.preview_in_flight = false;
        let should_refresh_again = state.preview_refresh_pending;
        state.preview_refresh_pending = false;
        should_refresh_again
    };

    present_preview(ui, state, preview);

    if should_refresh_again {
        refresh_preview(ui, state);
    }
}

fn refresh_preview_sync(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let texts = current_texts(state);
    let (document, clinic_logo) = {
        let state = state.borrow();
        (state.document.clone(), state.clinic_logo.clone())
    };
    let Some(mut document) = document else {
        let orientation = PageOrientation::from_key(&orientation_key_from_ui(ui));
        let grid_theme = GridTheme::from_key(&grid_theme_key_from_ui(ui));
        ui.set_preview_image(preview::render_empty(orientation, grid_theme, texts.page));
        ui.set_trace_visible(false);
        ui.set_status_text(texts.ui.empty_status.into());
        return;
    };

    sync_document_from_ui(ui, &mut document);
    let filter = FilterMode::from_key(&filter_key_from_ui(ui));
    let selected_leads = selected_leads_from_ui(ui);
    let options = current_render_options(ui, clinic_logo, texts);
    let live_plan = live_preview_plan(state, &document, &selected_leads, &options);
    let preview = render_preview_image(
        &document,
        filter,
        &selected_leads,
        &options,
        texts,
        live_plan,
    );
    present_preview(ui, state, preview);
}

fn live_preview_plan(
    state: &Rc<RefCell<AppState>>,
    document: &EcgDocument,
    selected_leads: &[&str],
    options: &preview::RenderOptions,
) -> Option<LivePreviewPlan> {
    if !document.kind.is_live() {
        return None;
    }

    let static_key = live_static_key(document, selected_leads, options);
    let state = state.borrow();
    let reuse_static = state.live_static_key == static_key;
    let refresh_measurements = !reuse_static
        || state
            .live_measurements_at
            .is_none_or(|instant| instant.elapsed() >= LIVE_MEASUREMENT_INTERVAL);
    Some(LivePreviewPlan {
        reuse_static,
        static_key,
        refresh_measurements,
        cached_measurement_lines: state.live_measurement_lines.clone(),
    })
}

fn live_static_key(
    document: &EcgDocument,
    selected_leads: &[&str],
    options: &preview::RenderOptions,
) -> String {
    let orientation = match options.orientation {
        PageOrientation::Portrait => "portrait",
        PageOrientation::Landscape => "landscape",
    };
    let theme = match options.grid_theme {
        GridTheme::TechnicalGray => "technical_gray",
        GridTheme::LightSalmon => "light_salmon",
    };
    let logo = options
        .clinic_logo
        .as_ref()
        .map(|logo| logo.display_name.as_str())
        .unwrap_or("");
    let mut key = format!(
        "{orientation}|{theme}|{}|{logo}|{}|{}|{}|{}|{}|{}|{}",
        options.show_calibration,
        document.clinic_name,
        document.physician_name,
        document.patient_name,
        document.patient_birth_date,
        document.exam_date,
        options.texts.patient,
        options.texts.no_leads,
    );
    for lead in &document.leads {
        key.push('|');
        key.push_str(&lead.name);
    }
    for name in selected_leads {
        key.push('|');
        key.push_str(name);
    }
    key
}

fn render_preview_image(
    document: &EcgDocument,
    filter: FilterMode,
    selected_leads: &[&str],
    options: &preview::RenderOptions,
    texts: &Texts,
    live_plan: Option<LivePreviewPlan>,
) -> RenderedPreview {
    let Some(live_plan) = live_plan else {
        let filtered = processing::apply_filter(document, filter);
        let displayed = document_with_selected_leads(&filtered, selected_leads);
        let status = status_for_document(&displayed, texts);
        let svg =
            preview::render_document_svg_with_measurements(&displayed, document, options.clone());
        return RenderedPreview::Page { svg, status };
    };

    let filtered_source = preview_samples_for_live_filter(document);
    let filtered = processing::apply_filter(&filtered_source, filter);
    let displayed = document_with_selected_leads(&filtered, selected_leads);
    let status = texts.status_for_document(
        document.kind.label(),
        displayed.leads.len(),
        document.sample_rate_hz(),
        document.duration_seconds(),
    );
    let measurement_lines = live_plan
        .refresh_measurements
        .then(|| preview::live_measurement_lines(document, options));
    let draw_lines = measurement_lines
        .as_deref()
        .unwrap_or(live_plan.cached_measurement_lines.as_slice());
    let signals_svg = preview::render_live_signals_svg(&displayed, options, draw_lines);
    let static_svg = if live_plan.reuse_static {
        None
    } else {
        Some(preview::render_live_static_svg(&displayed, options))
    };
    RenderedPreview::Live {
        static_svg,
        static_key: live_plan.static_key,
        signals_svg,
        measurement_lines,
        status,
    }
}

fn preview_samples_for_live_filter(document: &EcgDocument) -> EcgDocument {
    let mut clipped = document.clone();
    if document.sample_interval_seconds > 0.0 {
        let max_samples =
            (LIVE_PREVIEW_SECONDS / document.sample_interval_seconds).round() as usize;
        trim_live_display_document(&mut clipped, max_samples);
    }
    clipped
}

fn present_preview(ui: &AppWindow, state: &Rc<RefCell<AppState>>, preview: RenderedPreview) {
    match preview {
        RenderedPreview::Page { svg, status } => {
            {
                let mut state = state.borrow_mut();
                state.live_static_key.clear();
                state.live_measurement_lines.clear();
                state.live_measurements_at = None;
            }
            ui.set_preview_image(preview::image_from_svg(&svg));
            ui.set_trace_visible(false);
            ui.set_status_text(status.into());
        }
        RenderedPreview::Live {
            static_svg,
            static_key,
            signals_svg,
            measurement_lines,
            status,
        } => {
            {
                let mut state = state.borrow_mut();
                if static_svg.is_some() {
                    state.live_static_key = static_key;
                }
                if let Some(lines) = measurement_lines {
                    state.live_measurement_lines = lines;
                    state.live_measurements_at = Some(Instant::now());
                }
            }
            if let Some(svg) = static_svg.as_deref() {
                ui.set_preview_image(preview::image_from_svg(svg));
            }
            ui.set_trace_image(preview::image_from_svg(&signals_svg));
            ui.set_trace_visible(true);
            ui.set_status_text(status.into());
        }
    }
}

fn export_current_ecg(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let texts = current_texts(state);
    let Some(mut document) = output_document(state) else {
        let status = if state
            .borrow()
            .document
            .as_ref()
            .is_some_and(|document| document.kind.is_live())
        {
            texts.status.no_live_recording_export
        } else {
            texts.status.no_ecg_export
        };
        ui.set_status_text(status.into());
        return;
    };
    sync_document_from_ui(ui, &mut document);

    let format = ExportFormat::from_key(export_format_key_from_ui(ui));
    let last_directory = last_file_directory(state);
    let Some(path) = choose_export_path(format, &document, texts, last_directory.as_deref()) else {
        return;
    };
    remember_file_directory(ui, state, &path);
    let Some(background_tx) = state.borrow().background_tx.clone() else {
        ui.set_status_text(texts.status.background_queue_unavailable.into());
        return;
    };

    ui.set_status_text(format!("{}{}...", texts.status.saving_ecg_prefix, path.display()).into());
    let texts = *texts;
    thread::spawn(move || {
        let status = write_exported_ecg(format, &document, path, &texts);
        let _ = background_tx.send(BackgroundMessage::ExportFinished { status });
    });
}

fn output_document(state: &Rc<RefCell<AppState>>) -> Option<EcgDocument> {
    let state = state.borrow();
    let document = state.document.as_ref()?;
    if document.kind.is_live() {
        state
            .recorded_live_document
            .as_ref()
            .filter(|recorded| recorded.max_sample_count() > 0)
            .cloned()
    } else {
        Some(document.clone())
    }
}

#[derive(Clone, Copy)]
enum ExportFormat {
    Hl7Aecg,
    DicomEcg,
}

impl ExportFormat {
    fn from_key(key: &str) -> Self {
        match key {
            "hl7_aecg" => Self::Hl7Aecg,
            _ => Self::DicomEcg,
        }
    }
}

fn export_format_key_from_ui(ui: &AppWindow) -> &'static str {
    match ui.get_export_format_value().as_str() {
        "HL7 aECG" => "hl7_aecg",
        _ => "dicom_ecg",
    }
}

fn choose_export_path(
    format: ExportFormat,
    document: &EcgDocument,
    texts: &Texts,
    last_directory: Option<&Path>,
) -> Option<PathBuf> {
    match format {
        ExportFormat::Hl7Aecg => file_dialog_with_last_directory(last_directory)
            .set_title(texts.status.save_hl7_title)
            .set_file_name(default_export_name(document, "aecg", "xml"))
            .add_filter("HL7 aECG", &["xml", "aecg", "hl7"])
            .save_file(),
        ExportFormat::DicomEcg => file_dialog_with_last_directory(last_directory)
            .set_title(texts.status.save_dicom_title)
            .set_file_name(default_export_name(document, "dicom-ecg", "dcm"))
            .add_filter("DICOM-ECG", &["dcm", "dicom"])
            .save_file(),
    }
}

fn write_exported_ecg(
    format: ExportFormat,
    document: &EcgDocument,
    path: PathBuf,
    texts: &Texts,
) -> String {
    match format {
        ExportFormat::Hl7Aecg => {
            let xml = exporting::build_hl7_aecg(document);
            match fs::write(&path, xml) {
                Ok(()) => format!("{}{}", texts.status.hl7_saved_prefix, path.display()),
                Err(error) => format!("{}{error}", texts.status.hl7_save_failed_prefix),
            }
        }
        ExportFormat::DicomEcg => {
            match exporting::build_dicom_ecg(document).and_then(|bytes| {
                fs::write(&path, bytes)
                    .map_err(|error| format!("{}{error}", texts.status.dicom_write_failed_prefix))
            }) {
                Ok(()) => format!("{}{}", texts.status.dicom_saved_prefix, path.display()),
                Err(error) => error,
            }
        }
    }
}

fn default_export_name(document: &EcgDocument, suffix: &str, extension: &str) -> String {
    let patient = filename_part_or_default(&document.patient_name, "paciente");
    let exam_date = filename_part_or_default(&document.exam_date, "sem-data");
    format!("{patient}-{exam_date}-{suffix}.{extension}")
}

fn filename_part_or_default(value: &str, fallback: &str) -> String {
    let part = sanitize_filename_part(value);
    if part.is_empty() {
        fallback.to_owned()
    } else {
        part
    }
}

fn sanitize_filename_part(value: &str) -> String {
    let mut part = String::new();
    let mut last_was_separator = false;

    for ch in value.trim().chars() {
        let replacement = if ch.is_ascii_control()
            || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*')
        {
            '-'
        } else {
            ch
        };

        if replacement.is_whitespace() {
            if !last_was_separator {
                part.push(' ');
                last_was_separator = true;
            }
        } else if replacement == '-' {
            if !last_was_separator {
                part.push('-');
                last_was_separator = true;
            }
        } else {
            part.push(replacement);
            last_was_separator = false;
        }
    }

    part.trim_matches(|ch| ch == '-' || ch == ' ').to_owned()
}

fn start_live_capture(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let texts = current_texts(state);
    if state.borrow().live_session.is_some() {
        ui.set_status_text(texts.status.live_already_running.into());
        return;
    }

    if FilterMode::from_key(&filter_key_from_ui(ui)) == FilterMode::None {
        apply_filter_key(ui, texts, FilterMode::Monitor.key());
    }
    let device = LiveDevice::from_ui(&ui.get_live_device_value());
    let (session, rx) = match device {
        LiveDevice::Contec8000G => live::start_contec_8000g(live::LiveConfig),
        LiveDevice::Ecg90A => live::start_ecg90a(live::LiveConfig),
    };
    let document = new_live_document(ui, device);
    {
        let mut state = state.borrow_mut();
        state.document = Some(document);
        state.live_session = Some(session);
        state.live_rx = Some(rx);
        state.live_recording = false;
        state.live_stop_requested = false;
        state.live_sample_count = 0;
        state.recorded_live_document = None;
        state.pending_open_path = None;
        invalidate_preview_jobs(&mut state);
    }

    ui.set_file_name("".into());
    ui.set_live_running(true);
    ui.set_live_recording(false);
    ui.set_live_sample_text(texts.samples(0).into());
    ui.set_recorded_sample_text(texts.recorded_samples(0).into());
    refresh_preview(ui, state);
    ui.set_status_text(format!("{}{}...", texts.status.start_live_prefix, device.label()).into());
}

fn start_live_recording(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let texts = current_texts(state);
    if state.borrow().live_session.is_none() {
        ui.set_status_text(texts.status.start_recording_first.into());
        return;
    }
    if state.borrow().live_recording {
        ui.set_status_text(texts.status.live_recording_already_running.into());
        return;
    }

    let device = LiveDevice::from_ui(&ui.get_live_device_value());
    {
        let mut state = state.borrow_mut();
        state.recorded_live_document = Some(new_live_document(ui, device));
        state.live_recording = true;
    }
    ui.set_live_recording(true);
    ui.set_recorded_sample_text(texts.recorded_samples(0).into());
    ui.set_status_text(
        format!(
            "{}{} ({:.0} s).",
            texts.status.normal_recording_started_prefix,
            device.label(),
            NORMAL_LEAD_SAMPLE_SECONDS
        )
        .into(),
    );
}

fn stop_live_capture(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let texts = current_texts(state);
    let stop_requested = {
        let mut state = state.borrow_mut();
        if let Some(session) = state.live_session.as_ref() {
            session.request_stop();
            state.live_stop_requested = true;
            true
        } else {
            false
        }
    };

    if stop_requested {
        ui.set_status_text(texts.status.stopping_live.into());
        drain_live_messages(ui, state);
    } else {
        ui.set_live_running(false);
        ui.set_live_recording(false);
        ui.set_status_text(texts.status.no_live_running.into());
    }
}

fn drain_live_messages(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let texts = current_texts(state);
    let mut received_frames = Vec::new();
    let mut latest_status = None;
    let mut finished = None;

    loop {
        let message = {
            let state = state.borrow();
            state.live_rx.as_ref().and_then(|rx| rx.try_recv().ok())
        };

        let Some(message) = message else {
            break;
        };

        match message {
            LiveMessage::Status(status) => latest_status = Some(status),
            LiveMessage::Frames(frames) => received_frames.extend(frames),
            LiveMessage::Finished(result) => finished = Some(result),
        }
    }

    if !received_frames.is_empty() {
        let (sample_count, recording_completed) = append_live_frames(ui, state, received_frames);
        ui.set_live_sample_text(texts.samples(sample_count).into());
        refresh_preview(ui, state);
        if recording_completed {
            latest_status = Some(format!(
                "{}{:.0}{}",
                texts.status.normal_recording_completed_prefix,
                NORMAL_LEAD_SAMPLE_SECONDS,
                texts.status.normal_recording_completed_suffix
            ));
        }
    }

    if let Some(status) = latest_status {
        ui.set_status_text(status.into());
    }

    if let Some(result) = finished {
        let (pending_open_path, stop_requested) = {
            let mut state = state.borrow_mut();
            state.live_rx = None;
            state.live_session = None;
            let stop_requested = state.live_stop_requested;
            state.live_stop_requested = false;
            state.live_sample_count = 0;
            (state.pending_open_path.take(), stop_requested)
        };
        ui.set_live_running(false);
        let recorded = finalize_live_recording(ui, state);
        match result {
            Ok(()) if recorded && stop_requested => {
                ui.set_status_text(texts.status.live_stopped_recording_ready.into())
            }
            Ok(()) if stop_requested => ui.set_status_text(texts.status.live_stopped.into()),
            Ok(()) if recorded => {
                ui.set_status_text(texts.status.live_finished_recording_ready.into())
            }
            Ok(()) => ui.set_status_text(texts.status.live_finished.into()),
            Err(error) => {
                ui.set_status_text(format!("{}{error}", texts.status.live_error_prefix).into())
            }
        }
        if let Some(path) = pending_open_path {
            start_open_document(ui, state, path);
        }
    }
}

fn append_live_frames(
    ui: &AppWindow,
    state: &Rc<RefCell<AppState>>,
    frames: Vec<LiveSampleFrame>,
) -> (usize, bool) {
    let texts = current_texts(state);
    let mut state = state.borrow_mut();
    let device = LiveDevice::from_ui(&ui.get_live_device_value());
    let normal_sample_count = normal_live_sample_count(device);
    let display_sample_count = live_display_sample_count(device);
    let mut recording_completed = false;
    state.live_sample_count = state.live_sample_count.saturating_add(frames.len());
    let total_sample_count = state.live_sample_count;
    let document = state
        .document
        .get_or_insert_with(|| new_live_document(ui, device));
    ensure_live_leads(document);

    for frame in &frames {
        for (lead, sample) in document
            .leads
            .iter_mut()
            .zip(frame.leads_microvolts.iter().copied())
        {
            lead.samples_microvolts.push(sample);
        }
    }
    trim_live_display_document(document, display_sample_count);

    if state.live_recording
        && let Some(recorded) = state.recorded_live_document.as_mut()
    {
        ensure_live_leads(recorded);
        let remaining_samples = normal_sample_count.saturating_sub(recorded.max_sample_count());
        let frames_to_record = if normal_sample_count == 0 {
            frames.as_slice()
        } else {
            &frames[..frames.len().min(remaining_samples)]
        };

        for frame in frames_to_record {
            for (lead, sample) in recorded
                .leads
                .iter_mut()
                .zip(frame.leads_microvolts.iter().copied())
            {
                lead.samples_microvolts.push(sample);
            }
        }
        let recorded_sample_count = recorded.max_sample_count();
        ui.set_recorded_sample_text(texts.recorded_samples(recorded_sample_count).into());
        if normal_sample_count > 0 && recorded_sample_count >= normal_sample_count {
            state.live_recording = false;
            recording_completed = true;
        }
    }

    drop(state);
    if recording_completed {
        ui.set_live_recording(false);
    }

    (total_sample_count, recording_completed)
}

fn normal_live_sample_count(device: LiveDevice) -> usize {
    live_sample_count_for_seconds(device, NORMAL_LEAD_SAMPLE_SECONDS)
}

fn live_display_sample_count(device: LiveDevice) -> usize {
    live_sample_count_for_seconds(device, LIVE_DISPLAY_SECONDS)
}

fn live_sample_count_for_seconds(device: LiveDevice, seconds: f64) -> usize {
    let sample_interval_seconds = device.sample_interval_seconds();
    if sample_interval_seconds <= 0.0 {
        0
    } else {
        (seconds / sample_interval_seconds).round() as usize
    }
}

fn trim_live_display_document(document: &mut EcgDocument, max_samples: usize) {
    if max_samples == 0 {
        return;
    }

    for lead in &mut document.leads {
        let excess = lead.samples_microvolts.len().saturating_sub(max_samples);
        if excess > 0 {
            lead.samples_microvolts.drain(..excess);
        }
    }
}

fn finalize_live_recording(ui: &AppWindow, state: &Rc<RefCell<AppState>>) -> bool {
    let recorded = {
        let mut state = state.borrow_mut();
        state.live_recording = false;
        let recorded = state
            .recorded_live_document
            .as_ref()
            .filter(|document| document.max_sample_count() > 0)
            .cloned();
        if let Some(document) = recorded.clone() {
            state.document = Some(document);
            invalidate_preview_jobs(&mut state);
        }
        recorded.is_some()
    };

    ui.set_live_recording(false);
    if recorded {
        refresh_preview(ui, state);
    }
    recorded
}

fn new_live_document(ui: &AppWindow, device: LiveDevice) -> EcgDocument {
    let mut document = EcgDocument::new(DocumentKind::ContecLive, live_capture_path(device));
    document.sample_interval_seconds = device.sample_interval_seconds();
    sync_document_from_ui(ui, &mut document);
    ensure_live_leads(&mut document);
    document
}

fn ensure_live_leads(document: &mut EcgDocument) {
    if document.leads.len() == LIVE_LEAD_NAMES.len()
        && document
            .leads
            .iter()
            .zip(LIVE_LEAD_NAMES.iter())
            .all(|(lead, name)| lead.name == *name)
    {
        return;
    }

    document.leads = LIVE_LEAD_NAMES
        .iter()
        .map(|name| LeadData::new(*name, Vec::new()))
        .collect();
}
fn live_capture_path(device: LiveDevice) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    PathBuf::from(format!("{}-{millis}.ecg", device.capture_stem()))
}

fn print_current_ecg(ui: &AppWindow, state: &Rc<RefCell<AppState>>) {
    let texts = current_texts(state);
    let (document, clinic_logo, live_document) = {
        let state = state.borrow();
        (
            state.document.as_ref().and_then(|document| {
                if document.kind.is_live() {
                    state
                        .recorded_live_document
                        .as_ref()
                        .filter(|recorded| recorded.max_sample_count() > 0)
                        .cloned()
                } else {
                    Some(document.clone())
                }
            }),
            state.clinic_logo.clone(),
            state
                .document
                .as_ref()
                .is_some_and(|document| document.kind.is_live()),
        )
    };
    let Some(mut document) = document else {
        let status = if live_document {
            texts.status.no_live_recording_print
        } else {
            texts.status.no_ecg_print
        };
        ui.set_status_text(status.into());
        return;
    };
    sync_document_from_ui(ui, &mut document);

    let filter = FilterMode::from_key(&filter_key_from_ui(ui));
    let filtered = processing::apply_filter(&document, filter);
    let selected_leads = selected_leads_from_ui(ui);
    let displayed = document_with_selected_leads(&filtered, &selected_leads);
    if displayed.leads.is_empty() {
        ui.set_status_text(texts.status.select_lead_print.into());
        return;
    }
    let page = preview::render_document_page_with_measurements(
        &displayed,
        &document,
        current_render_options(ui, clinic_logo, texts),
    );

    ui.set_status_text(texts.status.opening_print_dialog.into());
    match printing::print_page(&page) {
        Ok(()) => ui.set_status_text(texts.status.print_sent.into()),
        Err(error) => ui.set_status_text(error.into()),
    }
}

fn sync_document_from_ui(ui: &AppWindow, document: &mut EcgDocument) {
    document.clinic_name = ui.get_clinic_name().trim().to_owned();
    document.physician_name = ui.get_physician_name().trim().to_owned();
    document.patient_name = ui.get_patient_name().trim().to_owned();
    document.patient_birth_date = ui.get_patient_birth_date().trim().to_owned();
    document.exam_date = ui.get_exam_date().trim().to_owned();
}

fn selected_leads_from_ui(ui: &AppWindow) -> Vec<&'static str> {
    [
        ("I", ui.get_lead_i_selected()),
        ("II", ui.get_lead_ii_selected()),
        ("III", ui.get_lead_iii_selected()),
        ("aVR", ui.get_lead_avr_selected()),
        ("aVL", ui.get_lead_avl_selected()),
        ("aVF", ui.get_lead_avf_selected()),
        ("V1", ui.get_lead_v1_selected()),
        ("V2", ui.get_lead_v2_selected()),
        ("V3", ui.get_lead_v3_selected()),
        ("V4", ui.get_lead_v4_selected()),
        ("V5", ui.get_lead_v5_selected()),
        ("V6", ui.get_lead_v6_selected()),
    ]
    .into_iter()
    .filter_map(|(lead_name, selected)| selected.then_some(lead_name))
    .collect()
}

fn selected_leads_value_from_ui(ui: &AppWindow) -> String {
    selected_leads_from_ui(ui).join(",")
}

fn apply_selected_leads_value(ui: &AppWindow, value: &str) {
    let selected = parse_selected_leads_value(value);
    if selected.is_empty() {
        return;
    }

    ui.set_lead_i_selected(selected.contains(&"I"));
    ui.set_lead_ii_selected(selected.contains(&"II"));
    ui.set_lead_iii_selected(selected.contains(&"III"));
    ui.set_lead_avr_selected(selected.contains(&"aVR"));
    ui.set_lead_avl_selected(selected.contains(&"aVL"));
    ui.set_lead_avf_selected(selected.contains(&"aVF"));
    ui.set_lead_v1_selected(selected.contains(&"V1"));
    ui.set_lead_v2_selected(selected.contains(&"V2"));
    ui.set_lead_v3_selected(selected.contains(&"V3"));
    ui.set_lead_v4_selected(selected.contains(&"V4"));
    ui.set_lead_v5_selected(selected.contains(&"V5"));
    ui.set_lead_v6_selected(selected.contains(&"V6"));
}

fn parse_selected_leads_value(value: &str) -> Vec<&'static str> {
    let mut selected = Vec::new();
    for part in value.split(',') {
        let Some(lead_name) = canonical_lead_name(part) else {
            continue;
        };
        if !selected.contains(&lead_name) {
            selected.push(lead_name);
        }
    }
    selected
}

fn document_with_selected_leads(document: &EcgDocument, selected_names: &[&str]) -> EcgDocument {
    let mut displayed = document.clone();
    if selected_names.is_empty() {
        displayed.leads.clear();
        return displayed;
    }

    let selected_leads = selected_names
        .iter()
        .filter_map(|selected_name| {
            document
                .leads
                .iter()
                .find(|lead| lead_name_matches(&lead.name, selected_name))
                .cloned()
        })
        .collect::<Vec<_>>();

    if !selected_leads.is_empty() {
        displayed.leads = selected_leads;
    }
    displayed
}

fn lead_name_matches(lead_name: &str, selected_name: &str) -> bool {
    canonical_lead_name(lead_name).is_some_and(|lead_name| lead_name == selected_name)
}

fn canonical_lead_name(value: &str) -> Option<&'static str> {
    let normalized = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect::<String>();

    match normalized.as_str() {
        "I" | "DI" => Some("I"),
        "II" | "DII" => Some("II"),
        "III" | "DIII" => Some("III"),
        "AVR" | "DAVR" => Some("aVR"),
        "AVL" | "DAVL" => Some("aVL"),
        "AVF" | "DAVF" => Some("aVF"),
        "V1" => Some("V1"),
        "V2" => Some("V2"),
        "V3" => Some("V3"),
        "V4" => Some("V4"),
        "V5" => Some("V5"),
        "V6" => Some("V6"),
        _ => None,
    }
}

fn saved_live_device_label(value: &str) -> &'static str {
    match LiveDevice::from_ui(value) {
        LiveDevice::Contec8000G => "CONTEC 8000G",
        LiveDevice::Ecg90A => "CONTEC ECG90A",
    }
}

fn format_birth_date_input(value: &str) -> String {
    let digits = value
        .chars()
        .filter(char::is_ascii_digit)
        .take(8)
        .collect::<String>();

    match digits.len() {
        0..=2 => digits,
        3..=4 => format!("{}/{}", &digits[..2], &digits[2..]),
        _ => format!("{}/{}/{}", &digits[..2], &digits[2..4], &digits[4..]),
    }
}

fn fill_default_exam_date(ui: &AppWindow) {
    if ui.get_exam_date().trim().is_empty() {
        ui.set_exam_date(current_exam_date_time().into());
    }
}

fn exam_date_for_ui(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return current_exam_date_time();
    }
    without_seconds(trimmed)
}

fn without_seconds(value: &str) -> String {
    let Some((date, time)) = value.split_once(' ') else {
        return value.to_owned();
    };
    let mut parts = time.split(':');
    let hour = parts.next().unwrap_or_default();
    let minute = parts.next().unwrap_or_default();
    if hour.len() == 2 && minute.len() == 2 {
        format!("{date} {hour}:{minute}")
    } else {
        value.to_owned()
    }
}

#[cfg(windows)]
fn current_exam_date_time() -> String {
    use windows::Win32::System::SystemInformation::GetLocalTime;

    let now = unsafe { GetLocalTime() };
    format!(
        "{:02}/{:02}/{:04} {:02}:{:02}",
        now.wDay, now.wMonth, now.wYear, now.wHour, now.wMinute
    )
}

#[cfg(not(windows))]
fn current_exam_date_time() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = seconds / 86_400;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_date_from_unix_days(days as i64);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    format!("{day:02}/{month:02}/{year:04} {hour:02}:{minute:02}")
}

#[cfg(not(windows))]
fn civil_date_from_unix_days(days: i64) -> (i32, u8, u8) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = y + i64::from(month <= 2);

    (year as i32, month as u8, day as u8)
}

fn current_render_options(
    ui: &AppWindow,
    clinic_logo: Option<ClinicLogo>,
    texts: &Texts,
) -> RenderOptions {
    RenderOptions {
        orientation: PageOrientation::from_key(&orientation_key_from_ui(ui)),
        show_calibration: ui.get_show_calibration(),
        grid_theme: GridTheme::from_key(&grid_theme_key_from_ui(ui)),
        clinic_logo,
        texts: texts.page,
    }
}

fn status_for_document(document: &EcgDocument, texts: &Texts) -> String {
    texts.status_for_document(
        document.kind.label(),
        document.leads.len(),
        document.sample_rate_hz(),
        document.duration_seconds(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_seconds_from_exam_date_for_ui() {
        assert_eq!(without_seconds("11/05/2026 09:07:33"), "11/05/2026 09:07");
    }

    #[test]
    fn default_export_name_uses_patient_and_exam_date() {
        let mut document = EcgDocument::new(DocumentKind::Xml, PathBuf::from("entrada.xml"));
        document.patient_name = "Jose da Silva".to_owned();
        document.exam_date = "11/05/2026 09:07".to_owned();

        assert_eq!(
            default_export_name(&document, "dicom-ecg", "dcm"),
            "Jose da Silva-11-05-2026 09-07-dicom-ecg.dcm"
        );
    }

    #[test]
    fn default_export_name_has_safe_fallbacks() {
        let mut document = EcgDocument::new(DocumentKind::Xml, PathBuf::from("entrada.xml"));
        document.patient_name = "  Ana/Teste: ECG  ".to_owned();

        assert_eq!(
            default_export_name(&document, "aecg", "xml"),
            "Ana-Teste-ECG-sem-data-aecg.xml"
        );
    }

    #[test]
    fn formats_birth_date_while_digits_are_entered() {
        assert_eq!(format_birth_date_input("1"), "1");
        assert_eq!(format_birth_date_input("120"), "12/0");
        assert_eq!(format_birth_date_input("12051990"), "12/05/1990");
        assert_eq!(format_birth_date_input("12/05/1990"), "12/05/1990");
        assert_eq!(format_birth_date_input("120519901"), "12/05/1990");
    }

    #[test]
    fn parses_selected_leads_with_aliases() {
        assert_eq!(
            parse_selected_leads_value("DI,aVr,V2,unknown,V2"),
            vec!["I", "aVR", "V2"]
        );
    }

    #[test]
    fn filters_display_document_in_selected_order() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.leads.push(LeadData::new("I", vec![1.0]));
        document.leads.push(LeadData::new("II", vec![2.0]));
        document.leads.push(LeadData::new("V1", vec![3.0]));

        let displayed = document_with_selected_leads(&document, &["V1", "I"]);
        let names = displayed
            .leads
            .iter()
            .map(|lead| lead.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["V1", "I"]);
    }

    #[test]
    fn uses_ten_second_normal_live_recording_windows() {
        assert_eq!(normal_live_sample_count(LiveDevice::Contec8000G), 5_000);
        assert_eq!(normal_live_sample_count(LiveDevice::Ecg90A), 8_000);
    }

    #[test]
    fn caps_live_display_to_recent_samples() {
        let mut document = EcgDocument::new(DocumentKind::ContecLive, Default::default());
        document
            .leads
            .push(LeadData::new("I", vec![1.0, 2.0, 3.0, 4.0, 5.0]));
        document.leads.push(LeadData::new("II", vec![10.0, 20.0]));

        trim_live_display_document(&mut document, 3);

        assert_eq!(
            document.find_lead("I").unwrap().samples_microvolts,
            vec![3.0, 4.0, 5.0]
        );
        assert_eq!(
            document.find_lead("II").unwrap().samples_microvolts,
            vec![10.0, 20.0]
        );
    }

    #[test]
    fn writes_hl7_export_without_ui() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.leads.push(LeadData::new("I", vec![1.0, -1.0]));
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!("ecg-studio-export-{millis}.xml"));

        let status = write_exported_ecg(
            ExportFormat::Hl7Aecg,
            &document,
            path.clone(),
            i18n::texts(Language::PtBr),
        );
        assert!(status.starts_with("HL7 aECG salvo:"));
        let xml = fs::read_to_string(&path).expect("exported HL7 should be readable");
        let _ = fs::remove_file(path);

        assert!(xml.contains("<AnnotatedECG"));
        assert!(xml.contains("<digits>1 -1</digits>"));
    }

    #[test]
    fn reads_initial_open_path_from_arguments() {
        assert_eq!(
            initial_open_path_from_args(["ecg-studio", "-psn_0_123", "exam.dcm"]),
            Some(PathBuf::from("exam.dcm"))
        );
    }

    #[test]
    fn decodes_file_uri_open_argument() {
        assert_eq!(
            initial_open_path_from_args(["ecg-studio", "file:///tmp/exam%201.dcm"]),
            Some(PathBuf::from("/tmp/exam 1.dcm"))
        );
    }
}
