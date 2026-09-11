mod c8k;
mod contec;
mod dicom;
mod xml;

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::Read;
use std::path::Path;

use crate::domain::EcgDocument;

#[derive(Debug)]
pub struct ReadError {
    message: String,
}

impl ReadError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ReadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ReadError {}

pub type ReadResult<T> = Result<T, ReadError>;

pub fn read_ecg(path: &Path) -> ReadResult<EcgDocument> {
    if looks_like_xml(path) {
        return xml::read(path);
    }

    match extension_lower(path).as_deref() {
        Some("xml" | "aecg" | "hl7") => xml::read(path),
        Some("c8k") => c8k::read(path),
        Some("ecg") => contec::read(path),
        Some("dcm" | "dicom") => dicom::read(path),
        _ => Err(ReadError::new(
            "Formato não suportado. Use .xml, .aecg, .hl7, .c8k, .ecg, .dcm ou .dicom.",
        )),
    }
}

fn read_bytes(path: &Path) -> ReadResult<Vec<u8>> {
    fs::read(path).map_err(|error| {
        ReadError::new(format!(
            "Não foi possível ler '{}': {error}",
            path.display()
        ))
    })
}

fn extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
}

fn looks_like_xml(path: &Path) -> bool {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };
    let mut bytes = vec![0; 512];
    let count = match file.read(&mut bytes) {
        Ok(count) => count,
        Err(_) => return false,
    };
    bytes.truncate(count);
    if bytes.is_empty() {
        return false;
    }

    if bytes.starts_with(&[0xFF, 0xFE]) {
        return looks_like_utf16le_xml(&bytes, 2);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return looks_like_utf16be_xml(&bytes, 2);
    }
    if bytes.len() >= 2 && bytes[1] == 0 && (bytes[0] == b'<' || is_ascii_whitespace(bytes[0])) {
        return looks_like_utf16le_xml(&bytes, 0);
    }
    if bytes.len() >= 2 && bytes[0] == 0 && (bytes[1] == b'<' || is_ascii_whitespace(bytes[1])) {
        return looks_like_utf16be_xml(&bytes, 0);
    }
    looks_like_utf8_xml(&bytes)
}

fn looks_like_utf8_xml(bytes: &[u8]) -> bool {
    let mut offset = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };
    while offset < bytes.len() && is_ascii_whitespace(bytes[offset]) {
        offset += 1;
    }
    bytes.get(offset) == Some(&b'<')
}

fn looks_like_utf16le_xml(bytes: &[u8], mut offset: usize) -> bool {
    while offset + 1 < bytes.len() && bytes[offset + 1] == 0 && is_ascii_whitespace(bytes[offset]) {
        offset += 2;
    }
    offset + 1 < bytes.len() && bytes[offset] == b'<' && bytes[offset + 1] == 0
}

fn looks_like_utf16be_xml(bytes: &[u8], mut offset: usize) -> bool {
    while offset + 1 < bytes.len() && bytes[offset] == 0 && is_ascii_whitespace(bytes[offset + 1]) {
        offset += 2;
    }
    offset + 1 < bytes.len() && bytes[offset] == 0 && bytes[offset + 1] == b'<'
}

fn is_ascii_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}
