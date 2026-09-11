use crate::domain::{EcgDocument, LeadData};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterMode {
    None,
    Baseline,
    LowPass40Hz,
    Diagnostic,
    Monitor,
}

impl FilterMode {
    pub fn from_key(key: &str) -> Self {
        match key {
            "baseline" => Self::Baseline,
            "low_pass_40" => Self::LowPass40Hz,
            "diagnostic" => Self::Diagnostic,
            "monitor" => Self::Monitor,
            _ => Self::None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Baseline => "baseline",
            Self::LowPass40Hz => "low_pass_40",
            Self::Diagnostic => "diagnostic",
            Self::Monitor => "monitor",
        }
    }
}

pub fn apply_filter(source: &EcgDocument, mode: FilterMode) -> EcgDocument {
    if mode == FilterMode::None || source.leads.is_empty() {
        return source.clone();
    }

    let sample_rate = source.sample_rate_hz();
    let mut filtered = source.clone();
    filtered.leads = source
        .leads
        .iter()
        .map(|lead| LeadData {
            name: lead.name.clone(),
            samples_microvolts: apply_samples_filter(&lead.samples_microvolts, sample_rate, mode),
        })
        .collect();
    filtered
}

fn apply_samples_filter(samples: &[f64], sample_rate: f64, mode: FilterMode) -> Vec<f64> {
    match mode {
        FilterMode::None => samples.to_vec(),
        FilterMode::Baseline => remove_baseline_wander(samples, sample_rate),
        FilterMode::LowPass40Hz => one_pole_low_pass(samples, sample_rate, 40.0),
        FilterMode::Diagnostic => one_pole_low_pass(
            &remove_baseline_wander(samples, sample_rate),
            sample_rate,
            40.0,
        ),
        FilterMode::Monitor => notch_60hz(
            &one_pole_low_pass(
                &remove_baseline_wander(samples, sample_rate),
                sample_rate,
                40.0,
            ),
            sample_rate,
        ),
    }
}

fn remove_baseline_wander(samples: &[f64], sample_rate: f64) -> Vec<f64> {
    if samples.len() < 3 {
        return samples.to_vec();
    }

    let window = clamp_window((sample_rate * 0.6).round() as usize, samples.len());
    let radius = window / 2;
    let mut prefix = Vec::with_capacity(samples.len() + 1);
    prefix.push(0.0);
    for sample in samples {
        prefix.push(prefix.last().copied().unwrap_or(0.0) + sample);
    }

    let mut filtered = Vec::with_capacity(samples.len());
    for index in 0..samples.len() {
        let begin = index.saturating_sub(radius);
        let end = (index + radius).min(samples.len() - 1);
        let count = (end - begin) + 1;
        let average = (prefix[end + 1] - prefix[begin]) / count as f64;
        filtered.push(samples[index] - average);
    }
    filtered
}

fn clamp_window(requested: usize, size: usize) -> usize {
    if size <= 1 {
        return 1;
    }

    let mut window = requested.max(1).min(size);
    if window.is_multiple_of(2) {
        window -= 1;
    }
    window.max(1)
}

fn one_pole_low_pass(samples: &[f64], sample_rate: f64, cutoff_hz: f64) -> Vec<f64> {
    if samples.len() < 3 || sample_rate <= 0.0 || cutoff_hz <= 0.0 {
        return samples.to_vec();
    }

    let alpha = (-2.0 * std::f64::consts::PI * cutoff_hz / sample_rate).exp();
    let mut forward = vec![0.0; samples.len()];
    let mut backward = vec![0.0; samples.len()];

    forward[0] = samples[0];
    for index in 1..samples.len() {
        forward[index] = ((1.0 - alpha) * samples[index]) + (alpha * forward[index - 1]);
    }

    let last = samples.len() - 1;
    backward[last] = forward[last];
    for index in (1..samples.len()).rev() {
        backward[index - 1] = ((1.0 - alpha) * forward[index - 1]) + (alpha * backward[index]);
    }

    backward
}

fn notch_60hz(samples: &[f64], sample_rate: f64) -> Vec<f64> {
    if samples.len() < 3 || sample_rate < 150.0 {
        return samples.to_vec();
    }

    let omega = 2.0 * std::f64::consts::PI * 60.0 / sample_rate;
    let alpha = omega.sin() / 40.0;
    let cos_omega = omega.cos();

    let b0 = 1.0;
    let b1 = -2.0 * cos_omega;
    let b2 = 1.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_omega;
    let a2 = 1.0 - alpha;

    let mut output = Vec::with_capacity(samples.len());
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    for &x0 in samples {
        let y0 = ((b0 / a0) * x0) + ((b1 / a0) * x1) + ((b2 / a0) * x2)
            - ((a1 / a0) * y1)
            - ((a2 / a0) * y2);
        output.push(y0);
        x2 = x1;
        x1 = x0;
        y2 = y1;
        y1 = y0;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_filter_keeps_length() {
        let samples = vec![1.0, 2.0, 4.0, 8.0, 16.0];
        assert_eq!(remove_baseline_wander(&samples, 500.0).len(), samples.len());
    }

    #[test]
    fn no_filter_copies_samples() {
        let samples = vec![10.0, -5.0, 2.0];
        assert_eq!(
            apply_samples_filter(&samples, 500.0, FilterMode::None),
            samples
        );
    }
}
