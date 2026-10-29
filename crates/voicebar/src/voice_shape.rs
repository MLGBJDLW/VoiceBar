//! Shared native path geometry, also used by the offline visual verification.
use crate::voice_visual::VisualFrame;
use std::f32::consts::{PI, TAU};

pub struct Stroke {
    pub points: Vec<(f32, f32)>,
    pub width: f32,
    pub alpha: f32,
}

fn band(frame: &VisualFrame, t: f32) -> f32 {
    if frame.bands.is_empty() {
        return 0.0;
    }
    let x = t.clamp(0.0, 1.0) * (frame.bands.len() - 1) as f32;
    let i = x as usize;
    let a = frame.bands[i];
    let b = frame.bands[(i + 1).min(frame.bands.len() - 1)];
    a + (b - a) * x.fract()
}

/// All coordinates remain inside the requested canvas, including stroke width.
/// At rest there is no autonomous phase or artificial minimum audio energy.
pub fn paths(
    effect: &str,
    frame: &VisualFrame,
    width: f32,
    height: f32,
    intensity: f32,
    combine: bool,
) -> Vec<Stroke> {
    let mut paths = Vec::new();
    if width < 8.0 || height < 8.0 {
        return paths;
    }
    let intensity = if intensity.is_finite() {
        intensity.clamp(0.0, 3.0)
    } else {
        1.0
    };
    let amplitude = (frame.amplitude * intensity).clamp(0.0, 1.0);
    let span = (width - 8.0) * frame.width;
    let left = (width - span) * 0.5;
    let middle = height * 0.5;
    let pulse = frame.pulse * if intensity > 0.0 { 1.0 } else { 0.0 };
    let alpha = 0.62 - frame.pause * 0.10 + amplitude * 0.30 + pulse * 0.08;
    if effect == "spectrum" || effect == "bars" {
        let count = if effect == "bars" { 29 } else { 61 };
        let step = span / count as f32;
        let thickness = if effect == "bars" { 3.8 } else { 1.8 };
        for i in 0..count {
            let t = i as f32 / (count - 1) as f32;
            let location = if effect == "bars" {
                (2.0 * t - 1.0).abs()
            } else {
                t
            };
            let energy = (band(frame, location) * intensity).clamp(0.0, 1.0);
            let taper = (PI * t).sin().max(0.0).powf(0.6);
            let h = (2.0 + height * 0.83 * energy * taper + pulse * height * 0.10 * taper)
                .min(height - 6.0);
            let x = left + step * (i as f32 + 0.5);
            let points = vec![(x, middle - h * 0.5), (x, middle + h * 0.5)];
            if combine && amplitude > 0.01 {
                paths.push(Stroke {
                    points: points.clone(),
                    width: thickness + 3.0,
                    alpha: 0.08 + amplitude * 0.06,
                });
            }
            paths.push(Stroke {
                points,
                width: thickness.min(step * 0.75),
                alpha,
            });
        }
    } else {
        let layers = if combine { 5 } else { 3 };
        for layer in (0..layers).rev() {
            let layer = layer as f32;
            let mut points = Vec::with_capacity(181);
            for i in 0..=180 {
                let t = i as f32 / 180.0;
                let (x, y) = if effect == "orbit" {
                    let angle = t * TAU;
                    let ripples = (angle * 3.0 - frame.phase + layer * 0.7).sin();
                    let energy = band(frame, t);
                    let rx = span * (0.31 + layer * 0.025) * (1.0 + pulse * 0.07);
                    let ry = (1.2
                        + amplitude * height * (0.26 + layer * 0.024)
                        + ripples * energy * height * 0.06 * intensity.min(1.0)
                        + pulse * height * 0.045)
                        .clamp(0.6, height * 0.43);
                    (width * 0.5 + angle.cos() * rx, middle + angle.sin() * ry)
                } else {
                    let taper = (PI * t).sin().max(0.0).powf(1.15);
                    let energy = band(frame, t);
                    let carrier =
                        (t * TAU * (2.0 + layer * 0.12) - frame.phase + layer * 0.52).sin();
                    let harmonic = (t * TAU * 4.0 + frame.phase * 0.6).sin();
                    let displacement = carrier * (amplitude * 0.70 + energy * intensity * 0.23)
                        + harmonic * energy * intensity * 0.12;
                    let y = middle
                        + displacement.clamp(-1.0, 1.0) * taper * height * (0.41 - layer * 0.025)
                        + (PI * t).sin() * pulse * height * 0.055 * (layer - 1.0);
                    (left + span * t, y)
                };
                points.push((x.clamp(3.0, width - 3.0), y.clamp(3.0, height - 3.0)));
            }
            paths.push(Stroke {
                points,
                width: if layer == 0.0 { 1.9 } else { 1.1 },
                alpha: if layer == 0.0 {
                    alpha
                } else {
                    0.16 + amplitude * 0.16 - layer * 0.018
                },
            });
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice_visual::{AudioFrame, Motion, BAND_COUNT};
    #[test]
    fn all_styles_stay_in_canvas_and_expand_for_real_signal() {
        for effect in ["ribbon", "orbit", "spectrum", "bars"] {
            let mut motion = Motion::default();
            let quiet = motion.advance(&AudioFrame::default(), 0.04, true, false, 0);
            let signal = AudioFrame {
                speaking: true,
                loudness: 1.0,
                bands: vec![1.0; BAND_COUNT],
                ..Default::default()
            };
            let mut loud = quiet.clone();
            for _ in 0..25 {
                loud = motion.advance(&signal, 0.04, true, false, 0);
            }
            let extent = |frame: &VisualFrame| {
                let mut ymin = 100.0_f32;
                let mut ymax = 0.0_f32;
                for path in paths(effect, frame, 400.0, 65.0, 3.0, true) {
                    for (x, y) in path.points {
                        assert!((0.0..400.0).contains(&x) && (0.0..65.0).contains(&y));
                        ymin = ymin.min(y);
                        ymax = ymax.max(y);
                    }
                }
                ymax - ymin
            };
            assert!(extent(&loud) > extent(&quiet) + 15.0, "{effect}");
        }
    }
}
