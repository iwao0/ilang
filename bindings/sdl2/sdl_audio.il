// SDL2 audio: queue-based playback. Open a device, queue raw PCM
// samples, hit `play()`. The device class wraps the lifecycle so
// `deinit` closes the device automatically.
//
// Format: signed 16-bit little-endian (AUDIO_S16LSB). Use mono
// (channels = 1) or stereo (channels = 2). For stereo the sample
// stream interleaves L, R, L, R, ...

@extern(C) {
    /// Mirror of C `SDL_AudioSpec` (32 B on 64-bit). We never pass
    /// a callback (queue-based playback), so `callback` and
    /// `userdata` stay zero.
    struct AudioSpec {
        /// Sample rate in Hz (e.g. 44100).
        freq: i32
        /// `AUDIO_*` format constant (we use `AUDIO_S16LSB = 0x8010`).
        format: u16
        /// Channel count: 1 = mono, 2 = stereo.
        channels: u8
        /// Silence value for the chosen format. SDL fills this in.
        silence: u8
        /// Device buffer size in samples (power of 2).
        samples: u16
        padding: u16
        /// Buffer size in bytes. SDL fills this in for the obtained spec.
        size: u32
        /// Optional fn pointer. NULL means "use the queue API".
        callback: i64
        /// Opaque user data passed to `callback`. NULL when unused.
        userdata: i64
    }

    @lib("SDL2") fn SDL_OpenAudioDevice(
        device: *const char, iscapture: i32,
        desired: *AudioSpec, obtained: *AudioSpec,
        allowed_changes: i32
    ): u32
    @lib("SDL2") fn SDL_CloseAudioDevice(dev: u32)
    @lib("SDL2") fn SDL_QueueAudio(dev: u32, data: *const void, len: u32): i32
    @lib("SDL2") fn SDL_PauseAudioDevice(dev: u32, pause_on: i32)
    @lib("SDL2") fn SDL_GetQueuedAudioSize(dev: u32): u32
    @lib("SDL2") fn SDL_ClearQueuedAudio(dev: u32)

    /// ARC-managed audio device. Open with `new AudioDevice(freq,
    /// channels)`, queue PCM samples via `queue`, start with
    /// `play`. `deinit` closes the underlying SDL device.
    class AudioDevice {
        /// SDL device id. `1+` on success, `0` when opening failed.
        id: u32

        /// Open the default playback device.
        ///
        /// Args:
        ///   freq     — sample rate in Hz (e.g. 44100).
        ///   channels — 1 = mono, 2 = stereo.
        ///
        /// After construction check `id == 0` to detect failure.
        init(freq: i32, channels: u8) {
            let desired = new AudioSpec()
            desired.freq = freq
            desired.format = 0x8010              // AUDIO_S16LSB
            desired.channels = channels
            desired.samples = 4096               // device buffer size
            // callback / userdata default to 0 (NULL) — queue API.

            let obtained = new AudioSpec()
            this.id = SDL_OpenAudioDevice(
                0 as *const char, 0,
                desired, obtained, 0
            )
        }

        deinit() {
            if this.id != 0 {
                SDL_CloseAudioDevice(this.id)
            }
        }

        /// Queue 16-bit signed PCM samples for playback.
        ///
        /// Args:
        ///   data — sample buffer. For stereo, two entries per frame
        ///          (interleaved L, R, L, R, ...).
        ///
        /// Returns:
        ///   0 on success, < 0 on failure (call `sdl.getError()`).
        queue(data: i16[]): i32 {
            // i16[] auto-coerces to *i16; reinterpret as *const void
            // for the C API. byte count = sample count × sizeof(i16).
            let p: *i16 = data
            SDL_QueueAudio(this.id, p as *const void, (data.length * 2) as u32)
        }

        /// Start (or resume) playback. Devices come up paused, so a
        /// freshly-opened device needs an explicit `play()` to begin
        /// draining the queue.
        play() {
            SDL_PauseAudioDevice(this.id, 0)
        }

        /// Pause playback without dropping queued samples.
        pause() {
            SDL_PauseAudioDevice(this.id, 1)
        }

        /// Bytes still waiting in the queue (decreases as the device
        /// plays). Useful for "wait until the buffer drains" loops
        /// or for deciding when to refill.
        ///
        /// Returns:
        ///   queued byte count.
        queuedBytes(): u32 {
            SDL_GetQueuedAudioSize(this.id)
        }

        /// Drop everything still in the queue. Playback restarts
        /// from silence the next time `play` runs.
        clear() {
            SDL_ClearQueuedAudio(this.id)
        }
    }
}
