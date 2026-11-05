//! Microphone capture for AI transcription: default input device → 16 kHz mono WAV.
//!
//! cpal streams are not `Send`, so each recording owns a thread that builds the stream,
//! keeps it alive until told to stop, and hands back the samples.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SizedSample};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// 16 kHz mono is what speech models are trained on, and keeps uploads small (~32 KB/s).
pub const TARGET_RATE: u32 = 16_000;
/// Hard cap so a forgotten recording can't grow without bound (≈ 9.6 MB of WAV).
pub const MAX_DURATION: Duration = Duration::from_secs(300);
const LEVEL_EVERY: Duration = Duration::from_millis(80);

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RecordError {
    #[error("no microphone found")]
    NoDevice,
    #[error("microphone error: {0}")]
    Device(String),
}

pub struct Recording {
    stop: mpsc::Sender<()>,
    thread: JoinHandle<Vec<f32>>,
    rate: u32,
}

/// Start recording. `on_level` gets the input level (0..1) about 12 times a second, and
/// `on_limit` fires once if `MAX_DURATION` is reached (the recording then stops itself).
pub fn start(
    on_level: impl Fn(f32) + Send + 'static,
    on_limit: impl FnOnce() + Send + 'static,
) -> Result<Recording, RecordError> {
    let (ready_tx, ready_rx) = mpsc::channel::<Result<u32, RecordError>>();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let thread = std::thread::Builder::new()
        .name("kaku-mic".into())
        .spawn(move || {
            let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
            let level = Arc::new(AtomicU32::new(0));
            let stream = match open_stream(samples.clone(), level.clone()) {
                Ok((stream, rate)) => {
                    let _ = ready_tx.send(Ok(rate));
                    stream
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return Vec::new();
                }
            };
            let started = Instant::now();
            let mut on_limit = Some(on_limit);
            loop {
                match stop_rx.recv_timeout(LEVEL_EVERY) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {
                        on_level(f32::from_bits(level.load(Ordering::Relaxed)));
                        if started.elapsed() >= MAX_DURATION {
                            if let Some(f) = on_limit.take() {
                                f();
                            }
                            // Keep the stream until stop() collects the samples.
                            let _ = stop_rx.recv();
                            break;
                        }
                    }
                }
            }
            drop(stream);
            std::mem::take(&mut *samples.lock().unwrap_or_else(|e| e.into_inner()))
        })
        .map_err(|e| RecordError::Device(e.to_string()))?;

    match ready_rx.recv() {
        Ok(Ok(rate)) => Ok(Recording {
            stop: stop_tx,
            thread,
            rate,
        }),
        Ok(Err(e)) => {
            let _ = thread.join();
            Err(e)
        }
        Err(_) => Err(RecordError::Device("recording thread ended".into())),
    }
}

impl Recording {
    /// Stop and return the audio as 16 kHz mono samples.
    pub fn finish(self) -> Vec<f32> {
        let _ = self.stop.send(());
        let samples = self.thread.join().unwrap_or_default();
        resample(&samples, self.rate, TARGET_RATE)
    }
}

fn open_stream(
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<(cpal::Stream, u32), RecordError> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or(RecordError::NoDevice)?;
    let supported = device
        .default_input_config()
        .map_err(|e| RecordError::Device(e.to_string()))?;
    let format = supported.sample_format();
    let config = supported.config();
    let channels = usize::from(config.channels.max(1));
    let rate = config.sample_rate;

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, &config, channels, samples, level),
        SampleFormat::I16 => build::<i16>(&device, &config, channels, samples, level),
        SampleFormat::U16 => build::<u16>(&device, &config, channels, samples, level),
        SampleFormat::I32 => build::<i32>(&device, &config, channels, samples, level),
        SampleFormat::U8 => build::<u8>(&device, &config, channels, samples, level),
        SampleFormat::F64 => build::<f64>(&device, &config, channels, samples, level),
        other => {
            return Err(RecordError::Device(format!(
                "unsupported sample format {other:?}"
            )));
        }
    }?;
    stream
        .play()
        .map_err(|e| RecordError::Device(e.to_string()))?;
    Ok((stream, rate))
}

fn build<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
) -> Result<cpal::Stream, RecordError>
where
    T: SizedSample,
    f32: cpal::FromSample<T>,
{
    device
        .build_input_stream::<T, _, _>(
            *config,
            move |data: &[T], _| {
                let mono: Vec<f32> = data
                    .chunks(channels)
                    .map(|frame| {
                        frame
                            .iter()
                            .map(|s| <f32 as cpal::FromSample<T>>::from_sample_(*s))
                            .sum::<f32>()
                            / frame.len() as f32
                    })
                    .collect();
                level.store(rms(&mono).to_bits(), Ordering::Relaxed);
                samples
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .extend_from_slice(&mono);
            },
            |e| log::warn!("microphone stream error: {e}"),
            None,
        )
        .map_err(|e| RecordError::Device(e.to_string()))
}

/// Loudness 0..1, scaled so normal speech reaches roughly the upper half.
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    (mean.sqrt() * 4.0).min(1.0)
}

/// Linear-interpolation resampling: plenty for speech.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() || from == 0 {
        return input.to_vec();
    }
    let ratio = f64::from(from) / f64::from(to);
    let len = (input.len() as f64 / ratio).floor() as usize;
    (0..len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = input[idx];
            let b = *input.get(idx + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

/// True when the recording has at least `min` of audio louder than background hiss.
pub fn has_speech(samples: &[f32], min: Duration) -> bool {
    let enough = samples.len() as f64 >= min.as_secs_f64() * f64::from(TARGET_RATE);
    enough && samples.iter().any(|s| s.abs() > 0.02)
}

/// 16-bit PCM WAV, mono, `TARGET_RATE`.
pub fn to_wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&TARGET_RATE.to_le_bytes());
    out.extend_from_slice(&(TARGET_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resamples_to_target_length() {
        let one_second_48k = vec![0.5f32; 48_000];
        let out = resample(&one_second_48k, 48_000, TARGET_RATE);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-6));
        assert_eq!(resample(&[0.1, 0.2], 16_000, 16_000), vec![0.1, 0.2]);
    }

    #[test]
    fn writes_a_valid_wav_header() {
        let wav = to_wav(&[0.0, 1.0, -1.0]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(wav[24..28].try_into().unwrap()), 16_000);
        assert_eq!(u32::from_le_bytes(wav[40..44].try_into().unwrap()), 6);
        assert_eq!(wav.len(), 50);
        assert_eq!(i16::from_le_bytes([wav[46], wav[47]]), i16::MAX);
    }

    #[test]
    fn detects_silence() {
        let silence = vec![0.001f32; 16_000];
        assert!(!has_speech(&silence, Duration::from_millis(300)));
        let mut speech = silence.clone();
        speech[8_000] = 0.3;
        assert!(has_speech(&speech, Duration::from_millis(300)));
        assert!(!has_speech(&[0.5; 100], Duration::from_millis(300)));
        assert_eq!(rms(&[]), 0.0);
    }
}
