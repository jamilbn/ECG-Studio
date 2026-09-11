use crate::domain::{EcgDocument, LeadData};
use crate::i18n::MeasurementTexts;

#[derive(Clone, Debug)]
pub struct AutomaticMeasurements {
    pub heart_rate_bpm: Option<f64>,
    pub heart_rate_min_bpm: Option<f64>,
    pub heart_rate_max_bpm: Option<f64>,
    pub rr_mean_ms: Option<f64>,
    pub rr_min_ms: Option<f64>,
    pub rr_max_ms: Option<f64>,
    pub rhythm_regularity: Option<RhythmRegularity>,
    pub pr_ms: Option<f64>,
    pub qrs_ms: Option<f64>,
    pub qt_ms: Option<f64>,
    pub qtc_bazett_ms: Option<f64>,
    pub qrs_axis_deg: Option<f64>,
    pub qrs_axis_label: String,
    pub quality_warnings: Vec<String>,
}

impl Default for AutomaticMeasurements {
    fn default() -> Self {
        Self {
            heart_rate_bpm: None,
            heart_rate_min_bpm: None,
            heart_rate_max_bpm: None,
            rr_mean_ms: None,
            rr_min_ms: None,
            rr_max_ms: None,
            rhythm_regularity: None,
            pr_ms: None,
            qrs_ms: None,
            qt_ms: None,
            qtc_bazett_ms: None,
            qrs_axis_deg: None,
            qrs_axis_label: "indisponível".to_owned(),
            quality_warnings: Vec::new(),
        }
    }
}

impl AutomaticMeasurements {
    pub fn report_lines_with_texts(&self, texts: MeasurementTexts) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!(
            "{}: {} bpm | RR: {} ms",
            texts.heart_rate,
            optional_round(self.heart_rate_bpm),
            optional_round(self.rr_mean_ms)
        ));

        let hr_range = match (self.heart_rate_min_bpm, self.heart_rate_max_bpm) {
            (Some(min), Some(max)) => format!("{:.0}-{:.0} bpm", min, max),
            _ => "--".to_owned(),
        };
        let rr_range = match (self.rr_min_ms, self.rr_max_ms) {
            (Some(min), Some(max)) => format!("{:.0}-{:.0} ms", min, max),
            _ => "--".to_owned(),
        };
        let rhythm = self
            .rhythm_regularity
            .map(|regularity| regularity.label_with_texts(texts))
            .unwrap_or("--");
        lines.push(format!(
            "{}: {hr_range} | RR: {rr_range} | {rhythm}",
            texts.range
        ));
        lines.push(format!(
            "PR: {} ms | QRS: {} ms",
            optional_round(self.pr_ms),
            optional_round(self.qrs_ms)
        ));
        lines.push(format!(
            "QT: {} ms | QTc Bazett: {} ms",
            optional_round(self.qt_ms),
            optional_round(self.qtc_bazett_ms)
        ));

        let axis = self
            .qrs_axis_deg
            .map(|value| format!("{value:.0} {}", texts.degrees))
            .unwrap_or_else(|| "--".to_owned());
        lines.push(format!(
            "{}: {axis} ({})",
            texts.qrs_axis,
            qrs_axis_label_with_texts(&self.qrs_axis_label, texts)
        ));

        lines
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RhythmRegularity {
    Regular,
    Irregular,
}

impl RhythmRegularity {
    fn label_with_texts(self, texts: MeasurementTexts) -> &'static str {
        match self {
            Self::Regular => texts.rhythm_regular,
            Self::Irregular => texts.rhythm_irregular,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HeartRateMeasurements {
    pub heart_rate_bpm: f64,
    pub heart_rate_min_bpm: f64,
    pub heart_rate_max_bpm: f64,
    pub rr_mean_ms: f64,
    pub rr_min_ms: f64,
    pub rr_max_ms: f64,
    pub rhythm_regularity: RhythmRegularity,
}

#[derive(Clone, Debug)]
pub struct QtMeasurements {
    pub qt_ms: f64,
    pub qtc_bazett_ms: f64,
    pub beat_count: usize,
}

#[derive(Clone, Debug)]
pub struct PrQrsMeasurements {
    pub pr_ms: Option<f64>,
    pub qrs_ms: Option<f64>,
    pub pr_beat_count: usize,
    pub qrs_beat_count: usize,
}

#[derive(Clone, Debug)]
pub struct AxisMeasurements {
    pub axis_deg: f64,
    pub label: String,
    pub beat_count: usize,
}

pub fn analyze_ecg(document: &EcgDocument) -> AutomaticMeasurements {
    let mut measurements = AutomaticMeasurements::default();
    let sample_rate = document.sample_rate_hz();

    if !sample_rate.is_finite() || sample_rate <= 0.0 {
        measurements
            .quality_warnings
            .push("Taxa de amostragem invalida.".to_owned());
        return measurements;
    }
    if !(100.0..=2_000.0).contains(&sample_rate) {
        measurements
            .quality_warnings
            .push("Taxa de amostragem fora da faixa usual para análise automática.".to_owned());
    }

    let Some(qrs_lead) = select_qrs_lead(document) else {
        measurements
            .quality_warnings
            .push("Nenhuma derivação adequada para detectar QRS.".to_owned());
        return measurements;
    };

    if qrs_lead.samples_microvolts.len() < samples_for_seconds(sample_rate, 2.0) {
        measurements
            .quality_warnings
            .push("Sinal curto para estimar medidas automáticas com confiança.".to_owned());
    }

    let r_peaks = detect_qrs_peaks(&qrs_lead.samples_microvolts, sample_rate);
    if r_peaks.len() < 2 {
        measurements.quality_warnings.push(
            "QRS insuficiente ou sinal ruidoso para calcular frequência cardíaca.".to_owned(),
        );
        return measurements;
    }

    if let Some(heart_rate) = calculate_heart_rate(&r_peaks, sample_rate) {
        measurements.heart_rate_bpm = Some(heart_rate.heart_rate_bpm);
        measurements.heart_rate_min_bpm = Some(heart_rate.heart_rate_min_bpm);
        measurements.heart_rate_max_bpm = Some(heart_rate.heart_rate_max_bpm);
        measurements.rr_mean_ms = Some(heart_rate.rr_mean_ms);
        measurements.rr_min_ms = Some(heart_rate.rr_min_ms);
        measurements.rr_max_ms = Some(heart_rate.rr_max_ms);
        measurements.rhythm_regularity = Some(heart_rate.rhythm_regularity);
    } else {
        measurements
            .quality_warnings
            .push("Intervalos RR fora da faixa fisiológica esperada.".to_owned());
    }

    match calculate_pr_qrs(document, &r_peaks, sample_rate) {
        Some(intervals) => {
            measurements.pr_ms = intervals.pr_ms;
            measurements.qrs_ms = intervals.qrs_ms;
            if intervals.pr_ms.is_none() {
                measurements
                    .quality_warnings
                    .push("PR indisponível: onda P mal definida.".to_owned());
            } else if intervals.pr_beat_count < 3 {
                measurements
                    .quality_warnings
                    .push("PR calculado com poucos batimentos válidos.".to_owned());
            }
            if intervals.qrs_ms.is_none() {
                measurements
                    .quality_warnings
                    .push("QRS indisponível: início ou fim do QRS mal definido.".to_owned());
            } else if intervals.qrs_beat_count < 3 {
                measurements
                    .quality_warnings
                    .push("QRS calculado com poucos batimentos válidos.".to_owned());
            }
        }
        None => measurements
            .quality_warnings
            .push("PR/QRS indisponíveis: limites do complexo não confiáveis.".to_owned()),
    }

    match calculate_qt_qtc(document, &r_peaks, sample_rate) {
        Some(qt) => {
            measurements.qt_ms = Some(qt.qt_ms);
            measurements.qtc_bazett_ms = Some(qt.qtc_bazett_ms);
            if qt.beat_count < 3 {
                measurements
                    .quality_warnings
                    .push("QT/QTc calculado com poucos batimentos válidos.".to_owned());
            }
        }
        None => measurements
            .quality_warnings
            .push("QT/QTc indisponível: onda T mal definida ou sinal ruidoso.".to_owned()),
    }

    match calculate_qrs_axis(document, &r_peaks, sample_rate) {
        Some(axis) => {
            measurements.qrs_axis_deg = Some(axis.axis_deg);
            measurements.qrs_axis_label = axis.label;
            if axis.beat_count < 3 {
                measurements
                    .quality_warnings
                    .push("Eixo QRS calculado com poucos batimentos válidos.".to_owned());
            }
        }
        None => measurements
            .quality_warnings
            .push("Eixo QRS indisponível: derivações I/aVF ausentes ou QRS instável.".to_owned()),
    }

    measurements.quality_warnings.sort();
    measurements.quality_warnings.dedup();
    measurements
}

pub fn detect_qrs_peaks(samples_microvolts: &[f64], sample_rate_hz: f64) -> Vec<usize> {
    if samples_microvolts.len() < 3 || !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 {
        return Vec::new();
    }

    let filtered = qrs_bandpass_like(samples_microvolts, sample_rate_hz);
    let integrated = qrs_energy(&filtered, sample_rate_hz);
    let mut energy_values = finite_values(&integrated);
    if energy_values.len() < 3 {
        return Vec::new();
    }

    energy_values.sort_by(f64::total_cmp);
    let median = percentile_sorted(&energy_values, 0.50);
    let p90 = percentile_sorted(&energy_values, 0.90);
    let p98 = percentile_sorted(&energy_values, 0.98);
    let threshold = (median + ((p90 - median) * 0.55)).max(p98 * 0.18);
    if !threshold.is_finite() || threshold <= f64::EPSILON {
        return Vec::new();
    }

    let refractory = samples_for_seconds(sample_rate_hz, 0.25).max(1);
    let search_radius = samples_for_seconds(sample_rate_hz, 0.08).max(1);
    let mut peaks = Vec::new();
    let mut index = 0;

    while index < integrated.len() {
        if integrated[index] < threshold {
            index += 1;
            continue;
        }

        let start = index;
        while index < integrated.len() && integrated[index] >= threshold {
            index += 1;
        }
        let end = index.saturating_sub(1);
        let search_start = start.saturating_sub(search_radius);
        let search_end = (end + search_radius).min(filtered.len().saturating_sub(1));
        if let Some(peak) = max_abs_index(&filtered, search_start, search_end, 0.0) {
            push_peak_with_refractory(&mut peaks, peak, refractory, &filtered);
        }
    }

    prune_low_amplitude_peaks(&filtered, &peaks, sample_rate_hz)
}

pub fn calculate_heart_rate(
    r_peaks: &[usize],
    sample_rate_hz: f64,
) -> Option<HeartRateMeasurements> {
    let rr_values = rr_intervals_ms(r_peaks, sample_rate_hz);
    if rr_values.is_empty() {
        return None;
    }

    let valid_rr: Vec<f64> = rr_values
        .into_iter()
        .filter(|rr| (250.0..=2_500.0).contains(rr))
        .collect();
    if valid_rr.is_empty() {
        return None;
    }

    let rr_mean_ms = mean(&valid_rr);
    if rr_mean_ms <= 0.0 {
        return None;
    }

    let rr_min_ms = valid_rr
        .iter()
        .copied()
        .fold(f64::INFINITY, |acc, value| acc.min(value));
    let rr_max_ms = valid_rr
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, |acc, value| acc.max(value));
    let rr_cv = coefficient_of_variation(&valid_rr, rr_mean_ms);

    Some(HeartRateMeasurements {
        heart_rate_bpm: 60_000.0 / rr_mean_ms,
        heart_rate_min_bpm: 60_000.0 / rr_max_ms,
        heart_rate_max_bpm: 60_000.0 / rr_min_ms,
        rr_mean_ms,
        rr_min_ms,
        rr_max_ms,
        rhythm_regularity: if rr_cv <= 0.10 {
            RhythmRegularity::Regular
        } else {
            RhythmRegularity::Irregular
        },
    })
}

pub fn calculate_pr_qrs(
    document: &EcgDocument,
    r_peaks: &[usize],
    sample_rate_hz: f64,
) -> Option<PrQrsMeasurements> {
    if r_peaks.is_empty() {
        return None;
    }

    let pr_lead = select_pr_lead(document)?;
    let qrs_lead = select_interval_qrs_lead(document)?;
    let pr_centered = remove_moving_average(
        &pr_lead.samples_microvolts,
        samples_for_seconds(sample_rate_hz, 0.60),
    );
    let qrs_centered = remove_moving_average(
        &qrs_lead.samples_microvolts,
        samples_for_seconds(sample_rate_hz, 0.60),
    );
    let pr_smooth = moving_average(&pr_centered, samples_for_seconds(sample_rate_hz, 0.018));
    let qrs_smooth = moving_average(&qrs_centered, samples_for_seconds(sample_rate_hz, 0.018));
    let median_rr_ms = median(&rr_intervals_ms(r_peaks, sample_rate_hz));
    let mut pr_values = Vec::new();
    let mut qrs_values = Vec::new();

    for (beat_index, peak) in r_peaks.iter().copied().enumerate() {
        if let Some(median_rr_ms) = median_rr_ms {
            let Some(rr_seconds) = local_rr_seconds(r_peaks, beat_index, sample_rate_hz) else {
                continue;
            };
            if ((rr_seconds * 1_000.0) - median_rr_ms).abs() / median_rr_ms > 0.20 {
                continue;
            }
        }

        let Some(qrs_onset) = detect_qrs_onset(&qrs_smooth, peak, sample_rate_hz) else {
            continue;
        };
        if let Some(qrs_offset) = detect_qrs_offset(&qrs_smooth, peak, sample_rate_hz)
            && qrs_offset > qrs_onset
        {
            let qrs_ms = (qrs_offset - qrs_onset) as f64 * 1_000.0 / sample_rate_hz;
            if (50.0..=180.0).contains(&qrs_ms) {
                qrs_values.push(qrs_ms);
            }
        }

        let Some(pr_qrs_onset) = detect_qrs_onset(&pr_smooth, peak, sample_rate_hz) else {
            continue;
        };
        if let Some(p_onset) = detect_p_onset(&pr_smooth, pr_qrs_onset, sample_rate_hz) {
            let pr_ms = (pr_qrs_onset - p_onset) as f64 * 1_000.0 / sample_rate_hz;
            if (80.0..=260.0).contains(&pr_ms) {
                pr_values.push(pr_ms);
            }
        }
    }

    if pr_values.is_empty() && qrs_values.is_empty() {
        return None;
    }

    Some(PrQrsMeasurements {
        pr_ms: (!pr_values.is_empty()).then(|| mean(&pr_values)),
        qrs_ms: (!qrs_values.is_empty()).then(|| mean(&qrs_values)),
        pr_beat_count: pr_values.len(),
        qrs_beat_count: qrs_values.len(),
    })
}

pub fn calculate_qt_qtc(
    document: &EcgDocument,
    r_peaks: &[usize],
    sample_rate_hz: f64,
) -> Option<QtMeasurements> {
    if r_peaks.len() < 2 {
        return None;
    }

    let lead = select_qt_lead(document)?;
    let centered = remove_moving_average(
        &lead.samples_microvolts,
        samples_for_seconds(sample_rate_hz, 0.60),
    );
    let smooth = moving_average(&centered, samples_for_seconds(sample_rate_hz, 0.018));
    let median_rr_ms = median(&rr_intervals_ms(r_peaks, sample_rate_hz))?;
    let mut qt_values = Vec::new();
    let mut qtc_values = Vec::new();

    for (beat_index, peak) in r_peaks.iter().copied().enumerate() {
        let Some(rr_seconds) = local_rr_seconds(r_peaks, beat_index, sample_rate_hz) else {
            continue;
        };
        if ((rr_seconds * 1_000.0) - median_rr_ms).abs() / median_rr_ms > 0.20 {
            continue;
        }

        let Some(onset) = detect_qrs_onset(&smooth, peak, sample_rate_hz) else {
            continue;
        };
        let Some(t_offset) = detect_t_offset(
            &smooth,
            peak,
            r_peaks.get(beat_index + 1).copied(),
            sample_rate_hz,
        ) else {
            continue;
        };
        if t_offset <= onset {
            continue;
        }

        let qt_seconds = (t_offset - onset) as f64 / sample_rate_hz;
        if !(0.24..=0.60).contains(&qt_seconds) || rr_seconds <= 0.0 {
            continue;
        }

        qt_values.push(qt_seconds * 1_000.0);
        qtc_values.push((qt_seconds / rr_seconds.sqrt()) * 1_000.0);
    }

    if qt_values.is_empty() {
        return None;
    }

    Some(QtMeasurements {
        qt_ms: mean(&qt_values),
        qtc_bazett_ms: mean(&qtc_values),
        beat_count: qt_values.len(),
    })
}

pub fn calculate_qrs_axis(
    document: &EcgDocument,
    r_peaks: &[usize],
    sample_rate_hz: f64,
) -> Option<AxisMeasurements> {
    if r_peaks.is_empty() {
        return None;
    }

    let lead_i = find_lead_by_alias(document, &["I", "DI"])?;
    let lead_avf = find_lead_by_alias(document, &["AVF", "DAVF"])?;
    let mut net_i_values = Vec::new();
    let mut net_avf_values = Vec::new();

    for peak in r_peaks.iter().copied() {
        let Some(net_i) = positive_qrs_peak(&lead_i.samples_microvolts, peak, sample_rate_hz)
        else {
            continue;
        };
        let Some(net_avf) = positive_qrs_peak(&lead_avf.samples_microvolts, peak, sample_rate_hz)
        else {
            continue;
        };
        if (net_i.abs() + net_avf.abs()) < 15.0 {
            continue;
        }
        net_i_values.push(net_i);
        net_avf_values.push(net_avf);
    }

    if net_i_values.is_empty() || net_avf_values.is_empty() {
        return None;
    }

    let net_i = mean(&net_i_values);
    let net_avf = mean(&net_avf_values);
    if (net_i * net_i + net_avf * net_avf).sqrt() < 10.0 {
        return None;
    }

    let axis_deg = net_avf.atan2(net_i).to_degrees();
    Some(AxisMeasurements {
        axis_deg,
        label: qrs_axis_label(axis_deg).to_owned(),
        beat_count: net_i_values.len(),
    })
}

fn select_qrs_lead(document: &EcgDocument) -> Option<&LeadData> {
    find_lead_by_alias(document, &["II", "DII"])
        .or_else(|| find_lead_by_alias(document, &["I", "DI"]))
        .or_else(|| find_lead_by_alias(document, &["V5"]))
        .or_else(|| {
            document
                .leads
                .iter()
                .find(|lead| !lead.samples_microvolts.is_empty())
        })
}

fn select_qt_lead(document: &EcgDocument) -> Option<&LeadData> {
    find_lead_by_alias(document, &["AVF", "DAVF"])
        .or_else(|| find_lead_by_alias(document, &["II", "DII"]))
        .or_else(|| find_lead_by_alias(document, &["V5"]))
        .or_else(|| find_lead_by_alias(document, &["I", "DI"]))
}

fn select_pr_lead(document: &EcgDocument) -> Option<&LeadData> {
    find_lead_by_alias(document, &["V5"])
        .or_else(|| find_lead_by_alias(document, &["II", "DII"]))
        .or_else(|| find_lead_by_alias(document, &["I", "DI"]))
        .or_else(|| find_lead_by_alias(document, &["AVF", "DAVF"]))
}

fn select_interval_qrs_lead(document: &EcgDocument) -> Option<&LeadData> {
    find_lead_by_alias(document, &["AVF", "DAVF"])
        .or_else(|| find_lead_by_alias(document, &["II", "DII"]))
        .or_else(|| find_lead_by_alias(document, &["V5"]))
        .or_else(|| find_lead_by_alias(document, &["I", "DI"]))
}

fn find_lead_by_alias<'a>(document: &'a EcgDocument, aliases: &[&str]) -> Option<&'a LeadData> {
    document.leads.iter().find(|lead| {
        let normalized = normalize_lead_name(&lead.name);
        aliases
            .iter()
            .any(|alias| normalized == normalize_lead_name(alias))
            && !lead.samples_microvolts.is_empty()
    })
}

fn normalize_lead_name(name: &str) -> String {
    let mut normalized: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect();

    normalized = match normalized.as_str() {
        "DI" => "I".to_owned(),
        "DII" => "II".to_owned(),
        "DIII" => "III".to_owned(),
        "DAVR" => "AVR".to_owned(),
        "DAVL" => "AVL".to_owned(),
        "DAVF" => "AVF".to_owned(),
        _ => normalized,
    };
    normalized
}

fn qrs_bandpass_like(samples: &[f64], sample_rate_hz: f64) -> Vec<f64> {
    let centered = remove_moving_average(samples, samples_for_seconds(sample_rate_hz, 0.60));
    moving_average(&centered, samples_for_seconds(sample_rate_hz, 0.035))
}

fn qrs_energy(samples: &[f64], sample_rate_hz: f64) -> Vec<f64> {
    if samples.len() < 5 {
        return Vec::new();
    }

    let mut derivative = vec![0.0; samples.len()];
    for index in 2..(samples.len() - 2) {
        derivative[index] = (samples[index + 2] + (2.0 * samples[index + 1])
            - (2.0 * samples[index - 1])
            - samples[index - 2])
            / 8.0;
    }

    let squared: Vec<f64> = derivative.iter().map(|value| value * value).collect();
    moving_average(&squared, samples_for_seconds(sample_rate_hz, 0.12))
}

fn push_peak_with_refractory(
    peaks: &mut Vec<usize>,
    peak: usize,
    refractory: usize,
    filtered: &[f64],
) {
    if let Some(last_peak) = peaks.last_mut()
        && peak.saturating_sub(*last_peak) < refractory
    {
        if filtered.get(peak).copied().unwrap_or(0.0).abs()
            > filtered.get(*last_peak).copied().unwrap_or(0.0).abs()
        {
            *last_peak = peak;
        }
        return;
    }

    peaks.push(peak);
}

fn prune_low_amplitude_peaks(filtered: &[f64], peaks: &[usize], sample_rate_hz: f64) -> Vec<usize> {
    if peaks.is_empty() {
        return Vec::new();
    }

    let search_radius = samples_for_seconds(sample_rate_hz, 0.045);
    let amplitudes: Vec<f64> = peaks
        .iter()
        .copied()
        .map(|peak| {
            let start = peak.saturating_sub(search_radius);
            let end = (peak + search_radius).min(filtered.len().saturating_sub(1));
            max_abs_value(filtered, start, end, 0.0)
        })
        .collect();
    let Some(median_amplitude) = median(&amplitudes) else {
        return peaks.to_vec();
    };
    let minimum = (median_amplitude * 0.35).max(20.0);

    peaks
        .iter()
        .copied()
        .zip(amplitudes)
        .filter_map(|(peak, amplitude)| (amplitude >= minimum).then_some(peak))
        .collect()
}

fn rr_intervals_ms(r_peaks: &[usize], sample_rate_hz: f64) -> Vec<f64> {
    if sample_rate_hz <= 0.0 {
        return Vec::new();
    }
    r_peaks
        .windows(2)
        .map(|peaks| (peaks[1].saturating_sub(peaks[0]) as f64 / sample_rate_hz) * 1_000.0)
        .collect()
}

fn local_rr_seconds(r_peaks: &[usize], beat_index: usize, sample_rate_hz: f64) -> Option<f64> {
    if sample_rate_hz <= 0.0 {
        return None;
    }
    if beat_index > 0 && beat_index + 1 < r_peaks.len() {
        return Some(
            (r_peaks[beat_index + 1].saturating_sub(r_peaks[beat_index - 1]) as f64)
                / (2.0 * sample_rate_hz),
        );
    }
    if beat_index > 0 {
        return Some(
            (r_peaks[beat_index].saturating_sub(r_peaks[beat_index - 1]) as f64) / sample_rate_hz,
        );
    }
    if beat_index + 1 < r_peaks.len() {
        return Some(
            (r_peaks[beat_index + 1].saturating_sub(r_peaks[beat_index]) as f64) / sample_rate_hz,
        );
    }
    None
}

fn detect_qrs_onset(samples: &[f64], peak: usize, sample_rate_hz: f64) -> Option<usize> {
    let (baseline, noise) =
        qrs_baseline_and_noise(samples, peak, sample_rate_hz).or_else(|| {
            let baseline = baseline_before(samples, peak, sample_rate_hz)?;
            let noise = noise_before(samples, peak, sample_rate_hz, baseline).unwrap_or(0.0);
            Some((baseline, noise))
        })?;
    let qrs_start = peak.saturating_sub(samples_for_seconds(sample_rate_hz, 0.14));
    let qrs_end = (peak + samples_for_seconds(sample_rate_hz, 0.08)).min(samples.len() - 1);
    if qrs_end <= qrs_start {
        return None;
    }

    let qrs_amplitude = max_abs_value(samples, qrs_start, qrs_end, baseline);
    if qrs_amplitude < 50.0 {
        return None;
    }
    let threshold = (qrs_amplitude * 0.08).max(noise * 3.0).max(20.0);
    let quiet_samples = samples_for_seconds(sample_rate_hz, 0.012).max(2);

    (qrs_start + quiet_samples..peak).rev().find(|&index| {
        stable_around_baseline(
            samples,
            index.saturating_sub(quiet_samples),
            index,
            baseline,
            threshold,
        )
    })
}

fn detect_qrs_offset(samples: &[f64], peak: usize, sample_rate_hz: f64) -> Option<usize> {
    let (baseline, noise) =
        qrs_baseline_and_noise(samples, peak, sample_rate_hz).or_else(|| {
            let baseline = baseline_before(samples, peak, sample_rate_hz)?;
            let noise = noise_before(samples, peak, sample_rate_hz, baseline).unwrap_or(0.0);
            Some((baseline, noise))
        })?;
    let qrs_start = peak.saturating_sub(samples_for_seconds(sample_rate_hz, 0.06));
    let qrs_end = (peak + samples_for_seconds(sample_rate_hz, 0.14)).min(samples.len() - 1);
    if qrs_end <= qrs_start {
        return None;
    }

    let qrs_amplitude = max_abs_value(samples, qrs_start, qrs_end, baseline);
    if qrs_amplitude < 50.0 {
        return None;
    }
    let threshold = (qrs_amplitude * 0.08).max(noise * 3.0).max(20.0);
    let quiet_samples = samples_for_seconds(sample_rate_hz, 0.012).max(2);
    if qrs_end <= peak + quiet_samples {
        return None;
    }

    (peak..=(qrs_end - quiet_samples)).find(|&index| {
        stable_around_baseline(samples, index, index + quiet_samples, baseline, threshold)
    })
}

fn qrs_baseline_and_noise(samples: &[f64], peak: usize, sample_rate_hz: f64) -> Option<(f64, f64)> {
    baseline_and_noise_in_window(samples, peak, sample_rate_hz, 0.34, 0.26)
}

fn detect_p_onset(samples: &[f64], qrs_onset: usize, sample_rate_hz: f64) -> Option<usize> {
    let (baseline, noise) =
        p_wave_baseline_and_noise(samples, qrs_onset, sample_rate_hz).or_else(|| {
            let baseline = baseline_before(samples, qrs_onset, sample_rate_hz)?;
            let noise = noise_before(samples, qrs_onset, sample_rate_hz, baseline).unwrap_or(0.0);
            Some((baseline, noise))
        })?;
    let search_start = qrs_onset.checked_sub(samples_for_seconds(sample_rate_hz, 0.24))?;
    let search_end = qrs_onset.checked_sub(samples_for_seconds(sample_rate_hz, 0.055))?;
    if search_end <= search_start || search_end >= samples.len() {
        return None;
    }

    let p_peak = max_abs_index(samples, search_start, search_end, baseline)?;
    let p_amplitude = (samples[p_peak] - baseline).abs();
    if p_amplitude < (noise * 4.0).max(25.0) {
        return None;
    }

    let threshold = (p_amplitude * 0.12).max(noise * 3.0).max(15.0);
    let quiet_samples = samples_for_seconds(sample_rate_hz, 0.025).max(2);
    if p_peak <= search_start + quiet_samples {
        return None;
    }

    (search_start + quiet_samples..=p_peak)
        .rev()
        .find(|&index| {
            stable_around_baseline(
                samples,
                index.saturating_sub(quiet_samples),
                index,
                baseline,
                threshold,
            )
        })
}

fn p_wave_baseline_and_noise(
    samples: &[f64],
    qrs_onset: usize,
    sample_rate_hz: f64,
) -> Option<(f64, f64)> {
    baseline_and_noise_in_window(samples, qrs_onset, sample_rate_hz, 0.34, 0.26)
}

fn baseline_and_noise_in_window(
    samples: &[f64],
    anchor: usize,
    sample_rate_hz: f64,
    start_seconds_before: f64,
    end_seconds_before: f64,
) -> Option<(f64, f64)> {
    let start = anchor.checked_sub(samples_for_seconds(sample_rate_hz, start_seconds_before))?;
    let end = anchor.checked_sub(samples_for_seconds(sample_rate_hz, end_seconds_before))?;
    if end <= start || end > samples.len() {
        return None;
    }

    let baseline = median(&samples[start..end])?;
    let deviations: Vec<f64> = samples[start..end]
        .iter()
        .map(|sample| (sample - baseline).abs())
        .collect();
    let noise = median(&deviations).unwrap_or(0.0) * 1.4826;
    Some((baseline, noise))
}

fn detect_t_offset(
    samples: &[f64],
    peak: usize,
    next_peak: Option<usize>,
    sample_rate_hz: f64,
) -> Option<usize> {
    let (baseline, noise) =
        qrs_baseline_and_noise(samples, peak, sample_rate_hz).or_else(|| {
            let baseline = baseline_before(samples, peak, sample_rate_hz)?;
            let noise = noise_before(samples, peak, sample_rate_hz, baseline).unwrap_or(0.0);
            Some((baseline, noise))
        })?;
    let start = peak + samples_for_seconds(sample_rate_hz, 0.08);
    if start >= samples.len() {
        return None;
    }

    let next_limit = next_peak
        .map(|next| next.saturating_sub(samples_for_seconds(sample_rate_hz, 0.08)))
        .unwrap_or(samples.len() - 1);
    let end = (peak + samples_for_seconds(sample_rate_hz, 0.60))
        .min(next_limit)
        .min(samples.len() - 1);
    if end <= start + samples_for_seconds(sample_rate_hz, 0.08) {
        return None;
    }

    let t_search_start = (peak + samples_for_seconds(sample_rate_hz, 0.12)).min(end);
    let t_peak = max_abs_index(samples, t_search_start, end, baseline)?;
    let t_amplitude = (samples[t_peak] - baseline).abs();
    if t_amplitude < (noise * 4.0).max(35.0) {
        return None;
    }

    let threshold = (t_amplitude * 0.129).max(noise * 3.0).max(25.0);
    let quiet_samples = samples_for_seconds(sample_rate_hz, 0.03).max(2);
    if end <= t_peak + quiet_samples {
        return None;
    }

    (t_peak..=(end - quiet_samples)).find(|&index| {
        stable_around_baseline(samples, index, index + quiet_samples, baseline, threshold)
    })
}

fn baseline_before(samples: &[f64], peak: usize, sample_rate_hz: f64) -> Option<f64> {
    let start = peak.saturating_sub(samples_for_seconds(sample_rate_hz, 0.25));
    let end = peak.saturating_sub(samples_for_seconds(sample_rate_hz, 0.12));
    if end <= start || end > samples.len() {
        return None;
    }
    median(&samples[start..end])
}

fn noise_before(samples: &[f64], peak: usize, sample_rate_hz: f64, baseline: f64) -> Option<f64> {
    let start = peak.saturating_sub(samples_for_seconds(sample_rate_hz, 0.25));
    let end = peak.saturating_sub(samples_for_seconds(sample_rate_hz, 0.12));
    if end <= start || end > samples.len() {
        return None;
    }

    let deviations: Vec<f64> = samples[start..end]
        .iter()
        .map(|sample| (sample - baseline).abs())
        .collect();
    median(&deviations).map(|mad| mad * 1.4826)
}

fn positive_qrs_peak(samples: &[f64], peak: usize, sample_rate_hz: f64) -> Option<f64> {
    let qrs_start = peak.checked_sub(samples_for_seconds(sample_rate_hz, 0.05))?;
    let qrs_end = (peak + samples_for_seconds(sample_rate_hz, 0.07)).min(samples.len());
    let baseline_start = peak.checked_sub(samples_for_seconds(sample_rate_hz, 0.16))?;
    let baseline_end = peak.checked_sub(samples_for_seconds(sample_rate_hz, 0.08))?;
    if qrs_end <= qrs_start || baseline_end <= baseline_start || qrs_end > samples.len() {
        return None;
    }

    let baseline = median(&samples[baseline_start..baseline_end])?;
    samples[qrs_start..qrs_end]
        .iter()
        .map(|sample| sample - baseline)
        .reduce(f64::max)
}

fn qrs_axis_label(axis_deg: f64) -> &'static str {
    if (-30.0..=90.0).contains(&axis_deg) {
        "normal"
    } else if (-90.0..-30.0).contains(&axis_deg) || (axis_deg - -90.0).abs() < f64::EPSILON {
        "desvio esquerdo"
    } else if axis_deg > 90.0 && axis_deg <= 180.0 {
        "desvio direito"
    } else if (-180.0..-90.0).contains(&axis_deg) {
        "eixo extremo"
    } else {
        "indeterminado"
    }
}

fn qrs_axis_label_with_texts(label: &str, texts: MeasurementTexts) -> &'static str {
    match label {
        "normal" => texts.axis_normal,
        "desvio esquerdo" => texts.axis_left,
        "desvio direito" => texts.axis_right,
        "eixo extremo" => texts.axis_extreme,
        "indeterminado" => texts.axis_indeterminate,
        "indisponível" => texts.axis_unavailable,
        _ => texts.axis_indeterminate,
    }
}

fn moving_average(samples: &[f64], requested_window: usize) -> Vec<f64> {
    if samples.len() < 3 {
        return samples.to_vec();
    }

    let window = requested_window.max(1).min(samples.len());
    let radius = window / 2;
    let mut prefix = Vec::with_capacity(samples.len() + 1);
    prefix.push(0.0);
    for sample in samples {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + sample);
    }

    let mut averaged = Vec::with_capacity(samples.len());
    for index in 0..samples.len() {
        let begin = index.saturating_sub(radius);
        let end = (index + radius).min(samples.len() - 1);
        let count = (end - begin) + 1;
        averaged.push((prefix[end + 1] - prefix[begin]) / count as f64);
    }
    averaged
}

fn remove_moving_average(samples: &[f64], requested_window: usize) -> Vec<f64> {
    if samples.len() < 3 {
        return samples.to_vec();
    }

    let baseline = moving_average(samples, requested_window);
    samples
        .iter()
        .zip(baseline)
        .map(|(sample, baseline)| sample - baseline)
        .collect()
}

fn stable_around_baseline(
    samples: &[f64],
    start: usize,
    end: usize,
    baseline: f64,
    threshold: f64,
) -> bool {
    if end <= start || end > samples.len() {
        return false;
    }
    samples[start..end]
        .iter()
        .all(|sample| (sample - baseline).abs() <= threshold)
}

fn max_abs_index(samples: &[f64], start: usize, end: usize, baseline: f64) -> Option<usize> {
    if samples.is_empty() || start > end || end >= samples.len() {
        return None;
    }

    let mut best_index = start;
    let mut best_value = (samples[start] - baseline).abs();
    for (index, sample) in samples.iter().enumerate().take(end + 1).skip(start + 1) {
        let value = (sample - baseline).abs();
        if value > best_value {
            best_index = index;
            best_value = value;
        }
    }
    Some(best_index)
}

fn max_abs_value(samples: &[f64], start: usize, end: usize, baseline: f64) -> f64 {
    max_abs_index(samples, start, end, baseline)
        .map(|index| (samples[index] - baseline).abs())
        .unwrap_or(0.0)
}

fn finite_values(values: &[f64]) -> Vec<f64> {
    values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect()
}

fn median(values: &[f64]) -> Option<f64> {
    let mut values = finite_values(values);
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    Some(percentile_sorted(&values, 0.50))
}

fn percentile_sorted(sorted: &[f64], percentile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = percentile.clamp(0.0, 1.0) * (sorted.len().saturating_sub(1)) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let fraction = rank - lower as f64;
        sorted[lower] + ((sorted[upper] - sorted[lower]) * fraction)
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn coefficient_of_variation(values: &[f64], mean: f64) -> f64 {
    if values.len() < 2 || mean <= 0.0 {
        return 0.0;
    }

    let variance = values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f64>()
        / values.len() as f64;
    variance.sqrt() / mean
}

fn samples_for_seconds(sample_rate_hz: f64, seconds: f64) -> usize {
    (sample_rate_hz * seconds).round().max(1.0) as usize
}

fn optional_round(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.0}"))
        .unwrap_or_else(|| "--".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DocumentKind, LeadData};

    #[test]
    fn calculates_heart_rate_from_rr_intervals() {
        let peaks = vec![250, 750, 1_250, 1_750];
        let heart_rate = calculate_heart_rate(&peaks, 500.0).expect("heart rate");

        assert!((heart_rate.heart_rate_bpm - 60.0).abs() < 0.1);
        assert_eq!(heart_rate.rhythm_regularity, RhythmRegularity::Regular);
    }

    #[test]
    fn detects_qrs_peaks_in_synthetic_signal() {
        let sample_rate = 500.0;
        let samples = synthetic_lead(sample_rate, 6.0, 1_000.0, 250.0);

        let peaks = detect_qrs_peaks(&samples, sample_rate);

        assert_eq!(peaks.len(), 5);
        assert!((peaks[0] as i64 - 500).abs() <= 8);
    }

    #[test]
    fn analyzes_basic_measurements_on_synthetic_twelve_lead_signal() {
        let document = synthetic_document();

        let measurements = analyze_ecg(&document);

        assert!((measurements.heart_rate_bpm.unwrap() - 60.0).abs() < 2.0);
        assert!(matches!(measurements.pr_ms, Some(value) if (120.0..=240.0).contains(&value)));
        assert!(matches!(measurements.qrs_ms, Some(value) if (60.0..=140.0).contains(&value)));
        assert!(measurements.qt_ms.is_some());
        assert!(measurements.qtc_bazett_ms.is_some());
        assert!((measurements.qrs_axis_deg.unwrap() - 45.0).abs() < 8.0);
        assert_eq!(measurements.qrs_axis_label, "normal");
    }

    #[test]
    fn returns_null_measurements_for_flat_signal() {
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.leads.push(LeadData::new("II", vec![0.0; 2_500]));
        document.leads.push(LeadData::new("I", vec![0.0; 2_500]));
        document.leads.push(LeadData::new("aVF", vec![0.0; 2_500]));

        let measurements = analyze_ecg(&document);

        assert!(measurements.heart_rate_bpm.is_none());
        assert!(measurements.pr_ms.is_none());
        assert!(measurements.qrs_ms.is_none());
        assert!(measurements.qt_ms.is_none());
        assert!(measurements.qrs_axis_deg.is_none());
        assert!(!measurements.quality_warnings.is_empty());
    }

    fn synthetic_document() -> EcgDocument {
        let sample_rate = 500.0;
        let mut document = EcgDocument::new(DocumentKind::Xml, Default::default());
        document.sample_interval_seconds = 1.0 / sample_rate;
        document.leads.push(LeadData::new(
            "I",
            synthetic_lead(sample_rate, 8.0, 700.0, 210.0),
        ));
        document.leads.push(LeadData::new(
            "II",
            synthetic_lead(sample_rate, 8.0, 1_000.0, 260.0),
        ));
        document.leads.push(LeadData::new(
            "aVF",
            synthetic_lead(sample_rate, 8.0, 700.0, 210.0),
        ));
        document
    }

    fn synthetic_lead(
        sample_rate: f64,
        duration_seconds: f64,
        qrs_amplitude: f64,
        t_amplitude: f64,
    ) -> Vec<f64> {
        let sample_count = (sample_rate * duration_seconds).round() as usize;
        let mut samples = vec![0.0; sample_count];
        let first_beat = 1.0;
        let beat_count = duration_seconds.floor() as usize - 1;

        for beat in 0..beat_count {
            let r_time = first_beat + beat as f64;
            add_gaussian(
                &mut samples,
                sample_rate,
                r_time - 0.160,
                0.12 * qrs_amplitude,
                0.035,
            );
            add_gaussian(
                &mut samples,
                sample_rate,
                r_time - 0.025,
                -0.20 * qrs_amplitude,
                0.010,
            );
            add_gaussian(&mut samples, sample_rate, r_time, qrs_amplitude, 0.014);
            add_gaussian(
                &mut samples,
                sample_rate,
                r_time + 0.030,
                -0.25 * qrs_amplitude,
                0.012,
            );
            add_gaussian(
                &mut samples,
                sample_rate,
                r_time + 0.310,
                t_amplitude,
                0.060,
            );
        }

        samples
    }

    fn add_gaussian(
        samples: &mut [f64],
        sample_rate: f64,
        center_seconds: f64,
        amplitude: f64,
        sigma_seconds: f64,
    ) {
        let center = (center_seconds * sample_rate).round() as isize;
        let radius = (sigma_seconds * sample_rate * 4.0).round() as isize;
        for offset in -radius..=radius {
            let index = center + offset;
            if index < 0 || index as usize >= samples.len() {
                continue;
            }
            let time_offset = offset as f64 / sample_rate;
            let exponent = -0.5 * (time_offset / sigma_seconds).powi(2);
            samples[index as usize] += amplitude * exponent.exp();
        }
    }
}
