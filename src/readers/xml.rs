use std::fs;
use std::path::Path;

use roxmltree::{Document, Node};

use crate::domain::{DocumentKind, EcgDocument, LeadData};

use super::{ReadError, ReadResult};

pub fn read(path: &Path) -> ReadResult<EcgDocument> {
    let text = read_xml_text(path)?;
    let xml = Document::parse(&text).map_err(|error| {
        ReadError::new(format!(
            "Erro ao carregar XML '{}': {error}",
            path.display()
        ))
    })?;

    let mut document = EcgDocument::new(DocumentKind::Xml, path.to_path_buf());
    fill_metadata(&xml, &mut document);
    fill_leads(&xml, &mut document);

    if document.leads.is_empty() {
        return Err(ReadError::new(
            "O XML foi carregado, mas nenhum canal de ECG foi encontrado.",
        ));
    }

    Ok(document)
}

fn read_xml_text(path: &Path) -> ReadResult<String> {
    let bytes = fs::read(path).map_err(|error| {
        ReadError::new(format!(
            "Não foi possível ler XML '{}': {error}",
            path.display()
        ))
    })?;

    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Ok(decode_utf16(&bytes[2..], Endian::Little));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Ok(decode_utf16(&bytes[2..], Endian::Big));
    }
    if bytes.len() >= 2 && bytes[1] == 0 {
        return Ok(decode_utf16(&bytes, Endian::Little));
    }
    if bytes.len() >= 2 && bytes[0] == 0 {
        return Ok(decode_utf16(&bytes, Endian::Big));
    }
    Ok(decode_single_byte_text(&bytes))
}

enum Endian {
    Little,
    Big,
}

fn decode_utf16(bytes: &[u8], endian: Endian) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|chunk| match endian {
            Endian::Little => u16::from_le_bytes([chunk[0], chunk[1]]),
            Endian::Big => u16::from_be_bytes([chunk[0], chunk[1]]),
        })
        .collect();
    String::from_utf16_lossy(&units)
}

fn decode_single_byte_text(bytes: &[u8]) -> String {
    std::str::from_utf8(bytes)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|_| bytes.iter().map(|byte| char::from(*byte)).collect())
}

fn fill_metadata(xml: &Document<'_>, document: &mut EcgDocument) {
    document.patient_name = first_person_name(xml, "subjectDemographicPerson")
        .or_else(|| first_person_name(xml, "patient"))
        .unwrap_or_default();
    document.patient_birth_date = first_birth_time(xml)
        .map(|date| format_hl7_timestamp(&date))
        .unwrap_or_default();

    if let Some(date) = first_effective_time(xml) {
        document.exam_date = format_hl7_timestamp(&date);
    }

    if let Some(increment) = first_increment(xml) {
        let value = attr(&increment, "value")
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(0.002);
        let unit = attr(&increment, "unit").unwrap_or_default();
        document.sample_interval_seconds = time_to_seconds(value, &unit);
    }
}

fn fill_leads(xml: &Document<'_>, document: &mut EcgDocument) {
    let sequence_nodes: Vec<_> = xml
        .descendants()
        .filter(|node| is_element_named(*node, "sequence"))
        .collect();
    document.leads.reserve(sequence_nodes.len());

    for sequence in sequence_nodes {
        if let Some(lead) = build_lead(sequence) {
            document.leads.push(lead);
        }
    }
}

fn build_lead(sequence: Node<'_, '_>) -> Option<LeadData> {
    let code = child_element(sequence, "code").and_then(|node| attr(&node, "code"))?;
    let name = friendly_lead_name(&code)?;
    let value_node = child_element(sequence, "value")?;
    let digits = child_text(value_node, "digits").unwrap_or_default();
    if digits.trim().is_empty() {
        return None;
    }

    let origin = child_element(value_node, "origin");
    let scale = child_element(value_node, "scale");
    let scale_value = scale
        .as_ref()
        .and_then(|node| attr(node, "value"))
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(1.0);
    let scale_unit = scale
        .as_ref()
        .and_then(|node| attr(node, "unit"))
        .unwrap_or_default();
    let origin_value = origin
        .as_ref()
        .and_then(|node| attr(node, "value"))
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    let origin_unit = origin
        .as_ref()
        .and_then(|node| attr(node, "unit"))
        .unwrap_or_else(|| scale_unit.clone());

    let scale_microvolts = amplitude_to_microvolts(scale_value, &scale_unit);
    let origin_microvolts = amplitude_to_microvolts(origin_value, &origin_unit);
    let samples = parse_number_list(&digits, scale_microvolts, origin_microvolts);
    if samples.is_empty() {
        return None;
    }
    Some(LeadData::new(name, samples))
}

fn first_person_name(xml: &Document<'_>, parent_name: &str) -> Option<String> {
    xml.descendants()
        .find(|node| is_element_named(*node, parent_name))
        .and_then(|node| child_text(node, "name"))
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

fn first_birth_time(xml: &Document<'_>) -> Option<String> {
    xml.descendants()
        .find(|node| is_element_named(*node, "birthTime"))
        .and_then(|node| attr(&node, "value"))
}

fn first_effective_time(xml: &Document<'_>) -> Option<String> {
    for node in xml
        .descendants()
        .filter(|node| is_element_named(*node, "effectiveTime"))
    {
        if let Some(value) = child_element(node, "center").and_then(|center| attr(&center, "value"))
        {
            return Some(value);
        }
        if let Some(value) = child_element(node, "low").and_then(|low| attr(&low, "value")) {
            return Some(value);
        }
    }
    None
}

fn first_increment<'a>(xml: &'a Document<'a>) -> Option<Node<'a, 'a>> {
    for sequence in xml
        .descendants()
        .filter(|node| is_element_named(*node, "sequence"))
    {
        let is_time_sequence = child_element(sequence, "code")
            .and_then(|code| attr(&code, "code"))
            .as_deref()
            == Some("TIME_ABSOLUTE");
        if is_time_sequence
            && let Some(increment) =
                child_element(sequence, "value").and_then(|value| child_element(value, "increment"))
        {
            return Some(increment);
        }
    }

    xml.descendants()
        .find(|node| is_element_named(*node, "increment"))
}

fn is_element_named(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name
}

fn child_element<'a>(node: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    node.children().find(|child| is_element_named(*child, name))
}

fn child_text(node: Node<'_, '_>, name: &str) -> Option<String> {
    child_element(node, name)
        .and_then(|child| child.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

fn attr(node: &Node<'_, '_>, name: &str) -> Option<String> {
    node.attribute(name)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_number_list(text: &str, scale: f64, origin: f64) -> Vec<f64> {
    let mut values = Vec::with_capacity(text.len() / 3);
    for token in text.split_whitespace() {
        if let Ok(raw) = token.parse::<f64>() {
            values.push(origin + (raw * scale));
        }
    }
    values
}

fn format_hl7_timestamp(raw: &str) -> String {
    if raw.len() < 8 {
        return raw.to_owned();
    }
    let mut output = format!("{}/{}/{}", &raw[6..8], &raw[4..6], &raw[0..4]);
    if raw.len() >= 14 {
        output.push(' ');
        output.push_str(&format!(
            "{}:{}:{}",
            &raw[8..10],
            &raw[10..12],
            &raw[12..14]
        ));
    }
    output
}

fn friendly_lead_name(code: &str) -> Option<String> {
    match code {
        "MDC_ECG_LEAD_I" | "1" => Some("I".to_owned()),
        "MDC_ECG_LEAD_II" | "2" => Some("II".to_owned()),
        "MDC_ECG_LEAD_III" | "61" => Some("III".to_owned()),
        "MDC_ECG_LEAD_AVR" | "62" => Some("aVR".to_owned()),
        "MDC_ECG_LEAD_AVL" | "63" => Some("aVL".to_owned()),
        "MDC_ECG_LEAD_AVF" | "64" => Some("aVF".to_owned()),
        "MDC_ECG_LEAD_V1" | "3" => Some("V1".to_owned()),
        "MDC_ECG_LEAD_V2" | "4" => Some("V2".to_owned()),
        "MDC_ECG_LEAD_V3" | "5" => Some("V3".to_owned()),
        "MDC_ECG_LEAD_V4" | "6" => Some("V4".to_owned()),
        "MDC_ECG_LEAD_V5" | "7" => Some("V5".to_owned()),
        "MDC_ECG_LEAD_V6" | "8" => Some("V6".to_owned()),
        _ if code.starts_with("MDC_ECG_LEAD_") && code != "MDC_ECG_LEAD_CONFIG" => {
            Some(code.trim_start_matches("MDC_ECG_LEAD_").to_owned())
        }
        _ => None,
    }
}

fn amplitude_to_microvolts(value: f64, unit: &str) -> f64 {
    let normalized = normalize_unit(unit);
    if normalized.is_empty() || has_micro_prefix(&normalized, 'v') {
        value
    } else if normalized == "mv" {
        value * 1_000.0
    } else if normalized == "v" {
        value * 1_000_000.0
    } else {
        value
    }
}

fn time_to_seconds(value: f64, unit: &str) -> f64 {
    let normalized = normalize_unit(unit);
    if normalized.is_empty() {
        return if value > 0.02 { value / 1_000.0 } else { value };
    }
    match normalized.as_str() {
        "s" | "sec" => value,
        "ms" | "msec" => value / 1_000.0,
        "ns" => value / 1_000_000_000.0,
        "hz" | "1/s" | "s-1" if value > 0.0 => 1.0 / value,
        _ if has_micro_prefix(&normalized, 's') => value / 1_000_000.0,
        _ => value,
    }
}

fn normalize_unit(unit: &str) -> String {
    let trimmed = unit.trim().trim_start_matches('[').trim_end_matches(']');
    trimmed.to_ascii_lowercase()
}

fn has_micro_prefix(unit: &str, suffix: char) -> bool {
    let mut chars = unit.chars();
    let Some(prefix) = chars.next() else {
        return false;
    };
    let Some(last) = chars.next() else {
        return false;
    };
    chars.next().is_none() && last == suffix && matches!(prefix, 'u' | 'µ' | 'μ')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_units() {
        assert_eq!(amplitude_to_microvolts(1.5, "mV"), 1500.0);
        assert_eq!(time_to_seconds(500.0, "Hz"), 0.002);
    }

    #[test]
    fn formats_hl7_date() {
        assert_eq!(
            format_hl7_timestamp("20260508123456"),
            "08/05/2026 12:34:56"
        );
    }

    #[test]
    fn decodes_latin1_xml_text_without_losing_brazilian_accents() {
        let text = decode_single_byte_text(b"<name>Jo\xe3o Gon\xe7alves</name>");
        assert_eq!(text, "<name>João Gonçalves</name>");
    }
}
