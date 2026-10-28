//! Microphone capture. The cpal stream is not `Send` on every platform, so it
//! lives on its own thread; callers talk to it through `Recorder`.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, StreamConfig};
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

pub const SAMPLE_RATE: u32 = 16_000;
/// Hard cap per clip; the backend rejects anything much longer.
pub const MAX_SECONDS: u32 = 300;

pub struct Recording {
    stop: mpsc::Sender<()>,
    done: mpsc::Receiver<Result<Vec<f32>, String>>,
    /// Latest RMS level, 0..=1000, for the overlay meter.
    pub level: Arc<AtomicU32>,
}

impl Recording {
    /// Stops capture and returns 16 kHz mono samples.
    pub fn finish(self) -> Result<Vec<f32>, String> {
        let _ = self.stop.send(());
        self.done
            .recv()
            .map_err(|_| "RecorderStopped".to_string())?
    }
}

pub fn start() -> Result<Recording, String> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let (done_tx, done_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let level = Arc::new(AtomicU32::new(0));
    let level_cb = level.clone();

    std::thread::spawn(move || {
        let host = cpal::default_host();
        let Some(device) = host.default_input_device() else {
            let _ = ready_tx.send(Err("NoMicrophone".into()));
            return;
        };
        let supported = match device.default_input_config() {
            Ok(c) => c,
            Err(_) => {
                let _ = ready_tx.send(Err("MicrophoneUnavailable".into()));
                return;
            }
        };
        let rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let max_frames = (rate as usize) * (MAX_SECONDS as usize);
        let buffer = Arc::new(Mutex::new(Vec::<f32>::with_capacity(rate as usize * 30)));

        let push = {
            let buffer = buffer.clone();
            move |mono: &mut dyn Iterator<Item = f32>| {
                let mut buf = buffer.lock().unwrap();
                let mut sum = 0f32;
                let mut n = 0usize;
                for s in mono {
                    if buf.len() >= max_frames {
                        break;
                    }
                    sum += s * s;
                    n += 1;
                    buf.push(s);
                }
                if n > 0 {
                    let rms = (sum / n as f32).sqrt();
                    level_cb.store((rms * 1000.0).min(1000.0) as u32, Ordering::Relaxed);
                }
            }
        };
        let err_fn = |e| log::error!("audio stream error: {e}");

        let stream = match format {
            SampleFormat::F32 => {
                device.build_input_stream(
                    &config,
                    move |data: &[f32], _| {
                        push(&mut data.chunks(channels).map(|f| f.iter().sum::<f32>() / channels as f32))
                    },
                    err_fn,
                    None,
                )
            }
            SampleFormat::I16 => {
                device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        push(&mut data.chunks(channels).map(|f| {
                            f.iter().map(|&s| s as f32 / i16::MAX as f32).sum::<f32>() / channels as f32
                        }))
                    },
                    err_fn,
                    None,
                )
            }
            SampleFormat::U16 => {
                device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        push(&mut data.chunks(channels).map(|f| {
                            f.iter()
                                .map(|&s| (s as f32 - 32768.0) / 32768.0)
                                .sum::<f32>()
                                / channels as f32
                        }))
                    },
                    err_fn,
                    None,
                )
            }
            _ => {
                let _ = ready_tx.send(Err("UnsupportedMicrophoneFormat".into()));
                return;
            }
        };
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                log::error!("build_input_stream: {e}");
                let _ = ready_tx.send(Err("MicrophonePermission".into()));
                return;
            }
        };
        if stream.play().is_err() {
            let _ = ready_tx.send(Err("MicrophonePermission".into()));
            return;
        }
        let _ = ready_tx.send(Ok(()));

        let _ = stop_rx.recv();
        drop(stream);
        let samples = std::mem::take(&mut *buffer.lock().unwrap());
        let _ = done_tx.send(Ok(resample(&samples, rate, SAMPLE_RATE)));
    });

    ready_rx
        .recv()
        .map_err(|_| "MicrophoneUnavailable".to_string())??;
    Ok(Recording { stop: stop_tx, done: done_rx, level })
}

/// Linear-interpolation resampler with a moving-average pre-filter when
/// downsampling. Plenty for speech recognition.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let filtered: Vec<f32> = if ratio > 1.0 {
        let w = ratio.round().max(1.0) as usize;
        let mut acc = 0f32;
        let mut out = Vec::with_capacity(input.len());
        for i in 0..input.len() {
            acc += input[i];
            if i >= w {
                acc -= input[i - w];
            }
            out.push(acc / w.min(i + 1) as f32);
        }
        out
    } else {
        input.to_vec()
    };
    let out_len = (input.len() as f64 / ratio).floor() as usize;
    (0..out_len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos.floor() as usize;
            let frac = (pos - idx as f64) as f32;
            let a = filtered[idx.min(filtered.len() - 1)];
            let b = filtered[(idx + 1).min(filtered.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

/// True when the clip is effectively silence (no point sending it anywhere).
pub fn is_silent(samples: &[f32]) -> bool {
    if samples.len() < (SAMPLE_RATE as usize) / 4 {
        return true;
    }
    let peak = samples.iter().fold(0f32, |m, s| m.max(s.abs()));
    peak < 0.01
}

/// 16-bit PCM WAV for the HTTP providers.
pub fn to_wav(samples: &[f32]) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut cursor, spec).map_err(|e| e.to_string())?;
        for s in samples {
            w.write_sample((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                .map_err(|e| e.to_string())?;
        }
        w.finalize().map_err(|e| e.to_string())?;
    }
    Ok(cursor.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resample_halves_length() {
        let input = vec![0.5f32; 48_000];
        let out = resample(&input, 48_000, 16_000);
        assert_eq!(out.len(), 16_000);
        assert!((out[100] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn silence_detection() {
        assert!(is_silent(&vec![0.0; 16_000]));
        assert!(is_silent(&vec![0.5; 100]));
        let mut speech = vec![0.0; 16_000];
        speech[8000] = 0.3;
        assert!(!is_silent(&speech));
    }

    #[test]
    fn wav_has_header_and_samples() {
        let wav = to_wav(&vec![0.0; 1600]).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 1600 * 2);
    }
}
