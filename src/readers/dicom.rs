use std::path::Path;

use crate::domain::{DocumentKind, EcgDocument, LeadData};

use super::{ReadError, ReadResult, read_bytes};

const PART_10_PREFIX_BYTES: usize = 132;
const EXPLICIT_VR_LITTLE_ENDIAN_UID: &str = "1.2.840.10008.1.2.1";

const TAG_TRANSFER_SYNTAX_UID: Tag = Tag(0x0002, 0x0010);
const TAG_STUDY_DATE: Tag = Tag(0x0008, 0x0020);
const TAG_STUDY_TIME: Tag = Tag(0x0008, 0x0030);
const TAG_ACQUISITION_DATE_TIME: Tag = Tag(0x0008, 0x002A);
const TAG_REFERRING_PHYSICIAN_NAME: Tag = Tag(0x0008, 0x0090);
const TAG_PATIENT_NAME: Tag = Tag(0x0010, 0x0010);
const TAG_PATIENT_BIRTH_DATE: Tag = Tag(0x0010, 0x0030);
const TAG_WAVEFORM_SEQUENCE: Tag = Tag(0x5400, 0x0100);

const TAG_NUMBER_OF_WAVEFORM_CHANNELS: Tag = Tag(0x003A, 0x0005);
const TAG_NUMBER_OF_WAVEFORM_SAMPLES: Tag = Tag(0x003A, 0x0010);
const TAG_SAMPLING_FREQUENCY: Tag = Tag(0x003A, 0x001A);
const TAG_CHANNEL_DEFINITION_SEQUENCE: Tag = Tag(0x003A, 0x0200);
const TAG_WAVEFORM_BITS_ALLOCATED: Tag = Tag(0x5400, 0x1004);
const TAG_WAVEFORM_SAMPLE_INTERPRETATION: Tag = Tag(0x5400, 0x1006);
const TAG_WAVEFORM_DATA: Tag = Tag(0x5400, 0x1010);

const TAG_CHANNEL_LABEL: Tag = Tag(0x003A, 0x0203);
const TAG_CHANNEL_SOURCE_SEQUENCE: Tag = Tag(0x003A, 0x0208);
const TAG_CHANNEL_SENSITIVITY: Tag = Tag(0x003A, 0x0210);
const TAG_CHANNEL_SENSITIVITY_UNITS_SEQUENCE: Tag = Tag(0x003A, 0x0211);
const TAG_CHANNEL_SENSITIVITY_CORRECTION_FACTOR: Tag = Tag(0x003A, 0x0212);
const TAG_CHANNEL_BASELINE: Tag = Tag(0x003A, 0x0213);

const TAG_CODE_VALUE: Tag = Tag(0x0008, 0x0100);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Tag(u16, u16);

#[derive(Clone, Copy)]
struct Element<'a> {
    tag: Tag,
    value: &'a [u8],
}

#[derive(Clone)]
struct ChannelDefinition {
    label: String,
    sensitivity: f64,
    correction_factor: f64,
    baseline: f64,
    units_to_microvolts: f64,
}

impl ChannelDefinition {
    fn fallback(index: usize) -> Self {
        Self {
            label: format!("Canal {}", index + 1),
            sensitivity: 1.0,
            correction_factor: 1.0,
            baseline: 0.0,
            units_to_microvolts: 1.0,
        }
    }
}

#[derive(Clone, Copy)]
enum SampleEncoding {
    Signed8,
    Unsigned8,
    Signed16,
    Unsigned16,
}

impl SampleEncoding {
    fn bytes_per_sample(self) -> usize {
        match self {
            Self::Signed8 | Self::Unsigned8 => 1,
            Self::Signed16 | Self::Unsigned16 => 2,
        }
    }
}

pub fn read(path: &Path) -> ReadResult<EcgDocument> {
    let bytes = read_bytes(path)?;
    let dataset_offset = dataset_offset(&bytes)?;
    let dataset = &bytes[dataset_offset..];

    let mut document = EcgDocument::new(DocumentKind::DicomEcg, path.to_path_buf());
    let mut acquisition_date_time = String::new();
    let mut study_date = String::new();
    let mut study_time = String::new();
    let mut waveform_sequence = None;

    let mut offset = 0;
    while let Some(element) = next_element(dataset, &mut offset)? {
        match element.tag {
            TAG_STUDY_DATE => study_date = text_value(element.value),
            TAG_STUDY_TIME => study_time = text_value(element.value),
            TAG_ACQUISITION_DATE_TIME => acquisition_date_time = text_value(element.value),
            TAG_REFERRING_PHYSICIAN_NAME => {
                document.physician_name = person_name(element.value);
            }
            TAG_PATIENT_NAME => document.patient_name = person_name(element.value),
            TAG_PATIENT_BIRTH_DATE => {
                document.patient_birth_date = format_dicom_date(&text_value(element.value));
            }
            TAG_WAVEFORM_SEQUENCE => waveform_sequence = Some(element.value),
            _ => {}
        }
    }

    document.exam_date = format_exam_date(&acquisition_date_time, &study_date, &study_time);

    let waveform_sequence = waveform_sequence
        .ok_or_else(|| ReadError::new("O arquivo DICOM-ECG não contém Waveform Sequence."))?;
    let waveform_item = first_sequence_item(waveform_sequence)?
        .ok_or_else(|| ReadError::new("A Waveform Sequence do DICOM-ECG está vazia."))?;
    fill_waveform(waveform_item, &mut document)?;

    Ok(document)
}

fn dataset_offset(bytes: &[u8]) -> ReadResult<usize> {
    if bytes.len() < PART_10_PREFIX_BYTES || &bytes[128..132] != b"DICM" {
        return Err(ReadError::new(
            "O arquivo DICOM-ECG não possui preâmbulo Part 10 com marcador DICM.",
        ));
    }

    let mut offset = PART_10_PREFIX_BYTES;
    let mut transfer_syntax_uid = None;
    while offset < bytes.len() {
        let element_offset = offset;
        let Some(element) = next_element(bytes, &mut offset)? else {
            break;
        };
        if element.tag.0 != 0x0002 {
            offset = element_offset;
            break;
        }
        if element.tag == TAG_TRANSFER_SYNTAX_UID {
            transfer_syntax_uid = Some(text_value(element.value));
        }
    }

    match transfer_syntax_uid.as_deref() {
        Some(EXPLICIT_VR_LITTLE_ENDIAN_UID) => Ok(offset),
        Some(uid) => Err(ReadError::new(format!(
            "Transfer Syntax DICOM não suportada para leitura de ECG: {uid}."
        ))),
        None => Err(ReadError::new(
            "O arquivo DICOM-ECG não informa Transfer Syntax UID.",
        )),
    }
}

fn fill_waveform(item: &[u8], document: &mut EcgDocument) -> ReadResult<()> {
    let mut channel_count = None;
    let mut sample_count = None;
    let mut sampling_frequency_hz = None;
    let mut channel_sequence = None;
    let mut bits_allocated = None;
    let mut sample_interpretation = None;
    let mut waveform_data = None;

    let mut offset = 0;
    while let Some(element) = next_element(item, &mut offset)? {
        match element.tag {
            TAG_NUMBER_OF_WAVEFORM_CHANNELS => {
                channel_count = read_u16_value(element.value).map(usize::from);
            }
            TAG_NUMBER_OF_WAVEFORM_SAMPLES => {
                sample_count = read_u32_value(element.value).map(|value| value as usize);
            }
            TAG_SAMPLING_FREQUENCY => {
                sampling_frequency_hz = parse_decimal(element.value);
            }
            TAG_CHANNEL_DEFINITION_SEQUENCE => channel_sequence = Some(element.value),
            TAG_WAVEFORM_BITS_ALLOCATED => bits_allocated = read_u16_value(element.value),
            TAG_WAVEFORM_SAMPLE_INTERPRETATION => {
                sample_interpretation = Some(text_value(element.value));
            }
            TAG_WAVEFORM_DATA => waveform_data = Some(element.value),
            _ => {}
        }
    }

    let channel_count = channel_count
        .ok_or_else(|| ReadError::new("O DICOM-ECG não informa o número de canais da waveform."))?;
    let sample_count = sample_count.ok_or_else(|| {
        ReadError::new("O DICOM-ECG não informa o número de amostras da waveform.")
    })?;
    if channel_count == 0 || sample_count == 0 {
        return Err(ReadError::new(
            "A waveform DICOM-ECG precisa conter canais e amostras.",
        ));
    }

    let sampling_frequency_hz = sampling_frequency_hz
        .ok_or_else(|| ReadError::new("O DICOM-ECG não informa a frequência de amostragem."))?;
    if sampling_frequency_hz <= 0.0 {
        return Err(ReadError::new(
            "A frequência de amostragem do DICOM-ECG precisa ser positiva.",
        ));
    }

    let channel_sequence = channel_sequence
        .ok_or_else(|| ReadError::new("O DICOM-ECG não contém Channel Definition Sequence."))?;
    let channels = channel_definitions(channel_sequence, channel_count)?;
    let bits_allocated = bits_allocated
        .ok_or_else(|| ReadError::new("O DICOM-ECG não informa Waveform Bits Allocated."))?;
    let sample_interpretation = sample_interpretation
        .ok_or_else(|| ReadError::new("O DICOM-ECG não informa Waveform Sample Interpretation."))?;
    let encoding = sample_encoding(bits_allocated, &sample_interpretation)?;
    let waveform_data =
        waveform_data.ok_or_else(|| ReadError::new("O DICOM-ECG não contém Waveform Data."))?;

    let expected_bytes = channel_count
        .checked_mul(sample_count)
        .and_then(|count| count.checked_mul(encoding.bytes_per_sample()))
        .ok_or_else(|| ReadError::new("A waveform DICOM-ECG excede o tamanho suportado."))?;
    if waveform_data.len() < expected_bytes {
        return Err(ReadError::new(
            "Waveform Data menor do que o número de canais e amostras declarados.",
        ));
    }

    let mut samples_by_channel = (0..channel_count)
        .map(|_| Vec::with_capacity(sample_count))
        .collect::<Vec<_>>();
    for sample_index in 0..sample_count {
        for channel_index in 0..channel_count {
            let data_index =
                (sample_index * channel_count + channel_index) * encoding.bytes_per_sample();
            let raw_value = read_sample(&waveform_data[data_index..], encoding);
            let channel = &channels[channel_index];
            samples_by_channel[channel_index].push(
                (channel.baseline + raw_value * channel.sensitivity * channel.correction_factor)
                    * channel.units_to_microvolts,
            );
        }
    }

    document.sample_interval_seconds = 1.0 / sampling_frequency_hz;
    document.leads.reserve(channel_count);
    for (channel, samples) in channels.into_iter().zip(samples_by_channel) {
        document.leads.push(LeadData::new(channel.label, samples));
    }

    Ok(())
}

fn channel_definitions(value: &[u8], expected_count: usize) -> ReadResult<Vec<ChannelDefinition>> {
    let items = sequence_items(value)?;
    if items.len() < expected_count {
        return Err(ReadError::new(
            "Channel Definition Sequence possui menos canais do que o declarado.",
        ));
    }

    let mut channels = Vec::with_capacity(expected_count);
    for (index, item) in items.into_iter().take(expected_count).enumerate() {
        channels.push(channel_definition(item, index)?);
    }
    Ok(channels)
}

fn channel_definition(item: &[u8], index: usize) -> ReadResult<ChannelDefinition> {
    let mut channel = ChannelDefinition::fallback(index);
    let mut source_code = None;

    let mut offset = 0;
    while let Some(element) = next_element(item, &mut offset)? {
        match element.tag {
            TAG_CHANNEL_LABEL => {
                let label = text_value(element.value);
                if !label.is_empty() {
                    channel.label = label;
                }
            }
            TAG_CHANNEL_SOURCE_SEQUENCE => source_code = first_code_value(element.value)?,
            TAG_CHANNEL_SENSITIVITY => {
                channel.sensitivity = parse_decimal(element.value).unwrap_or(channel.sensitivity);
            }
            TAG_CHANNEL_SENSITIVITY_UNITS_SEQUENCE => {
                channel.units_to_microvolts =
                    units_to_microvolts(first_code_value(element.value)?.as_deref());
            }
            TAG_CHANNEL_SENSITIVITY_CORRECTION_FACTOR => {
                channel.correction_factor =
                    parse_decimal(element.value).unwrap_or(channel.correction_factor);
            }
            TAG_CHANNEL_BASELINE => {
                channel.baseline = parse_decimal(element.value).unwrap_or(channel.baseline);
            }
            _ => {}
        }
    }

    if channel.label.starts_with("Canal ")
        && let Some(label) = source_code.as_deref().and_then(friendly_lead_name)
    {
        channel.label = label.to_owned();
    }

    Ok(channel)
}

fn first_code_value(value: &[u8]) -> ReadResult<Option<String>> {
    let Some(item) = first_sequence_item(value)? else {
        return Ok(None);
    };
    let mut offset = 0;
    while let Some(element) = next_element(item, &mut offset)? {
        if element.tag == TAG_CODE_VALUE {
            return Ok(Some(text_value(element.value)));
        }
    }
    Ok(None)
}

fn first_sequence_item(value: &[u8]) -> ReadResult<Option<&[u8]>> {
    Ok(sequence_items(value)?.into_iter().next())
}

fn sequence_items(value: &[u8]) -> ReadResult<Vec<&[u8]>> {
    let mut items = Vec::new();
    let mut offset = 0;
    while offset < value.len() {
        if offset + 8 > value.len() {
            return Err(ReadError::new(
                "Sequência DICOM-ECG truncada ao ler um item.",
            ));
        }
        let tag = Tag(
            u16::from_le_bytes([value[offset], value[offset + 1]]),
            u16::from_le_bytes([value[offset + 2], value[offset + 3]]),
        );
        if tag != Tag(0xFFFE, 0xE000) {
            return Err(ReadError::new(
                "Sequência DICOM-ECG contém item inesperado.",
            ));
        }
        let length = u32::from_le_bytes([
            value[offset + 4],
            value[offset + 5],
            value[offset + 6],
            value[offset + 7],
        ]);
        if length == u32::MAX {
            return Err(ReadError::new(
                "Sequências DICOM-ECG com tamanho indefinido ainda não são suportadas.",
            ));
        }
        let item_start = offset + 8;
        let item_end = item_start
            .checked_add(length as usize)
            .ok_or_else(|| ReadError::new("Item DICOM-ECG excede o tamanho suportado."))?;
        if item_end > value.len() {
            return Err(ReadError::new(
                "Sequência DICOM-ECG truncada dentro de um item.",
            ));
        }
        items.push(&value[item_start..item_end]);
        offset = item_end;
    }
    Ok(items)
}

fn next_element<'a>(bytes: &'a [u8], offset: &mut usize) -> ReadResult<Option<Element<'a>>> {
    if *offset == bytes.len() {
        return Ok(None);
    }
    if *offset + 8 > bytes.len() {
        return Err(ReadError::new("Elemento DICOM-ECG truncado no cabeçalho."));
    }

    let tag = Tag(
        u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]),
        u16::from_le_bytes([bytes[*offset + 2], bytes[*offset + 3]]),
    );
    let vr = std::str::from_utf8(&bytes[*offset + 4..*offset + 6])
        .map_err(|_| ReadError::new("VR DICOM inválido ao ler o arquivo ECG."))?;
    let (value_start, value_len) = if uses_32_bit_length(vr) {
        if *offset + 12 > bytes.len() {
            return Err(ReadError::new(
                "Elemento DICOM-ECG truncado ao ler tamanho de 32 bits.",
            ));
        }
        let value_len = u32::from_le_bytes([
            bytes[*offset + 8],
            bytes[*offset + 9],
            bytes[*offset + 10],
            bytes[*offset + 11],
        ]);
        if value_len == u32::MAX {
            return Err(ReadError::new(
                "Elementos DICOM-ECG com tamanho indefinido ainda não são suportados.",
            ));
        }
        (*offset + 12, value_len as usize)
    } else {
        let value_len = u16::from_le_bytes([bytes[*offset + 6], bytes[*offset + 7]]);
        (*offset + 8, usize::from(value_len))
    };

    let value_end = value_start
        .checked_add(value_len)
        .ok_or_else(|| ReadError::new("Elemento DICOM-ECG excede o tamanho suportado."))?;
    if value_end > bytes.len() {
        return Err(ReadError::new(
            "Elemento DICOM-ECG truncado no valor declarado.",
        ));
    }

    *offset = value_end;
    Ok(Some(Element {
        tag,
        value: &bytes[value_start..value_end],
    }))
}

fn uses_32_bit_length(vr: &str) -> bool {
    matches!(
        vr,
        "OB" | "OD" | "OF" | "OL" | "OW" | "SQ" | "UC" | "UN" | "UR" | "UT"
    )
}

fn sample_encoding(bits_allocated: u16, sample_interpretation: &str) -> ReadResult<SampleEncoding> {
    match (bits_allocated, sample_interpretation.trim()) {
        (8, "SB") => Ok(SampleEncoding::Signed8),
        (8, "UB") => Ok(SampleEncoding::Unsigned8),
        (16, "SS") => Ok(SampleEncoding::Signed16),
        (16, "US") => Ok(SampleEncoding::Unsigned16),
        _ => Err(ReadError::new(format!(
            "Combinação DICOM de bits/amostra não suportada: {bits_allocated} bits {sample_interpretation}."
        ))),
    }
}

fn read_sample(bytes: &[u8], encoding: SampleEncoding) -> f64 {
    match encoding {
        SampleEncoding::Signed8 => bytes[0] as i8 as f64,
        SampleEncoding::Unsigned8 => bytes[0] as f64,
        SampleEncoding::Signed16 => i16::from_le_bytes([bytes[0], bytes[1]]) as f64,
        SampleEncoding::Unsigned16 => u16::from_le_bytes([bytes[0], bytes[1]]) as f64,
    }
}

fn read_u16_value(value: &[u8]) -> Option<u16> {
    (value.len() >= 2).then(|| u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32_value(value: &[u8]) -> Option<u32> {
    (value.len() >= 4).then(|| u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn parse_decimal(value: &[u8]) -> Option<f64> {
    text_value(value).parse().ok()
}

fn text_value(value: &[u8]) -> String {
    decode_single_byte_text(value)
        .trim_matches(['\0', ' '])
        .trim()
        .to_owned()
}

fn decode_single_byte_text(value: &[u8]) -> String {
    std::str::from_utf8(value)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|_| value.iter().map(|byte| char::from(*byte)).collect())
}

fn person_name(value: &[u8]) -> String {
    text_value(value).replace('^', " ").trim().to_owned()
}

fn format_exam_date(acquisition: &str, study_date: &str, study_time: &str) -> String {
    if !acquisition.trim().is_empty() {
        return format_dicom_timestamp(acquisition);
    }
    if study_date.trim().is_empty() {
        return String::new();
    }
    let joined = format!("{study_date}{study_time}");
    format_dicom_timestamp(&joined)
}

fn format_dicom_date(raw: &str) -> String {
    format_dicom_timestamp(raw)
        .split_once(' ')
        .map(|(date, _)| date.to_owned())
        .unwrap_or_else(|| format_dicom_timestamp(raw))
}

fn format_dicom_timestamp(raw: &str) -> String {
    let digits: String = raw.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if digits.len() < 8 {
        return raw.trim().to_owned();
    }

    let mut output = format!("{}/{}/{}", &digits[6..8], &digits[4..6], &digits[0..4]);
    if digits.len() >= 14 {
        output.push(' ');
        output.push_str(&format!(
            "{}:{}:{}",
            &digits[8..10],
            &digits[10..12],
            &digits[12..14]
        ));
    }
    output
}

fn units_to_microvolts(code: Option<&str>) -> f64 {
    match code {
        Some("V") => 1_000_000.0,
        Some("mV") => 1_000.0,
        _ => 1.0,
    }
}

fn friendly_lead_name(code: &str) -> Option<&'static str> {
    match code {
        "MDC_ECG_LEAD_I" | "1" => Some("I"),
        "MDC_ECG_LEAD_II" | "2" => Some("II"),
        "MDC_ECG_LEAD_III" | "61" => Some("III"),
        "MDC_ECG_LEAD_AVR" | "62" => Some("aVR"),
        "MDC_ECG_LEAD_AVL" | "63" => Some("aVL"),
        "MDC_ECG_LEAD_AVF" | "64" => Some("aVF"),
        "MDC_ECG_LEAD_V1" | "3" => Some("V1"),
        "MDC_ECG_LEAD_V2" | "4" => Some("V2"),
        "MDC_ECG_LEAD_V3" | "5" => Some("V3"),
        "MDC_ECG_LEAD_V4" | "6" => Some("V4"),
        "MDC_ECG_LEAD_V5" | "7" => Some("V5"),
        "MDC_ECG_LEAD_V6" | "8" => Some("V6"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DocumentKind, EcgDocument, LeadData};
    use crate::exporting::build_dicom_ecg;

    #[test]
    fn reads_dicom_waveform_exported_by_the_app() {
        let path = std::env::temp_dir().join(format!(
            "ecg-studio-dicom-roundtrip-{}.dcm",
            std::process::id()
        ));
        let mut exported = EcgDocument::new(DocumentKind::Xml, Default::default());
        exported.patient_name = "Teste Paciente".to_owned();
        exported.patient_birth_date = "11/05/2026".to_owned();
        exported.exam_date = "12/05/2026 09:07:33".to_owned();
        exported.physician_name = "Dra Teste".to_owned();
        exported.sample_interval_seconds = 0.002;
        exported
            .leads
            .push(LeadData::new("II", vec![10.0, -2.0, 14.0]));
        exported
            .leads
            .push(LeadData::new("V1", vec![4.0, 5.0, 6.0]));

        std::fs::write(&path, build_dicom_ecg(&exported).unwrap()).unwrap();
        let document = read(&path).unwrap();
        let _ = std::fs::remove_file(path);

        assert_eq!(document.kind, DocumentKind::DicomEcg);
        assert_eq!(document.patient_name, "Teste Paciente");
        assert_eq!(document.patient_birth_date, "11/05/2026");
        assert_eq!(document.exam_date, "12/05/2026 09:07:33");
        assert_eq!(document.physician_name, "Dra Teste");
        assert_eq!(document.sample_interval_seconds, 0.002);
        assert_eq!(
            document.find_lead("II").unwrap().samples_microvolts,
            vec![10.0, -2.0, 14.0]
        );
        assert_eq!(
            document.find_lead("V1").unwrap().samples_microvolts,
            vec![4.0, 5.0, 6.0]
        );
    }

    #[test]
    fn formats_dicom_timestamp_for_ui() {
        assert_eq!(
            format_dicom_timestamp("20260512090733"),
            "12/05/2026 09:07:33"
        );
    }

    #[test]
    fn decodes_latin1_person_names_without_replacement_characters() {
        assert_eq!(person_name(b"Jo\xe3o^Gon\xe7alves"), "João Gonçalves");
    }
}
