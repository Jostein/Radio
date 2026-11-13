use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

/// Plays whatever ffmpeg decodes into raw f32 PCM.
///
/// Decoding is delegated to `ffmpeg` because it is the only practical way to
/// cover MP3, AAC, Ogg and HLS with one command on both Linux and Windows. It
/// has to be installed (`pacman -S ffmpeg` on Arch, `winget install ffmpeg` on
/// Windows); a native decoder is the obvious follow-up for plain streams.
pub struct Audio {
    /// The stream only exists while something is playing. Holding the device
    /// open costs a scheduling thread in the audio server for silence, and keeps
    /// the speakers away from every other app.
    stream: Mutex<Option<cpal::Stream>>,
    device: cpal::Device,
    config: cpal::StreamConfig,
    ring: Arc<Ring>,
    volume: Arc<AtomicU32>,
    stopped: Arc<AtomicBool>,
    /// Total samples handed to the ring for the current playback. The watchdog
    /// watches this instead of the fill level: a consumer that runs ahead (ALSA's
    /// null device, or a device whose callback drains eagerly) keeps the buffer
    /// empty while audio is playing perfectly well.
    pushed: Arc<AtomicU64>,
    pub sample_rate: u32,
    pub channels: u16,
}

struct RingInner {
    samples: VecDeque<f32>,
    closed: bool,
}

struct Ring {
    inner: Mutex<RingInner>,
    space: Condvar,
    capacity: usize,
}

impl Ring {
    fn new(capacity: usize) -> Ring {
        Ring {
            inner: Mutex::new(RingInner {
                samples: VecDeque::with_capacity(capacity),
                closed: false,
            }),
            space: Condvar::new(),
            capacity,
        }
    }

    /// Blocks while the buffer is full, which is what pushes back on ffmpeg and
    /// from there on the network.
    fn push(&self, samples: &[f32], stopped: &AtomicBool) {
        let mut guard = match self.inner.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        for sample in samples {
            while guard.samples.len() >= self.capacity {
                if stopped.load(Ordering::Relaxed) || guard.closed {
                    return;
                }
                guard = match self.space.wait_timeout(guard, Duration::from_millis(100)) {
                    Ok((guard, _)) => guard,
                    Err(_) => return,
                };
            }

            guard.samples.push_back(*sample);
        }
    }

    fn drain(&self, out: &mut [f32], gain: f32) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        for sample in out.iter_mut() {
            *sample = guard.samples.pop_front().map(|value| value * gain).unwrap_or(0.0);
        }
        drop(guard);
        self.space.notify_all();
    }

    fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.samples.clear();
        }
        self.space.notify_all();
    }

    fn close(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.closed = true;
        }
        self.space.notify_all();
    }

    fn reopen(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.closed = false;
        }
    }

}

impl Audio {
    pub fn start(volume: f32) -> Result<Audio, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No audio output device is available.".to_string())?;

        // The device's own default rate is the one every backend accepts; asking
        // for the maximum rate makes some (for example ALSA's null plugin) reject
        // the configuration outright.
        let default = device
            .default_output_config()
            .map_err(|error| format!("The audio output could not be described: {error}"))?;
        let preferred_rate = default.sample_rate();

        let supported = device
            .supported_output_configs()
            .map_err(|error| format!("Audio devices could not be queried: {error}"))?
            .filter(|range| range.sample_format() == cpal::SampleFormat::F32)
            .find_map(|range| range.try_with_sample_rate(preferred_rate))
            .or_else(|| {
                device
                    .supported_output_configs()
                    .ok()?
                    .filter(|range| range.sample_format() == cpal::SampleFormat::F32)
                    .next()
                    .map(|range| range.with_max_sample_rate())
            })
            .ok_or_else(|| "The audio device has no float output format.".to_string())?;

        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels();
        let config = supported.config();

        let ring = Arc::new(Ring::new((sample_rate as usize * channels as usize) / 2));
        let volume = Arc::new(AtomicU32::new(volume.clamp(0.0, 1.0).to_bits()));
        let pushed = Arc::new(AtomicU64::new(0));

        Ok(Audio {
            stream: Mutex::new(None),
            device,
            config,
            ring,
            volume,
            stopped: Arc::new(AtomicBool::new(false)),
            pushed,
            sample_rate,
            channels,
        })
    }

    /// Opens the device and starts the callbacks, if they are not running.
    pub fn active(&self) {
        let mut guard = match self.stream.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if guard.is_some() {
            return;
        }

        let ring = self.ring.clone();
        let volume = self.volume.clone();
        let opened = self
            .device
            .build_output_stream(
                &self.config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let gain = f32::from_bits(volume.load(Ordering::Relaxed));
                    ring.drain(data, gain);
                },
                |error| eprintln!("radio: audio output error: {error}"),
                None,
            )
            .map_err(|error| error.to_string());

        match opened {
            Ok(stream) => match stream.play() {
                Ok(()) => *guard = Some(stream),
                Err(error) => eprintln!("radio: audio output could not start: {error}"),
            },
            Err(error) => eprintln!("radio: audio output could not be opened: {error}"),
        }
    }

    /// Closes the device. Dropping the stream releases the ALSA/PipeWire client,
    /// which is what makes an idle app cost nothing and leave the sound card to
    /// whoever else wants it.
    pub fn idle(&self) {
        let mut guard = match self.stream.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let _ = guard.take();
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// Reads a running ffmpeg's PCM output into the ring until it exits.
    pub fn pump_child(&self, child: &mut Child) {
        let Some(stdout) = child.stdout.take() else {
            return;
        };

        self.stopped.store(false, Ordering::Relaxed);
        self.ring.reopen();
        self.ring.clear();
        self.pushed.store(0, Ordering::Relaxed);

        let ring = self.ring.clone();
        let stopped = self.stopped.clone();
        let pushed = self.pushed.clone();

        thread::Builder::new()
            .name("audio-pump".to_string())
            .spawn(move || {
                let mut reader = stdout;
                let mut chunk = [0u8; 16_384];
                let mut carry: Vec<u8> = Vec::new();

                loop {
                    if stopped.load(Ordering::Relaxed) {
                        return;
                    }

                    match reader.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(count) => {
                            carry.extend_from_slice(&chunk[..count]);

                            // 4 bytes per f32 sample; keep a partial tail.
                            let complete = carry.len() / 4 * 4;
                            let mut samples = Vec::with_capacity(complete / 4);
                            for bytes in carry[..complete].chunks_exact(4) {
                                samples.push(f32::from_le_bytes([
                                    bytes[0], bytes[1], bytes[2], bytes[3],
                                ]));
                            }
                            carry.drain(..complete);

                            pushed.fetch_add(samples.len() as u64, Ordering::Relaxed);
                            ring.push(&samples, &stopped);
                        }
                        Err(_) => break,
                    }
                }

                ring.close();
            })
            .ok();
    }

    /// Stops feeding the speaker and drops what is queued.
    pub fn silence(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.ring.close();
        self.ring.clear();
    }

    /// Samples fed to the speaker since this playback started.
    pub fn pushed(&self) -> u64 {
        self.pushed.load(Ordering::Relaxed)
    }
}

/// Spawns ffmpeg decoding `url` to raw f32 PCM at the output device's rate.
pub fn spawn_ffmpeg(url: &str, sample_rate: u32, channels: u16) -> Result<Child, String> {
    let mut command = Command::new("ffmpeg");
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            // Keep a dropped HTTP stream alive instead of restarting the process.
            "-reconnect",
            "1",
            "-reconnect_streamed",
            "1",
            "-reconnect_delay_max",
            "5",
            "-i",
            url,
            "-vn",
            "-f",
            "f32le",
        ])
        .arg("-ac")
        .arg(channels.to_string())
        .arg("-ar")
        .arg(sample_rate.to_string())
        .arg("-");
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "ffmpeg is required for playback but was not found on PATH.".to_string()
        } else {
            format!("Playback could not start: {error}")
        }
    })
}
