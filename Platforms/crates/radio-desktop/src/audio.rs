use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

/// Plays whatever ffmpeg decodes into raw f32 PCM.
///
/// Decoding is delegated to `ffmpeg` because it is the only practical way to
/// cover MP3, AAC, Ogg and HLS with one command on both Linux and Windows. On
/// Linux it is installed (`pacman -S ffmpeg` on Arch); the Windows release zip
/// carries a trimmed `ffmpeg.exe` beside `radio.exe`, so playback there needs no
/// install. A native decoder is the obvious follow-up for plain streams.
pub struct Audio {
    /// The stream only exists while something is playing. Holding the device
    /// open costs a scheduling thread in the audio server for silence, and keeps
    /// the speakers away from every other app.
    stream: Mutex<Option<cpal::Stream>>,
    /// The chosen output device, or `None` when the machine had none at startup.
    /// The app then keeps its tray, says so in the menu, and picks a device up
    /// when one appears instead of exiting before anything is drawn.
    output: Mutex<Option<Output>>,
    ring: Arc<Ring>,
    volume: Arc<AtomicU32>,
    stopped: Arc<AtomicBool>,
    /// Set by the stream's error callback when the device under it disappears.
    failed: Arc<AtomicBool>,
    /// Total samples handed to the ring for the current playback. The watchdog
    /// watches this instead of the fill level: a consumer that runs ahead (ALSA's
    /// null device, or a device whose callback drains eagerly) keeps the buffer
    /// empty while audio is playing perfectly well.
    pushed: Arc<AtomicU64>,
    sample_rate: AtomicU32,
    channels: AtomicU16,
}

/// What has to be known before a stream can be built. Absent while the machine
/// has no default output device.
struct Output {
    device: cpal::Device,
    config: cpal::StreamConfig,
}

/// The message the menu shows when the machine offers no default output device:
/// a server image whose audio services are stopped, or a remote session that does
/// not redirect audio.
pub const NO_OUTPUT: &str = "No audio output device is available.";

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
    /// An app with a device. Prefer `start_or_silent`, which keeps the tray on a
    /// machine that has none.
    pub fn start(volume: f32) -> Result<Audio, String> {
        let (device, config, sample_rate, channels) = query_output()?;
        Ok(Audio::new(volume, Some(Output { device, config }), sample_rate, channels))
    }

    /// Always succeeds. Without an output device the player reports the problem
    /// in its menu, instead of the app exiting before the tray exists.
    pub fn start_or_silent(volume: f32) -> Audio {
        match Audio::start(volume) {
            Ok(audio) => audio,
            Err(_) => Audio::new(volume, None, 48_000, 2),
        }
    }

    fn new(volume: f32, output: Option<Output>, sample_rate: u32, channels: u16) -> Audio {
        Audio {
            stream: Mutex::new(None),
            output: Mutex::new(output),
            ring: Arc::new(Ring::new((sample_rate as usize * channels as usize) / 2)),
            volume: Arc::new(AtomicU32::new(volume.clamp(0.0, 1.0).to_bits())),
            stopped: Arc::new(AtomicBool::new(false)),
            failed: Arc::new(AtomicBool::new(false)),
            pushed: Arc::new(AtomicU64::new(0)),
            sample_rate: AtomicU32::new(sample_rate),
            channels: AtomicU16::new(channels),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    pub fn channels(&self) -> u16 {
        self.channels.load(Ordering::Relaxed)
    }

    /// True when there is a device to play through.
    pub fn has_output(&self) -> bool {
        match self.output.lock() {
            Ok(guard) => guard.is_some(),
            Err(poisoned) => poisoned.into_inner().is_some(),
        }
    }

    /// Opens the default device if the app started without one, and says whether
    /// a device is available afterwards. Called once a second while there is none,
    /// so starting the audio service or plugging a speaker in is enough.
    pub fn refresh_output(&self) -> bool {
        if self.has_output() {
            return true;
        }

        let Ok((device, config, sample_rate, channels)) = query_output() else {
            return false;
        };

        // Anything that plays later asks for these again, so a device whose rate
        // differs from the silent default still gets the stream it wants.
        self.sample_rate.store(sample_rate, Ordering::Relaxed);
        self.channels.store(channels, Ordering::Relaxed);

        let output = Some(Output { device, config });
        match self.output.lock() {
            Ok(mut guard) => *guard = output,
            Err(poisoned) => *poisoned.into_inner() = output,
        }
        true
    }

    /// True when the device under a running stream went away: WASAPI invalidates
    /// the client — an unplugged speaker, or a remote session reconnecting — and
    /// the callbacks stop, while the stream object itself stays behind.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    /// Drops the dead stream and the cached device, asks the system for its
    /// current default, and opens a stream on that. Without it the app keeps a
    /// stream that will never make a sound and `active` keeps returning early.
    pub fn reopen(&self) {
        self.idle();
        match self.output.lock() {
            Ok(mut guard) => *guard = None,
            Err(poisoned) => *poisoned.into_inner() = None,
        }
        self.failed.store(false, Ordering::Relaxed);
        self.active();
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

        if !self.refresh_output() {
            return;
        }

        let ring = self.ring.clone();
        let volume = self.volume.clone();
        let failed = self.failed.clone();
        let opened = {
            let output = match self.output.lock() {
                Ok(output) => output,
                Err(poisoned) => poisoned.into_inner(),
            };
            let Some(output) = output.as_ref() else {
                return;
            };
            output
                .device
                .build_output_stream(
                    &output.config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        let gain = f32::from_bits(volume.load(Ordering::Relaxed));
                        ring.drain(data, gain);
                    },
                    move |error| {
                        failed.store(true, Ordering::Relaxed);
                        eprintln!("radio: audio output error: {error}");
                    },
                    None,
                )
                .map_err(|error| error.to_string())
        };

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

/// Asks the system for its default output device and the format to feed it.
fn query_output() -> Result<(cpal::Device, cpal::StreamConfig, u32, u16), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| NO_OUTPUT.to_string())?;

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
    Ok((device, supported.config(), sample_rate, channels))
}

/// The decoder binary: a copy beside the app first, then `PATH`.
///
/// Windows searches a few more places than `PATH` on its own, but relying on
/// that is what made the missing-decoder case confusing: the zip ships an
/// `ffmpeg.exe` next to `radio.exe`, and naming it explicitly means the bundled
/// copy is the one that plays. Nothing is bundled on Linux, and the Arch package
/// leaves ffmpeg to the system, so both fall back to `PATH` as before.
fn ffmpeg_command() -> Command {
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let beside_the_app = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(name)));

    match beside_the_app {
        Some(path) if path.is_file() => Command::new(path),
        _ => Command::new(name),
    }
}

/// Spawns ffmpeg decoding `url` to raw f32 PCM at the output device's rate.
pub fn spawn_ffmpeg(url: &str, sample_rate: u32, channels: u16) -> Result<Child, String> {
    let mut command = ffmpeg_command();
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

    // ffmpeg is a console program and this app is not, so without this every
    // play opens a console window over the flyout. Linux has no such window.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "ffmpeg is required for playback but was not found next to the app or on PATH."
                .to_string()
        } else {
            format!("Playback could not start: {error}")
        }
    })
}
