//! Audio-driven native waveform state, independent of capture and rendering.
//!
//! Input is 16 kHz mono PCM: one 640-sample (40 ms) block per `analyze` call.
//! `speaking` is an estimate from level, noise floor and spectral distribution;
//! it is not semantic VAD and must not decide transcript/sentence boundaries.
//! `utterance` only supplies a visual confirmation pulse from the caller.

use std::f32::consts::TAU;

pub const BAND_COUNT: usize = 22;
const FFT_SIZE: usize = 512;
const BINS: usize = FFT_SIZE / 2 + 1;
const BLOCK_SIZE: usize = 640;
const SAMPLE_RATE: f32 = 16_000.0;
const FRAME_MS: u64 = 40;
const MIN_FREQUENCY: f32 = 80.0;
const MAX_FREQUENCY: f32 = 7_600.0;
const ATTACK: f32 = 0.060;
const RELEASE: f32 = 0.240;
const REST_WIDTH: f32 = 0.35;

/// Scalar levels and bands are normalized to 0..=1; `rms` is linear PCM RMS.
/// Bands have ascending logarithmic centres, approximately 98 Hz..6.24 kHz.
/// `pause_ms` counts consecutive blocks without estimated voice energy.
#[derive(Clone, Debug, Default)]
pub struct AudioFrame {
    pub rms: f32,
    pub bands: Vec<f32>,
    pub loudness: f32,
    pub speaking: bool,
    pub pause_ms: u64,
}

pub struct Analyzer {
    window: [f32; FFT_SIZE],
    weights: [[f32; BINS]; BAND_COUNT],
    power_scale: f32,
    noise_db: f32,
    speaking: bool,
    pause_ms: u64,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyzer {
    pub fn new() -> Self {
        let mut window = [0.0; FFT_SIZE];
        for (index, value) in window.iter_mut().enumerate() {
            *value = 0.5 - 0.5 * (TAU * index as f32 / (FFT_SIZE - 1) as f32).cos();
        }
        let window_power: f32 = window.iter().map(|value| value * value).sum();
        let mut weights = [[0.0; BINS]; BAND_COUNT];
        let log_min = MIN_FREQUENCY.ln();
        let log_step = (MAX_FREQUENCY / MIN_FREQUENCY).ln() / (BAND_COUNT + 1) as f32;
        for (band, bins) in weights.iter_mut().enumerate() {
            let centre = (band + 1) as f32;
            for (bin, weight) in bins.iter_mut().enumerate().skip(1) {
                let frequency = bin as f32 * SAMPLE_RATE / FFT_SIZE as f32;
                let log_position = (frequency.ln() - log_min) / log_step;
                *weight = (1.0 - (log_position - centre).abs()).max(0.0);
            }
        }
        Self {
            window,
            weights,
            // One-sided Parseval power, corrected for Hann window energy.
            power_scale: 2.0 / (FFT_SIZE as f32 * window_power),
            noise_db: -74.0,
            speaking: false,
            pause_ms: 0,
        }
    }

    pub fn analyze(&mut self, samples: &[i16]) -> AudioFrame {
        // Expected input is exactly one block. Defensive short input is padded;
        // oversize input uses the latest block, with no indexing panics.
        let count = samples.len().min(BLOCK_SIZE);
        let input = &samples[samples.len() - count..];
        let mut block = [0.0_f32; BLOCK_SIZE];
        let mean = if count == 0 {
            0.0
        } else {
            input
                .iter()
                .map(|sample| *sample as f32 / 32_768.0)
                .sum::<f32>()
                / count as f32
        };
        let mut square_sum = 0.0;
        for (out, sample) in block.iter_mut().zip(input) {
            *out = *sample as f32 / 32_768.0 - mean;
            square_sum += *out * *out;
        }
        let rms = if count == 0 {
            0.0
        } else {
            unit((square_sum / count as f32).sqrt())
        };
        let mut power = [0.0_f32; BINS];
        // Overlapping windows at 0 and 128 use all 640 samples, including the
        // tail which a single FFT would discard. No fake/random band motion.
        for offset in [0, BLOCK_SIZE - FFT_SIZE] {
            let mut real = [0.0_f32; FFT_SIZE];
            let mut imaginary = [0.0_f32; FFT_SIZE];
            for index in 0..FFT_SIZE {
                real[index] = block[offset + index] * self.window[index];
            }
            fft(&mut real, &mut imaginary);
            for bin in 1..BINS {
                let nyquist = if bin == FFT_SIZE / 2 { 0.5 } else { 1.0 };
                power[bin] += (real[bin] * real[bin] + imaginary[bin] * imaginary[bin])
                    * self.power_scale
                    * 0.5
                    * nyquist;
            }
        }
        let total_power = power.iter().sum::<f32>();
        let mut voice_power = 0.0;
        let mut log_power = 0.0;
        let mut voice_bins = 0;
        for (bin, value) in power.iter().enumerate().skip(1) {
            let frequency = bin as f32 * SAMPLE_RATE / FFT_SIZE as f32;
            if (100.0..=4_200.0).contains(&frequency) {
                voice_power += *value;
                log_power += value.max(1.0e-20).ln();
                voice_bins += 1;
            }
        }
        let voice_db = power_db(voice_power);
        let voice_fraction = voice_power / total_power.max(1.0e-20);
        let flatness =
            (log_power / voice_bins as f32).exp() / (voice_power / voice_bins as f32).max(1.0e-20);
        // Absolute floor prevents quantization noise from becoming "speech"
        // after a long quiet period. SNR hysteresis avoids gate chatter.
        let threshold = if self.speaking {
            (-70.0_f32).max(self.noise_db + 6.0)
        } else {
            (-66.0_f32).max(self.noise_db + 10.0)
        };
        let minimum_voice_fraction = if self.speaking { 0.50 } else { 0.58 };
        let spectral_gate = voice_fraction >= minimum_voice_fraction && flatness < 0.50;
        self.speaking = rms > 0.0 && voice_db > threshold && spectral_gate;
        if !self.speaking {
            // Follow background rises in ~0.5 s and quieter floors in ~0.3 s.
            // Freeze during estimated voice so sustained dictation is not
            // progressively learned as background and attenuated away.
            let observed = voice_db.clamp(-90.0, -20.0);
            let rate = if observed < self.noise_db { 0.12 } else { 0.08 };
            self.noise_db += (observed - self.noise_db) * rate;
        }
        self.pause_ms = if self.speaking {
            0
        } else {
            self.pause_ms.saturating_add(FRAME_MS)
        };
        let confidence = unit((voice_db - self.noise_db - 5.0) / 10.0);
        let loudness = if self.speaking {
            unit((voice_db + 66.0) / 54.0).powf(0.85) * confidence
        } else {
            0.0
        };
        let mut bands = vec![0.0; BAND_COUNT];
        if self.speaking {
            for (level, weights) in bands.iter_mut().zip(&self.weights) {
                let energy = power.iter().zip(weights).map(|(p, w)| p * w).sum::<f32>();
                // Absolute dB mapping preserves loud/quiet contrast; do not
                // normalize each spectrum by its own maximum.
                *level = unit((power_db(energy) + 78.0) / 60.0).powf(0.85) * confidence;
            }
        }
        AudioFrame {
            rms,
            bands,
            loudness,
            speaking: self.speaking,
            pause_ms: self.pause_ms,
        }
    }
}

/// Rendering values only: amplitude/bands/pause/pulse are 0..=1, width is
/// 0.35..=1, and phase is radians in 0..TAU. A renderer supplies its own style.
#[derive(Clone, Debug)]
pub struct VisualFrame {
    pub amplitude: f32,
    pub width: f32,
    pub bands: Vec<f32>,
    pub phase: f32,
    pub pause: f32,
    pub pulse: f32,
}

pub struct Motion {
    amplitude: f32,
    width: f32,
    bands: [f32; BAND_COUNT],
    phase: f32,
    phase_speed: f32,
    silence_seconds: f32,
    pause_anchor: f32,
    voiced: bool,
    was_speaking: bool,
    pulse: f32,
    utterance: Option<u64>,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            amplitude: 0.0,
            width: REST_WIDTH,
            bands: [0.0; BAND_COUNT],
            phase: 0.0,
            phase_speed: 0.0,
            silence_seconds: 1.0,
            pause_anchor: REST_WIDTH,
            voiced: false,
            was_speaking: false,
            pulse: 0.0,
            utterance: None,
        }
    }
}

impl Motion {
    /// `dt` is elapsed seconds; call once per render tick, reusing the latest
    /// AudioFrame. Invalid dt does not advance time; stalls are capped at 250 ms.
    /// `active` means capturing/processing; `reduced` freezes continuous phase.
    /// A changed `utterance` produces a ~270 ms settle pulse. The first count
    /// establishes a baseline, including when attaching to an existing session.
    pub fn advance(
        &mut self,
        frame: &AudioFrame,
        dt: f32,
        active: bool,
        reduced: bool,
        utterance: u64,
    ) -> VisualFrame {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.25)
        } else {
            0.0
        };
        let speaking = active && frame.speaking;
        let loudness = if speaking { unit(frame.loudness) } else { 0.0 };
        if speaking {
            self.silence_seconds = 0.0;
            self.voiced = true;
        } else {
            if self.was_speaking {
                self.pause_anchor = self.width;
            }
            self.silence_seconds = (self.silence_seconds + dt).min(60.0);
            if active {
                // Audio updates are slower than render ticks; the local clock
                // also settles correctly if no new audio frame is published.
                self.silence_seconds = self
                    .silence_seconds
                    .max(frame.pause_ms.min(60_000) as f32 / 1_000.0);
            }
        }
        let pause = if speaking {
            0.0
        } else {
            smoothstep((self.silence_seconds - 0.30) / 0.70)
        };
        self.amplitude = envelope(self.amplitude, loudness, dt);
        for (index, level) in self.bands.iter_mut().enumerate() {
            let target = if speaking {
                frame.bands.get(index).copied().map(unit).unwrap_or(0.0)
            } else {
                0.0
            };
            *level = envelope(*level, target, dt);
        }
        if speaking {
            let target = REST_WIDTH + (1.0 - REST_WIDTH) * loudness.sqrt();
            self.width = envelope(self.width, target, dt);
        } else if active && self.voiced {
            // A 300 ms hold spans inter-syllable gaps. The explicit smoothstep
            // reaches rest at 1 s without a lingering low-level idle wobble.
            self.width = REST_WIDTH + (self.pause_anchor - REST_WIDTH) * (1.0 - pause);
        } else {
            self.width = envelope(self.width, REST_WIDTH, dt);
        }
        if !speaking && self.silence_seconds >= 1.0 {
            self.amplitude = 0.0;
            self.bands.fill(0.0);
            self.phase_speed = 0.0;
        }
        if reduced {
            self.phase_speed = 0.0;
        } else {
            let target = if speaking { 1.5 + loudness * 5.0 } else { 0.0 };
            self.phase_speed = envelope(self.phase_speed, target, dt);
            self.phase = (self.phase + self.phase_speed * dt).rem_euclid(TAU);
        }
        self.pulse *= (-dt / 0.090).exp();
        if active && self.utterance.is_some_and(|previous| previous != utterance) {
            self.pulse = 1.0;
        }
        self.utterance = Some(utterance);
        self.was_speaking = speaking;
        if self.pulse < 0.001 {
            self.pulse = 0.0;
        }
        if self.width - REST_WIDTH < 0.0001 {
            self.width = REST_WIDTH;
        }
        VisualFrame {
            amplitude: unit(self.amplitude),
            width: self.width.clamp(REST_WIDTH, 1.0),
            bands: self.bands.iter().copied().map(unit).collect(),
            phase: self.phase,
            pause: unit(pause),
            pulse: unit(self.pulse),
        }
    }
}

fn unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn power_db(power: f32) -> f32 {
    10.0 * power.max(1.0e-12).log10()
}

fn smoothstep(value: f32) -> f32 {
    let value = unit(value);
    value * value * (3.0 - 2.0 * value)
}

fn envelope(current: f32, target: f32, dt: f32) -> f32 {
    let time = if target > current { ATTACK } else { RELEASE };
    current + (target - current) * (1.0 - (-dt / time).exp())
}

/// Iterative in-place radix-2 FFT; no allocation or external DSP dependencies.
fn fft(real: &mut [f32; FFT_SIZE], imaginary: &mut [f32; FFT_SIZE]) {
    let mut reversed = 0;
    for index in 1..FFT_SIZE {
        let mut bit = FFT_SIZE >> 1;
        while reversed & bit != 0 {
            reversed ^= bit;
            bit >>= 1;
        }
        reversed ^= bit;
        if index < reversed {
            real.swap(index, reversed);
            imaginary.swap(index, reversed);
        }
    }
    let mut length = 2;
    while length <= FFT_SIZE {
        let angle = -TAU / length as f32;
        let (step_imaginary, step_real) = angle.sin_cos();
        for start in (0..FFT_SIZE).step_by(length) {
            let (mut rotation_real, mut rotation_imaginary) = (1.0, 0.0);
            for offset in 0..length / 2 {
                let first = start + offset;
                let second = first + length / 2;
                let product_real =
                    real[second] * rotation_real - imaginary[second] * rotation_imaginary;
                let product_imaginary =
                    real[second] * rotation_imaginary + imaginary[second] * rotation_real;
                real[second] = real[first] - product_real;
                imaginary[second] = imaginary[first] - product_imaginary;
                real[first] += product_real;
                imaginary[first] += product_imaginary;
                let next_real = rotation_real * step_real - rotation_imaginary * step_imaginary;
                rotation_imaginary =
                    rotation_real * step_imaginary + rotation_imaginary * step_real;
                rotation_real = next_real;
            }
        }
        length *= 2;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frequency: f32, amplitude: f32) -> Vec<i16> {
        (0..BLOCK_SIZE)
            .map(|index| {
                ((TAU * frequency * index as f32 / SAMPLE_RATE).sin() * amplitude * 32_767.0) as i16
            })
            .collect()
    }

    fn voiced(level: f32) -> AudioFrame {
        AudioFrame {
            rms: level * 0.1,
            bands: vec![level; BAND_COUNT],
            loudness: level,
            speaking: true,
            pause_ms: 0,
        }
    }

    fn peak(frame: &AudioFrame) -> usize {
        frame
            .bands
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0
    }

    #[test]
    fn silence_hum_and_settled_broadband_noise_do_not_keep_animation_expanded() {
        let mut analyzer = Analyzer::new();
        let mut motion = Motion::default();
        for _ in 0..100 {
            let frame = analyzer.analyze(&[0; BLOCK_SIZE]);
            assert_eq!(frame.rms, 0.0);
            assert!(!frame.speaking);
            assert_eq!(frame.loudness, 0.0);
            assert!(frame.bands.iter().all(|level| *level == 0.0));
            let visual = motion.advance(&frame, 0.04, true, false, 0);
            assert_eq!(visual.amplitude, 0.0);
            assert_eq!(visual.width, REST_WIDTH);
            assert_eq!(visual.phase, 0.0);
        }
        let hum = analyzer.analyze(&tone(50.0, 0.2));
        assert!(hum.rms > 0.1);
        assert!(!hum.speaking, "low-frequency hum is not voice-range energy");
        let mut seed = 0x1234_u32;
        for index in 0..125 {
            let noise: Vec<i16> = (0..BLOCK_SIZE)
                .map(|_| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    (((seed >> 16) as i32 - 32_768) / 8) as i16
                })
                .collect();
            let frame = analyzer.analyze(&noise);
            if index >= 75 {
                assert!(!frame.speaking, "steady background must settle");
                assert_eq!(frame.loudness, 0.0);
            }
        }
    }

    #[test]
    fn louder_same_frequency_has_larger_rms_loudness_and_band_response() {
        let quiet = Analyzer::new().analyze(&tone(1_000.0, 0.012));
        let loud = Analyzer::new().analyze(&tone(1_000.0, 0.20));
        assert!(quiet.speaking && loud.speaking);
        assert!(quiet.loudness > 0.15, "soft input must still move");
        assert!(loud.rms > quiet.rms * 10.0);
        assert!(loud.loudness > quiet.loudness + 0.25);
        assert!(loud.bands[12] > quiet.bands[12] + 0.25);
    }

    #[test]
    fn fft_places_known_tones_in_their_logarithmic_bands() {
        // The specified 22 triangles have centres 1049.4 and 2824.1 Hz at
        // indices 12 and 17. These are independent known frequency fixtures.
        let low = Analyzer::new().analyze(&tone(1_000.0, 0.15));
        let high = Analyzer::new().analyze(&tone(3_000.0, 0.15));
        assert_eq!(low.bands.len(), 22);
        assert_eq!(peak(&low), 12);
        assert_eq!(peak(&high), 17);
        assert!(low.bands[12] > low.bands[17] + 0.4);
        assert!(high.bands[17] > high.bands[12] + 0.4);
    }

    #[test]
    fn pause_holds_then_contracts_and_resume_expands_with_a_settle_pulse() {
        let mut motion = Motion::default();
        let speech = voiced(0.8);
        let mut visual = motion.advance(&speech, 0.04, true, false, 0);
        assert!(
            visual.amplitude > 0.0 && visual.amplitude < 0.8,
            "onset is smoothed"
        );
        for _ in 0..49 {
            visual = motion.advance(&speech, 0.04, true, false, 0);
        }
        let expanded = visual.width;
        assert!(expanded > 0.9);
        assert_eq!(visual.pulse, 0.0);
        for index in 1..=25 {
            let silent = AudioFrame {
                pause_ms: index * 40,
                ..AudioFrame::default()
            };
            visual = motion.advance(&silent, 0.04, true, false, 0);
            if index <= 7 {
                assert!((visual.width - expanded).abs() < 0.001);
            }
            if index == 15 {
                assert!(visual.width < expanded - 0.15);
            }
        }
        assert_eq!(visual.width, REST_WIDTH);
        assert_eq!(visual.amplitude, 0.0);
        assert_eq!(visual.pause, 1.0);
        let stationary = visual.phase;
        visual = motion.advance(
            &AudioFrame {
                pause_ms: 1_200,
                ..AudioFrame::default()
            },
            0.04,
            true,
            false,
            0,
        );
        assert_eq!(visual.phase, stationary);
        visual = motion.advance(&speech, 0.04, true, false, 1);
        assert!(visual.width > 0.60 && visual.width < expanded);
        assert!(visual.amplitude > 0.30);
        assert_eq!(visual.pulse, 1.0);
        for _ in 0..10 {
            visual = motion.advance(&speech, 0.04, true, false, 1);
        }
        assert!(visual.width > 0.90);
        assert!(visual.pulse < 0.02);
        for _ in 0..50 {
            visual = motion.advance(&speech, 0.04, false, false, 2);
        }
        assert!(visual.width < 0.351);
        assert_eq!(visual.amplitude, 0.0);
        assert!(visual.bands.iter().all(|level| *level == 0.0));
        assert_eq!(visual.pulse, 0.0);
    }

    #[test]
    fn reduced_motion_freezes_phase_but_keeps_feedback_and_sanitizes_values() {
        let mut motion = Motion::default();
        let mut visual = motion.advance(&voiced(0.2), 0.04, true, false, 7);
        for _ in 0..10 {
            visual = motion.advance(&voiced(0.2), 0.04, true, false, 7);
        }
        assert!(visual.phase > 0.0);
        let phase = visual.phase;
        let width = visual.width;
        for _ in 0..10 {
            visual = motion.advance(&voiced(0.95), 0.04, true, true, 7);
        }
        assert_eq!(visual.phase, phase);
        assert!(visual.amplitude > 0.9);
        assert!(visual.width > width + 0.2);
        let invalid = AudioFrame {
            rms: f32::INFINITY,
            bands: vec![f32::NAN, f32::INFINITY, -5.0, 12.0],
            loudness: f32::NAN,
            speaking: true,
            pause_ms: u64::MAX,
        };
        for dt in [f32::NAN, f32::INFINITY, -1.0, 0.04, f32::MAX] {
            visual = motion.advance(&invalid, dt, true, true, 8);
            assert!((0.0..=1.0).contains(&visual.amplitude));
            assert!((REST_WIDTH..=1.0).contains(&visual.width));
            assert!((0.0..TAU).contains(&visual.phase));
            assert_eq!(visual.phase, phase);
            assert!((0.0..=1.0).contains(&visual.pause));
            assert!((0.0..=1.0).contains(&visual.pulse));
            assert_eq!(visual.bands.len(), BAND_COUNT);
            assert!(visual.bands.iter().all(|level| (0.0..=1.0).contains(level)));
        }
    }
}
