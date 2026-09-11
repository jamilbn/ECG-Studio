use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::EcgDocument;

const DICOM_TRANSFER_SYNTAX_EXPLICIT_VR_LE: &str = "1.2.840.10008.1.2.1";
const DICOM_TWELVE_LEAD_ECG_STORAGE: &str = "1.2.840.10008.5.1.4.1.1.9.1.1";
const DICOM_GENERAL_ECG_STORAGE: &str = "1.2.840.10008.5.1.4.1.1.9.1.2";
const DICOM_IMPLEMENTATION_CLASS_UID: &str = "1.2.826.0.1.3680043.10.5432.1";
const STANDARD_TWELVE_LEADS: [&str; 12] = [
    "I", "II", "III", "aVR", "aVL", "aVF", "V1", "V2", "V3", "V4", "V5", "V6",
];

pub fn build_hl7_aecg(document: &EcgDocument) -> String {
    let timestamp = hl7_timestamp(&document.exam_date);
    let interval = format_decimal(document.sample_interval_seconds);
    let mut output = String::new();

    output.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    output.push_str("<AnnotatedECG xmlns=\"urn:hl7-org:v3\">\n");
    output.push_str("  <id root=\"");
    output.push_str(&generated_uid(1));
    output.push_str("\"/>\n");
    output.push_str("  <code code=\"93000\" codeSystem=\"2.16.840.1.113883.6.12\" displayName=\"Electrocardiogram\"/>\n");
    if !timestamp.is_empty() {
        output.push_str("  <effectiveTime><low value=\"");
        output.push_str(&timestamp);
        output.push_str("\"/></effectiveTime>\n");
    }
    output.push_str("  <component>\n");
    output.push_str("    <series>\n");
    output.push_str("      <subjectOf><subjectDemographicPerson><name>");
    output.push_str(&escape_xml(value_or_unknown(&document.patient_name)));
    output.push_str("</name>");
    if let Some(birth_date) = document.patient_birth_date_yyyymmdd() {
        output.push_str("<birthTime value=\"");
        output.push_str(&birth_date);
        output.push_str("\"/>");
    }
    output.push_str("</subjectDemographicPerson></subjectOf>\n");
    output.push_str("      <author><assignedEntity>");
    if !document.physician_name.trim().is_empty() {
        output.push_str("<assignedPerson><name>");
        output.push_str(&escape_xml(document.physician_name.trim()));
        output.push_str("</name></assignedPerson>");
    }
    output.push_str("<representedOrganization><name>");
    output.push_str(&escape_xml(value_or_unknown(&document.clinic_name)));
    output.push_str("</name></representedOrganization></assignedEntity></author>\n");
    output.push_str("      <component>\n");
    output.push_str("        <sequence>\n");
    output.push_str("          <code code=\"TIME_ABSOLUTE\"/>\n");
    output.push_str("          <value>\n");
    output.push_str("            <origin value=\"0\" unit=\"s\"/>\n");
    output.push_str("            <increment value=\"");
    output.push_str(&interval);
    output.push_str("\" unit=\"s\"/>\n");
    output.push_str("          </value>\n");
    output.push_str("        </sequence>\n");
    output.push_str("      </component>\n");

    for lead in &document.leads {
        output.push_str("      <component>\n");
        output.push_str("        <sequence>\n");
        output.push_str("          <code code=\"");
        output.push_str(lead_code(&lead.name));
        output.push_str("\" displayName=\"");
        output.push_str(&escape_xml(&lead.name));
        output.push_str("\"/>\n");
        output.push_str("          <value>\n");
        output.push_str("            <origin value=\"0\" unit=\"uV\"/>\n");
        output.push_str("            <scale value=\"1\" unit=\"uV\"/>\n");
        output.push_str("            <digits>");
        write_digits(&mut output, &lead.samples_microvolts);
        output.push_str("</digits>\n");
        output.push_str("          </value>\n");
        output.push_str("        </sequence>\n");
        output.push_str("      </component>\n");
    }

    output.push_str("    </series>\n");
    output.push_str("  </component>\n");
    output.push_str("</AnnotatedECG>\n");
    output
}

pub fn build_dicom_ecg(document: &EcgDocument) -> Result<Vec<u8>, String> {
    let lead_count = document.leads.len();
    if lead_count == 0 {
        return Err("Não há derivações para exportar em DICOM-ECG.".to_owned());
    }
    let sample_count = document
        .leads
        .iter()
        .map(|lead| lead.samples_microvolts.len())
        .min()
        .unwrap_or(0);
    if sample_count == 0 {
        return Err("Não há amostras para exportar em DICOM-ECG.".to_owned());
    }

    let sop_class_uid = if has_standard_twelve_lead_set(document) {
        DICOM_TWELVE_LEAD_ECG_STORAGE
    } else {
        DICOM_GENERAL_ECG_STORAGE
    };
    let sop_instance_uid = generated_uid(2);
    let study_uid = generated_uid(3);
    let series_uid = generated_uid(4);
    let (study_date, study_time) = dicom_date_time(&document.exam_date);

    let mut file = vec![0; 128];
    file.extend_from_slice(b"DICM");

    let mut meta = Vec::new();
    write_ob(&mut meta, (0x0002, 0x0001), &[0, 1]);
    write_ui(&mut meta, (0x0002, 0x0002), sop_class_uid);
    write_ui(&mut meta, (0x0002, 0x0003), &sop_instance_uid);
    write_ui(
        &mut meta,
        (0x0002, 0x0010),
        DICOM_TRANSFER_SYNTAX_EXPLICIT_VR_LE,
    );
    write_ui(&mut meta, (0x0002, 0x0012), DICOM_IMPLEMENTATION_CLASS_UID);
    write_tag_header(&mut file, (0x0002, 0x0000), "UL", 4);
    file.extend_from_slice(&(meta.len() as u32).to_le_bytes());
    file.extend_from_slice(&meta);

    write_cs(&mut file, (0x0008, 0x0005), "ISO_IR 100");
    write_ui(&mut file, (0x0008, 0x0016), sop_class_uid);
    write_ui(&mut file, (0x0008, 0x0018), &sop_instance_uid);
    if !study_date.is_empty() {
        write_da(&mut file, (0x0008, 0x0020), &study_date);
    }
    if !study_time.is_empty() {
        write_tm(&mut file, (0x0008, 0x0030), &study_time);
    }
    if !study_date.is_empty() {
        write_dt(
            &mut file,
            (0x0008, 0x002A),
            &format!("{study_date}{study_time}"),
        );
    }
    write_cs(&mut file, (0x0008, 0x0060), "ECG");
    write_lo(&mut file, (0x0008, 0x0070), "ECG Studio");
    write_lo(&mut file, (0x0008, 0x1030), "Electrocardiogram");
    write_lo(&mut file, (0x0008, 0x1090), document.kind.label());
    if !document.physician_name.trim().is_empty() {
        write_pn(
            &mut file,
            (0x0008, 0x0090),
            &patient_name_for_dicom(&document.physician_name),
        );
    }
    write_pn(
        &mut file,
        (0x0010, 0x0010),
        &patient_name_for_dicom(&document.patient_name),
    );
    if let Some(birth_date) = document.patient_birth_date_yyyymmdd() {
        write_da(&mut file, (0x0010, 0x0030), &birth_date);
    }
    write_lo(&mut file, (0x0010, 0x0020), "ECG-STUDIO");
    write_ui(&mut file, (0x0020, 0x000D), &study_uid);
    write_ui(&mut file, (0x0020, 0x000E), &series_uid);
    write_is(&mut file, (0x0020, 0x0011), "1");
    write_is(&mut file, (0x0020, 0x0013), "1");

    let mut waveform_item = Vec::new();
    write_cs(&mut waveform_item, (0x003A, 0x0004), "ORIGINAL");
    write_us(&mut waveform_item, (0x003A, 0x0005), lead_count as u16);
    write_ul(&mut waveform_item, (0x003A, 0x0010), sample_count as u32);
    write_ds(
        &mut waveform_item,
        (0x003A, 0x001A),
        &format_decimal(document.sample_rate_hz()),
    );
    write_sh(&mut waveform_item, (0x003A, 0x0020), "ECG");
    write_sequence(
        &mut waveform_item,
        (0x003A, 0x0200),
        &channel_definition_items(document),
    );
    write_us(&mut waveform_item, (0x5400, 0x1002), 16);
    write_us(&mut waveform_item, (0x5400, 0x1004), 16);
    write_cs(&mut waveform_item, (0x5400, 0x1006), "SS");
    write_ow(
        &mut waveform_item,
        (0x5400, 0x1010),
        &waveform_data(document, sample_count),
    );
    write_sequence(&mut file, (0x5400, 0x0100), &[waveform_item]);

    Ok(file)
}

fn channel_definition_items(document: &EcgDocument) -> Vec<Vec<u8>> {
    document
        .leads
        .iter()
        .enumerate()
        .map(|(index, lead)| {
            let mut item = Vec::new();
            write_is(&mut item, (0x003A, 0x0202), &(index + 1).to_string());
            write_sh(&mut item, (0x003A, 0x0203), &lead.name);
            write_sequence(
                &mut item,
                (0x003A, 0x0208),
                &[code_sequence_item(lead_code(&lead.name), "MDC", &lead.name)],
            );
            write_ds(&mut item, (0x003A, 0x0210), "1");
            write_sequence(
                &mut item,
                (0x003A, 0x0211),
                &[code_sequence_item("uV", "UCUM", "microvolt")],
            );
            write_ds(&mut item, (0x003A, 0x0213), "0");
            write_ds(&mut item, (0x003A, 0x0214), "0");
            item
        })
        .collect()
}

fn code_sequence_item(code_value: &str, coding_scheme: &str, meaning: &str) -> Vec<u8> {
    let mut item = Vec::new();
    write_sh(&mut item, (0x0008, 0x0100), code_value);
    write_sh(&mut item, (0x0008, 0x0102), coding_scheme);
    write_lo(&mut item, (0x0008, 0x0104), meaning);
    item
}

fn waveform_data(document: &EcgDocument, sample_count: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(sample_count * document.leads.len() * 2);
    for sample_index in 0..sample_count {
        for lead in &document.leads {
            let value = lead.samples_microvolts[sample_index]
                .round()
                .clamp(i16::MIN as f64, i16::MAX as f64) as i16;
            data.extend_from_slice(&value.to_le_bytes());
        }
    }
    data
}

fn write_sequence(output: &mut Vec<u8>, tag: (u16, u16), items: &[Vec<u8>]) {
    let mut value = Vec::new();
    for item in items {
        value.extend_from_slice(&0xFFFE_u16.to_le_bytes());
        value.extend_from_slice(&0xE000_u16.to_le_bytes());
        value.extend_from_slice(&(item.len() as u32).to_le_bytes());
        value.extend_from_slice(item);
    }
    write_tag_header(output, tag, "SQ", value.len() as u32);
    output.extend_from_slice(&value);
}

fn write_ob(output: &mut Vec<u8>, tag: (u16, u16), value: &[u8]) {
    write_binary_element(output, tag, "OB", value);
}

fn write_ow(output: &mut Vec<u8>, tag: (u16, u16), value: &[u8]) {
    write_binary_element(output, tag, "OW", value);
}

fn write_binary_element(output: &mut Vec<u8>, tag: (u16, u16), vr: &str, value: &[u8]) {
    let mut padded = value.to_vec();
    if padded.len() % 2 == 1 {
        padded.push(0);
    }
    write_tag_header(output, tag, vr, padded.len() as u32);
    output.extend_from_slice(&padded);
}

fn write_ui(output: &mut Vec<u8>, tag: (u16, u16), value: &str) {
    let mut bytes = value.trim().as_bytes().to_vec();
    if bytes.len() % 2 == 1 {
        bytes.push(0);
    }
    write_tag_header(output, tag, "UI", bytes.len() as u32);
    output.extend_from_slice(&bytes);
}

fn write_cs(output: &mut Vec<u8>, tag: (u16, u16), value: &str) {
    write_text_element(output, tag, "CS", value);
}

fn write_da(output: &mut Vec<u8>, tag: (u16, u16), value: &str) {
    write_text_element(output, tag, "DA", value);
}

fn write_ds(output: &mut Vec<u8>, tag: (u16, u16), value: &str) {
    write_text_element(output, tag, "DS", value);
}

fn write_dt(output: &mut Vec<u8>, tag: (u16, u16), value: &str) {
    write_text_element(output, tag, "DT", value);
}

fn write_is(output: &mut Vec<u8>, tag: (u16, u16), value: &str) {
    write_text_element(output, tag, "IS", value);
}

fn write_lo(output: &mut Vec<u8>, tag: (u16, u16), value: &str) {
    write_text_element(output, tag, "LO", value);
}

fn write_pn(output: &mut Vec<u8>, tag: (u16, u16), value: &str) {
    write_text_element(output, tag, "PN", value);
}

fn write_sh(output: &mut Vec<u8>, tag: (u16, u16), value: &str) {
    write_text_element(output, tag, "SH", value);
}

fn write_tm(output: &mut Vec<u8>, tag: (u16, u16), value: &str) {
    write_text_element(output, tag, "TM", value);
}

fn write_text_element(output: &mut Vec<u8>, tag: (u16, u16), vr: &str, value: &str) {
    let mut bytes = sanitize_dicom_text(value).into_bytes();
    if bytes.len() % 2 == 1 {
        bytes.push(b' ');
    }
    write_tag_header(output, tag, vr, bytes.len() as u32);
    output.extend_from_slice(&bytes);
}

fn write_us(output: &mut Vec<u8>, tag: (u16, u16), value: u16) {
    write_tag_header(output, tag, "US", 2);
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_ul(output: &mut Vec<u8>, tag: (u16, u16), value: u32) {
    write_tag_header(output, tag, "UL", 4);
    output.extend_from_slice(&value.to_le_bytes());
}

fn write_tag_header(output: &mut Vec<u8>, tag: (u16, u16), vr: &str, length: u32) {
    output.extend_from_slice(&tag.0.to_le_bytes());
    output.extend_from_slice(&tag.1.to_le_bytes());
    output.extend_from_slice(vr.as_bytes());
    if uses_32_bit_length(vr) {
        output.extend_from_slice(&[0, 0]);
        output.extend_from_slice(&length.to_le_bytes());
    } else {
        output.extend_from_slice(&(length as u16).to_le_bytes());
    }
}

fn uses_32_bit_length(vr: &str) -> bool {
    matches!(
        vr,
        "OB" | "OD" | "OF" | "OL" | "OW" | "SQ" | "UC" | "UN" | "UR" | "UT"
    )
}

fn write_digits(output: &mut String, samples: &[f64]) {
    for (index, sample) in samples.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        output.push_str(&(sample.round() as i64).to_string());
    }
}

fn lead_code(name: &str) -> &'static str {
    match name {
        "I" => "MDC_ECG_LEAD_I",
        "II" => "MDC_ECG_LEAD_II",
        "III" => "MDC_ECG_LEAD_III",
        "aVR" => "MDC_ECG_LEAD_AVR",
        "aVL" => "MDC_ECG_LEAD_AVL",
        "aVF" => "MDC_ECG_LEAD_AVF",
        "V1" => "MDC_ECG_LEAD_V1",
        "V2" => "MDC_ECG_LEAD_V2",
        "V3" => "MDC_ECG_LEAD_V3",
        "V4" => "MDC_ECG_LEAD_V4",
        "V5" => "MDC_ECG_LEAD_V5",
        "V6" => "MDC_ECG_LEAD_V6",
        _ => "MDC_ECG_LEAD_UNKNOWN",
    }
}

fn has_standard_twelve_lead_set(document: &EcgDocument) -> bool {
    document.leads.len() == STANDARD_TWELVE_LEADS.len()
        && STANDARD_TWELVE_LEADS
            .iter()
            .all(|lead_name| document.find_lead(lead_name).is_some())
}

fn value_or_unknown(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "UNKNOWN"
    } else {
        trimmed
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn patient_name_for_dicom(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "UNKNOWN".to_owned();
    }
    trimmed.replace(' ', "^")
}

fn sanitize_dicom_text(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            '\0' | '\r' | '\n' | '\t' | '\\' => ' ',
            other if other.is_ascii() => other,
            _ => '?',
        })
        .collect::<String>()
        .trim()
        .to_owned()
}

fn hl7_timestamp(raw: &str) -> String {
    let (date, time) = dicom_date_time(raw);
    if date.is_empty() {
        String::new()
    } else {
        format!("{date}{time}")
    }
}

fn dicom_date_time(raw: &str) -> (String, String) {
    let digits: String = raw.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if digits.len() >= 14 && starts_with_year(&digits) {
        return (digits[0..8].to_owned(), digits[8..14].to_owned());
    }
    if digits.len() >= 14 {
        return (
            format!("{}{}{}", &digits[4..8], &digits[2..4], &digits[0..2]),
            digits[8..14].to_owned(),
        );
    }
    if digits.len() >= 12 && starts_with_year(&digits) {
        return (
            digits[0..8].to_owned(),
            format!("{}{}00", &digits[8..10], &digits[10..12]),
        );
    }
    if digits.len() >= 12 {
        return (
            format!("{}{}{}", &digits[4..8], &digits[2..4], &digits[0..2]),
            format!("{}{}00", &digits[8..10], &digits[10..12]),
        );
    }
    if digits.len() >= 8 && starts_with_year(&digits) {
        return (digits[0..8].to_owned(), String::new());
    }
    if digits.len() >= 8 {
        return (
            format!("{}{}{}", &digits[4..8], &digits[2..4], &digits[0..2]),
            String::new(),
        );
    }
    (String::new(), String::new())
}

fn starts_with_year(digits: &str) -> bool {
    digits
        .get(0..4)
        .and_then(|year| year.parse::<u16>().ok())
        .is_some_and(|year| (1900..=2200).contains(&year))
}

fn format_decimal(value: f64) -> String {
    let mut text = format!("{value:.6}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    text
}

fn generated_uid(suffix: u32) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("1.2.826.0.1.3680043.10.5432.{millis}.{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DocumentKind, EcgDocument, LeadData};

    #[test]
    fn aecg_contains_lead_digits() {
        let mut document = EcgDocument::new(DocumentKind::ContecLive, Default::default());
        document.patient_name = "Teste".to_owned();
        document.patient_birth_date = "11/05/2026".to_owned();
        document.leads.push(LeadData::new("II", vec![10.0, -2.0]));

        let xml = build_hl7_aecg(&document);

        assert!(xml.contains("MDC_ECG_LEAD_II"));
        assert!(xml.contains("<birthTime value=\"20260511\"/>"));
        assert!(xml.contains("<digits>10 -2</digits>"));
    }

    #[test]
    fn dicom_has_part_10_preamble() {
        let mut document = EcgDocument::new(DocumentKind::ContecLive, Default::default());
        document.leads.push(LeadData::new("II", vec![10.0, -2.0]));

        let bytes = build_dicom_ecg(&document).expect("dicom should be built");

        assert_eq!(&bytes[128..132], b"DICM");
        assert!(bytes.windows(2).any(|window| window == b"SS"));
        assert!(bytes.windows(4).any(|window| window == b"UCUM"));
    }

    #[test]
    fn parses_exam_date_without_seconds_for_exports() {
        assert_eq!(
            dicom_date_time("11/05/2026 09:07"),
            ("20260511".to_owned(), "090700".to_owned())
        );
    }
}
