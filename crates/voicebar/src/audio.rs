use crate::runtime::Runtime;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::FromSample;
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

pub fn devices() -> Result<Vec<String>, String> {
    cpal::default_host()
        .input_devices()
        .map_err(|e| e.to_string())?
        .map(|d| {
            d.description()
                .map(|description| description.name().to_owned())
                .map_err(|e| e.to_string())
        })
        .collect()
}

fn input_device(name: &str) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    if name.is_empty() {
        host.default_input_device()
            .ok_or_else(|| "没有可用的麦克风".into())
    } else {
        host.input_devices()
            .map_err(|e| e.to_string())?
            .find(|device| device.description().is_ok_and(|d| d.name() == name))
            .ok_or_else(|| "所选麦克风已断开，请刷新设备后重新选择".into())
    }
}

#[derive(Default)]
struct Meter {
    level: AtomicU8,
    heard: AtomicBool,
}

impl Meter {
    fn observe<T: cpal::Sample>(&self, samples: &[T])
    where
        f32: FromSample<T>,
    {
        if samples.is_empty() {
            return;
        }
        let power = samples
            .iter()
            .map(|v| v.to_sample::<f32>().powi(2))
            .sum::<f32>()
            / samples.len() as f32;
        // Map -60..0 dBFS onto the visible meter. Keep a separate latch so a
        // pause at the end of the test does not erase the successful result.
        let level = ((10.0 * power.log10() + 60.0) * 100.0 / 60.0).clamp(0.0, 100.0);
        self.level.store(level.round() as u8, Ordering::Relaxed);
        if power >= 0.012_f32.powi(2) {
            self.heard.store(true, Ordering::Relaxed);
        }
    }
}

pub struct MicrophoneTest {
    // Dropping the sender cancels the test, including while opening a device.
    _cancel: mpsc::Sender<()>,
    result: mpsc::Receiver<String>,
    meter: Arc<Meter>,
}

impl MicrophoneTest {
    pub fn start(runtime: Arc<Runtime>, device_name: String) -> Self {
        let (cancel, cancelled) = mpsc::channel();
        let (done, result) = mpsc::channel();
        let meter = Arc::new(Meter::default());
        let capture_meter = meter.clone();
        std::thread::spawn(move || {
            let result = test_microphone(&runtime, &device_name, capture_meter, cancelled);
            let _ = done.send(result.unwrap_or_else(|e| format!("麦克风测试失败：{e}")));
        });
        Self {
            _cancel: cancel,
            result,
            meter,
        }
    }

    pub fn level(&self) -> u8 {
        self.meter.level.load(Ordering::Relaxed)
    }

    pub fn result(&self) -> Option<String> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some("麦克风测试异常结束，请重试。".into()),
        }
    }
}

fn meter_stream<T: cpal::SizedSample>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    meter: Arc<Meter>,
    errors: mpsc::SyncSender<cpal::Error>,
) -> Result<cpal::Stream, String>
where
    f32: FromSample<T>,
{
    device
        .build_input_stream(
            config,
            move |samples: &[T], _| meter.observe(samples),
            move |err| {
                let _ = errors.try_send(err);
            },
            None,
        )
        .map_err(|e| e.to_string())
}

fn test_microphone(
    runtime: &Runtime,
    device_name: &str,
    meter: Arc<Meter>,
    cancelled: mpsc::Receiver<()>,
) -> Result<String, String> {
    let interrupted = || runtime.active.lock().unwrap().is_some();
    if interrupted() {
        return Ok("听写进行中，请在结束后测试。".into());
    }
    // Share the device with normal dictation. A new recording claims active
    // first, then waits for this test to close its CPAL stream before opening.
    let _device_guard = runtime
        .microphone_stream
        .try_lock()
        .map_err(|_| "麦克风忙，请稍后重试".to_owned())?;
    if cancelled.try_recv() != Err(mpsc::TryRecvError::Empty) {
        return Ok("麦克风测试已停止。".into());
    }
    let device = input_device(device_name)?;
    let format = device.default_input_config().map_err(|e| e.to_string())?;
    let config = format.clone().into();
    let (error_tx, errors) = mpsc::sync_channel(1);
    let stream = match format.sample_format() {
        cpal::SampleFormat::F32 => meter_stream::<f32>(&device, config, meter.clone(), error_tx),
        cpal::SampleFormat::I16 => meter_stream::<i16>(&device, config, meter.clone(), error_tx),
        cpal::SampleFormat::U16 => meter_stream::<u16>(&device, config, meter.clone(), error_tx),
        f => Err(format!("暂不支持麦克风格式 {f:?}")),
    }?;
    if interrupted() || cancelled.try_recv() != Err(mpsc::TryRecvError::Empty) {
        return Ok("麦克风测试已停止。".into());
    }
    stream.play().map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if let Ok(error) = errors.try_recv() {
            return Err(error.to_string());
        }
        if interrupted() {
            return Ok("已结束测试，麦克风已交给听写。".into());
        }
        if Instant::now() >= deadline {
            return Ok(if meter.heard.load(Ordering::Relaxed) {
                "测试完成，已检测到声音。"
            } else {
                "未检测到明显声音，请检查设备选择、静音开关和 Windows 麦克风权限。"
            }
            .into());
        }
        if cancelled.recv_timeout(Duration::from_millis(40)) != Err(mpsc::RecvTimeoutError::Timeout)
        {
            return Ok("麦克风测试已停止。".into());
        }
    }
}

pub struct Capture {
    stop: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Capture {
    pub fn start(
        device_name: String,
        audio: tokio::sync::mpsc::Sender<Vec<i16>>,
        errors: tokio::sync::mpsc::UnboundedSender<String>,
        runtime: Arc<Runtime>,
        silence_stop_secs: u64,
        silence_end: tokio::sync::oneshot::Sender<()>,
    ) -> Result<Self, String> {
        let (stop, rx) = mpsc::channel();
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            let state = runtime.clone();
            let _device_guard = state.microphone_stream.lock().unwrap();
            let result = (|| -> Result<cpal::Stream, String> {
                let device = input_device(&device_name)?;
                let supported = device.default_input_config().map_err(|e| e.to_string())?;
                let config: cpal::StreamConfig = supported.clone().into();
                let silence = SilenceDetector::new(silence_stop_secs, silence_end);
                match supported.sample_format() {
                    cpal::SampleFormat::F32 => {
                        build::<f32>(&device, &config, audio, errors.clone(), runtime, silence)
                    }
                    cpal::SampleFormat::I16 => {
                        build::<i16>(&device, &config, audio, errors.clone(), runtime, silence)
                    }
                    cpal::SampleFormat::U16 => {
                        build::<u16>(&device, &config, audio, errors.clone(), runtime, silence)
                    }
                    f => Err(format!("暂不支持麦克风格式 {f:?}")),
                }
            })();
            match result {
                Ok(stream) => {
                    if let Err(e) = stream.play() {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                    let _ = ready_tx.send(Ok(()));
                    let _ = rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        });
        match ready.recv_timeout(std::time::Duration::from_secs(8)) {
            Ok(Ok(())) => Ok(Self {
                stop,
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => {
                let _ = stop.send(());
                Err("麦克风启动超时，请检查 Windows 麦克风权限".into())
            }
        }
    }
    pub fn stop(&mut self) {
        let _ = self.stop.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    audio: tokio::sync::mpsc::Sender<Vec<i16>>,
    errors: tokio::sync::mpsc::UnboundedSender<String>,
    runtime: Arc<Runtime>,
    mut silence: SilenceDetector,
) -> Result<cpal::Stream, String>
where
    T: cpal::SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let mut converter = Converter::new(config.sample_rate);
    let mut analyzer = crate::voice_visual::Analyzer::new();
    let session_id = runtime.counter.load(std::sync::atomic::Ordering::SeqCst);
    let stream_errors = errors.clone();
    let mut failed = false;
    device
        .build_input_stream(
            *config,
            move |data: &[T], _| {
                if failed {
                    return;
                }
                for frame in data.chunks_exact(channels) {
                    let mono =
                        frame.iter().map(|v| v.to_sample::<f32>()).sum::<f32>() / channels as f32;
                    for sample in converter.push(mono) {
                        if let Some(packet) = sample {
                            let power = packet
                                .iter()
                                .map(|v| (*v as f32 / 32768.0).powi(2))
                                .sum::<f32>()
                                / packet.len() as f32;
                            // Count captured samples, not network/ASR response latency.
                            silence.observe(power.sqrt(), packet.len());
                            let analyzed = analyzer.analyze(&packet);
                            // Never block the audio callback on a desktop repaint.
                            if let Ok(mut level) = runtime.level.try_lock() {
                                if level.session_id == session_id {
                                    level.audio = analyzed;
                                    level.updated_at = Some(std::time::Instant::now());
                                }
                            }
                            if audio.try_send(packet).is_err() {
                                failed = true;
                                let _ = errors.send(
                                    "识别服务处理过慢，音频缓冲已满。录音已停止以避免丢失音频。"
                                        .into(),
                                );
                                break;
                            }
                        }
                    }
                }
            },
            move |e| {
                let _ = stream_errors.send(format!("麦克风错误：{e}"));
            },
            None,
        )
        .map_err(|e| e.to_string())
}
// Optional volume-based endpointing. Initial quiet and brief clicks must not end
// a session; arm only after 120 ms of sustained voice-level input. A lower hold
// threshold avoids counting quiet syllables as the start of a pause.
struct SilenceDetector {
    limit: usize,
    voice_samples: usize,
    quiet_samples: usize,
    armed: bool,
    end: Option<tokio::sync::oneshot::Sender<()>>,
}
impl SilenceDetector {
    fn new(seconds: u64, end: tokio::sync::oneshot::Sender<()>) -> Self {
        Self {
            limit: seconds as usize * 16000,
            voice_samples: 0,
            quiet_samples: 0,
            armed: false,
            end: Some(end),
        }
    }
    fn observe(&mut self, rms: f32, samples: usize) {
        if self.limit == 0 || self.end.is_none() {
            return;
        }
        if !self.armed {
            if rms >= 0.012 {
                self.voice_samples += samples;
                self.armed = self.voice_samples >= 1920;
            } else {
                self.voice_samples = 0;
            }
        } else if rms >= 0.008 {
            self.quiet_samples = 0;
        } else {
            self.quiet_samples += samples;
            if self.quiet_samples >= self.limit {
                if let Some(end) = self.end.take() {
                    let _ = end.send(());
                }
            }
        }
    }
}

// Stateful linear resampling; the phase survives callback boundaries.
struct Converter {
    ratio: f64,
    index: u64,
    next: f64,
    previous: f32,
    started: bool,
    packet: Vec<i16>,
}
impl Converter {
    fn new(rate: u32) -> Self {
        Self {
            ratio: rate as f64 / 16000.0,
            index: 0,
            next: 0.0,
            previous: 0.0,
            started: false,
            packet: Vec::with_capacity(640),
        }
    }
    fn push(&mut self, current: f32) -> Vec<Option<Vec<i16>>> {
        let mut ready = Vec::new();
        if !self.started {
            self.previous = current;
            self.started = true;
        }
        while self.next <= self.index as f64 {
            let t = (self.next - (self.index as f64 - 1.0)).clamp(0.0, 1.0) as f32;
            let sample = self.previous + (current - self.previous) * t;
            self.packet
                .push((sample.clamp(-1.0, 1.0) * 32767.0).round() as i16);
            if self.packet.len() == 640 {
                ready.push(Some(std::mem::replace(
                    &mut self.packet,
                    Vec::with_capacity(640),
                )));
            }
            self.next += self.ratio;
        }
        self.index += 1;
        self.previous = current;
        ready
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn microphone_meter_distinguishes_silence_and_remembers_signal() {
        let meter = Meter::default();
        meter.observe(&[32768u16; 4]);
        assert_eq!(meter.level.load(Ordering::Relaxed), 0);
        assert!(!meter.heard.load(Ordering::Relaxed));

        // -20 dBFS maps to two thirds of the 60 dB display. Opposite samples
        // must not cancel one another, including on a stereo microphone.
        meter.observe(&[3277i16, -3277]);
        assert_eq!(meter.level.load(Ordering::Relaxed), 67);
        meter.observe(&[0.0f32; 8]);
        assert_eq!(meter.level.load(Ordering::Relaxed), 0);
        assert!(meter.heard.load(Ordering::Relaxed));
    }

    #[test]
    fn microphone_test_respects_the_device_lock_before_opening_hardware() {
        let runtime = Runtime::new(crate::config::AppConfig::default(), "unused".into());
        let _recording_device = runtime.microphone_stream.lock().unwrap();
        let (_cancel, stop) = mpsc::channel();
        let result = test_microphone(&runtime, "no device", Arc::new(Meter::default()), stop);
        assert!(result.unwrap_err().contains("麦克风忙"));
    }

    #[test]
    fn silence_ends_only_after_voice_and_a_complete_pause() {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let mut detector = SilenceDetector::new(2, tx);
        detector.observe(0.0, 16000 * 10);
        detector.observe(0.1, 640); // a short click must not arm endpointing
        detector.observe(0.0, 16000 * 3);
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        for _ in 0..3 {
            detector.observe(0.02, 640);
        }
        detector.observe(0.0, 16000);
        detector.observe(0.009, 640); // a quiet syllable resets the pause
        detector.observe(0.0, 16000 * 2 - 1);
        assert!(rx.try_recv().is_err());
        detector.observe(0.0, 1);
        assert_eq!(rx.try_recv(), Ok(()));
        detector.observe(0.0, 16000 * 10); // sends once
    }
    #[test]
    fn silence_stop_can_be_disabled() {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let mut detector = SilenceDetector::new(0, tx);
        detector.observe(0.1, 16000);
        detector.observe(0.0, 16000 * 60);
        assert!(matches!(
            rx.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
    }
    #[test]
    fn resample_count_and_continuity() {
        for rate in [16000, 44100, 48000] {
            let mut c = Converter::new(rate);
            let mut output = vec![];
            for _ in 0..rate {
                for p in c.push(0.25).into_iter().flatten() {
                    output.extend(p);
                }
            }
            assert_eq!(output.len(), 16000);
            assert!(output.iter().all(|&v| (v as i32 - 8192).abs() <= 1));
        }
    }
}
