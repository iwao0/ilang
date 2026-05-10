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
    pub struct AudioSpec {
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

    /// Opaque SDL_AudioStream — created by `SDL_NewAudioStream`,
    /// destroyed by `SDL_FreeAudioStream`.
    pub struct SDL_AudioStream {}

    @lib("SDL2") pub fn SDL_OpenAudioDevice(
        device: *const char, iscapture: i32,
        desired: *AudioSpec, obtained: *AudioSpec,
        allowed_changes: i32
    ): u32
    @lib("SDL2") pub fn SDL_CloseAudioDevice(dev: u32)
    @lib("SDL2") pub fn SDL_QueueAudio(dev: u32, data: *const void, len: u32): i32
    @lib("SDL2") pub fn SDL_PauseAudioDevice(dev: u32, pause_on: i32)
    @lib("SDL2") pub fn SDL_GetQueuedAudioSize(dev: u32): u32
    @lib("SDL2") pub fn SDL_ClearQueuedAudio(dev: u32)

    // ─── device enumeration / status ─────────────────────────────
    @lib("SDL2") pub fn SDL_GetNumAudioDevices(iscapture: i32): i32
    @lib("SDL2") pub fn SDL_GetAudioDeviceName(
        index: i32, iscapture: i32
    ): *const char
    @lib("SDL2") pub fn SDL_GetAudioDeviceStatus(dev: u32): i32

    // ─── lock / unlock for callback-style devices ────────────────
    @lib("SDL2") pub fn SDL_LockAudioDevice(dev: u32)
    @lib("SDL2") pub fn SDL_UnlockAudioDevice(dev: u32)

    // ─── mixing ──────────────────────────────────────────────────
    @lib("SDL2") pub fn SDL_MixAudioFormat(
        dst: *u8, src: *const u8,
        format: u16, len: u32, volume: i32
    )

    // ─── driver enumeration ──────────────────────────────────────
    @lib("SDL2") pub fn SDL_GetCurrentAudioDriver(): *const char
    @lib("SDL2") pub fn SDL_GetNumAudioDrivers(): i32
    @lib("SDL2") pub fn SDL_GetAudioDriver(index: i32): *const char

    // ─── AudioStream (sample-rate / format conversion) ───────────
    @lib("SDL2") pub fn SDL_NewAudioStream(
        src_format: u16, src_channels: u8, src_rate: i32,
        dst_format: u16, dst_channels: u8, dst_rate: i32
    ): *SDL_AudioStream
    @lib("SDL2") pub fn SDL_AudioStreamPut(
        stream: *SDL_AudioStream, buf: *const void, len: i32
    ): i32
    @lib("SDL2") pub fn SDL_AudioStreamGet(
        stream: *SDL_AudioStream, buf: *const void, len: i32
    ): i32
    @lib("SDL2") pub fn SDL_AudioStreamAvailable(stream: *SDL_AudioStream): i32
    @lib("SDL2") pub fn SDL_AudioStreamFlush(stream: *SDL_AudioStream): i32
    @lib("SDL2") pub fn SDL_AudioStreamClear(stream: *SDL_AudioStream)
    @lib("SDL2") pub fn SDL_FreeAudioStream(stream: *SDL_AudioStream)

    // ─── WAV loading ─────────────────────────────────────────────
    // Allocates a buffer SDL owns; pair `LoadWAV_RW` with `FreeWAV`.
    // `freesrc != 0` makes SDL close the RWops for us.
    @lib("SDL2") pub fn SDL_LoadWAV_RW(
        src: *const void, freesrc: i32,
        spec: *AudioSpec, audio_buf: **u8, audio_len: *u32
    ): *AudioSpec
    @lib("SDL2") pub fn SDL_FreeWAV(audio_buf: *u8)

    // ─── AudioCVT (SDL's built-in resampler / format converter) ──
    // SDL_AudioCVT is a 132-byte struct: bookkeeping flags, source
    // format, target format, callback pointer, length / pos
    // counters, then a 10-slot fixed conversion-filter array. We
    // mirror it field-for-field so `BuildAudioCVT` /
    // `ConvertAudio` operate on the same layout libSDL2 expects.
    pub struct AudioCVT {
        /// Set by BuildAudioCVT to 1 when conversion is needed, 0
        /// when the formats already match.
        needed: i32
        src_format: u16
        dst_format: u16
        rate_incr: f64
        /// Caller-allocated scratch buffer (`len * len_mult` bytes).
        buf: i64
        /// Bytes of meaningful data in `buf` *before* the call;
        /// updated to the converted length afterwards.
        len: i32
        len_cvt: i32
        /// `len * len_mult` bytes is the largest `buf` will ever
        /// need to hold during conversion.
        len_mult: i32
        len_ratio: f64
        /// 10 SDL_AudioFilter slots × 8 bytes each = 80 B.
        filters: u8[80]
        filter_index: i32
    }

    @lib("SDL2") pub fn SDL_BuildAudioCVT(
        cvt: *AudioCVT,
        src_format: u16, src_channels: u8, src_rate: i32,
        dst_format: u16, dst_channels: u8, dst_rate: i32
    ): i32
    @lib("SDL2") pub fn SDL_ConvertAudio(cvt: *AudioCVT): i32

    /// ARC-managed audio device. Open with `new AudioDevice(freq,
    /// channels)`, queue PCM samples via `queue`, start with
    /// `play`. `deinit` closes the underlying SDL device.
    pub class AudioDevice {
        /// SDL device id. `1+` on success, `0` when opening failed.
        pub id: u32

        /// Open the default playback device.
        ///
        /// Args:
        ///   freq     — sample rate in Hz (e.g. 44100).
        ///   channels — 1 = mono, 2 = stereo.
        ///
        /// After construction check `id == 0` to detect failure.
        pub init(freq: i32, channels: u8) {
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

        pub deinit() {
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
        pub queue(data: i16[]): i32 {
            // i16[] auto-coerces to *i16; reinterpret as *const void
            // for the C API. byte count = sample count × sizeof(i16).
            let p: *i16 = data
            SDL_QueueAudio(this.id, p as *const void, (data.length * 2) as u32)
        }

        /// Start (or resume) playback. Devices come up paused, so a
        /// freshly-opened device needs an explicit `play()` to begin
        /// draining the queue.
        pub play() {
            SDL_PauseAudioDevice(this.id, 0)
        }

        /// Pause playback without dropping queued samples.
        pub pause() {
            SDL_PauseAudioDevice(this.id, 1)
        }

        /// Bytes still waiting in the queue (decreases as the device
        /// plays). Useful for "wait until the buffer drains" loops
        /// or for deciding when to refill.
        ///
        /// Returns:
        ///   queued byte count.
        pub queuedBytes(): u32 {
            SDL_GetQueuedAudioSize(this.id)
        }

        /// Drop everything still in the queue. Playback restarts
        /// from silence the next time `play` runs.
        pub clear() {
            SDL_ClearQueuedAudio(this.id)
        }

        /// Returns:
        ///   `0` stopped, `1` playing, `2` paused.
        pub status(): i32 {
            SDL_GetAudioDeviceStatus(this.id)
        }

        /// Acquire the device's audio lock — blocks the audio
        /// callback (when one is wired up) so the caller can mutate
        /// shared state. Pair with `unlock()`.
        pub lock() {
            SDL_LockAudioDevice(this.id)
        }

        pub unlock() {
            SDL_UnlockAudioDevice(this.id)
        }
    }

    /// Returns:
    ///   number of available output devices when `capture` is false,
    ///   or capture devices when true. `-1` means "unknown" (some
    ///   backends can't enumerate without opening a device first).
    pub fn numAudioDevices(capture: bool): i32 {
        let cap: i32 = if capture { 1 } else { 0 }
        SDL_GetNumAudioDevices(cap)
    }

    /// Args:
    ///   index   — `[0, numAudioDevices(capture))`.
    ///   capture — true for capture devices, false for output.
    ///
    /// Returns:
    ///   human-readable device name, or empty when out of range.
    pub fn audioDeviceName(index: i32, capture: bool): string {
        let cap: i32 = if capture { 1 } else { 0 }
        stringFromCstr(SDL_GetAudioDeviceName(index, cap))
    }

    /// Returns:
    ///   the audio backend currently in use (e.g. `"coreaudio"`,
    ///   `"pulseaudio"`, `"wasapi"`), or empty when audio isn't
    ///   initialised.
    pub fn currentAudioDriver(): string {
        stringFromCstr(SDL_GetCurrentAudioDriver())
    }

    /// Returns:
    ///   how many audio drivers SDL was built with.
    pub fn numAudioDrivers(): i32 {
        SDL_GetNumAudioDrivers()
    }

    /// Args:
    ///   index — `[0, numAudioDrivers())`.
    ///
    /// Returns:
    ///   the driver name at that index.
    pub fn audioDriver(index: i32): string {
        stringFromCstr(SDL_GetAudioDriver(index))
    }

    /// Mix `len` bytes of `src` (in `format`) into `dst` at the
    /// given `volume` (0..128 — 128 is full volume). Useful for
    /// callback-style devices that hand-roll their own mixer.
    ///
    /// Args:
    ///   dst, src — byte buffers (must be `len` bytes long).
    ///   format   — `AUDIO_*` format constant.
    ///   len      — byte count to mix.
    ///   volume   — 0..128.
    pub fn mixAudioFormat(dst: u8[], src: u8[], format: u16, len: u32, volume: i32) {
        let pd: *u8 = dst
        let ps: *u8 = src
        SDL_MixAudioFormat(pd, ps as *const u8, format, len, volume)
    }

    /// ARC-managed wrapper around `SDL_AudioStream` — SDL's built-in
    /// sample-rate / channel-count / format converter. Push raw
    /// source-format bytes via `put()`, then pull converted bytes
    /// via `get()` once enough are queued. `deinit` releases the
    /// underlying stream.
    ///
    /// Format constants are the same `AUDIO_*` values
    /// (`AUDIO_S16LSB = 0x8010`, `AUDIO_F32LSB = 0x8120`, etc.).
    pub class AudioStream {
        /// Raw `SDL_AudioStream*` cast to i64. `0` when allocation
        /// failed at construction time.
        pub handle: i64

        /// Args:
        ///   srcFormat   — input `AUDIO_*` format.
        ///   srcChannels — 1 = mono, 2 = stereo.
        ///   srcRate     — input sample rate (Hz).
        ///   dstFormat   — output `AUDIO_*` format.
        ///   dstChannels — output channel count.
        ///   dstRate     — output sample rate (Hz).
        pub init(
            srcFormat: u16, srcChannels: u8, srcRate: i32,
            dstFormat: u16, dstChannels: u8, dstRate: i32
        ) {
            let p = SDL_NewAudioStream(
                srcFormat, srcChannels, srcRate,
                dstFormat, dstChannels, dstRate
            )
            this.handle = p as i64
        }

        pub deinit() {
            if this.handle != 0 {
                SDL_FreeAudioStream(this.handle as *SDL_AudioStream)
            }
        }

        /// Feed raw source-format bytes into the stream.
        ///
        /// Args:
        ///   bytes — input buffer in the stream's source format.
        ///
        /// Returns:
        ///   `0` on success, `< 0` on failure.
        pub put(bytes: u8[]): i32 {
            if this.handle == 0 { return -1 }
            let p: *u8 = bytes
            SDL_AudioStreamPut(
                this.handle as *SDL_AudioStream,
                p as *const void, bytes.length as i32
            )
        }

        /// Drain converted bytes from the stream into `bytes`. The
        /// caller pre-allocates `bytes` to the desired pull size.
        ///
        /// Args:
        ///   bytes — output buffer (must already be sized).
        ///
        /// Returns:
        ///   number of bytes actually written (`<= bytes.length`),
        ///   or `< 0` on error.
        pub get(bytes: u8[]): i32 {
            if this.handle == 0 { return -1 }
            let p: *u8 = bytes
            SDL_AudioStreamGet(
                this.handle as *SDL_AudioStream,
                p as *const void, bytes.length as i32
            )
        }

        /// Returns:
        ///   bytes of converted output ready to be pulled.
        pub available(): i32 {
            if this.handle == 0 { return 0 }
            SDL_AudioStreamAvailable(this.handle as *SDL_AudioStream)
        }

        /// Drain any partially-buffered samples through the
        /// converter. Call before the last `get()` so trailing
        /// samples don't get stuck in SDL's internal scratch.
        pub flush(): i32 {
            if this.handle == 0 { return 0 }
            SDL_AudioStreamFlush(this.handle as *SDL_AudioStream)
        }

        /// Throw away every queued / partially-converted byte.
        pub clear() {
            if this.handle != 0 {
                SDL_AudioStreamClear(this.handle as *SDL_AudioStream)
            }
        }
    }
}
