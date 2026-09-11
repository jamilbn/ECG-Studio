use std::path::Path;

use crate::domain::{DocumentKind, EcgDocument, LeadData};

use super::{ReadError, ReadResult, read_bytes};

const LEAD_NAMES: [&str; 12] = [
    "I", "II", "III", "aVR", "aVL", "aVF", "V1", "V2", "V3", "V4", "V5", "V6",
];
const MICROVOLTS_PER_LSB: f64 = 2.5;
const ADC_OFFSET: f64 = 2048.0;
const SAMPLE_INTERVAL_SECONDS: f64 = 0.002;

pub fn read(path: &Path) -> ReadResult<EcgDocument> {
    let bytes = read_bytes(path)?;
    if bytes.is_empty() {
        return Err(ReadError::new("O arquivo C8K está vazio."));
    }
    if bytes.len() % std::mem::size_of::<i16>() != 0 {
        return Err(ReadError::new(
            "O arquivo C8K não possui tamanho compatível com amostras de 16 bits.",
        ));
    }

    let sample_count = bytes.len() / std::mem::size_of::<i16>();
    if !sample_count.is_multiple_of(LEAD_NAMES.len()) {
        return Err(ReadError::new(
            "O leitor C8K espera 12 canais intercalados.",
        ));
    }

    let samples_per_lead = sample_count / LEAD_NAMES.len();
    let mut document = EcgDocument::new(DocumentKind::C8k, path.to_path_buf());
    document.sample_interval_seconds = SAMPLE_INTERVAL_SECONDS;
    document.leads.reserve(LEAD_NAMES.len());

    for (lead_index, lead_name) in LEAD_NAMES.iter().enumerate() {
        let mut samples = Vec::with_capacity(samples_per_lead);
        for sample_index in 0..samples_per_lead {
            let raw_index = (sample_index * LEAD_NAMES.len()) + lead_index;
            let raw = read_i16_le(&bytes, raw_index);
            samples.push((raw as f64 - ADC_OFFSET) * MICROVOLTS_PER_LSB);
        }
        document.leads.push(LeadData::new(*lead_name, samples));
    }

    Ok(document)
}

fn read_i16_le(bytes: &[u8], sample_index: usize) -> i16 {
    let offset = sample_index * std::mem::size_of::<i16>();
    i16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::NORMAL_LEAD_SAMPLE_SECONDS;

    #[test]
    fn converts_adc_to_microvolts() {
        let bytes = 2049_i16.to_le_bytes();
        assert_eq!(
            (read_i16_le(&bytes, 0) as f64 - ADC_OFFSET) * MICROVOLTS_PER_LSB,
            2.5
        );
    }

    #[test]
    fn reads_workstation_c8k_waveforms_as_ten_seconds_at_500_hz() {
        let path = std::env::temp_dir().join(format!("ecg-studio-c8k-rate-{}", std::process::id()));
        let samples_per_lead = 5_000;
        let bytes = 2048_i16
            .to_le_bytes()
            .repeat(LEAD_NAMES.len() * samples_per_lead);
        std::fs::write(&path, bytes).unwrap();

        let document = read(&path).unwrap();

        assert_eq!(document.sample_interval_seconds, SAMPLE_INTERVAL_SECONDS);
        assert_eq!(document.sample_rate_hz(), 500.0);
        assert_eq!(document.max_sample_count(), samples_per_lead);
        assert!((document.duration_seconds() - NORMAL_LEAD_SAMPLE_SECONDS).abs() < 0.000_001);

        let _ = std::fs::remove_file(path);
    }
}
