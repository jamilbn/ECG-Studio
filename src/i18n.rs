#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Language {
    #[default]
    PtBr,
    PtPt,
    En,
    EsLatam,
    EsEs,
    Fr,
    It,
    De,
}

impl Language {
    fn from_locale(locale: &str) -> Option<Self> {
        let normalized = locale.trim().replace('_', "-").to_ascii_lowercase();
        if normalized.is_empty() {
            return None;
        }

        let language = normalized.split('-').next().unwrap_or_default();
        match language {
            "pt" if normalized == "pt-pt" => Some(Self::PtPt),
            "pt" => Some(Self::PtBr),
            "en" => Some(Self::En),
            "es" if normalized == "es-es" => Some(Self::EsEs),
            "es" => Some(Self::EsLatam),
            "fr" => Some(Self::Fr),
            "it" => Some(Self::It),
            "de" => Some(Self::De),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LanguageSelection {
    #[default]
    System,
    Language(Language),
}

impl LanguageSelection {
    pub fn from_code(code: &str) -> Self {
        match code.trim() {
            "pt-BR" => Self::Language(Language::PtBr),
            "pt-PT" => Self::Language(Language::PtPt),
            "en" | "en-US" => Self::Language(Language::En),
            "es-419" => Self::Language(Language::EsLatam),
            "es-ES" => Self::Language(Language::EsEs),
            "fr" | "fr-FR" => Self::Language(Language::Fr),
            "it" | "it-IT" => Self::Language(Language::It),
            "de" | "de-DE" => Self::Language(Language::De),
            _ => Self::System,
        }
    }

    pub fn from_label(label: &str) -> Self {
        match label.trim() {
            LANGUAGE_PT_BR_LABEL => Self::Language(Language::PtBr),
            LANGUAGE_PT_PT_LABEL => Self::Language(Language::PtPt),
            LANGUAGE_EN_LABEL => Self::Language(Language::En),
            LANGUAGE_ES_LATAM_LABEL => Self::Language(Language::EsLatam),
            LANGUAGE_ES_ES_LABEL => Self::Language(Language::EsEs),
            LANGUAGE_FR_LABEL => Self::Language(Language::Fr),
            LANGUAGE_IT_LABEL => Self::Language(Language::It),
            LANGUAGE_DE_LABEL => Self::Language(Language::De),
            _ => Self::System,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Language(Language::PtBr) => "pt-BR",
            Self::Language(Language::PtPt) => "pt-PT",
            Self::Language(Language::En) => "en-US",
            Self::Language(Language::EsLatam) => "es-419",
            Self::Language(Language::EsEs) => "es-ES",
            Self::Language(Language::Fr) => "fr-FR",
            Self::Language(Language::It) => "it-IT",
            Self::Language(Language::De) => "de-DE",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => LANGUAGE_SYSTEM_LABEL,
            Self::Language(Language::PtBr) => LANGUAGE_PT_BR_LABEL,
            Self::Language(Language::PtPt) => LANGUAGE_PT_PT_LABEL,
            Self::Language(Language::En) => LANGUAGE_EN_LABEL,
            Self::Language(Language::EsLatam) => LANGUAGE_ES_LATAM_LABEL,
            Self::Language(Language::EsEs) => LANGUAGE_ES_ES_LABEL,
            Self::Language(Language::Fr) => LANGUAGE_FR_LABEL,
            Self::Language(Language::It) => LANGUAGE_IT_LABEL,
            Self::Language(Language::De) => LANGUAGE_DE_LABEL,
        }
    }
}

pub const LANGUAGE_SYSTEM_LABEL: &str = "Windows";
pub const LANGUAGE_PT_BR_LABEL: &str = "Português (Brasil)";
pub const LANGUAGE_PT_PT_LABEL: &str = "Português (Portugal)";
pub const LANGUAGE_EN_LABEL: &str = "English";
pub const LANGUAGE_ES_LATAM_LABEL: &str = "Español (Latinoamérica)";
pub const LANGUAGE_ES_ES_LABEL: &str = "Español (España)";
pub const LANGUAGE_FR_LABEL: &str = "Français";
pub const LANGUAGE_IT_LABEL: &str = "Italiano";
pub const LANGUAGE_DE_LABEL: &str = "Deutsch";

pub fn resolve_language(selection: LanguageSelection) -> Language {
    match selection {
        LanguageSelection::System => detect_system_language(),
        LanguageSelection::Language(language) => language,
    }
}

pub fn detect_system_language() -> Language {
    system_locale()
        .and_then(|locale| Language::from_locale(&locale))
        .unwrap_or(Language::PtBr)
}

#[cfg(windows)]
pub fn system_locale() -> Option<String> {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;

    let mut buffer = [0_u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if len <= 1 {
        return None;
    }

    let len = (len as usize).saturating_sub(1).min(buffer.len());
    String::from_utf16(&buffer[..len]).ok()
}

#[cfg(not(windows))]
pub fn system_locale() -> Option<String> {
    std::env::var("LC_ALL")
        .ok()
        .or_else(|| std::env::var("LANG").ok())
}

#[derive(Clone, Copy, Debug)]
pub struct Texts {
    pub ui: UiTexts,
    pub status: StatusTexts,
    pub page: PageTexts,
}

impl Texts {
    pub fn samples(self, count: usize) -> String {
        let noun = if count == 1 {
            self.status.sample_singular
        } else {
            self.status.sample_plural
        };
        format!("{count} {noun}")
    }

    pub fn recorded_samples(self, count: usize) -> String {
        let noun = if count == 1 {
            self.status.recorded_sample_singular
        } else {
            self.status.recorded_sample_plural
        };
        format!("{count} {noun}")
    }

    pub fn status_for_document(
        self,
        kind: &str,
        lead_count: usize,
        sample_rate_hz: f64,
        duration_seconds: f64,
    ) -> String {
        let lead_word = if lead_count == 1 {
            self.status.lead_singular
        } else {
            self.status.lead_plural
        };
        format!(
            "{kind} | {lead_count} {lead_word} | {sample_rate_hz:.1} Hz | {duration_seconds:.2} s"
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct UiTexts {
    pub clinic_default: &'static str,
    pub no_logo: &'static str,
    pub no_file_loaded: &'static str,
    pub empty_status: &'static str,
    pub live_capture: &'static str,
    pub device: &'static str,
    pub recording: &'static str,
    pub record: &'static str,
    pub capturing: &'static str,
    pub start_live: &'static str,
    pub stop: &'static str,
    pub header: &'static str,
    pub clinic: &'static str,
    pub physician: &'static str,
    pub physician_placeholder: &'static str,
    pub logo: &'static str,
    pub choose: &'static str,
    pub remove: &'static str,
    pub patient: &'static str,
    pub patient_placeholder: &'static str,
    pub birth_date: &'static str,
    pub birth_date_placeholder: &'static str,
    pub exam_date: &'static str,
    pub exam_date_placeholder: &'static str,
    pub view: &'static str,
    pub filter: &'static str,
    pub orientation: &'static str,
    pub grid: &'static str,
    pub printed_leads: &'static str,
    pub calibration_pulse: &'static str,
    pub save_label: &'static str,
    pub open: &'static str,
    pub save: &'static str,
    pub print: &'static str,
    pub about: &'static str,
    pub settings: &'static str,
    pub about_title: &'static str,
    pub version: &'static str,
    pub made_with: &'static str,
    pub settings_title: &'static str,
    pub language: &'static str,
    pub cancel: &'static str,
    pub filter_none: &'static str,
    pub filter_baseline: &'static str,
    pub filter_low_pass: &'static str,
    pub filter_diagnostic: &'static str,
    pub filter_monitor: &'static str,
    pub orientation_landscape: &'static str,
    pub orientation_portrait: &'static str,
    pub grid_light_salmon: &'static str,
    pub grid_technical_gray: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct StatusTexts {
    pub sample_singular: &'static str,
    pub sample_plural: &'static str,
    pub recorded_sample_singular: &'static str,
    pub recorded_sample_plural: &'static str,
    pub lead_singular: &'static str,
    pub lead_plural: &'static str,
    pub background_queue_unavailable: &'static str,
    pub open_ecg_title: &'static str,
    pub choose_logo_title: &'static str,
    pub all_files_filter: &'static str,
    pub image_filter: &'static str,
    pub invalid_image_format_prefix: &'static str,
    pub unsupported_logo_format: &'static str,
    pub load_logo_failed_prefix: &'static str,
    pub saved_logo_load_failed_prefix: &'static str,
    pub open_about_failed_prefix: &'static str,
    pub open_settings_failed_prefix: &'static str,
    pub loading_ecg_prefix: &'static str,
    pub saving_ecg_prefix: &'static str,
    pub error_prefix: &'static str,
    pub no_live_recording_export: &'static str,
    pub no_ecg_export: &'static str,
    pub no_live_recording_print: &'static str,
    pub no_ecg_print: &'static str,
    pub select_lead_print: &'static str,
    pub opening_print_dialog: &'static str,
    pub print_sent: &'static str,
    pub live_already_running: &'static str,
    pub start_recording_first: &'static str,
    pub live_recording_already_running: &'static str,
    pub stopping_live: &'static str,
    pub live_stopped_recording_ready: &'static str,
    pub live_stopped: &'static str,
    pub no_live_running: &'static str,
    pub live_finished_recording_ready: &'static str,
    pub live_finished: &'static str,
    pub live_error_prefix: &'static str,
    pub start_live_prefix: &'static str,
    pub normal_recording_started_prefix: &'static str,
    pub normal_recording_completed_prefix: &'static str,
    pub normal_recording_completed_suffix: &'static str,
    pub settings_saved: &'static str,
    pub hl7_saved_prefix: &'static str,
    pub hl7_save_failed_prefix: &'static str,
    pub dicom_saved_prefix: &'static str,
    pub dicom_write_failed_prefix: &'static str,
    pub save_hl7_title: &'static str,
    pub save_dicom_title: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct PageTexts {
    pub clinic_fallback: &'static str,
    pub patient: &'static str,
    pub edit_placeholder: &'static str,
    pub exam_date: &'static str,
    pub physician: &'static str,
    pub birth: &'static str,
    pub age: &'static str,
    pub year_singular: &'static str,
    pub year_plural: &'static str,
    pub no_leads: &'static str,
    pub measurements: MeasurementTexts,
}

#[derive(Clone, Copy, Debug)]
pub struct MeasurementTexts {
    pub heart_rate: &'static str,
    pub range: &'static str,
    pub qrs_axis: &'static str,
    pub degrees: &'static str,
    pub rhythm_regular: &'static str,
    pub rhythm_irregular: &'static str,
    pub axis_unavailable: &'static str,
    pub axis_normal: &'static str,
    pub axis_left: &'static str,
    pub axis_right: &'static str,
    pub axis_extreme: &'static str,
    pub axis_indeterminate: &'static str,
}

pub fn texts(language: Language) -> &'static Texts {
    match language {
        Language::PtBr => &PT_BR,
        Language::PtPt => &PT_PT,
        Language::En => &EN,
        Language::EsLatam => &ES_LATAM,
        Language::EsEs => &ES_ES,
        Language::Fr => &FR,
        Language::It => &IT,
        Language::De => &DE,
    }
}

pub fn all_texts() -> [&'static Texts; 8] {
    [&PT_BR, &PT_PT, &EN, &ES_LATAM, &ES_ES, &FR, &IT, &DE]
}

macro_rules! texts {
    (
        ui: {
            clinic_default: $clinic_default:expr,
            no_logo: $no_logo:expr,
            no_file_loaded: $no_file_loaded:expr,
            empty_status: $empty_status:expr,
            live_capture: $live_capture:expr,
            device: $device:expr,
            recording: $recording:expr,
            record: $record:expr,
            capturing: $capturing:expr,
            start_live: $start_live:expr,
            stop: $stop:expr,
            header: $header:expr,
            clinic: $clinic:expr,
            physician: $physician:expr,
            physician_placeholder: $physician_placeholder:expr,
            logo: $logo:expr,
            choose: $choose:expr,
            remove: $remove:expr,
            patient: $patient:expr,
            patient_placeholder: $patient_placeholder:expr,
            birth_date: $birth_date:expr,
            birth_date_placeholder: $birth_date_placeholder:expr,
            exam_date: $exam_date:expr,
            exam_date_placeholder: $exam_date_placeholder:expr,
            view: $view:expr,
            filter: $filter:expr,
            orientation: $orientation:expr,
            grid: $grid:expr,
            printed_leads: $printed_leads:expr,
            calibration_pulse: $calibration_pulse:expr,
            save_label: $save_label:expr,
            open: $open:expr,
            save: $save:expr,
            print: $print:expr,
            about: $about:expr,
            settings: $settings:expr,
            about_title: $about_title:expr,
            version: $version:expr,
            made_with: $made_with:expr,
            settings_title: $settings_title:expr,
            language: $language:expr,
            cancel: $cancel:expr,
            filter_none: $filter_none:expr,
            filter_baseline: $filter_baseline:expr,
            filter_low_pass: $filter_low_pass:expr,
            filter_diagnostic: $filter_diagnostic:expr,
            filter_monitor: $filter_monitor:expr,
            orientation_landscape: $orientation_landscape:expr,
            orientation_portrait: $orientation_portrait:expr,
            grid_light_salmon: $grid_light_salmon:expr,
            grid_technical_gray: $grid_technical_gray:expr $(,)?
        },
        status: {
            sample_singular: $sample_singular:expr,
            sample_plural: $sample_plural:expr,
            recorded_sample_singular: $recorded_sample_singular:expr,
            recorded_sample_plural: $recorded_sample_plural:expr,
            lead_singular: $lead_singular:expr,
            lead_plural: $lead_plural:expr,
            background_queue_unavailable: $background_queue_unavailable:expr,
            open_ecg_title: $open_ecg_title:expr,
            choose_logo_title: $choose_logo_title:expr,
            all_files_filter: $all_files_filter:expr,
            image_filter: $image_filter:expr,
            invalid_image_format_prefix: $invalid_image_format_prefix:expr,
            unsupported_logo_format: $unsupported_logo_format:expr,
            load_logo_failed_prefix: $load_logo_failed_prefix:expr,
            saved_logo_load_failed_prefix: $saved_logo_load_failed_prefix:expr,
            open_about_failed_prefix: $open_about_failed_prefix:expr,
            open_settings_failed_prefix: $open_settings_failed_prefix:expr,
            loading_ecg_prefix: $loading_ecg_prefix:expr,
            saving_ecg_prefix: $saving_ecg_prefix:expr,
            error_prefix: $error_prefix:expr,
            no_live_recording_export: $no_live_recording_export:expr,
            no_ecg_export: $no_ecg_export:expr,
            no_live_recording_print: $no_live_recording_print:expr,
            no_ecg_print: $no_ecg_print:expr,
            select_lead_print: $select_lead_print:expr,
            opening_print_dialog: $opening_print_dialog:expr,
            print_sent: $print_sent:expr,
            live_already_running: $live_already_running:expr,
            start_recording_first: $start_recording_first:expr,
            live_recording_already_running: $live_recording_already_running:expr,
            stopping_live: $stopping_live:expr,
            live_stopped_recording_ready: $live_stopped_recording_ready:expr,
            live_stopped: $live_stopped:expr,
            no_live_running: $no_live_running:expr,
            live_finished_recording_ready: $live_finished_recording_ready:expr,
            live_finished: $live_finished:expr,
            live_error_prefix: $live_error_prefix:expr,
            start_live_prefix: $start_live_prefix:expr,
            normal_recording_started_prefix: $normal_recording_started_prefix:expr,
            normal_recording_completed_prefix: $normal_recording_completed_prefix:expr,
            normal_recording_completed_suffix: $normal_recording_completed_suffix:expr,
            settings_saved: $settings_saved:expr,
            hl7_saved_prefix: $hl7_saved_prefix:expr,
            hl7_save_failed_prefix: $hl7_save_failed_prefix:expr,
            dicom_saved_prefix: $dicom_saved_prefix:expr,
            dicom_write_failed_prefix: $dicom_write_failed_prefix:expr,
            save_hl7_title: $save_hl7_title:expr,
            save_dicom_title: $save_dicom_title:expr $(,)?
        },
        page: {
            clinic_fallback: $page_clinic_fallback:expr,
            patient: $page_patient:expr,
            edit_placeholder: $page_edit_placeholder:expr,
            exam_date: $page_exam_date:expr,
            physician: $page_physician:expr,
            birth: $page_birth:expr,
            age: $page_age:expr,
            year_singular: $page_year_singular:expr,
            year_plural: $page_year_plural:expr,
            no_leads: $page_no_leads:expr,
            measurements: {
                heart_rate: $heart_rate:expr,
                range: $range:expr,
                qrs_axis: $qrs_axis:expr,
                degrees: $degrees:expr,
                rhythm_regular: $rhythm_regular:expr,
                rhythm_irregular: $rhythm_irregular:expr,
                axis_unavailable: $axis_unavailable:expr,
                axis_normal: $axis_normal:expr,
                axis_left: $axis_left:expr,
                axis_right: $axis_right:expr,
                axis_extreme: $axis_extreme:expr,
                axis_indeterminate: $axis_indeterminate:expr $(,)?
            } $(,)?
        } $(,)?
    ) => {
        Texts {
            ui: UiTexts {
                clinic_default: $clinic_default,
                no_logo: $no_logo,
                no_file_loaded: $no_file_loaded,
                empty_status: $empty_status,
                live_capture: $live_capture,
                device: $device,
                recording: $recording,
                record: $record,
                capturing: $capturing,
                start_live: $start_live,
                stop: $stop,
                header: $header,
                clinic: $clinic,
                physician: $physician,
                physician_placeholder: $physician_placeholder,
                logo: $logo,
                choose: $choose,
                remove: $remove,
                patient: $patient,
                patient_placeholder: $patient_placeholder,
                birth_date: $birth_date,
                birth_date_placeholder: $birth_date_placeholder,
                exam_date: $exam_date,
                exam_date_placeholder: $exam_date_placeholder,
                view: $view,
                filter: $filter,
                orientation: $orientation,
                grid: $grid,
                printed_leads: $printed_leads,
                calibration_pulse: $calibration_pulse,
                save_label: $save_label,
                open: $open,
                save: $save,
                print: $print,
                about: $about,
                settings: $settings,
                about_title: $about_title,
                version: $version,
                made_with: $made_with,
                settings_title: $settings_title,
                language: $language,
                cancel: $cancel,
                filter_none: $filter_none,
                filter_baseline: $filter_baseline,
                filter_low_pass: $filter_low_pass,
                filter_diagnostic: $filter_diagnostic,
                filter_monitor: $filter_monitor,
                orientation_landscape: $orientation_landscape,
                orientation_portrait: $orientation_portrait,
                grid_light_salmon: $grid_light_salmon,
                grid_technical_gray: $grid_technical_gray,
            },
            status: StatusTexts {
                sample_singular: $sample_singular,
                sample_plural: $sample_plural,
                recorded_sample_singular: $recorded_sample_singular,
                recorded_sample_plural: $recorded_sample_plural,
                lead_singular: $lead_singular,
                lead_plural: $lead_plural,
                background_queue_unavailable: $background_queue_unavailable,
                open_ecg_title: $open_ecg_title,
                choose_logo_title: $choose_logo_title,
                all_files_filter: $all_files_filter,
                image_filter: $image_filter,
                invalid_image_format_prefix: $invalid_image_format_prefix,
                unsupported_logo_format: $unsupported_logo_format,
                load_logo_failed_prefix: $load_logo_failed_prefix,
                saved_logo_load_failed_prefix: $saved_logo_load_failed_prefix,
                open_about_failed_prefix: $open_about_failed_prefix,
                open_settings_failed_prefix: $open_settings_failed_prefix,
                loading_ecg_prefix: $loading_ecg_prefix,
                saving_ecg_prefix: $saving_ecg_prefix,
                error_prefix: $error_prefix,
                no_live_recording_export: $no_live_recording_export,
                no_ecg_export: $no_ecg_export,
                no_live_recording_print: $no_live_recording_print,
                no_ecg_print: $no_ecg_print,
                select_lead_print: $select_lead_print,
                opening_print_dialog: $opening_print_dialog,
                print_sent: $print_sent,
                live_already_running: $live_already_running,
                start_recording_first: $start_recording_first,
                live_recording_already_running: $live_recording_already_running,
                stopping_live: $stopping_live,
                live_stopped_recording_ready: $live_stopped_recording_ready,
                live_stopped: $live_stopped,
                no_live_running: $no_live_running,
                live_finished_recording_ready: $live_finished_recording_ready,
                live_finished: $live_finished,
                live_error_prefix: $live_error_prefix,
                start_live_prefix: $start_live_prefix,
                normal_recording_started_prefix: $normal_recording_started_prefix,
                normal_recording_completed_prefix: $normal_recording_completed_prefix,
                normal_recording_completed_suffix: $normal_recording_completed_suffix,
                settings_saved: $settings_saved,
                hl7_saved_prefix: $hl7_saved_prefix,
                hl7_save_failed_prefix: $hl7_save_failed_prefix,
                dicom_saved_prefix: $dicom_saved_prefix,
                dicom_write_failed_prefix: $dicom_write_failed_prefix,
                save_hl7_title: $save_hl7_title,
                save_dicom_title: $save_dicom_title,
            },
            page: PageTexts {
                clinic_fallback: $page_clinic_fallback,
                patient: $page_patient,
                edit_placeholder: $page_edit_placeholder,
                exam_date: $page_exam_date,
                physician: $page_physician,
                birth: $page_birth,
                age: $page_age,
                year_singular: $page_year_singular,
                year_plural: $page_year_plural,
                no_leads: $page_no_leads,
                measurements: MeasurementTexts {
                    heart_rate: $heart_rate,
                    range: $range,
                    qrs_axis: $qrs_axis,
                    degrees: $degrees,
                    rhythm_regular: $rhythm_regular,
                    rhythm_irregular: $rhythm_irregular,
                    axis_unavailable: $axis_unavailable,
                    axis_normal: $axis_normal,
                    axis_left: $axis_left,
                    axis_right: $axis_right,
                    axis_extreme: $axis_extreme,
                    axis_indeterminate: $axis_indeterminate,
                },
            },
        }
    };
}

static PT_BR: Texts = texts! {
    ui: {
        clinic_default: "Clínica", no_logo: "Sem logotipo", no_file_loaded: "Nenhum arquivo carregado",
        empty_status: "Abra um ECG para visualizar a página.", live_capture: "Captura ao vivo",
        device: "Aparelho", recording: "Gravando", record: "Gravar", capturing: "Capturando",
        start_live: "Iniciar live", stop: "Parar", header: "Cabeçalho", clinic: "Clínica",
        physician: "Médico", physician_placeholder: "Nome do médico", logo: "Logotipo",
        choose: "Escolher", remove: "Remover", patient: "Paciente",
        patient_placeholder: "Nome do paciente", birth_date: "Data de nascimento",
        birth_date_placeholder: "dd/mm/aaaa", exam_date: "Data do exame",
        exam_date_placeholder: "dd/mm/aaaa hh:mm", view: "Visualização", filter: "Filtro",
        orientation: "Orientação", grid: "Grade", printed_leads: "Canais impressos",
        calibration_pulse: "Pulso de calibração", save_label: "Salvar", open: "Abrir",
        save: "Salvar", print: "Imprimir", about: "Sobre", settings: "Configurações",
        about_title: "Sobre ECG Studio", version: "Versão 1.0", made_with: "Feito com:",
        settings_title: "Configurações", language: "Idioma", cancel: "Cancelar",
        filter_none: "Sem filtro", filter_baseline: "Remover linha de base",
        filter_low_pass: "Baixa passagem 40 Hz", filter_diagnostic: "Diagnóstico",
        filter_monitor: "Monitor", orientation_landscape: "Paisagem", orientation_portrait: "Retrato",
        grid_light_salmon: "Salmão Claro", grid_technical_gray: "Cinza Técnico",
    },
    status: {
        sample_singular: "amostra", sample_plural: "amostras",
        recorded_sample_singular: "amostra gravada", recorded_sample_plural: "amostras gravadas",
        lead_singular: "derivação", lead_plural: "derivações",
        background_queue_unavailable: "Fila de trabalho em segundo plano indisponível.",
        open_ecg_title: "Abrir ECG", choose_logo_title: "Escolher logotipo",
        all_files_filter: "Todos os arquivos", image_filter: "Imagem",
        invalid_image_format_prefix: "formato de imagem inválido",
        unsupported_logo_format: "use PNG, JPG ou BMP",
        load_logo_failed_prefix: "Falha ao carregar logotipo: ",
        saved_logo_load_failed_prefix: "Logotipo salvo não carregou: ",
        open_about_failed_prefix: "Falha ao abrir Sobre: ",
        open_settings_failed_prefix: "Falha ao abrir Configurações: ",
        loading_ecg_prefix: "Carregando ECG: ", saving_ecg_prefix: "Salvando ECG: ",
        error_prefix: "Erro: ", no_live_recording_export: "Nenhuma gravação ao vivo está pronta para exportar.",
        no_ecg_export: "Nenhum ECG carregado para exportar.",
        no_live_recording_print: "Nenhuma gravação ao vivo está pronta para imprimir.",
        no_ecg_print: "Nenhum ECG carregado para imprimir.",
        select_lead_print: "Selecione ao menos um canal para imprimir.",
        opening_print_dialog: "Abrindo diálogo de impressão.", print_sent: "ECG enviado para impressão.",
        live_already_running: "A captura ao vivo já está em andamento.",
        start_recording_first: "Inicie a captura ao vivo antes de gravar.",
        live_recording_already_running: "A gravação do ECG ao vivo já está em andamento.",
        stopping_live: "Parando captura ao vivo...", live_stopped_recording_ready: "Captura ao vivo parada. A gravação está pronta para salvar.",
        live_stopped: "Captura ao vivo parada.", no_live_running: "Nenhuma captura ao vivo está em andamento.",
        live_finished_recording_ready: "Captura ao vivo finalizada. A gravação está pronta para salvar.",
        live_finished: "Captura ao vivo finalizada.", live_error_prefix: "Erro na captura ao vivo: ",
        start_live_prefix: "Iniciando captura ao vivo do ",
        normal_recording_started_prefix: "Gravação normal do ECG ao vivo iniciada no ",
        normal_recording_completed_prefix: "Gravação normal concluída: ",
        normal_recording_completed_suffix: " s por derivação.", settings_saved: "Configurações salvas.",
        hl7_saved_prefix: "HL7 aECG salvo: ", hl7_save_failed_prefix: "Falha ao salvar HL7 aECG: ",
        dicom_saved_prefix: "DICOM-ECG salvo: ", dicom_write_failed_prefix: "Falha ao gravar arquivo: ",
        save_hl7_title: "Salvar HL7 aECG", save_dicom_title: "Salvar DICOM-ECG",
    },
    page: {
        clinic_fallback: "Clínica", patient: "Paciente", edit_placeholder: "(editar)",
        exam_date: "Data do exame", physician: "Médico", birth: "Nascimento", age: "Idade",
        year_singular: "ano", year_plural: "anos",
        no_leads: "Nenhuma derivação selecionada ou encontrada para visualizar.",
        measurements: {
            heart_rate: "FC", range: "Faixa", qrs_axis: "Eixo QRS", degrees: "graus",
            rhythm_regular: "ritmo RR regular", rhythm_irregular: "ritmo RR irregular",
            axis_unavailable: "indisponível", axis_normal: "normal",
            axis_left: "desvio esquerdo", axis_right: "desvio direito",
            axis_extreme: "eixo extremo", axis_indeterminate: "indeterminado",
        },
    },
};

static PT_PT: Texts = texts! {
    ui: {
        clinic_default: "Clínica", no_logo: "Sem logótipo", no_file_loaded: "Nenhum ficheiro carregado",
        empty_status: "Abra um ECG para visualizar a página.", live_capture: "Captura em direto",
        device: "Aparelho", recording: "A gravar", record: "Gravar", capturing: "A capturar",
        start_live: "Iniciar direto", stop: "Parar", header: "Cabeçalho", clinic: "Clínica",
        physician: "Médico", physician_placeholder: "Nome do médico", logo: "Logótipo",
        choose: "Escolher", remove: "Remover", patient: "Paciente",
        patient_placeholder: "Nome do paciente", birth_date: "Data de nascimento",
        birth_date_placeholder: "dd/mm/aaaa", exam_date: "Data do exame",
        exam_date_placeholder: "dd/mm/aaaa hh:mm", view: "Visualização", filter: "Filtro",
        orientation: "Orientação", grid: "Grelha", printed_leads: "Derivações impressas",
        calibration_pulse: "Impulso de calibração", save_label: "Guardar", open: "Abrir",
        save: "Guardar", print: "Imprimir", about: "Sobre", settings: "Definições",
        about_title: "Sobre ECG Studio", version: "Versão 1.0", made_with: "Feito com:",
        settings_title: "Definições", language: "Idioma", cancel: "Cancelar",
        filter_none: "Sem filtro", filter_baseline: "Remover linha de base",
        filter_low_pass: "Baixa passagem 40 Hz", filter_diagnostic: "Diagnóstico",
        filter_monitor: "Monitor", orientation_landscape: "Paisagem", orientation_portrait: "Retrato",
        grid_light_salmon: "Salmão Claro", grid_technical_gray: "Cinza Técnico",
    },
    status: {
        sample_singular: "amostra", sample_plural: "amostras",
        recorded_sample_singular: "amostra gravada", recorded_sample_plural: "amostras gravadas",
        lead_singular: "derivação", lead_plural: "derivações",
        background_queue_unavailable: "Fila de trabalho em segundo plano indisponível.",
        open_ecg_title: "Abrir ECG", choose_logo_title: "Escolher logótipo",
        all_files_filter: "Todos os ficheiros", image_filter: "Imagem",
        invalid_image_format_prefix: "formato de imagem inválido",
        unsupported_logo_format: "use PNG, JPG ou BMP",
        load_logo_failed_prefix: "Falha ao carregar logótipo: ",
        saved_logo_load_failed_prefix: "O logótipo guardado não carregou: ",
        open_about_failed_prefix: "Falha ao abrir Sobre: ",
        open_settings_failed_prefix: "Falha ao abrir Definições: ",
        loading_ecg_prefix: "A carregar ECG: ", saving_ecg_prefix: "A guardar ECG: ",
        error_prefix: "Erro: ", no_live_recording_export: "Nenhuma gravação em direto está pronta para exportar.",
        no_ecg_export: "Nenhum ECG carregado para exportar.",
        no_live_recording_print: "Nenhuma gravação em direto está pronta para imprimir.",
        no_ecg_print: "Nenhum ECG carregado para imprimir.",
        select_lead_print: "Selecione pelo menos uma derivação para imprimir.",
        opening_print_dialog: "A abrir diálogo de impressão.", print_sent: "ECG enviado para impressão.",
        live_already_running: "A captura em direto já está em curso.",
        start_recording_first: "Inicie a captura em direto antes de gravar.",
        live_recording_already_running: "A gravação do ECG em direto já está em curso.",
        stopping_live: "A parar captura em direto...", live_stopped_recording_ready: "Captura em direto parada. A gravação está pronta para guardar.",
        live_stopped: "Captura em direto parada.", no_live_running: "Nenhuma captura em direto está em curso.",
        live_finished_recording_ready: "Captura em direto terminada. A gravação está pronta para guardar.",
        live_finished: "Captura em direto terminada.", live_error_prefix: "Erro na captura em direto: ",
        start_live_prefix: "A iniciar captura em direto do ",
        normal_recording_started_prefix: "Gravação normal do ECG em direto iniciada no ",
        normal_recording_completed_prefix: "Gravação normal concluída: ",
        normal_recording_completed_suffix: " s por derivação.", settings_saved: "Definições guardadas.",
        hl7_saved_prefix: "HL7 aECG guardado: ", hl7_save_failed_prefix: "Falha ao guardar HL7 aECG: ",
        dicom_saved_prefix: "DICOM-ECG guardado: ", dicom_write_failed_prefix: "Falha ao gravar ficheiro: ",
        save_hl7_title: "Guardar HL7 aECG", save_dicom_title: "Guardar DICOM-ECG",
    },
    page: {
        clinic_fallback: "Clínica", patient: "Paciente", edit_placeholder: "(editar)",
        exam_date: "Data do exame", physician: "Médico", birth: "Nascimento", age: "Idade",
        year_singular: "ano", year_plural: "anos",
        no_leads: "Nenhuma derivação selecionada ou encontrada para visualizar.",
        measurements: {
            heart_rate: "FC", range: "Faixa", qrs_axis: "Eixo QRS", degrees: "graus",
            rhythm_regular: "ritmo RR regular", rhythm_irregular: "ritmo RR irregular",
            axis_unavailable: "indisponível", axis_normal: "normal",
            axis_left: "desvio esquerdo", axis_right: "desvio direito",
            axis_extreme: "eixo extremo", axis_indeterminate: "indeterminado",
        },
    },
};

static EN: Texts = texts! {
    ui: {
        clinic_default: "Clinic", no_logo: "No logo", no_file_loaded: "No file loaded",
        empty_status: "Open an ECG to preview the page.", live_capture: "Live capture",
        device: "Device", recording: "Recording", record: "Record", capturing: "Capturing",
        start_live: "Start live", stop: "Stop", header: "Header", clinic: "Clinic",
        physician: "Physician", physician_placeholder: "Physician name", logo: "Logo",
        choose: "Choose", remove: "Remove", patient: "Patient",
        patient_placeholder: "Patient name", birth_date: "Date of birth",
        birth_date_placeholder: "dd/mm/yyyy", exam_date: "Exam date",
        exam_date_placeholder: "dd/mm/yyyy hh:mm", view: "View", filter: "Filter",
        orientation: "Orientation", grid: "Grid", printed_leads: "Printed leads",
        calibration_pulse: "Calibration pulse", save_label: "Save", open: "Open",
        save: "Save", print: "Print", about: "About", settings: "Settings",
        about_title: "About ECG Studio", version: "Version 1.0", made_with: "Built with:",
        settings_title: "Settings", language: "Language", cancel: "Cancel",
        filter_none: "No filter", filter_baseline: "Remove baseline wander",
        filter_low_pass: "Low pass 40 Hz", filter_diagnostic: "Diagnostic",
        filter_monitor: "Monitor", orientation_landscape: "Landscape", orientation_portrait: "Portrait",
        grid_light_salmon: "Light Salmon", grid_technical_gray: "Technical Gray",
    },
    status: {
        sample_singular: "sample", sample_plural: "samples",
        recorded_sample_singular: "recorded sample", recorded_sample_plural: "recorded samples",
        lead_singular: "lead", lead_plural: "leads",
        background_queue_unavailable: "Background work queue is unavailable.",
        open_ecg_title: "Open ECG", choose_logo_title: "Choose logo",
        all_files_filter: "All files", image_filter: "Image",
        invalid_image_format_prefix: "invalid image format",
        unsupported_logo_format: "use PNG, JPG, or BMP",
        load_logo_failed_prefix: "Failed to load logo: ",
        saved_logo_load_failed_prefix: "Saved logo did not load: ",
        open_about_failed_prefix: "Failed to open About: ",
        open_settings_failed_prefix: "Failed to open Settings: ",
        loading_ecg_prefix: "Loading ECG: ", saving_ecg_prefix: "Saving ECG: ",
        error_prefix: "Error: ", no_live_recording_export: "No live recording is ready to export.",
        no_ecg_export: "No ECG loaded to export.",
        no_live_recording_print: "No live recording is ready to print.",
        no_ecg_print: "No ECG loaded to print.",
        select_lead_print: "Select at least one lead to print.",
        opening_print_dialog: "Opening print dialog.", print_sent: "ECG sent to printer.",
        live_already_running: "Live capture is already running.",
        start_recording_first: "Start live capture before recording.",
        live_recording_already_running: "Live ECG recording is already running.",
        stopping_live: "Stopping live capture...", live_stopped_recording_ready: "Live capture stopped. The recording is ready to save.",
        live_stopped: "Live capture stopped.", no_live_running: "No live capture is running.",
        live_finished_recording_ready: "Live capture finished. The recording is ready to save.",
        live_finished: "Live capture finished.", live_error_prefix: "Live capture error: ",
        start_live_prefix: "Starting live capture from ",
        normal_recording_started_prefix: "Normal live ECG recording started on ",
        normal_recording_completed_prefix: "Normal recording complete: ",
        normal_recording_completed_suffix: " s per lead.", settings_saved: "Settings saved.",
        hl7_saved_prefix: "HL7 aECG saved: ", hl7_save_failed_prefix: "Failed to save HL7 aECG: ",
        dicom_saved_prefix: "DICOM-ECG saved: ", dicom_write_failed_prefix: "Failed to write file: ",
        save_hl7_title: "Save HL7 aECG", save_dicom_title: "Save DICOM-ECG",
    },
    page: {
        clinic_fallback: "Clinic", patient: "Patient", edit_placeholder: "(edit)",
        exam_date: "Exam date", physician: "Physician", birth: "Birth", age: "Age",
        year_singular: "year", year_plural: "years",
        no_leads: "No leads selected or found to preview.",
        measurements: {
            heart_rate: "HR", range: "Range", qrs_axis: "QRS axis", degrees: "degrees",
            rhythm_regular: "regular RR rhythm", rhythm_irregular: "irregular RR rhythm",
            axis_unavailable: "unavailable", axis_normal: "normal",
            axis_left: "left axis deviation", axis_right: "right axis deviation",
            axis_extreme: "extreme axis", axis_indeterminate: "indeterminate",
        },
    },
};

static ES_LATAM: Texts = texts! {
    ui: {
        clinic_default: "Clínica", no_logo: "Sin logotipo", no_file_loaded: "Ningún archivo cargado",
        empty_status: "Abra un ECG para previsualizar la página.", live_capture: "Captura en vivo",
        device: "Equipo", recording: "Grabando", record: "Grabar", capturing: "Capturando",
        start_live: "Iniciar en vivo", stop: "Detener", header: "Encabezado", clinic: "Clínica",
        physician: "Médico", physician_placeholder: "Nombre del médico", logo: "Logotipo",
        choose: "Elegir", remove: "Quitar", patient: "Paciente",
        patient_placeholder: "Nombre del paciente", birth_date: "Fecha de nacimiento",
        birth_date_placeholder: "dd/mm/aaaa", exam_date: "Fecha del examen",
        exam_date_placeholder: "dd/mm/aaaa hh:mm", view: "Visualización", filter: "Filtro",
        orientation: "Orientación", grid: "Cuadrícula", printed_leads: "Derivaciones impresas",
        calibration_pulse: "Pulso de calibración", save_label: "Guardar", open: "Abrir",
        save: "Guardar", print: "Imprimir", about: "Acerca de", settings: "Configuración",
        about_title: "Acerca de ECG Studio", version: "Versión 1.0", made_with: "Hecho con:",
        settings_title: "Configuración", language: "Idioma", cancel: "Cancelar",
        filter_none: "Sin filtro", filter_baseline: "Eliminar línea de base",
        filter_low_pass: "Paso bajo 40 Hz", filter_diagnostic: "Diagnóstico",
        filter_monitor: "Monitor", orientation_landscape: "Horizontal", orientation_portrait: "Vertical",
        grid_light_salmon: "Salmón claro", grid_technical_gray: "Gris técnico",
    },
    status: {
        sample_singular: "muestra", sample_plural: "muestras",
        recorded_sample_singular: "muestra grabada", recorded_sample_plural: "muestras grabadas",
        lead_singular: "derivación", lead_plural: "derivaciones",
        background_queue_unavailable: "La cola de trabajo en segundo plano no está disponible.",
        open_ecg_title: "Abrir ECG", choose_logo_title: "Elegir logotipo",
        all_files_filter: "Todos los archivos", image_filter: "Imagen",
        invalid_image_format_prefix: "formato de imagen inválido",
        unsupported_logo_format: "use PNG, JPG o BMP",
        load_logo_failed_prefix: "No se pudo cargar el logotipo: ",
        saved_logo_load_failed_prefix: "El logotipo guardado no se cargó: ",
        open_about_failed_prefix: "No se pudo abrir Acerca de: ",
        open_settings_failed_prefix: "No se pudo abrir Configuración: ",
        loading_ecg_prefix: "Cargando ECG: ", saving_ecg_prefix: "Guardando ECG: ",
        error_prefix: "Error: ", no_live_recording_export: "No hay una grabación en vivo lista para exportar.",
        no_ecg_export: "No hay un ECG cargado para exportar.",
        no_live_recording_print: "No hay una grabación en vivo lista para imprimir.",
        no_ecg_print: "No hay un ECG cargado para imprimir.",
        select_lead_print: "Seleccione al menos una derivación para imprimir.",
        opening_print_dialog: "Abriendo diálogo de impresión.", print_sent: "ECG enviado a la impresora.",
        live_already_running: "La captura en vivo ya está en curso.",
        start_recording_first: "Inicie la captura en vivo antes de grabar.",
        live_recording_already_running: "La grabación del ECG en vivo ya está en curso.",
        stopping_live: "Deteniendo captura en vivo...", live_stopped_recording_ready: "Captura en vivo detenida. La grabación está lista para guardar.",
        live_stopped: "Captura en vivo detenida.", no_live_running: "No hay captura en vivo en curso.",
        live_finished_recording_ready: "Captura en vivo finalizada. La grabación está lista para guardar.",
        live_finished: "Captura en vivo finalizada.", live_error_prefix: "Error en la captura en vivo: ",
        start_live_prefix: "Iniciando captura en vivo desde ",
        normal_recording_started_prefix: "Grabación normal del ECG en vivo iniciada en ",
        normal_recording_completed_prefix: "Grabación normal completada: ",
        normal_recording_completed_suffix: " s por derivación.", settings_saved: "Configuración guardada.",
        hl7_saved_prefix: "HL7 aECG guardado: ", hl7_save_failed_prefix: "No se pudo guardar HL7 aECG: ",
        dicom_saved_prefix: "DICOM-ECG guardado: ", dicom_write_failed_prefix: "No se pudo escribir el archivo: ",
        save_hl7_title: "Guardar HL7 aECG", save_dicom_title: "Guardar DICOM-ECG",
    },
    page: {
        clinic_fallback: "Clínica", patient: "Paciente", edit_placeholder: "(editar)",
        exam_date: "Fecha del examen", physician: "Médico", birth: "Nacimiento", age: "Edad",
        year_singular: "año", year_plural: "años",
        no_leads: "No hay derivaciones seleccionadas o encontradas para previsualizar.",
        measurements: {
            heart_rate: "FC", range: "Rango", qrs_axis: "Eje QRS", degrees: "grados",
            rhythm_regular: "ritmo RR regular", rhythm_irregular: "ritmo RR irregular",
            axis_unavailable: "no disponible", axis_normal: "normal",
            axis_left: "desviación izquierda", axis_right: "desviación derecha",
            axis_extreme: "eje extremo", axis_indeterminate: "indeterminado",
        },
    },
};

static ES_ES: Texts = texts! {
    ui: {
        clinic_default: "Clínica", no_logo: "Sin logotipo", no_file_loaded: "Ningún archivo cargado",
        empty_status: "Abra un ECG para previsualizar la página.", live_capture: "Captura en directo",
        device: "Equipo", recording: "Grabando", record: "Grabar", capturing: "Capturando",
        start_live: "Iniciar directo", stop: "Detener", header: "Encabezado", clinic: "Clínica",
        physician: "Médico", physician_placeholder: "Nombre del médico", logo: "Logotipo",
        choose: "Elegir", remove: "Quitar", patient: "Paciente",
        patient_placeholder: "Nombre del paciente", birth_date: "Fecha de nacimiento",
        birth_date_placeholder: "dd/mm/aaaa", exam_date: "Fecha del examen",
        exam_date_placeholder: "dd/mm/aaaa hh:mm", view: "Visualización", filter: "Filtro",
        orientation: "Orientación", grid: "Cuadrícula", printed_leads: "Derivaciones impresas",
        calibration_pulse: "Pulso de calibración", save_label: "Guardar", open: "Abrir",
        save: "Guardar", print: "Imprimir", about: "Acerca de", settings: "Configuración",
        about_title: "Acerca de ECG Studio", version: "Versión 1.0", made_with: "Hecho con:",
        settings_title: "Configuración", language: "Idioma", cancel: "Cancelar",
        filter_none: "Sin filtro", filter_baseline: "Eliminar línea de base",
        filter_low_pass: "Paso bajo 40 Hz", filter_diagnostic: "Diagnóstico",
        filter_monitor: "Monitor", orientation_landscape: "Horizontal", orientation_portrait: "Vertical",
        grid_light_salmon: "Salmón claro", grid_technical_gray: "Gris técnico",
    },
    status: {
        sample_singular: "muestra", sample_plural: "muestras",
        recorded_sample_singular: "muestra grabada", recorded_sample_plural: "muestras grabadas",
        lead_singular: "derivación", lead_plural: "derivaciones",
        background_queue_unavailable: "La cola de trabajo en segundo plano no está disponible.",
        open_ecg_title: "Abrir ECG", choose_logo_title: "Elegir logotipo",
        all_files_filter: "Todos los archivos", image_filter: "Imagen",
        invalid_image_format_prefix: "formato de imagen inválido",
        unsupported_logo_format: "use PNG, JPG o BMP",
        load_logo_failed_prefix: "No se pudo cargar el logotipo: ",
        saved_logo_load_failed_prefix: "El logotipo guardado no se cargó: ",
        open_about_failed_prefix: "No se pudo abrir Acerca de: ",
        open_settings_failed_prefix: "No se pudo abrir Configuración: ",
        loading_ecg_prefix: "Cargando ECG: ", saving_ecg_prefix: "Guardando ECG: ",
        error_prefix: "Error: ", no_live_recording_export: "No hay una grabación en directo lista para exportar.",
        no_ecg_export: "No hay un ECG cargado para exportar.",
        no_live_recording_print: "No hay una grabación en directo lista para imprimir.",
        no_ecg_print: "No hay un ECG cargado para imprimir.",
        select_lead_print: "Seleccione al menos una derivación para imprimir.",
        opening_print_dialog: "Abriendo diálogo de impresión.", print_sent: "ECG enviado a la impresora.",
        live_already_running: "La captura en directo ya está en curso.",
        start_recording_first: "Inicie la captura en directo antes de grabar.",
        live_recording_already_running: "La grabación del ECG en directo ya está en curso.",
        stopping_live: "Deteniendo captura en directo...",
        live_stopped_recording_ready: "Captura en directo detenida. La grabación está lista para guardar.",
        live_stopped: "Captura en directo detenida.",
        no_live_running: "No hay captura en directo en curso.",
        live_finished_recording_ready: "Captura en directo finalizada. La grabación está lista para guardar.",
        live_finished: "Captura en directo finalizada.",
        live_error_prefix: "Error en la captura en directo: ",
        start_live_prefix: "Iniciando captura en directo desde ",
        normal_recording_started_prefix: "Grabación normal del ECG en directo iniciada en ",
        normal_recording_completed_prefix: "Grabación normal completada: ",
        normal_recording_completed_suffix: " s por derivación.", settings_saved: "Configuración guardada.",
        hl7_saved_prefix: "HL7 aECG guardado: ", hl7_save_failed_prefix: "No se pudo guardar HL7 aECG: ",
        dicom_saved_prefix: "DICOM-ECG guardado: ", dicom_write_failed_prefix: "No se pudo escribir el archivo: ",
        save_hl7_title: "Guardar HL7 aECG", save_dicom_title: "Guardar DICOM-ECG",
    },
    page: {
        clinic_fallback: "Clínica", patient: "Paciente", edit_placeholder: "(editar)",
        exam_date: "Fecha del examen", physician: "Médico", birth: "Nacimiento", age: "Edad",
        year_singular: "año", year_plural: "años",
        no_leads: "No hay derivaciones seleccionadas o encontradas para previsualizar.",
        measurements: {
            heart_rate: "FC", range: "Rango", qrs_axis: "Eje QRS", degrees: "grados",
            rhythm_regular: "ritmo RR regular", rhythm_irregular: "ritmo RR irregular",
            axis_unavailable: "no disponible", axis_normal: "normal",
            axis_left: "desviación izquierda", axis_right: "desviación derecha",
            axis_extreme: "eje extremo", axis_indeterminate: "indeterminado",
        },
    },
};

static FR: Texts = texts! {
    ui: {
        clinic_default: "Clinique", no_logo: "Aucun logo", no_file_loaded: "Aucun fichier chargé",
        empty_status: "Ouvrez un ECG pour prévisualiser la page.", live_capture: "Acquisition en direct",
        device: "Appareil", recording: "Enregistrement", record: "Enregistrer", capturing: "Acquisition",
        start_live: "Démarrer", stop: "Arrêter", header: "En-tête", clinic: "Clinique",
        physician: "Médecin", physician_placeholder: "Nom du médecin", logo: "Logo",
        choose: "Choisir", remove: "Supprimer", patient: "Patient",
        patient_placeholder: "Nom du patient", birth_date: "Date de naissance",
        birth_date_placeholder: "jj/mm/aaaa", exam_date: "Date de l'examen",
        exam_date_placeholder: "jj/mm/aaaa hh:mm", view: "Affichage", filter: "Filtre",
        orientation: "Orientation", grid: "Grille", printed_leads: "Dérivations imprimées",
        calibration_pulse: "Impulsion d'étalonnage", save_label: "Enregistrer", open: "Ouvrir",
        save: "Enregistrer", print: "Imprimer", about: "À propos", settings: "Paramètres",
        about_title: "À propos d'ECG Studio", version: "Version 1.0", made_with: "Réalisé avec:",
        settings_title: "Paramètres", language: "Langue", cancel: "Annuler",
        filter_none: "Aucun filtre", filter_baseline: "Supprimer la ligne de base",
        filter_low_pass: "Passe-bas 40 Hz", filter_diagnostic: "Diagnostic",
        filter_monitor: "Moniteur", orientation_landscape: "Paysage", orientation_portrait: "Portrait",
        grid_light_salmon: "Saumon clair", grid_technical_gray: "Gris technique",
    },
    status: {
        sample_singular: "échantillon", sample_plural: "échantillons",
        recorded_sample_singular: "échantillon enregistré", recorded_sample_plural: "échantillons enregistrés",
        lead_singular: "dérivation", lead_plural: "dérivations",
        background_queue_unavailable: "La file de travail en arrière-plan est indisponible.",
        open_ecg_title: "Ouvrir ECG", choose_logo_title: "Choisir un logo",
        all_files_filter: "Tous les fichiers", image_filter: "Image",
        invalid_image_format_prefix: "format d'image invalide",
        unsupported_logo_format: "utilisez PNG, JPG ou BMP",
        load_logo_failed_prefix: "Échec du chargement du logo: ",
        saved_logo_load_failed_prefix: "Le logo enregistré n'a pas été chargé: ",
        open_about_failed_prefix: "Échec de l'ouverture À propos: ",
        open_settings_failed_prefix: "Échec de l'ouverture des paramètres: ",
        loading_ecg_prefix: "Chargement ECG: ", saving_ecg_prefix: "Enregistrement ECG: ",
        error_prefix: "Erreur: ", no_live_recording_export: "Aucun enregistrement en direct n'est prêt à exporter.",
        no_ecg_export: "Aucun ECG chargé à exporter.",
        no_live_recording_print: "Aucun enregistrement en direct n'est prêt à imprimer.",
        no_ecg_print: "Aucun ECG chargé à imprimer.",
        select_lead_print: "Sélectionnez au moins une dérivation à imprimer.",
        opening_print_dialog: "Ouverture de la boîte de dialogue d'impression.", print_sent: "ECG envoyé à l'imprimante.",
        live_already_running: "L'acquisition en direct est déjà en cours.",
        start_recording_first: "Démarrez l'acquisition en direct avant d'enregistrer.",
        live_recording_already_running: "L'enregistrement ECG en direct est déjà en cours.",
        stopping_live: "Arrêt de l'acquisition en direct...", live_stopped_recording_ready: "Acquisition en direct arrêtée. L'enregistrement est prêt à être sauvegardé.",
        live_stopped: "Acquisition en direct arrêtée.", no_live_running: "Aucune acquisition en direct en cours.",
        live_finished_recording_ready: "Acquisition en direct terminée. L'enregistrement est prêt à être sauvegardé.",
        live_finished: "Acquisition en direct terminée.", live_error_prefix: "Erreur d'acquisition en direct: ",
        start_live_prefix: "Démarrage de l'acquisition en direct depuis ",
        normal_recording_started_prefix: "Enregistrement ECG en direct normal démarré sur ",
        normal_recording_completed_prefix: "Enregistrement normal terminé: ",
        normal_recording_completed_suffix: " s par dérivation.", settings_saved: "Paramètres enregistrés.",
        hl7_saved_prefix: "HL7 aECG enregistré: ", hl7_save_failed_prefix: "Échec de l'enregistrement HL7 aECG: ",
        dicom_saved_prefix: "DICOM-ECG enregistré: ", dicom_write_failed_prefix: "Échec de l'écriture du fichier: ",
        save_hl7_title: "Enregistrer HL7 aECG", save_dicom_title: "Enregistrer DICOM-ECG",
    },
    page: {
        clinic_fallback: "Clinique", patient: "Patient", edit_placeholder: "(modifier)",
        exam_date: "Date de l'examen", physician: "Médecin", birth: "Naissance", age: "Âge",
        year_singular: "an", year_plural: "ans",
        no_leads: "Aucune dérivation sélectionnée ou trouvée pour la prévisualisation.",
        measurements: {
            heart_rate: "FC", range: "Plage", qrs_axis: "Axe QRS", degrees: "degrés",
            rhythm_regular: "rythme RR régulier", rhythm_irregular: "rythme RR irrégulier",
            axis_unavailable: "indisponible", axis_normal: "normal",
            axis_left: "déviation gauche", axis_right: "déviation droite",
            axis_extreme: "axe extrême", axis_indeterminate: "indéterminé",
        },
    },
};

static IT: Texts = texts! {
    ui: {
        clinic_default: "Clinica", no_logo: "Nessun logo", no_file_loaded: "Nessun file caricato",
        empty_status: "Apri un ECG per visualizzare l'anteprima della pagina.", live_capture: "Acquisizione live",
        device: "Dispositivo", recording: "Registrazione", record: "Registra", capturing: "Acquisizione",
        start_live: "Avvia live", stop: "Ferma", header: "Intestazione", clinic: "Clinica",
        physician: "Medico", physician_placeholder: "Nome del medico", logo: "Logo",
        choose: "Scegli", remove: "Rimuovi", patient: "Paziente",
        patient_placeholder: "Nome del paziente", birth_date: "Data di nascita",
        birth_date_placeholder: "gg/mm/aaaa", exam_date: "Data esame",
        exam_date_placeholder: "gg/mm/aaaa hh:mm", view: "Visualizzazione", filter: "Filtro",
        orientation: "Orientamento", grid: "Griglia", printed_leads: "Derivazioni stampate",
        calibration_pulse: "Impulso di calibrazione", save_label: "Salva", open: "Apri",
        save: "Salva", print: "Stampa", about: "Informazioni", settings: "Impostazioni",
        about_title: "Informazioni su ECG Studio", version: "Versione 1.0", made_with: "Realizzato con:",
        settings_title: "Impostazioni", language: "Lingua", cancel: "Annulla",
        filter_none: "Nessun filtro", filter_baseline: "Rimuovi linea di base",
        filter_low_pass: "Passa-basso 40 Hz", filter_diagnostic: "Diagnostico",
        filter_monitor: "Monitor", orientation_landscape: "Orizzontale", orientation_portrait: "Verticale",
        grid_light_salmon: "Salmone chiaro", grid_technical_gray: "Grigio tecnico",
    },
    status: {
        sample_singular: "campione", sample_plural: "campioni",
        recorded_sample_singular: "campione registrato", recorded_sample_plural: "campioni registrati",
        lead_singular: "derivazione", lead_plural: "derivazioni",
        background_queue_unavailable: "La coda di lavoro in background non è disponibile.",
        open_ecg_title: "Apri ECG", choose_logo_title: "Scegli logo",
        all_files_filter: "Tutti i file", image_filter: "Immagine",
        invalid_image_format_prefix: "formato immagine non valido",
        unsupported_logo_format: "usa PNG, JPG o BMP",
        load_logo_failed_prefix: "Impossibile caricare il logo: ",
        saved_logo_load_failed_prefix: "Il logo salvato non è stato caricato: ",
        open_about_failed_prefix: "Impossibile aprire Informazioni: ",
        open_settings_failed_prefix: "Impossibile aprire Impostazioni: ",
        loading_ecg_prefix: "Caricamento ECG: ", saving_ecg_prefix: "Salvataggio ECG: ",
        error_prefix: "Errore: ", no_live_recording_export: "Nessuna registrazione live pronta per l'esportazione.",
        no_ecg_export: "Nessun ECG caricato da esportare.",
        no_live_recording_print: "Nessuna registrazione live pronta per la stampa.",
        no_ecg_print: "Nessun ECG caricato da stampare.",
        select_lead_print: "Seleziona almeno una derivazione da stampare.",
        opening_print_dialog: "Apertura della finestra di stampa.", print_sent: "ECG inviato alla stampante.",
        live_already_running: "L'acquisizione live è già in corso.",
        start_recording_first: "Avvia l'acquisizione live prima di registrare.",
        live_recording_already_running: "La registrazione ECG live è già in corso.",
        stopping_live: "Arresto acquisizione live...", live_stopped_recording_ready: "Acquisizione live arrestata. La registrazione è pronta per il salvataggio.",
        live_stopped: "Acquisizione live arrestata.", no_live_running: "Nessuna acquisizione live in corso.",
        live_finished_recording_ready: "Acquisizione live terminata. La registrazione è pronta per il salvataggio.",
        live_finished: "Acquisizione live terminata.", live_error_prefix: "Errore acquisizione live: ",
        start_live_prefix: "Avvio acquisizione live da ",
        normal_recording_started_prefix: "Registrazione ECG live normale avviata su ",
        normal_recording_completed_prefix: "Registrazione normale completata: ",
        normal_recording_completed_suffix: " s per derivazione.", settings_saved: "Impostazioni salvate.",
        hl7_saved_prefix: "HL7 aECG salvato: ", hl7_save_failed_prefix: "Impossibile salvare HL7 aECG: ",
        dicom_saved_prefix: "DICOM-ECG salvato: ", dicom_write_failed_prefix: "Impossibile scrivere il file: ",
        save_hl7_title: "Salva HL7 aECG", save_dicom_title: "Salva DICOM-ECG",
    },
    page: {
        clinic_fallback: "Clinica", patient: "Paziente", edit_placeholder: "(modifica)",
        exam_date: "Data esame", physician: "Medico", birth: "Nascita", age: "Età",
        year_singular: "anno", year_plural: "anni",
        no_leads: "Nessuna derivazione selezionata o trovata per l'anteprima.",
        measurements: {
            heart_rate: "FC", range: "Intervallo", qrs_axis: "Asse QRS", degrees: "gradi",
            rhythm_regular: "ritmo RR regolare", rhythm_irregular: "ritmo RR irregolare",
            axis_unavailable: "non disponibile", axis_normal: "normale",
            axis_left: "deviazione sinistra", axis_right: "deviazione destra",
            axis_extreme: "asse estremo", axis_indeterminate: "indeterminato",
        },
    },
};

static DE: Texts = texts! {
    ui: {
        clinic_default: "Klinik", no_logo: "Kein Logo", no_file_loaded: "Keine Datei geladen",
        empty_status: "Öffnen Sie ein EKG, um die Seite anzuzeigen.", live_capture: "Live-Erfassung",
        device: "Gerät", recording: "Aufzeichnung", record: "Aufzeichnen", capturing: "Erfassung",
        start_live: "Live starten", stop: "Stopp", header: "Kopfzeile", clinic: "Klinik",
        physician: "Arzt", physician_placeholder: "Name des Arztes", logo: "Logo",
        choose: "Auswählen", remove: "Entfernen", patient: "Patient",
        patient_placeholder: "Name des Patienten", birth_date: "Geburtsdatum",
        birth_date_placeholder: "tt/mm/jjjj", exam_date: "Untersuchungsdatum",
        exam_date_placeholder: "tt/mm/jjjj hh:mm", view: "Ansicht", filter: "Filter",
        orientation: "Ausrichtung", grid: "Raster", printed_leads: "Gedruckte Ableitungen",
        calibration_pulse: "Kalibrierimpuls", save_label: "Speichern", open: "Öffnen",
        save: "Speichern", print: "Drucken", about: "Info", settings: "Einstellungen",
        about_title: "Info zu ECG Studio", version: "Version 1.0", made_with: "Erstellt mit:",
        settings_title: "Einstellungen", language: "Sprache", cancel: "Abbrechen",
        filter_none: "Kein Filter", filter_baseline: "Grundlinie entfernen",
        filter_low_pass: "Tiefpass 40 Hz", filter_diagnostic: "Diagnostisch",
        filter_monitor: "Monitor", orientation_landscape: "Querformat", orientation_portrait: "Hochformat",
        grid_light_salmon: "Helles Lachsrosa", grid_technical_gray: "Technisches Grau",
    },
    status: {
        sample_singular: "Probe", sample_plural: "Proben",
        recorded_sample_singular: "aufgezeichnete Probe", recorded_sample_plural: "aufgezeichnete Proben",
        lead_singular: "Ableitung", lead_plural: "Ableitungen",
        background_queue_unavailable: "Die Hintergrund-Arbeitswarteschlange ist nicht verfügbar.",
        open_ecg_title: "EKG öffnen", choose_logo_title: "Logo auswählen",
        all_files_filter: "Alle Dateien", image_filter: "Bild",
        invalid_image_format_prefix: "ungültiges Bildformat",
        unsupported_logo_format: "verwenden Sie PNG, JPG oder BMP",
        load_logo_failed_prefix: "Logo konnte nicht geladen werden: ",
        saved_logo_load_failed_prefix: "Gespeichertes Logo wurde nicht geladen: ",
        open_about_failed_prefix: "Info konnte nicht geöffnet werden: ",
        open_settings_failed_prefix: "Einstellungen konnten nicht geöffnet werden: ",
        loading_ecg_prefix: "EKG wird geladen: ", saving_ecg_prefix: "EKG wird gespeichert: ",
        error_prefix: "Fehler: ", no_live_recording_export: "Keine Live-Aufzeichnung zum Exportieren bereit.",
        no_ecg_export: "Kein EKG zum Exportieren geladen.",
        no_live_recording_print: "Keine Live-Aufzeichnung zum Drucken bereit.",
        no_ecg_print: "Kein EKG zum Drucken geladen.",
        select_lead_print: "Wählen Sie mindestens eine Ableitung zum Drucken aus.",
        opening_print_dialog: "Druckdialog wird geöffnet.", print_sent: "EKG an Drucker gesendet.",
        live_already_running: "Die Live-Erfassung läuft bereits.",
        start_recording_first: "Starten Sie die Live-Erfassung vor der Aufzeichnung.",
        live_recording_already_running: "Die Live-EKG-Aufzeichnung läuft bereits.",
        stopping_live: "Live-Erfassung wird gestoppt...", live_stopped_recording_ready: "Live-Erfassung gestoppt. Die Aufzeichnung kann gespeichert werden.",
        live_stopped: "Live-Erfassung gestoppt.", no_live_running: "Keine Live-Erfassung läuft.",
        live_finished_recording_ready: "Live-Erfassung abgeschlossen. Die Aufzeichnung kann gespeichert werden.",
        live_finished: "Live-Erfassung abgeschlossen.", live_error_prefix: "Fehler bei Live-Erfassung: ",
        start_live_prefix: "Live-Erfassung wird gestartet von ",
        normal_recording_started_prefix: "Normale Live-EKG-Aufzeichnung gestartet auf ",
        normal_recording_completed_prefix: "Normale Aufzeichnung abgeschlossen: ",
        normal_recording_completed_suffix: " s pro Ableitung.", settings_saved: "Einstellungen gespeichert.",
        hl7_saved_prefix: "HL7 aECG gespeichert: ", hl7_save_failed_prefix: "HL7 aECG konnte nicht gespeichert werden: ",
        dicom_saved_prefix: "DICOM-ECG gespeichert: ", dicom_write_failed_prefix: "Datei konnte nicht geschrieben werden: ",
        save_hl7_title: "HL7 aECG speichern", save_dicom_title: "DICOM-ECG speichern",
    },
    page: {
        clinic_fallback: "Klinik", patient: "Patient", edit_placeholder: "(bearbeiten)",
        exam_date: "Untersuchungsdatum", physician: "Arzt", birth: "Geburt", age: "Alter",
        year_singular: "Jahr", year_plural: "Jahre",
        no_leads: "Keine Ableitungen ausgewählt oder gefunden.",
        measurements: {
            heart_rate: "HF", range: "Bereich", qrs_axis: "QRS-Achse", degrees: "Grad",
            rhythm_regular: "regelmäßiger RR-Rhythmus", rhythm_irregular: "unregelmäßiger RR-Rhythmus",
            axis_unavailable: "nicht verfügbar", axis_normal: "normal",
            axis_left: "Linksabweichung", axis_right: "Rechtsabweichung",
            axis_extreme: "extreme Achse", axis_indeterminate: "unbestimmt",
        },
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_latin_american_spanish_locales_to_latam_spanish() {
        assert_eq!(Language::from_locale("es-MX"), Some(Language::EsLatam));
        assert_eq!(Language::from_locale("es-AR"), Some(Language::EsLatam));
        assert_eq!(Language::from_locale("es-ES"), Some(Language::EsEs));
    }

    #[test]
    fn uses_portugal_portuguese_only_for_pt_pt() {
        assert_eq!(Language::from_locale("pt-PT"), Some(Language::PtPt));
        assert_eq!(Language::from_locale("pt-BR"), Some(Language::PtBr));
        assert_eq!(Language::from_locale("pt-AO"), Some(Language::PtBr));
    }

    #[test]
    fn round_trips_language_selection_labels() {
        let selection = LanguageSelection::Language(Language::De);
        assert_eq!(LanguageSelection::from_label(selection.label()), selection);
        assert_eq!(LanguageSelection::from_code(selection.code()), selection);
    }
}
