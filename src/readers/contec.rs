use std::array;
use std::path::Path;

use crate::domain::{DocumentKind, EcgDocument, LeadData};

use super::{ReadError, ReadResult, read_bytes};

const HEADER_BYTES: usize = 43;
const FOOTER_BYTES: usize = 37;
const STORED_LEAD_COUNT: usize = 8;
const FRAME_BYTES: usize = STORED_LEAD_COUNT * std::mem::size_of::<u16>();
const SAMPLE_INTERVAL_SECONDS: f64 = 1.0 / 800.0;
const MICROVOLTS_PER_LSB: f64 = 5.0;
const ADC_OFFSET: f64 = 2048.0;
const DISCONNECTED_SAMPLE: u16 = 0x6800;
const STORED_LEAD_NAMES: [&str; STORED_LEAD_COUNT] =
    ["I", "II", "V1", "V2", "V3", "V4", "V5", "V6"];

pub fn read(path: &Path) -> ReadResult<EcgDocument> {
    let bytes = read_bytes(path)?;
    if bytes.is_empty() {
        return Err(ReadError::new("O arquivo ECG está vazio."));
    }

    let payload_bytes = resolve_payload_bytes(&bytes).ok_or_else(|| {
        ReadError::new(
            "O arquivo ECG não corresponde ao formato Contec esperado (43 bytes de cabeçalho, 8 canais e rodapé opcional).",
        )
    })?;
    let samples_per_lead = payload_bytes / FRAME_BYTES;
    if samples_per_lead < 2 {
        return Err(ReadError::new(
            "O arquivo ECG não possui amostras suficientes para montar o traçado.",
        ));
    }

    let mut stored_leads: [Vec<f64>; STORED_LEAD_COUNT] =
        array::from_fn(|_| Vec::with_capacity(samples_per_lead));
    let mut last_valid_values = [0.0; STORED_LEAD_COUNT];
    let mut has_last_valid = [false; STORED_LEAD_COUNT];

    for sample_index in 0..samples_per_lead {
        let frame_offset = HEADER_BYTES + (sample_index * FRAME_BYTES);
        for lead_index in 0..STORED_LEAD_COUNT {
            let raw = read_u16_le(
                &bytes,
                frame_offset + (lead_index * std::mem::size_of::<u16>()),
            );
            let sample = if raw == DISCONNECTED_SAMPLE {
                if has_last_valid[lead_index] {
                    last_valid_values[lead_index]
                } else {
                    0.0
                }
            } else {
                let value = (raw as f64 - ADC_OFFSET) * MICROVOLTS_PER_LSB;
                last_valid_values[lead_index] = value;
                has_last_valid[lead_index] = true;
                value
            };
            stored_leads[lead_index].push(sample);
        }
    }

    let mut document = EcgDocument::new(DocumentKind::ContecEcg, path.to_path_buf());
    document.patient_name = read_ascii_field(&bytes, 32, 8);
    document.exam_date = format_contec_timestamp(&read_ascii_field(&bytes, 10, 20));
    document.sample_interval_seconds = SAMPLE_INTERVAL_SECONDS;
    document.leads.reserve(12);

    let lead_i = std::mem::take(&mut stored_leads[0]);
    let lead_ii = std::mem::take(&mut stored_leads[1]);
    document.leads.push(LeadData::new("I", lead_i));
    document.leads.push(LeadData::new("II", lead_ii));
    document.leads.push(LeadData::new(
        "III",
        derive_lead_iii(
            &document.leads[0].samples_microvolts,
            &document.leads[1].samples_microvolts,
        ),
    ));
    document.leads.push(LeadData::new(
        "aVR",
        derive_lead_avr(
            &document.leads[0].samples_microvolts,
            &document.leads[1].samples_microvolts,
        ),
    ));
    document.leads.push(LeadData::new(
        "aVL",
        derive_lead_avl(
            &document.leads[0].samples_microvolts,
            &document.leads[1].samples_microvolts,
        ),
    ));
    document.leads.push(LeadData::new(
        "aVF",
        derive_lead_avf(
            &document.leads[0].samples_microvolts,
            &document.leads[1].samples_microvolts,
        ),
    ));

    for lead_index in 2..STORED_LEAD_COUNT {
        document.leads.push(LeadData::new(
            STORED_LEAD_NAMES[lead_index],
            std::mem::take(&mut stored_leads[lead_index]),
        ));
    }

    Ok(document)
}

fn resolve_payload_bytes(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < HEADER_BYTES + FRAME_BYTES {
        return None;
    }
    if bytes.len() >= HEADER_BYTES + FOOTER_BYTES {
        let without_footer = bytes.len() - HEADER_BYTES - FOOTER_BYTES;
        if without_footer.is_multiple_of(FRAME_BYTES) {
            return Some(without_footer);
        }
    }
    let payload = bytes.len() - HEADER_BYTES;
    payload.is_multiple_of(FRAME_BYTES).then_some(payload)
}

fn read_u16_le(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn read_ascii_field(bytes: &[u8], offset: usize, length: usize) -> String {
    if offset >= bytes.len() {
        return String::new();
    }
    let end = (offset + length).min(bytes.len());
    bytes[offset..end]
        .iter()
        .copied()
        .take_while(|byte| *byte != 0)
        .map(char::from)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn format_contec_timestamp(raw: &str) -> String {
    if raw.len() < 19 {
        return raw.to_owned();
    }
    let bytes = raw.as_bytes();
    if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') || bytes.get(10) != Some(&b' ') {
        return raw.to_owned();
    }
    format!(
        "{}/{}/{} {}",
        &raw[8..10],
        &raw[5..7],
        &raw[0..4],
        &raw[11..19]
    )
}

fn derive_lead_iii(lead_i: &[f64], lead_ii: &[f64]) -> Vec<f64> {
    zip_map(lead_i, lead_ii, |i, ii| ii - i)
}

fn derive_lead_avr(lead_i: &[f64], lead_ii: &[f64]) -> Vec<f64> {
    zip_map(lead_i, lead_ii, |i, ii| -0.5 * (i + ii))
}

fn derive_lead_avl(lead_i: &[f64], lead_ii: &[f64]) -> Vec<f64> {
    zip_map(lead_i, lead_ii, |i, ii| i - (0.5 * ii))
}

fn derive_lead_avf(lead_i: &[f64], lead_ii: &[f64]) -> Vec<f64> {
    zip_map(lead_i, lead_ii, |i, ii| ii - (0.5 * i))
}

fn zip_map<F>(lead_i: &[f64], lead_ii: &[f64], mut map: F) -> Vec<f64>
where
    F: FnMut(f64, f64) -> f64,
{
    lead_i
        .iter()
        .copied()
        .zip(lead_ii.iter().copied())
        .map(|(i, ii)| map(i, ii))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_footer_payload() {
        let bytes = vec![0; HEADER_BYTES + (FRAME_BYTES * 2) + FOOTER_BYTES];
        assert_eq!(resolve_payload_bytes(&bytes), Some(FRAME_BYTES * 2));
    }

    #[test]
    fn formats_timestamp() {
        assert_eq!(
            format_contec_timestamp("2026-05-08 13:44:10"),
            "08/05/2026 13:44:10"
        );
    }

    #[test]
    fn derives_frontal_leads_from_stored_i_and_ii_channels() {
        let path = std::env::temp_dir().join(format!(
            "ecg-studio-contec-limb-order-{}.ecg",
            std::process::id()
        ));
        let mut bytes = vec![0; HEADER_BYTES + (FRAME_BYTES * 2) + FOOTER_BYTES];
        for sample_index in 0..2 {
            for lead_index in 0..STORED_LEAD_COUNT {
                write_contec_sample(&mut bytes, sample_index, lead_index, 2048);
            }
        }
        write_contec_sample(&mut bytes, 0, 0, 2050);
        write_contec_sample(&mut bytes, 0, 1, 2055);
        write_contec_sample(&mut bytes, 1, 0, 2046);
        write_contec_sample(&mut bytes, 1, 1, 2052);

        std::fs::write(&path, bytes).unwrap();
        let document = read(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(
            document.find_lead("I").unwrap().samples_microvolts,
            vec![10.0, -10.0]
        );
        assert_eq!(
            document.find_lead("II").unwrap().samples_microvolts,
            vec![35.0, 20.0]
        );
        assert_eq!(
            document.find_lead("III").unwrap().samples_microvolts,
            vec![25.0, 30.0]
        );
        assert_eq!(
            document.find_lead("aVR").unwrap().samples_microvolts,
            vec![-22.5, -5.0]
        );
        assert_eq!(
            document.find_lead("aVL").unwrap().samples_microvolts,
            vec![-7.5, -20.0]
        );
        assert_eq!(
            document.find_lead("aVF").unwrap().samples_microvolts,
            vec![30.0, 25.0]
        );
    }

    fn write_contec_sample(bytes: &mut [u8], sample_index: usize, lead_index: usize, raw: u16) {
        let offset =
            HEADER_BYTES + (sample_index * FRAME_BYTES) + (lead_index * std::mem::size_of::<u16>());
        bytes[offset..offset + std::mem::size_of::<u16>()].copy_from_slice(&raw.to_le_bytes());
    }
}
