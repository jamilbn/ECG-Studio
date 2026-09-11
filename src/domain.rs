use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const NORMAL_LEAD_SAMPLE_SECONDS: f64 = 10.0;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentKind {
    Xml,
    C8k,
    ContecEcg,
    DicomEcg,
    ContecLive,
}

impl DocumentKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Xml => "XML HL7/aECG",
            Self::C8k => "C8K",
            Self::ContecEcg => "Contec ECG",
            Self::DicomEcg => "DICOM-ECG",
            Self::ContecLive => "Contec live",
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self, Self::ContecLive)
    }
}

#[derive(Clone, Debug)]
pub struct LeadData {
    pub name: String,
    pub samples_microvolts: Vec<f64>,
}

impl LeadData {
    pub fn new(name: impl Into<String>, samples_microvolts: Vec<f64>) -> Self {
        Self {
            name: name.into(),
            samples_microvolts,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EcgDocument {
    pub kind: DocumentKind,
    pub source_path: PathBuf,
    pub clinic_name: String,
    pub physician_name: String,
    pub patient_name: String,
    pub patient_birth_date: String,
    pub exam_date: String,
    pub sample_interval_seconds: f64,
    pub leads: Vec<LeadData>,
}

impl EcgDocument {
    pub fn new(kind: DocumentKind, source_path: PathBuf) -> Self {
        Self {
            kind,
            source_path,
            clinic_name: "Clínica".to_owned(),
            physician_name: String::new(),
            patient_name: String::new(),
            patient_birth_date: String::new(),
            exam_date: String::new(),
            sample_interval_seconds: 0.002,
            leads: Vec::new(),
        }
    }

    pub fn sample_rate_hz(&self) -> f64 {
        if self.sample_interval_seconds > 0.0 {
            1.0 / self.sample_interval_seconds
        } else {
            500.0
        }
    }

    pub fn max_sample_count(&self) -> usize {
        self.leads
            .iter()
            .map(|lead| lead.samples_microvolts.len())
            .max()
            .unwrap_or(0)
    }

    pub fn duration_seconds(&self) -> f64 {
        self.max_sample_count() as f64 * self.sample_interval_seconds.max(0.0)
    }

    pub fn display_file_name(&self) -> String {
        self.source_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("(sem nome)")
            .to_owned()
    }

    pub fn find_lead(&self, name: &str) -> Option<&LeadData> {
        self.leads.iter().find(|lead| lead.name == name)
    }

    pub fn patient_age_years(&self) -> Option<u32> {
        let birth = SimpleDate::parse(&self.patient_birth_date)?;
        let reference = SimpleDate::parse(&self.exam_date).or_else(SimpleDate::today)?;
        birth.age_years_on(reference)
    }

    pub fn patient_birth_date_yyyymmdd(&self) -> Option<String> {
        SimpleDate::parse(&self.patient_birth_date).map(SimpleDate::yyyymmdd)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SimpleDate {
    year: i32,
    month: u8,
    day: u8,
}

impl SimpleDate {
    fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }

        let groups: Vec<&str> = trimmed
            .split(|ch: char| !ch.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .collect();
        if groups.len() >= 3 {
            let first = groups[0];
            let (year, month, day) = if first.len() == 4 {
                (
                    first.parse().ok()?,
                    groups[1].parse().ok()?,
                    groups[2].parse().ok()?,
                )
            } else {
                (
                    groups[2].parse().ok()?,
                    groups[1].parse().ok()?,
                    first.parse().ok()?,
                )
            };
            return Self::new(year, month, day);
        }

        let digits: String = trimmed.chars().filter(|ch| ch.is_ascii_digit()).collect();
        if digits.len() < 8 {
            return None;
        }

        if starts_with_year(&digits) {
            Self::new(
                digits[0..4].parse().ok()?,
                digits[4..6].parse().ok()?,
                digits[6..8].parse().ok()?,
            )
        } else {
            Self::new(
                digits[4..8].parse().ok()?,
                digits[2..4].parse().ok()?,
                digits[0..2].parse().ok()?,
            )
        }
    }

    fn today() -> Option<Self> {
        let days = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs() / 86_400;
        Some(Self::from_unix_days(days as i64))
    }

    fn new(year: i32, month: u8, day: u8) -> Option<Self> {
        if !(1800..=2200).contains(&year) || month == 0 || month > 12 {
            return None;
        }
        let max_day = days_in_month(year, month);
        if day == 0 || day > max_day {
            return None;
        }
        Some(Self { year, month, day })
    }

    fn age_years_on(self, reference: Self) -> Option<u32> {
        if (reference.year, reference.month, reference.day) < (self.year, self.month, self.day) {
            return None;
        }
        let had_birthday = (reference.month, reference.day) >= (self.month, self.day);
        Some((reference.year - self.year - i32::from(!had_birthday)) as u32)
    }

    fn yyyymmdd(self) -> String {
        format!("{:04}{:02}{:02}", self.year, self.month, self.day)
    }

    fn from_unix_days(days: i64) -> Self {
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

        Self {
            year: year as i32,
            month: month as u8,
            day: day as u8,
        }
    }
}

fn starts_with_year(digits: &str) -> bool {
    digits
        .get(0..4)
        .and_then(|year| year.parse::<i32>().ok())
        .is_some_and(|year| (1800..=2200).contains(&year))
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_patient_age_from_exam_date() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.patient_birth_date = "10/05/1980".to_owned();
        document.exam_date = "11/05/2026".to_owned();

        assert_eq!(document.patient_age_years(), Some(46));
    }

    #[test]
    fn waits_for_birthday_before_incrementing_age() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.patient_birth_date = "12/05/1980".to_owned();
        document.exam_date = "11/05/2026".to_owned();

        assert_eq!(document.patient_age_years(), Some(45));
    }

    #[test]
    fn parses_hl7_compact_dates() {
        assert_eq!(
            SimpleDate::parse("20260511"),
            Some(SimpleDate {
                year: 2026,
                month: 5,
                day: 11,
            })
        );
    }

    #[test]
    fn formats_patient_birth_date_for_exports() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.patient_birth_date = "11/05/2026".to_owned();

        assert_eq!(
            document.patient_birth_date_yyyymmdd(),
            Some("20260511".to_owned())
        );
    }
}
