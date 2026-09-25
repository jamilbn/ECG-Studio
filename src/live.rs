use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const START_MARKER: [u8; 2] = [0xEE, 0x10];
const INIT_IDLE: [u8; 2] = [0x90, 0x00];
const UNKNOWN_PREPARE: [u8; 2] = [0x85, 0x01];
const START_STREAM: [u8; 2] = [0x90, 0x05];
const STOP_STREAM: [u8; 2] = [0x90, 0x00];

const LIVE_SAMPLE_INTERVAL_SECONDS: f64 = 0.002;
const ECG90A_SAMPLE_INTERVAL_SECONDS: f64 = 1.0 / 800.0;
pub const LIVE_ECG_LEAD_COUNT: usize = 12;
const LIVE_STORED_LEAD_COUNT: usize = 8;
const LIVE_A0_RECORD_BYTES: usize = 15;
const LIVE_B0_RECORD_BYTES: usize = 6;
const LIVE_ADC_OFFSET: f64 = 2048.0;
const LIVE_MICROVOLTS_PER_LSB: f64 = 2.5;
const ECG90A_MICROVOLTS_PER_LSB: f64 = 5.0;
const LIVE_STATUS_INTERVAL: Duration = Duration::from_secs(1);
const ECG90A_REPORT_BYTES: usize = 64;
const ECG90A_REPORT_MARKER: u8 = 0xE0;
const ECG90A_FRAMES_PER_REPORT: usize = 5;
const ECG90A_FRAME_BYTES: usize = 12;
const ECG90A_INIT_REPORT: [u8; 8] = [0x91, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
const ECG90A_INFO_REPORT: [u8; 8] = [0x82, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
const ECG90A_RATE_REPORT: [u8; 8] = [0x93, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
const ECG90A_PREPARE_REPORT: [u8; 8] = [0xA1, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
const ECG90A_START_REPORT: [u8; 8] = [0x90, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

#[derive(Clone, Debug, Default)]
pub struct LiveConfig;

#[derive(Debug)]
pub enum LiveMessage {
    Status(String),
    Frames(Vec<LiveSampleFrame>),
    Finished(Result<(), String>),
}

#[derive(Clone, Copy, Debug)]
pub struct LiveSampleFrame {
    pub leads_microvolts: [f64; LIVE_ECG_LEAD_COUNT],
}

pub struct LiveSession {
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl LiveSession {
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn live_sample_interval_seconds() -> f64 {
    LIVE_SAMPLE_INTERVAL_SECONDS
}

pub fn ecg90a_sample_interval_seconds() -> f64 {
    ECG90A_SAMPLE_INTERVAL_SECONDS
}

pub fn start_contec_8000g(config: LiveConfig) -> (LiveSession, Receiver<LiveMessage>) {
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        let result = run_contec_8000g_capture(config, thread_stop, &tx);
        let _ = tx.send(LiveMessage::Finished(result));
    });

    (
        LiveSession {
            stop,
            handle: Some(handle),
        },
        rx,
    )
}

pub fn start_ecg90a(config: LiveConfig) -> (LiveSession, Receiver<LiveMessage>) {
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let thread_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || {
        let result = run_ecg90a_capture(config, thread_stop, &tx);
        let _ = tx.send(LiveMessage::Finished(result));
    });

    (
        LiveSession {
            stop,
            handle: Some(handle),
        },
        rx,
    )
}

pub struct ContecLiveParser {
    buffer: Vec<u8>,
    streaming: bool,
    pending_stored_frame: Option<[f64; LIVE_STORED_LEAD_COUNT]>,
}

impl ContecLiveParser {
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(4096),
            streaming: false,
            pending_stored_frame: None,
        }
    }

    pub fn push_bytes(&mut self, bytes: &[u8]) -> Vec<LiveSampleFrame> {
        self.buffer.extend_from_slice(bytes);
        let mut frames = Vec::new();

        loop {
            if !self.streaming {
                if let Some(position) = find_bytes(&self.buffer, &START_MARKER) {
                    self.buffer.drain(..position + START_MARKER.len());
                    self.streaming = true;
                    self.reset_channel_demux();
                } else if let Some(position) = find_a0_record_start(&self.buffer) {
                    if self.buffer.len() - position < LIVE_A0_RECORD_BYTES {
                        let tail_len = self.buffer.len() - position;
                        keep_tail(&mut self.buffer, tail_len);
                        break;
                    }
                    self.buffer.drain(..position);
                    self.streaming = true;
                    self.reset_channel_demux();
                } else {
                    keep_tail(&mut self.buffer, LIVE_A0_RECORD_BYTES - 1);
                    break;
                }
            }

            if self.buffer.len() >= START_MARKER.len()
                && self.buffer[..START_MARKER.len()] == START_MARKER
            {
                self.buffer.drain(..START_MARKER.len());
                self.streaming = false;
                self.reset_channel_demux();
                continue;
            }

            let Some(&marker) = self.buffer.first() else {
                break;
            };

            match marker {
                0xA0 => {
                    if self.buffer.len() < LIVE_A0_RECORD_BYTES {
                        break;
                    }
                    let mut record = [0_u8; LIVE_A0_RECORD_BYTES];
                    record.copy_from_slice(&self.buffer[..LIVE_A0_RECORD_BYTES]);
                    frames.extend(self.decode_a0_record(&record));
                    self.buffer.drain(..LIVE_A0_RECORD_BYTES);
                }
                0xB0 => {
                    if self.buffer.len() < LIVE_B0_RECORD_BYTES {
                        break;
                    }
                    self.buffer.drain(..LIVE_B0_RECORD_BYTES);
                }
                _ => {
                    let next_marker = self
                        .buffer
                        .iter()
                        .position(|byte| matches!(*byte, 0xA0 | 0xB0 | 0xEE))
                        .unwrap_or(self.buffer.len());
                    self.buffer.drain(..next_marker.max(1));
                }
            }
        }

        frames
    }

    fn decode_a0_record(&mut self, record: &[u8]) -> Vec<LiveSampleFrame> {
        let stored_frame = decode_a0_stored_leads_microvolts(record);

        if let Some(previous) = self.pending_stored_frame.take() {
            let averaged = average_stored_frames(previous, stored_frame);
            vec![contec_8000g_stored_to_twelve_lead_frame(averaged)]
        } else {
            self.pending_stored_frame = Some(stored_frame);
            Vec::new()
        }
    }

    fn reset_channel_demux(&mut self) {
        self.pending_stored_frame = None;
    }
}

pub struct Ecg90aLiveParser;

impl Ecg90aLiveParser {
    pub fn new() -> Self {
        Self
    }

    pub fn push_report(&mut self, report: &[u8]) -> Vec<LiveSampleFrame> {
        let payload_bytes = ECG90A_FRAMES_PER_REPORT * ECG90A_FRAME_BYTES;
        if report.first() != Some(&ECG90A_REPORT_MARKER) || report.len() < 1 + payload_bytes {
            return Vec::new();
        }

        let payload = &report[1..1 + payload_bytes];
        payload
            .chunks_exact(ECG90A_FRAME_BYTES)
            .map(decode_ecg90a_frame)
            .collect()
    }
}

fn decode_ecg90a_frame(frame: &[u8]) -> LiveSampleFrame {
    let mut stored = [0.0; LIVE_STORED_LEAD_COUNT];
    for (pair_index, chunk) in frame.chunks_exact(3).enumerate() {
        // ECG90A packs the first ADC high nibble in the low half-byte.
        let first = (((chunk[0] & 0x0F) as u16) << 8) | chunk[1] as u16;
        let second = (((chunk[0] & 0xF0) as u16) << 4) | chunk[2] as u16;
        let output_index = pair_index * 2;
        stored[output_index] = ecg90a_adc_to_microvolts(first);
        stored[output_index + 1] = ecg90a_adc_to_microvolts(second);
    }
    ecg90a_stored_to_twelve_lead_frame(stored)
}

fn decode_a0_stored_leads_microvolts(record: &[u8]) -> [f64; LIVE_STORED_LEAD_COUNT] {
    let mut payload = [0_u8; 12];
    unpack_a0_payload(record, &mut payload);

    let mut values = [0.0; LIVE_STORED_LEAD_COUNT];
    for (pair_index, chunk) in payload.chunks_exact(3).enumerate() {
        let first = (((chunk[0] & 0xF0) as u16) << 4) | chunk[1] as u16;
        let second = (((chunk[0] & 0x0F) as u16) << 8) | chunk[2] as u16;
        let output_index = pair_index * 2;
        values[output_index] = adc_to_microvolts(first);
        values[output_index + 1] = adc_to_microvolts(second);
    }
    values
}

fn adc_to_microvolts(raw: u16) -> f64 {
    (raw as f64 - LIVE_ADC_OFFSET) * LIVE_MICROVOLTS_PER_LSB
}

fn ecg90a_adc_to_microvolts(raw: u16) -> f64 {
    (raw as f64 - LIVE_ADC_OFFSET) * ECG90A_MICROVOLTS_PER_LSB
}

fn average_stored_frames(
    first: [f64; LIVE_STORED_LEAD_COUNT],
    second: [f64; LIVE_STORED_LEAD_COUNT],
) -> [f64; LIVE_STORED_LEAD_COUNT] {
    let mut averaged = [0.0; LIVE_STORED_LEAD_COUNT];
    for index in 0..LIVE_STORED_LEAD_COUNT {
        averaged[index] = (first[index] + second[index]) * 0.5;
    }
    averaged
}

fn contec_8000g_stored_to_twelve_lead_frame(
    stored: [f64; LIVE_STORED_LEAD_COUNT],
) -> LiveSampleFrame {
    let lead_i = stored[0];
    let lead_ii = stored[1];
    let mut leads_microvolts = [0.0; LIVE_ECG_LEAD_COUNT];
    leads_microvolts[0] = lead_i;
    leads_microvolts[1] = lead_ii;
    leads_microvolts[2] = lead_ii - lead_i;
    leads_microvolts[3] = -0.5 * (lead_i + lead_ii);
    leads_microvolts[4] = lead_i - (0.5 * lead_ii);
    leads_microvolts[5] = lead_ii - (0.5 * lead_i);
    leads_microvolts[6..].copy_from_slice(&stored[2..]);

    LiveSampleFrame { leads_microvolts }
}

fn ecg90a_stored_to_twelve_lead_frame(stored: [f64; LIVE_STORED_LEAD_COUNT]) -> LiveSampleFrame {
    // ECG90A live reports keep II first and III second; I is derived from them.
    let lead_ii = stored[0];
    let lead_iii = stored[1];
    let mut leads_microvolts = [0.0; LIVE_ECG_LEAD_COUNT];
    leads_microvolts[0] = lead_ii - lead_iii;
    leads_microvolts[1] = lead_ii;
    leads_microvolts[2] = lead_iii;
    leads_microvolts[3] = (0.5 * lead_iii) - lead_ii;
    leads_microvolts[4] = (0.5 * lead_ii) - lead_iii;
    leads_microvolts[5] = 0.5 * (lead_ii + lead_iii);
    leads_microvolts[6..].copy_from_slice(&stored[2..]);

    LiveSampleFrame { leads_microvolts }
}

fn unpack_a0_payload(record: &[u8], output: &mut [u8; 12]) {
    for index in 0..output.len() {
        let low = record[3 + index];
        let mask = record[1 + (index / 7)];
        let high = (mask << (7 - (index % 7))) & 0x80;
        output[index] = low | high;
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn find_a0_record_start(bytes: &[u8]) -> Option<usize> {
    bytes.iter().position(|byte| *byte == 0xA0)
}

fn keep_tail(buffer: &mut Vec<u8>, tail_len: usize) {
    if buffer.len() <= tail_len {
        return;
    }
    let drain_to = buffer.len() - tail_len;
    buffer.drain(..drain_to);
}

#[cfg(any(windows, unix))]
fn run_contec_8000g_capture(
    _config: LiveConfig,
    stop: Arc<AtomicBool>,
    tx: &Sender<LiveMessage>,
) -> Result<(), String> {
    let mut port = serial::SerialPort::open_auto(230_400)?;
    let _ = tx.send(LiveMessage::Status(format!(
        "Porta serial {} aberta a 230400 bps.",
        port.name()
    )));

    port.write_all(&INIT_IDLE)?;
    thread::sleep(Duration::from_millis(40));
    port.write_all(&UNKNOWN_PREPARE)?;
    thread::sleep(Duration::from_millis(40));
    port.write_all(&START_STREAM)?;
    let _ = tx.send(LiveMessage::Status(
        "Aquisição CONTEC 8000G iniciada.".to_owned(),
    ));

    let mut parser = ContecLiveParser::new();
    let mut read_buffer = [0_u8; 512];
    let mut bytes_received = 0_usize;
    let mut samples_received = 0_usize;
    let mut last_status = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        match port.read(&mut read_buffer) {
            Ok(0) => thread::sleep(Duration::from_millis(5)),
            Ok(count) => {
                bytes_received += count;
                let frames = parser.push_bytes(&read_buffer[..count]);
                if !frames.is_empty() {
                    samples_received += frames.len();
                    let _ = tx.send(LiveMessage::Frames(frames));
                }
                if last_status.elapsed() >= LIVE_STATUS_INTERVAL {
                    send_live_progress(tx, bytes_received, samples_received);
                    last_status = Instant::now();
                }
            }
            Err(error) if error == "timeout" => {}
            Err(error) => return Err(error),
        }
    }

    port.write_all(&STOP_STREAM)?;
    let _ = tx.send(LiveMessage::Status(
        "Aquisição CONTEC 8000G parada.".to_owned(),
    ));
    Ok(())
}

#[cfg(any(windows, unix))]
fn send_live_progress(tx: &Sender<LiveMessage>, bytes_received: usize, samples_received: usize) {
    let status = if samples_received > 0 {
        format!(
            "Captura CONTEC 8000G: {samples_received} amostras, {bytes_received} bytes recebidos."
        )
    } else if bytes_received > 0 {
        format!(
            "Recebendo dados do CONTEC 8000G ({bytes_received} bytes), aguardando sincronismo A0."
        )
    } else {
        "Aguardando dados do CONTEC 8000G.".to_owned()
    };
    let _ = tx.send(LiveMessage::Status(status));
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn run_ecg90a_capture(
    _config: LiveConfig,
    stop: Arc<AtomicBool>,
    tx: &Sender<LiveMessage>,
) -> Result<(), String> {
    let mut device = hid::Ecg90aHidDevice::open_auto()?;
    let _ = tx.send(LiveMessage::Status(
        "CONTEC ECG90A HID detectado em VID_0483&PID_5750.".to_owned(),
    ));

    device.write_report(&ECG90A_INIT_REPORT)?;
    thread::sleep(Duration::from_millis(320));
    device.write_report(&ECG90A_INFO_REPORT)?;
    thread::sleep(Duration::from_millis(15));
    device.write_report(&ECG90A_RATE_REPORT)?;
    thread::sleep(Duration::from_millis(15));
    device.write_report(&ECG90A_PREPARE_REPORT)?;
    thread::sleep(Duration::from_millis(15));
    device.write_report(&ECG90A_START_REPORT)?;
    let _ = tx.send(LiveMessage::Status(
        "Aquisição CONTEC ECG90A iniciada.".to_owned(),
    ));

    let mut parser = Ecg90aLiveParser::new();
    let mut read_buffer = [0_u8; ECG90A_REPORT_BYTES];
    let mut bytes_received = 0_usize;
    let mut samples_received = 0_usize;
    let mut last_status = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        match device.read_report(&mut read_buffer) {
            Ok(0) => thread::sleep(Duration::from_millis(5)),
            Ok(count) => {
                bytes_received += count;
                let frames = parser.push_report(&read_buffer[..count]);
                if !frames.is_empty() {
                    samples_received += frames.len();
                    let _ = tx.send(LiveMessage::Frames(frames));
                }
                if last_status.elapsed() >= LIVE_STATUS_INTERVAL {
                    send_ecg90a_progress(tx, bytes_received, samples_received);
                    last_status = Instant::now();
                }
            }
            Err(error) => return Err(error),
        }
    }

    device.write_report(&ECG90A_INIT_REPORT)?;
    let _ = tx.send(LiveMessage::Status(
        "Aquisição CONTEC ECG90A parada.".to_owned(),
    ));
    Ok(())
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn send_ecg90a_progress(tx: &Sender<LiveMessage>, bytes_received: usize, samples_received: usize) {
    let status = if samples_received > 0 {
        format!(
            "Captura CONTEC ECG90A: {samples_received} amostras, {bytes_received} bytes recebidos."
        )
    } else if bytes_received > 0 {
        format!("Recebendo dados do CONTEC ECG90A ({bytes_received} bytes), aguardando reports E0.")
    } else {
        "Aguardando dados do CONTEC ECG90A.".to_owned()
    };
    let _ = tx.send(LiveMessage::Status(status));
}

#[cfg(not(any(windows, unix)))]
fn run_contec_8000g_capture(
    _config: LiveConfig,
    _stop: Arc<AtomicBool>,
    _tx: &Sender<LiveMessage>,
) -> Result<(), String> {
    Err("Captura CONTEC 8000G ao vivo está disponível apenas no Windows.".to_owned())
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn run_ecg90a_capture(
    _config: LiveConfig,
    _stop: Arc<AtomicBool>,
    _tx: &Sender<LiveMessage>,
) -> Result<(), String> {
    Err("Captura CONTEC ECG90A ao vivo está disponível apenas no Windows.".to_owned())
}

#[cfg(windows)]
mod hid {
    use std::mem::size_of;

    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO, SP_DEVICE_INTERFACE_DATA,
        SP_DEVICE_INTERFACE_DETAIL_DATA_W, SetupDiDestroyDeviceInfoList,
        SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
    };
    use windows::Win32::Devices::HumanInterfaceDevice::{
        HIDP_CAPS, HIDP_STATUS_SUCCESS, HidD_FreePreparsedData, HidD_GetHidGuid,
        HidD_GetPreparsedData, HidP_GetCaps, PHIDP_PREPARSED_DATA,
    };
    use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_MODE,
        OPEN_EXISTING, ReadFile, WriteFile,
    };
    use windows::core::PCWSTR;

    const ECG90A_DEVICE_ID: &str = "vid_0483&pid_5750";

    pub struct Ecg90aHidDevice {
        handle: HANDLE,
        input_report_len: usize,
        output_report_len: usize,
    }

    impl Ecg90aHidDevice {
        pub fn open_auto() -> Result<Self, String> {
            let path = enumerate_paths()?
                .into_iter()
                .find(|path| path.to_ascii_lowercase().contains(ECG90A_DEVICE_ID))
                .ok_or_else(|| {
                    "Nenhum HID CONTEC ECG90A VID_0483&PID_5750 foi encontrado.".to_owned()
                })?;
            Self::open(&path)
        }

        fn open(path: &str) -> Result<Self, String> {
            let wide = wide_null(path);
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(wide.as_ptr()),
                    (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                    FILE_SHARE_MODE(0),
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
            }
            .map_err(|error| {
                format!(
                    "Não foi possível abrir o HID do CONTEC ECG90A: {error}. Feche o software oficial se ele estiver usando o aparelho."
                )
            })?;

            if handle == INVALID_HANDLE_VALUE {
                return Err("Não foi possível abrir o HID do CONTEC ECG90A.".to_owned());
            }

            let report_lengths = report_lengths(handle)?;
            Ok(Self {
                handle,
                input_report_len: report_lengths.input,
                output_report_len: report_lengths.output,
            })
        }

        pub fn read_report(&mut self, buffer: &mut [u8]) -> Result<usize, String> {
            let mut hid_report = vec![0_u8; self.input_report_len];
            let mut read = 0_u32;
            unsafe { ReadFile(self.handle, Some(&mut hid_report), Some(&mut read), None) }
                .map_err(|error| format!("Falha lendo report HID do CONTEC ECG90A: {error}"))?;

            let count = read as usize;
            let payload = report_payload(&hid_report[..count.min(hid_report.len())]);
            if payload.len() > buffer.len() {
                return Err(format!(
                    "O CONTEC ECG90A retornou {} bytes de payload HID, acima dos {} bytes esperados.",
                    payload.len(),
                    buffer.len()
                ));
            }
            buffer[..payload.len()].copy_from_slice(payload);
            Ok(payload.len())
        }

        pub fn write_report(&mut self, report: &[u8]) -> Result<(), String> {
            if self.output_report_len <= report.len() {
                return Err(format!(
                    "O CONTEC ECG90A anunciou reports HID de saída com {} bytes; são necessários ao menos {} bytes.",
                    self.output_report_len,
                    report.len() + 1
                ));
            }

            let mut hid_report = vec![0_u8; self.output_report_len];
            hid_report[1..1 + report.len()].copy_from_slice(report);
            let mut written = 0_u32;
            unsafe { WriteFile(self.handle, Some(&hid_report), Some(&mut written), None) }
                .map_err(|error| format!("Falha enviando report HID ao CONTEC ECG90A: {error}"))?;
            if written as usize == hid_report.len() {
                Ok(())
            } else {
                Err("O HID do CONTEC ECG90A aceitou apenas parte do report.".to_owned())
            }
        }
    }

    impl Drop for Ecg90aHidDevice {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }

    struct HidReportLengths {
        input: usize,
        output: usize,
    }

    fn report_lengths(handle: HANDLE) -> Result<HidReportLengths, String> {
        let mut preparsed_data = PHIDP_PREPARSED_DATA::default();
        if !unsafe { HidD_GetPreparsedData(handle, &mut preparsed_data) } {
            return Err(
                "Não foi possível consultar os descritores HID do CONTEC ECG90A.".to_owned(),
            );
        }

        let result = (|| {
            let mut caps = HIDP_CAPS::default();
            let status = unsafe { HidP_GetCaps(preparsed_data, &mut caps) };
            if status != HIDP_STATUS_SUCCESS {
                return Err("Não foi possível ler as capacidades HID do CONTEC ECG90A.".to_owned());
            }

            let input = caps.InputReportByteLength as usize;
            let output = caps.OutputReportByteLength as usize;
            if input == 0 || output == 0 {
                return Err("O CONTEC ECG90A retornou tamanhos HID inválidos.".to_owned());
            }

            Ok(HidReportLengths { input, output })
        })();

        unsafe {
            let _ = HidD_FreePreparsedData(preparsed_data);
        }
        result
    }

    fn report_payload(report: &[u8]) -> &[u8] {
        if report.first() == Some(&0) {
            &report[1..]
        } else {
            report
        }
    }

    fn enumerate_paths() -> Result<Vec<String>, String> {
        let hid_guid = unsafe { HidD_GetHidGuid() };
        let info_set = unsafe {
            SetupDiGetClassDevsW(
                Some(&hid_guid),
                PCWSTR::null(),
                None,
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        }
        .map_err(|error| format!("Não foi possível enumerar dispositivos HID: {error}"))?;
        let info_set = DeviceInfoSet { handle: info_set };

        let mut paths = Vec::new();
        let mut index = 0_u32;
        loop {
            let mut interface_data = SP_DEVICE_INTERFACE_DATA {
                cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };
            if unsafe {
                SetupDiEnumDeviceInterfaces(
                    info_set.handle,
                    None,
                    &hid_guid,
                    index,
                    &mut interface_data,
                )
            }
            .is_err()
            {
                break;
            }

            if let Some(path) = detail_path(info_set.handle, &interface_data)? {
                paths.push(path);
            }
            index += 1;
        }

        Ok(paths)
    }

    fn detail_path(
        info_set: HDEVINFO,
        interface_data: &SP_DEVICE_INTERFACE_DATA,
    ) -> Result<Option<String>, String> {
        let mut required_size = 0_u32;
        let _ = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                info_set,
                interface_data,
                None,
                0,
                Some(&mut required_size),
                None,
            )
        };
        if required_size == 0 {
            return Ok(None);
        }

        let mut buffer = vec![0_u8; required_size as usize];
        let detail = buffer
            .as_mut_ptr()
            .cast::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>();
        unsafe {
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            SetupDiGetDeviceInterfaceDetailW(
                info_set,
                interface_data,
                Some(detail),
                required_size,
                Some(&mut required_size),
                None,
            )
        }
        .map_err(|error| format!("Falha lendo o caminho HID do CONTEC ECG90A: {error}"))?;

        let path_capacity = buffer.len().saturating_sub(size_of::<u32>()) / size_of::<u16>();
        let path_units =
            unsafe { std::slice::from_raw_parts((*detail).DevicePath.as_ptr(), path_capacity) };
        let path_len = path_units
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(path_units.len());
        Ok(Some(String::from_utf16_lossy(&path_units[..path_len])))
    }

    struct DeviceInfoSet {
        handle: HDEVINFO,
    }

    impl Drop for DeviceInfoSet {
        fn drop(&mut self) {
            unsafe {
                let _ = SetupDiDestroyDeviceInfoList(self.handle);
            }
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(target_os = "linux")]
mod hid {
    use std::fs::{self, File, OpenOptions};
    use std::io::{ErrorKind, Read, Write};
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::PathBuf;

    const ECG90A_VENDOR_ID: u32 = 0x0483;
    const ECG90A_PRODUCT_ID: u32 = 0x5750;
    const HIDRAW_NONBLOCK: i32 = 0o4000;

    pub struct Ecg90aHidDevice {
        file: File,
    }

    impl Ecg90aHidDevice {
        pub fn open_auto() -> Result<Self, String> {
            let candidates = hidraw_candidates()?;
            let path = candidates.first().ok_or_else(|| {
                "Nenhum HID CONTEC ECG90A VID_0483&PID_5750 foi encontrado em /dev/hidraw*."
                    .to_owned()
            })?;
            Self::open(path)
        }

        fn open(path: &PathBuf) -> Result<Self, String> {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(HIDRAW_NONBLOCK)
                .open(path)
                .map_err(|error| {
                    format!(
                        "Nao foi possivel abrir {}: {error}. Verifique permissoes do hidraw/udev.",
                        path.display()
                    )
                })?;
            Ok(Self { file })
        }

        pub fn read_report(&mut self, buffer: &mut [u8]) -> Result<usize, String> {
            let mut hid_report = [0_u8; super::ECG90A_REPORT_BYTES + 1];
            match self.file.read(&mut hid_report) {
                Ok(0) => Ok(0),
                Ok(count) => {
                    let payload = report_payload(&hid_report[..count]);
                    if payload.len() > buffer.len() {
                        return Err(format!(
                            "O CONTEC ECG90A retornou {} bytes de payload HID, acima dos {} bytes esperados.",
                            payload.len(),
                            buffer.len()
                        ));
                    }
                    buffer[..payload.len()].copy_from_slice(payload);
                    Ok(payload.len())
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) =>
                {
                    Ok(0)
                }
                Err(error) => Err(format!("Falha lendo report HID do CONTEC ECG90A: {error}")),
            }
        }

        pub fn write_report(&mut self, report: &[u8]) -> Result<(), String> {
            let mut hid_report = Vec::with_capacity(report.len() + 1);
            hid_report.push(0);
            hid_report.extend_from_slice(report);
            self.file
                .write_all(&hid_report)
                .map_err(|error| format!("Falha enviando report HID ao CONTEC ECG90A: {error}"))
        }
    }

    fn hidraw_candidates() -> Result<Vec<PathBuf>, String> {
        let entries = fs::read_dir("/sys/class/hidraw")
            .map_err(|error| format!("Nao foi possivel enumerar /sys/class/hidraw: {error}"))?;
        let mut candidates = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| format!("Falha lendo entrada hidraw: {error}"))?;
            let name = entry.file_name();
            let uevent_path = entry.path().join("device").join("uevent");
            let Ok(uevent) = fs::read_to_string(uevent_path) else {
                continue;
            };
            if hid_id_matches(&uevent) {
                candidates.push(PathBuf::from("/dev").join(name));
            }
        }
        candidates.sort();
        Ok(candidates)
    }

    fn hid_id_matches(uevent: &str) -> bool {
        uevent
            .lines()
            .find_map(|line| line.strip_prefix("HID_ID="))
            .and_then(|value| {
                let mut parts = value.split(':');
                let _bus = parts.next()?;
                let vendor = parse_hex_id(parts.next()?)?;
                let product = parse_hex_id(parts.next()?)?;
                Some(vendor == ECG90A_VENDOR_ID && product == ECG90A_PRODUCT_ID)
            })
            .unwrap_or(false)
    }

    fn parse_hex_id(value: &str) -> Option<u32> {
        u32::from_str_radix(value.trim_start_matches("0x"), 16).ok()
    }

    fn report_payload(report: &[u8]) -> &[u8] {
        if report.first() == Some(&0) {
            &report[1..]
        } else {
            report
        }
    }
}

#[cfg(target_os = "macos")]
mod hid {
    use std::ffi::{CString, c_char, c_double, c_int, c_uint, c_void};
    use std::ptr;
    use std::slice;
    use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
    use std::time::Duration;

    type CFAllocatorRef = *const c_void;
    type CFIndex = isize;
    type CFNumberRef = *const c_void;
    type CFRunLoopMode = *const c_void;
    type CFRunLoopRef = *mut c_void;
    type CFSetRef = *const c_void;
    type CFStringEncoding = u32;
    type CFStringRef = *const c_void;
    type CFTypeRef = *const c_void;
    type IOHIDDeviceRef = *mut c_void;
    type IOHIDManagerRef = *mut c_void;
    type IOHIDReportType = c_int;
    type IOReturn = c_int;

    const ECG90A_VENDOR_ID: i32 = 0x0483;
    const ECG90A_PRODUCT_ID: i32 = 0x5750;
    const K_CF_STRING_ENCODING_UTF8: CFStringEncoding = 0x0800_0100;
    const K_CF_NUMBER_INT_TYPE: c_int = 9;
    const K_IOHID_OPTIONS_TYPE_NONE: c_uint = 0;
    const K_IOHID_REPORT_TYPE_INPUT: IOHIDReportType = 0;
    const K_IOHID_REPORT_TYPE_OUTPUT: IOHIDReportType = 1;
    const K_IORETURN_SUCCESS: IOReturn = 0;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFRunLoopDefaultMode: CFRunLoopMode;

        fn CFRelease(cf: CFTypeRef);
        fn CFRetain(cf: CFTypeRef) -> CFTypeRef;
        fn CFNumberGetValue(number: CFNumberRef, the_type: c_int, value_ptr: *mut c_void) -> u8;
        fn CFRunLoopGetCurrent() -> CFRunLoopRef;
        fn CFRunLoopRunInMode(
            mode: CFRunLoopMode,
            seconds: c_double,
            return_after_source_handled: u8,
        ) -> c_int;
        fn CFSetGetCount(set: CFSetRef) -> CFIndex;
        fn CFSetGetValues(set: CFSetRef, values: *mut *const c_void);
        fn CFStringCreateWithCString(
            alloc: CFAllocatorRef,
            c_str: *const c_char,
            encoding: CFStringEncoding,
        ) -> CFStringRef;
    }

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOHIDDeviceClose(device: IOHIDDeviceRef, options: c_uint) -> IOReturn;
        fn IOHIDDeviceGetProperty(device: IOHIDDeviceRef, key: CFStringRef) -> CFTypeRef;
        fn IOHIDDeviceOpen(device: IOHIDDeviceRef, options: c_uint) -> IOReturn;
        fn IOHIDDeviceRegisterInputReportCallback(
            device: IOHIDDeviceRef,
            report: *mut u8,
            report_length: CFIndex,
            callback: extern "C" fn(
                *mut c_void,
                IOReturn,
                *mut c_void,
                IOHIDReportType,
                u32,
                *mut u8,
                CFIndex,
            ),
            context: *mut c_void,
        );
        fn IOHIDDeviceScheduleWithRunLoop(
            device: IOHIDDeviceRef,
            run_loop: CFRunLoopRef,
            mode: CFRunLoopMode,
        );
        fn IOHIDDeviceSetReport(
            device: IOHIDDeviceRef,
            report_type: IOHIDReportType,
            report_id: u32,
            report: *const u8,
            report_length: CFIndex,
        ) -> IOReturn;
        fn IOHIDDeviceUnscheduleFromRunLoop(
            device: IOHIDDeviceRef,
            run_loop: CFRunLoopRef,
            mode: CFRunLoopMode,
        );
        fn IOHIDManagerCopyDevices(manager: IOHIDManagerRef) -> CFSetRef;
        fn IOHIDManagerCreate(allocator: CFAllocatorRef, options: c_uint) -> IOHIDManagerRef;
        fn IOHIDManagerOpen(manager: IOHIDManagerRef, options: c_uint) -> IOReturn;
        fn IOHIDManagerSetDeviceMatching(manager: IOHIDManagerRef, matching: *const c_void);
    }

    pub struct Ecg90aHidDevice {
        device: IOHIDDeviceRef,
        run_loop: CFRunLoopRef,
        _report_buffer: Vec<u8>,
        _callback_context: Box<InputCallbackContext>,
        rx: Receiver<Vec<u8>>,
    }

    struct InputCallbackContext {
        tx: SyncSender<Vec<u8>>,
    }

    impl Ecg90aHidDevice {
        pub fn open_auto() -> Result<Self, String> {
            let device = find_ecg90a_device()?.ok_or_else(|| {
                "Nenhum HID CONTEC ECG90A VID_0483&PID_5750 foi encontrado no macOS.".to_owned()
            })?;
            Self::open(device)
        }

        fn open(device: IOHIDDeviceRef) -> Result<Self, String> {
            let opened = unsafe { IOHIDDeviceOpen(device, K_IOHID_OPTIONS_TYPE_NONE) };
            if opened != K_IORETURN_SUCCESS {
                unsafe {
                    CFRelease(device as CFTypeRef);
                }
                return Err(format!(
                    "Nao foi possivel abrir o HID do CONTEC ECG90A no macOS: codigo 0x{:08X}.",
                    opened as u32
                ));
            }

            let run_loop = unsafe { CFRunLoopGetCurrent() };
            let (tx, rx) = mpsc::sync_channel(8);
            let mut report_buffer = vec![0_u8; super::ECG90A_REPORT_BYTES + 1];
            let mut callback_context = Box::new(InputCallbackContext { tx });
            unsafe {
                IOHIDDeviceRegisterInputReportCallback(
                    device,
                    report_buffer.as_mut_ptr(),
                    report_buffer.len() as CFIndex,
                    input_report_callback,
                    (&mut *callback_context as *mut InputCallbackContext).cast(),
                );
                IOHIDDeviceScheduleWithRunLoop(device, run_loop, kCFRunLoopDefaultMode);
            }

            Ok(Self {
                device,
                run_loop,
                _report_buffer: report_buffer,
                _callback_context: callback_context,
                rx,
            })
        }

        pub fn read_report(&mut self, buffer: &mut [u8]) -> Result<usize, String> {
            match self.rx.try_recv() {
                Ok(report) => return copy_report(&report, buffer),
                Err(TryRecvError::Disconnected) => {
                    return Err("Canal HID do CONTEC ECG90A foi encerrado.".to_owned());
                }
                Err(TryRecvError::Empty) => {}
            }

            unsafe {
                let _ = CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.05, 1);
            }

            match self.rx.recv_timeout(Duration::from_millis(1)) {
                Ok(report) => copy_report(&report, buffer),
                Err(mpsc::RecvTimeoutError::Timeout) => Ok(0),
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    Err("Canal HID do CONTEC ECG90A foi encerrado.".to_owned())
                }
            }
        }

        pub fn write_report(&mut self, report: &[u8]) -> Result<(), String> {
            let result = unsafe {
                IOHIDDeviceSetReport(
                    self.device,
                    K_IOHID_REPORT_TYPE_OUTPUT,
                    0,
                    report.as_ptr(),
                    report.len() as CFIndex,
                )
            };
            if result == K_IORETURN_SUCCESS {
                Ok(())
            } else {
                Err(format!(
                    "Falha enviando report HID ao CONTEC ECG90A no macOS: codigo 0x{:08X}.",
                    result as u32
                ))
            }
        }
    }

    impl Drop for Ecg90aHidDevice {
        fn drop(&mut self) {
            unsafe {
                IOHIDDeviceUnscheduleFromRunLoop(self.device, self.run_loop, kCFRunLoopDefaultMode);
                let _ = IOHIDDeviceClose(self.device, K_IOHID_OPTIONS_TYPE_NONE);
                CFRelease(self.device as CFTypeRef);
            }
        }
    }

    extern "C" fn input_report_callback(
        context: *mut c_void,
        result: IOReturn,
        _sender: *mut c_void,
        report_type: IOHIDReportType,
        report_id: u32,
        report: *mut u8,
        report_length: CFIndex,
    ) {
        if context.is_null()
            || result != K_IORETURN_SUCCESS
            || report_type != K_IOHID_REPORT_TYPE_INPUT
            || report.is_null()
            || report_length <= 0
        {
            return;
        }

        let context = unsafe { &*(context as *mut InputCallbackContext) };
        let bytes = unsafe { slice::from_raw_parts(report, report_length as usize) };
        let mut payload = Vec::with_capacity(bytes.len() + usize::from(report_id != 0));
        if report_id != 0 {
            payload.push(report_id as u8);
        }
        payload.extend_from_slice(bytes);
        let _ = context.tx.try_send(payload);
    }

    fn find_ecg90a_device() -> Result<Option<IOHIDDeviceRef>, String> {
        let manager = unsafe { IOHIDManagerCreate(ptr::null(), K_IOHID_OPTIONS_TYPE_NONE) };
        if manager.is_null() {
            return Err("Nao foi possivel criar o gerenciador HID do macOS.".to_owned());
        }

        let result = (|| {
            unsafe {
                IOHIDManagerSetDeviceMatching(manager, ptr::null());
            }
            let opened = unsafe { IOHIDManagerOpen(manager, K_IOHID_OPTIONS_TYPE_NONE) };
            if opened != K_IORETURN_SUCCESS {
                return Err(format!(
                    "Nao foi possivel enumerar HID no macOS: codigo 0x{:08X}.",
                    opened as u32
                ));
            }

            let set = unsafe { IOHIDManagerCopyDevices(manager) };
            if set.is_null() {
                return Ok(None);
            }

            let device = find_matching_device_in_set(set);
            unsafe {
                CFRelease(set as CFTypeRef);
            }
            Ok(device)
        })();

        unsafe {
            CFRelease(manager as CFTypeRef);
        }
        result
    }

    fn find_matching_device_in_set(set: CFSetRef) -> Option<IOHIDDeviceRef> {
        let count = unsafe { CFSetGetCount(set) };
        if count <= 0 {
            return None;
        }

        let mut values = vec![ptr::null(); count as usize];
        unsafe {
            CFSetGetValues(set, values.as_mut_ptr());
        }

        values.into_iter().find_map(|value| {
            let device = value as IOHIDDeviceRef;
            if property_int(device, "VendorID") == Some(ECG90A_VENDOR_ID)
                && property_int(device, "ProductID") == Some(ECG90A_PRODUCT_ID)
            {
                Some(unsafe { CFRetain(device as CFTypeRef) as IOHIDDeviceRef })
            } else {
                None
            }
        })
    }

    fn property_int(device: IOHIDDeviceRef, key: &str) -> Option<i32> {
        let key = cf_string(key)?;
        let value = unsafe { IOHIDDeviceGetProperty(device, key) };
        unsafe {
            CFRelease(key as CFTypeRef);
        }
        if value.is_null() {
            return None;
        }

        let mut output = 0_i32;
        let ok = unsafe {
            CFNumberGetValue(
                value as CFNumberRef,
                K_CF_NUMBER_INT_TYPE,
                (&mut output as *mut i32).cast(),
            )
        };
        (ok != 0).then_some(output)
    }

    fn cf_string(value: &str) -> Option<CFStringRef> {
        let c_string = CString::new(value).ok()?;
        let string = unsafe {
            CFStringCreateWithCString(ptr::null(), c_string.as_ptr(), K_CF_STRING_ENCODING_UTF8)
        };
        (!string.is_null()).then_some(string)
    }

    fn copy_report(report: &[u8], buffer: &mut [u8]) -> Result<usize, String> {
        if report.len() > buffer.len() {
            return Err(format!(
                "O CONTEC ECG90A retornou {} bytes de payload HID, acima dos {} bytes esperados.",
                report.len(),
                buffer.len()
            ));
        }
        buffer[..report.len()].copy_from_slice(report);
        Ok(report.len())
    }
}

#[cfg(windows)]
mod serial {
    use std::mem::size_of;
    use windows::Win32::Devices::Communication::{
        COMMTIMEOUTS, DCB, GetCommState, NOPARITY, ONESTOPBIT, PURGE_RXCLEAR, PURGE_TXCLEAR,
        PurgeComm, SetCommState, SetCommTimeouts,
    };
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, HANDLE, INVALID_HANDLE_VALUE, NO_ERROR,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_MODE,
        OPEN_EXISTING, ReadFile, WriteFile,
    };
    use windows::Win32::System::Registry::{
        HKEY, HKEY_LOCAL_MACHINE, KEY_READ, REG_EXPAND_SZ, REG_SZ, RegCloseKey, RegEnumValueW,
        RegOpenKeyExW,
    };
    use windows::core::{PCWSTR, PWSTR};

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct SerialCandidate {
        port_name: String,
        label: String,
    }

    pub struct SerialPort {
        handle: HANDLE,
        name: String,
    }

    impl SerialPort {
        pub fn open_auto(baud_rate: u32) -> Result<Self, String> {
            let port_name = auto_detect_port_name()?;
            Self::open(&port_name, baud_rate)
        }

        fn open(port_name: &str, baud_rate: u32) -> Result<Self, String> {
            let path = normalize_port_name(port_name);
            let wide = wide_null(&path);
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(wide.as_ptr()),
                    (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                    FILE_SHARE_MODE(0),
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    None,
                )
            }
            .map_err(|error| {
                format!(
                    "Não foi possível abrir a porta serial detectada: {error}. Feche o ECG Workstation ou outro programa usando a porta."
                )
            })?;

            if handle == INVALID_HANDLE_VALUE {
                return Err("Não foi possível abrir a porta serial detectada.".to_owned());
            }

            configure_port(handle, baud_rate)?;
            unsafe {
                let _ = PurgeComm(handle, PURGE_RXCLEAR | PURGE_TXCLEAR);
            }
            Ok(Self {
                handle,
                name: port_name.to_owned(),
            })
        }

        pub fn name(&self) -> &str {
            &self.name
        }

        pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, String> {
            let mut read = 0_u32;
            let ok = unsafe { ReadFile(self.handle, Some(buffer), Some(&mut read), None) };
            match ok {
                Ok(()) => {
                    if read == 0 {
                        Err("timeout".to_owned())
                    } else {
                        Ok(read as usize)
                    }
                }
                Err(error) => Err(format!("Falha lendo porta serial: {error}")),
            }
        }

        pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), String> {
            let mut written = 0_u32;
            unsafe { WriteFile(self.handle, Some(bytes), Some(&mut written), None) }
                .map_err(|error| format!("Falha escrevendo na porta serial: {error}"))?;
            if written as usize == bytes.len() {
                Ok(())
            } else {
                Err("A porta serial aceitou apenas parte do comando.".to_owned())
            }
        }
    }

    impl Drop for SerialPort {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }

    fn configure_port(handle: HANDLE, baud_rate: u32) -> Result<(), String> {
        let mut dcb = DCB {
            DCBlength: size_of::<DCB>() as u32,
            ..Default::default()
        };
        unsafe { GetCommState(handle, &mut dcb) }
            .map_err(|error| format!("Falha lendo configuração serial: {error}"))?;
        dcb.BaudRate = baud_rate;
        dcb.ByteSize = 8;
        dcb.Parity = NOPARITY;
        dcb.StopBits = ONESTOPBIT;
        unsafe { SetCommState(handle, &dcb) }
            .map_err(|error| format!("Falha configurando porta serial: {error}"))?;

        let timeouts = COMMTIMEOUTS {
            ReadIntervalTimeout: 20,
            ReadTotalTimeoutMultiplier: 0,
            ReadTotalTimeoutConstant: 40,
            WriteTotalTimeoutMultiplier: 0,
            WriteTotalTimeoutConstant: 250,
        };
        unsafe { SetCommTimeouts(handle, &timeouts) }
            .map_err(|error| format!("Falha configurando timeout serial: {error}"))?;
        Ok(())
    }

    fn auto_detect_port_name() -> Result<String, String> {
        let candidates = serial_candidates()?;
        choose_serial_port(&candidates)
    }

    fn choose_serial_port(candidates: &[SerialCandidate]) -> Result<String, String> {
        if candidates.is_empty() {
            return Err("Nenhuma porta serial foi encontrada para a captura ao vivo.".to_owned());
        }

        if candidates.len() == 1 {
            return Ok(candidates[0].port_name.clone());
        }

        let mut scored: Vec<(i32, &SerialCandidate)> = candidates
            .iter()
            .map(|candidate| (serial_candidate_score(candidate), candidate))
            .collect();
        scored.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| {
                    com_port_number(&left.1.port_name).cmp(&com_port_number(&right.1.port_name))
                })
                .then_with(|| left.1.port_name.cmp(&right.1.port_name))
        });

        let best_score = scored[0].0;
        let best_count = scored
            .iter()
            .take_while(|(score, _)| *score == best_score)
            .count();
        if best_score > 0 && best_count == 1 {
            return Ok(scored[0].1.port_name.clone());
        }

        Err(
            "Não foi possível escolher automaticamente uma porta serial única para o CONTEC 8000G. Deixe apenas o aparelho conectado ou remova adaptadores seriais extras."
                .to_owned(),
        )
    }

    fn serial_candidate_score(candidate: &SerialCandidate) -> i32 {
        let label = format!("{} {}", candidate.port_name, candidate.label).to_ascii_lowercase();
        let mut score = 0;

        if contains_any(&label, &["contec", "8000", "ecg"]) {
            score += 100;
        }
        if contains_any(&label, &["cp210", "silab", "silicon"]) {
            score += 80;
        }
        if contains_any(&label, &["usbser", "usb"]) {
            score += 30;
        }
        if contains_any(&label, &["vcp", "uart"]) {
            score += 10;
        }
        if label.contains(r"\device\serial") {
            score -= 25;
        }

        score
    }

    fn contains_any(value: &str, needles: &[&str]) -> bool {
        needles.iter().any(|needle| value.contains(needle))
    }

    fn serial_candidates() -> Result<Vec<SerialCandidate>, String> {
        let key = RegistryKey::open_local_machine(r"HARDWARE\DEVICEMAP\SERIALCOMM")?;
        key.enum_serial_candidates()
    }

    struct RegistryKey {
        handle: HKEY,
    }

    impl RegistryKey {
        fn open_local_machine(path: &str) -> Result<Self, String> {
            let path = wide_null(path);
            let mut handle = HKEY::default();
            let result = unsafe {
                RegOpenKeyExW(
                    HKEY_LOCAL_MACHINE,
                    PCWSTR(path.as_ptr()),
                    None,
                    KEY_READ,
                    &mut handle,
                )
            };
            if result != NO_ERROR {
                return Err("Não foi possível consultar as portas seriais do Windows.".to_owned());
            }
            Ok(Self { handle })
        }

        fn enum_serial_candidates(&self) -> Result<Vec<SerialCandidate>, String> {
            let mut candidates = Vec::new();
            let mut index = 0;

            while let Some((label, port_name)) = self.enum_value(index)? {
                if port_name.starts_with("COM") {
                    candidates.push(SerialCandidate { port_name, label });
                }
                index += 1;
            }

            candidates.sort_by(|left, right| {
                com_port_number(&left.port_name)
                    .cmp(&com_port_number(&right.port_name))
                    .then_with(|| left.port_name.cmp(&right.port_name))
            });
            candidates.dedup_by(|left, right| left.port_name == right.port_name);
            Ok(candidates)
        }

        fn enum_value(&self, index: u32) -> Result<Option<(String, String)>, String> {
            let mut name_capacity = 256_usize;
            let mut data_capacity = 512_usize;

            loop {
                let mut name = vec![0_u16; name_capacity];
                let mut name_len = name.len() as u32;
                let mut data = vec![0_u8; data_capacity];
                let mut data_len = data.len() as u32;
                let mut value_type = 0_u32;
                let result = unsafe {
                    RegEnumValueW(
                        self.handle,
                        index,
                        Some(PWSTR(name.as_mut_ptr())),
                        &mut name_len,
                        None,
                        Some(&mut value_type),
                        Some(data.as_mut_ptr()),
                        Some(&mut data_len),
                    )
                };

                if result == ERROR_NO_MORE_ITEMS {
                    return Ok(None);
                }
                if result == ERROR_MORE_DATA {
                    name_capacity *= 2;
                    data_capacity *= 2;
                    continue;
                }
                if result != NO_ERROR {
                    return Err("Falha ao enumerar as portas seriais do Windows.".to_owned());
                }

                if value_type != REG_SZ.0 && value_type != REG_EXPAND_SZ.0 {
                    return Ok(Some((
                        utf16_lossy(&name[..name_len as usize]),
                        String::new(),
                    )));
                }

                let label = utf16_lossy(&name[..name_len as usize]);
                let port_name = reg_utf16_string(&data[..data_len as usize]);
                return Ok(Some((label, port_name)));
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

    fn normalize_port_name(port_name: &str) -> String {
        let trimmed = port_name.trim();
        if trimmed.starts_with(r"\\.\") {
            trimmed.to_owned()
        } else {
            format!(r"\\.\{trimmed}")
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn reg_utf16_string(bytes: &[u8]) -> String {
        let mut units = Vec::with_capacity(bytes.len() / 2);
        for chunk in bytes.chunks_exact(2) {
            units.push(u16::from_le_bytes([chunk[0], chunk[1]]));
        }
        while units.last() == Some(&0) {
            units.pop();
        }
        utf16_lossy(&units)
    }

    fn utf16_lossy(units: &[u16]) -> String {
        String::from_utf16_lossy(units).trim().to_owned()
    }

    fn com_port_number(port_name: &str) -> u32 {
        port_name
            .trim()
            .strip_prefix("COM")
            .and_then(|number| number.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn chooses_cp210_candidate_over_builtin_serial_port() {
            let candidates = vec![
                SerialCandidate {
                    port_name: "COM1".to_owned(),
                    label: r"\Device\Serial0".to_owned(),
                },
                SerialCandidate {
                    port_name: "COM6".to_owned(),
                    label: r"\Device\Silabser0".to_owned(),
                },
            ];

            assert_eq!(choose_serial_port(&candidates), Ok("COM6".to_owned()));
        }

        #[test]
        fn uses_single_port_when_it_is_the_only_candidate() {
            let candidates = vec![SerialCandidate {
                port_name: "COM7".to_owned(),
                label: r"\Device\USBSER000".to_owned(),
            }];

            assert_eq!(choose_serial_port(&candidates), Ok("COM7".to_owned()));
        }

        #[test]
        fn refuses_to_guess_between_multiple_unknown_ports() {
            let candidates = vec![
                SerialCandidate {
                    port_name: "COM8".to_owned(),
                    label: r"\Device\Unknown0".to_owned(),
                },
                SerialCandidate {
                    port_name: "COM9".to_owned(),
                    label: r"\Device\Unknown1".to_owned(),
                },
            ];

            assert!(choose_serial_port(&candidates).is_err());
        }
    }
}

#[cfg(unix)]
mod serial {
    use std::collections::HashSet;
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, ErrorKind, Read, Write};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::Duration;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct SerialCandidate {
        path: PathBuf,
        label: String,
    }

    pub struct SerialPort {
        file: File,
        name: String,
    }

    impl SerialPort {
        pub fn open_auto(baud_rate: u32) -> Result<Self, String> {
            let path = auto_detect_port_path()?;
            Self::open(&path, baud_rate)
        }

        fn open(path: &Path, baud_rate: u32) -> Result<Self, String> {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NOCTTY | libc::O_NONBLOCK)
                .open(path)
                .map_err(|error| {
                    format!(
                        "Nao foi possivel abrir a porta serial {}: {error}. Verifique permissoes do dispositivo.",
                        path.display()
                    )
                })?;
            configure_port(file.as_raw_fd(), baud_rate)?;
            Ok(Self {
                file,
                name: path.display().to_string(),
            })
        }

        pub fn name(&self) -> &str {
            &self.name
        }

        pub fn read(&mut self, buffer: &mut [u8]) -> Result<usize, String> {
            match self.file.read(buffer) {
                Ok(count) => Ok(count),
                Err(error)
                    if matches!(
                        error.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) =>
                {
                    Err("timeout".to_owned())
                }
                Err(error) => Err(format!("Falha lendo porta serial: {error}")),
            }
        }

        pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), String> {
            let mut offset = 0;
            let mut stalls = 0;
            while offset < bytes.len() {
                match self.file.write(&bytes[offset..]) {
                    Ok(0) => {
                        stalls += 1;
                        if stalls > 20 {
                            return Err(
                                "A porta serial nao aceitou o comando do CONTEC 8000G.".to_owned()
                            );
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Ok(count) => {
                        offset += count;
                        stalls = 0;
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            ErrorKind::WouldBlock | ErrorKind::Interrupted
                        ) =>
                    {
                        stalls += 1;
                        if stalls > 20 {
                            return Err(format!("Falha escrevendo na porta serial: {error}"));
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => {
                        return Err(format!("Falha escrevendo na porta serial: {error}"));
                    }
                }
            }
            if unsafe { libc::tcdrain(self.file.as_raw_fd()) } != 0 {
                return Err(format!(
                    "Falha aguardando a transmissao serial: {}",
                    io::Error::last_os_error()
                ));
            }
            Ok(())
        }
    }

    fn auto_detect_port_path() -> Result<PathBuf, String> {
        let candidates = serial_candidates()?;
        choose_serial_port(&candidates)
    }

    fn choose_serial_port(candidates: &[SerialCandidate]) -> Result<PathBuf, String> {
        if candidates.is_empty() {
            return Err(
                "Nenhuma porta serial USB foi encontrada para o CONTEC 8000G. Conecte o aparelho e reconecte o cabo depois de instalar o pacote. Se o ttyUSB sumir ao plugar, o brltty tomou o adaptador CP210x."
                    .to_owned(),
            );
        }

        if candidates.len() == 1 {
            return Ok(candidates[0].path.clone());
        }

        let mut scored: Vec<(i32, &SerialCandidate)> = candidates
            .iter()
            .map(|candidate| (serial_candidate_score(candidate), candidate))
            .collect();
        scored.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.path.cmp(&right.1.path))
        });

        let best_score = scored[0].0;
        let best_count = scored
            .iter()
            .take_while(|(score, _)| *score == best_score)
            .count();
        if best_score > 0 && best_count == 1 {
            return Ok(scored[0].1.path.clone());
        }

        Err(
            "Nao foi possivel escolher automaticamente uma porta serial unica para o CONTEC 8000G. Deixe apenas o aparelho conectado ou remova adaptadores seriais extras."
                .to_owned(),
        )
    }

    fn serial_candidate_score(candidate: &SerialCandidate) -> i32 {
        let label =
            format!("{} {}", candidate.path.display(), candidate.label).to_ascii_lowercase();
        let mut score = 0;

        if contains_any(&label, &["contec", "8000", "ecg"]) {
            score += 100;
        }
        if contains_any(&label, &["cp210", "silab", "silicon", "slab"]) {
            score += 80;
        }
        if contains_any(&label, &["usbserial", "usbmodem", "ttyusb", "ttyacm"]) {
            score += 40;
        }
        if contains_any(&label, &["usb", "vcp", "uart"]) {
            score += 15;
        }
        if contains_any(&label, &["bluetooth", "debug-console"]) {
            score -= 100;
        }

        score
    }

    fn contains_any(value: &str, needles: &[&str]) -> bool {
        needles.iter().any(|needle| value.contains(needle))
    }

    fn serial_candidates() -> Result<Vec<SerialCandidate>, String> {
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();

        #[cfg(target_os = "linux")]
        add_linux_by_id_candidates(&mut candidates, &mut seen);

        add_dev_candidates(&mut candidates, &mut seen)?;
        candidates.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(candidates)
    }

    #[cfg(target_os = "linux")]
    fn add_linux_by_id_candidates(
        candidates: &mut Vec<SerialCandidate>,
        seen: &mut HashSet<String>,
    ) {
        let Ok(entries) = fs::read_dir("/dev/serial/by-id") else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let label = entry.file_name().to_string_lossy().into_owned();
            push_candidate(candidates, seen, path, label);
        }
    }

    fn add_dev_candidates(
        candidates: &mut Vec<SerialCandidate>,
        seen: &mut HashSet<String>,
    ) -> Result<(), String> {
        let entries = fs::read_dir("/dev")
            .map_err(|error| format!("Nao foi possivel enumerar /dev: {error}"))?;

        for entry in entries {
            let entry = entry.map_err(|error| format!("Falha lendo entrada em /dev: {error}"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if serial_device_name_is_candidate(&name) {
                push_candidate(candidates, seen, entry.path(), name);
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn serial_device_name_is_candidate(name: &str) -> bool {
        name.starts_with("ttyUSB") || name.starts_with("ttyACM")
    }

    #[cfg(target_os = "macos")]
    fn serial_device_name_is_candidate(name: &str) -> bool {
        name.starts_with("cu.usbserial")
            || name.starts_with("cu.SLAB")
            || name.starts_with("cu.usbmodem")
            || name.starts_with("cu.wchusbserial")
            || name.starts_with("tty.usbserial")
            || name.starts_with("tty.SLAB")
            || name.starts_with("tty.usbmodem")
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    fn serial_device_name_is_candidate(_name: &str) -> bool {
        false
    }

    fn push_candidate(
        candidates: &mut Vec<SerialCandidate>,
        seen: &mut HashSet<String>,
        path: PathBuf,
        label: String,
    ) {
        let key = fs::canonicalize(&path)
            .unwrap_or_else(|_| path.clone())
            .display()
            .to_string();
        if seen.insert(key) {
            candidates.push(SerialCandidate { path, label });
        }
    }

    #[cfg(target_os = "linux")]
    fn configure_port(fd: i32, baud_rate: u32) -> Result<(), String> {
        baud_to_speed(baud_rate)?;
        let mut termios = unsafe { std::mem::zeroed::<libc::termios2>() };
        if unsafe { libc::ioctl(fd, libc::TCGETS2, &mut termios) } != 0 {
            return Err(format!(
                "Falha lendo a configuracao da porta serial: {}",
                io::Error::last_os_error()
            ));
        }

        termios.c_iflag &= !(libc::IGNBRK
            | libc::BRKINT
            | libc::PARMRK
            | libc::ISTRIP
            | libc::INLCR
            | libc::IGNCR
            | libc::ICRNL
            | libc::IXON
            | libc::IXOFF
            | libc::IXANY);
        termios.c_oflag &= !libc::OPOST;
        termios.c_lflag &= !(libc::ECHO | libc::ECHONL | libc::ICANON | libc::ISIG | libc::IEXTEN);
        termios.c_cflag &= !(libc::CSIZE
            | libc::PARENB
            | libc::CSTOPB
            | libc::CRTSCTS
            | libc::CBAUD
            | libc::CIBAUD);
        termios.c_cflag |= libc::CS8 | libc::CLOCAL | libc::CREAD | libc::BOTHER;
        termios.c_ispeed = baud_rate as libc::speed_t;
        termios.c_ospeed = baud_rate as libc::speed_t;
        termios.c_cc[libc::VMIN] = 0;
        termios.c_cc[libc::VTIME] = 1;

        if unsafe { libc::ioctl(fd, libc::TCSETS2, &termios) } != 0 {
            return Err(format!(
                "Falha configurando a porta serial em {baud_rate} bps: {}",
                io::Error::last_os_error()
            ));
        }

        let mut applied = unsafe { std::mem::zeroed::<libc::termios2>() };
        if unsafe { libc::ioctl(fd, libc::TCGETS2, &mut applied) } != 0 {
            return Err(format!(
                "Falha confirmando a configuracao da porta serial: {}",
                io::Error::last_os_error()
            ));
        }
        if !baud_rate_matches(baud_rate, applied.c_ospeed as u32) {
            return Err(format!(
                "A porta serial ficou em {} bps em vez de {baud_rate}.",
                applied.c_ospeed
            ));
        }
        finish_serial_open(fd)
    }

    #[cfg(not(target_os = "linux"))]
    fn configure_port(fd: i32, baud_rate: u32) -> Result<(), String> {
        let speed = baud_to_speed(baud_rate)?;
        let mut termios = unsafe { std::mem::zeroed::<libc::termios>() };
        if unsafe { libc::tcgetattr(fd, &mut termios) } != 0 {
            return Err(format!(
                "Falha lendo a configuracao da porta serial: {}",
                io::Error::last_os_error()
            ));
        }

        unsafe {
            libc::cfmakeraw(&mut termios);
        }
        termios.c_cflag |= libc::CLOCAL | libc::CREAD;
        termios.c_cflag &= !(libc::PARENB | libc::CSTOPB | libc::CSIZE | libc::CRTSCTS);
        termios.c_cflag |= libc::CS8;
        termios.c_iflag &= !(libc::IXON | libc::IXOFF | libc::IXANY);
        termios.c_cc[libc::VMIN] = 0;
        termios.c_cc[libc::VTIME] = 1;

        if unsafe { libc::cfsetispeed(&mut termios, speed) } != 0
            || unsafe { libc::cfsetospeed(&mut termios, speed) } != 0
        {
            return Err(format!(
                "Falha definindo {baud_rate} bps: {}",
                io::Error::last_os_error()
            ));
        }
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &termios) } != 0 {
            return Err(format!(
                "Falha configurando a porta serial em {baud_rate} bps: {}",
                io::Error::last_os_error()
            ));
        }

        let mut applied = unsafe { std::mem::zeroed::<libc::termios>() };
        if unsafe { libc::tcgetattr(fd, &mut applied) } != 0 {
            return Err(format!(
                "Falha confirmando a configuracao da porta serial: {}",
                io::Error::last_os_error()
            ));
        }
        let applied_speed = unsafe { libc::cfgetospeed(&applied) };
        if applied_speed != speed {
            return Err(format!("A porta serial nao aceitou {baud_rate} bps."));
        }
        finish_serial_open(fd)
    }

    fn finish_serial_open(fd: i32) -> Result<(), String> {
        unsafe {
            libc::tcflush(fd, libc::TCIOFLUSH);
            let mut lines = libc::TIOCM_DTR | libc::TIOCM_RTS;
            libc::ioctl(fd, libc::TIOCMBIS, &mut lines);
            libc::ioctl(fd, libc::TIOCEXCL);
        }
        Ok(())
    }

    fn baud_rate_matches(requested: u32, actual: u32) -> bool {
        if requested == 0 || actual == 0 {
            return false;
        }
        // The CP2102 clock programs 230400 as 24000000/104 = 230769.
        requested.abs_diff(actual).saturating_mul(100) <= requested.saturating_mul(2)
    }

    fn baud_to_speed(baud_rate: u32) -> Result<libc::speed_t, String> {
        match baud_rate {
            230_400 => Ok(libc::B230400),
            460_800 => Ok(libc::B460800),
            _ => Err(format!(
                "Velocidade serial {baud_rate} bps nao e suportada para o CONTEC 8000G."
            )),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn accepts_the_cp2102_divisor_for_230400() {
            assert!(baud_rate_matches(230_400, 230_769));
            assert!(!baud_rate_matches(230_400, 9600));
        }

        #[test]
        fn maps_contec_baud_rates() {
            assert_eq!(baud_to_speed(230_400).unwrap(), libc::B230400);
            assert_eq!(baud_to_speed(460_800).unwrap(), libc::B460800);
            assert!(baud_to_speed(9600).is_err());
        }

        #[test]
        fn prefers_cp210x_by_id_over_another_usb_serial_port() {
            let candidates = vec![
                SerialCandidate {
                    path: PathBuf::from("/dev/ttyUSB1"),
                    label: "ttyUSB1".to_owned(),
                },
                SerialCandidate {
                    path: PathBuf::from("/dev/serial/by-id/usb-Silicon_Labs_CP2102-if00-port0"),
                    label: "usb-Silicon_Labs_CP2102-if00-port0".to_owned(),
                },
            ];

            assert_eq!(
                choose_serial_port(&candidates).unwrap(),
                PathBuf::from("/dev/serial/by-id/usb-Silicon_Labs_CP2102-if00-port0")
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a0_records_into_twelve_channel_sample_frames() {
        let mut parser = ContecLiveParser::new();
        let mut stream = vec![0xEE, 0x10];
        stream.extend_from_slice(&a0_test_record([
            2048, 2049, 2050, 2051, 2052, 2053, 2054, 2055,
        ]));
        stream.extend_from_slice(&a0_test_record([
            2048, 2049, 2050, 2051, 2052, 2053, 2054, 2055,
        ]));
        stream.extend_from_slice(&START_MARKER);

        let frames = parser.push_bytes(&stream);

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].leads_microvolts[0], 0.0);
        assert_eq!(frames[0].leads_microvolts[1], 2.5);
        assert_eq!(frames[0].leads_microvolts[2], 2.5);
        assert_eq!(frames[0].leads_microvolts[11], 17.5);
    }

    #[test]
    fn can_synchronize_when_stream_marker_was_missed() {
        let mut parser = ContecLiveParser::new();
        let mut stream = vec![0x55, 0x00, 0x12];
        stream.extend_from_slice(&a0_test_record([
            2048, 2049, 2050, 2051, 2052, 2053, 2054, 2055,
        ]));
        stream.extend_from_slice(&a0_test_record([
            2048, 2049, 2050, 2051, 2052, 2053, 2054, 2055,
        ]));

        let frames = parser.push_bytes(&stream);

        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].leads_microvolts[11], 17.5);
    }

    #[test]
    fn derives_contec_8000g_frontal_leads_from_i_and_ii() {
        let frame =
            contec_8000g_stored_to_twelve_lead_frame([10.0, 35.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);

        assert_eq!(frame.leads_microvolts[0], 10.0);
        assert_eq!(frame.leads_microvolts[1], 35.0);
        assert_eq!(frame.leads_microvolts[2], 25.0);
        assert_eq!(frame.leads_microvolts[3], -22.5);
        assert_eq!(frame.leads_microvolts[4], -7.5);
        assert_eq!(frame.leads_microvolts[5], 30.0);
    }

    #[test]
    fn decodes_a0_payload_with_mask_bits() {
        let values = decode_a0_stored_leads_microvolts(&[
            0xA0, 0x49, 0x04, 0x08, 0x41, 0x00, 0x08, 0x00, 0x00, 0x08, 0x00, 0x00, 0x08, 0x00,
            0x00,
        ]);

        assert_eq!(values[0], 162.5);
        assert_eq!(values[1], 0.0);
    }

    #[test]
    fn parses_ecg90a_reports_into_five_sample_frames() {
        let mut parser = Ecg90aLiveParser::new();
        let report = ecg90a_test_report([2029, 2061, 2050, 2051, 2052, 2053, 2054, 2055]);

        let frames = parser.push_report(&report);

        assert_eq!(frames.len(), ECG90A_FRAMES_PER_REPORT);
        assert_eq!(frames[0].leads_microvolts[0], -160.0);
        assert_eq!(frames[0].leads_microvolts[1], -95.0);
        assert_eq!(frames[0].leads_microvolts[2], 65.0);
        assert_eq!(frames[0].leads_microvolts[11], 35.0);
    }

    #[test]
    fn ignores_ecg90a_status_reports() {
        let mut parser = Ecg90aLiveParser::new();
        let mut report = [0_u8; ECG90A_REPORT_BYTES];
        report[0] = 0xFF;

        assert!(parser.push_report(&report).is_empty());
    }

    fn a0_test_record(adc_values: [u16; LIVE_STORED_LEAD_COUNT]) -> [u8; LIVE_A0_RECORD_BYTES] {
        let mut record = [0_u8; LIVE_A0_RECORD_BYTES];
        record[0] = 0xA0;
        for (pair_index, pair) in adc_values.chunks_exact(2).enumerate() {
            let packed = (((pair[0] >> 8) as u8) << 4) | ((pair[1] >> 8) as u8 & 0x0F);
            let offset = pair_index * 3;
            pack_a0_payload_byte(&mut record, offset, packed);
            pack_a0_payload_byte(&mut record, offset + 1, pair[0] as u8);
            pack_a0_payload_byte(&mut record, offset + 2, pair[1] as u8);
        }
        record
    }

    fn pack_a0_payload_byte(record: &mut [u8; LIVE_A0_RECORD_BYTES], index: usize, value: u8) {
        record[3 + index] = value & 0x7F;
        if value & 0x80 != 0 {
            record[1 + (index / 7)] |= 1 << (index % 7);
        }
    }

    fn ecg90a_test_report(adc_values: [u16; LIVE_STORED_LEAD_COUNT]) -> [u8; ECG90A_REPORT_BYTES] {
        let mut report = [0_u8; ECG90A_REPORT_BYTES];
        report[0] = ECG90A_REPORT_MARKER;
        for frame_index in 0..ECG90A_FRAMES_PER_REPORT {
            let frame_offset = 1 + (frame_index * ECG90A_FRAME_BYTES);
            for (pair_index, pair) in adc_values.chunks_exact(2).enumerate() {
                let offset = frame_offset + (pair_index * 3);
                report[offset] = (((pair[1] >> 8) as u8) << 4) | ((pair[0] >> 8) as u8 & 0x0F);
                report[offset + 1] = pair[0] as u8;
                report[offset + 2] = pair[1] as u8;
            }
        }
        report
    }
}
