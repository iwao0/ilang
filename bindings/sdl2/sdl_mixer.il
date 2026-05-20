/// SDL2_mixer — multi-channel audio mixer for short SE (`Chunk`)
/// and streaming BGM (`Music`). Separate library
/// (`libSDL2_mixer`); not re-exported from `sdl.il` so programs
/// that only need raw queue-based playback through `sdl_audio.il`
/// don't have to dlopen it.
///
/// Lifecycle:
///   sdl.init(InitFlag.audio)
///   sdl.Mixer.init(MixInitFlag.ogg | MixInitFlag.mp3)
///   sdl.Mixer.openAudioDefault()            // 44.1k / s16 / stereo
///   // or: sdl.Mixer.openAudio(44100, sdl.AudioFormat.s16Lsb, 2, 2048)
///   ... game loop ...
///   sdl.Mixer.closeAudio()
///   sdl.Mixer.quit()
///   sdl.quit()

use sdl_audio { AudioFormat }

/// `MIX_INIT_*` — decoder modules to initialise. Returned bitmask
/// from `mixInit` reports which actually loaded; missing bits
/// mean an optional dependency wasn't bundled.
@flags
pub enum MixInitFlag: i32 {
    flac    = 0x01
    mod     = 0x02
    mp3     = 0x08
    ogg     = 0x10
    mid     = 0x20
    opus    = 0x40
    wavpack = 0x80
}

@extern(C, "SDL2_mixer") {
    /// Opaque `Mix_Chunk *` — short PCM buffer (sound effects).
    pub struct Mix_Chunk {}
    /// Opaque `Mix_Music *` — streaming track (BGM).
    pub struct Mix_Music {}

    // ─── lifecycle ───────────────────────────────────────────────
    @lib pub fn Mix_Init(flags: i32): i32
    @lib pub fn Mix_Quit()
    @lib pub fn Mix_OpenAudio(
        frequency: i32, format: u16, channels: i32, chunksize: i32
    ): i32
    @lib pub fn Mix_CloseAudio()
    @lib pub fn Mix_AllocateChannels(numchans: i32): i32
    @lib pub fn Mix_QuerySpec(
        frequency: *i32, format: *u16, channels: *i32
    ): i32

    // ─── chunk (sound-effect) lifecycle + playback ───────────────
    @lib pub fn Mix_LoadWAV(file: *const char): *Mix_Chunk
    @lib pub fn Mix_FreeChunk(chunk: *Mix_Chunk)
    @lib pub fn Mix_PlayChannelTimed(
        channel: i32, chunk: *Mix_Chunk, loops: i32, ticks: i32
    ): i32
    @lib pub fn Mix_FadeInChannelTimed(
        channel: i32, chunk: *Mix_Chunk, loops: i32,
        ms: i32, ticks: i32
    ): i32
    @lib pub fn Mix_VolumeChunk(
        chunk: *Mix_Chunk, volume: i32
    ): i32

    // ─── per-channel control ─────────────────────────────────────
    @lib pub fn Mix_Volume(channel: i32, volume: i32): i32
    @lib pub fn Mix_Pause(channel: i32)
    @lib pub fn Mix_Resume(channel: i32)
    @lib pub fn Mix_HaltChannel(channel: i32): i32
    @lib pub fn Mix_FadeOutChannel(channel: i32, ms: i32): i32
    @lib pub fn Mix_Playing(channel: i32): i32
    @lib pub fn Mix_Paused(channel: i32): i32

    // ─── music (streaming) lifecycle + playback ──────────────────
    @lib pub fn Mix_LoadMUS(file: *const char): *Mix_Music
    @lib pub fn Mix_FreeMusic(music: *Mix_Music)
    @lib pub fn Mix_PlayMusic(music: *Mix_Music, loops: i32): i32
    @lib pub fn Mix_FadeInMusic(
        music: *Mix_Music, loops: i32, ms: i32
    ): i32
    @lib pub fn Mix_FadeOutMusic(ms: i32): i32
    @lib pub fn Mix_VolumeMusic(volume: i32): i32
    @lib pub fn Mix_PauseMusic()
    @lib pub fn Mix_ResumeMusic()
    @lib pub fn Mix_RewindMusic()
    @lib pub fn Mix_HaltMusic(): i32
    @lib pub fn Mix_PlayingMusic(): i32
    @lib pub fn Mix_PausedMusic(): i32

    // ─── error reporting ─────────────────────────────────────────
    // SDL2_mixer's Mix_GetError macro calls SDL_GetError; declare
    // our own thin wrapper so the namespaced form stays available
    // alongside the existing `sdl.getError()`.
    @lib @symbol("SDL_GetError") pub fn Mix_GetError(): *const char

    /// Snapshot of the negotiated audio device parameters as
    /// returned by `Mixer.querySpec()`. Mirrors the three out-
    /// parameters of `Mix_QuerySpec` — the underlying device may
    /// hold additional state but only these three are exposed.
    pub class MixerSpec {
        /// Sample rate in Hz (e.g. 44100).
        pub frequency: i32
        /// Sample format actually in use.
        pub format: AudioFormat
        /// Channel count: 1 = mono, 2 = stereo.
        pub channels: i32

        pub init() {
            this.frequency = 0
            this.format = AudioFormat.s16Lsb
            this.channels = 0
        }
    }

    /// Namespace class for SDL2_mixer's global state — the audio
    /// device, channel pool, and per-channel control. All methods
    /// are `static` (there's no per-instance Mixer to construct).
    /// Mirror of how `Joystick` / `GameController` / `Mouse` collect their
    /// module-level helpers.
    pub class Mixer {
        /// Default `frequency` argument for `openAudio` — 44.1 kHz.
        pub const defaultFrequency: i64 = 44100
        /// Default channel count — stereo. (For the format default
        /// use `AudioFormat.s16Lsb`.)
        pub const defaultChannels:  i64 = 2
        /// Default mixer sample buffer (samples per channel).
        pub const defaultChunksize: i64 = 2048
        /// Volume range: 0 = silent, 128 = full. Multiply against
        /// per-chunk / per-channel / music volumes.
        pub const maxVolume:        i64 = 128

        /// Initialise SDL2_mixer with the requested decoder set.
        ///
        /// **Args:**
        /// - `flags` — bitwise OR of `MixInitFlag.*` values.
        ///
        /// **Returns:**
        ///   the subset of `flags` whose decoders actually loaded
        ///   (as another `MixInitFlag`). Compare against the
        ///   requested mask to detect missing optional deps
        ///   (`(init(req) as i32) & (req as i32) != req as i32`).
        pub static init(flags: MixInitFlag): MixInitFlag {
            Mix_Init(flags as i32) as MixInitFlag
        }

        /// Tear down SDL2_mixer. Call after `Mixer.closeAudio()`
        /// and before `sdl.quit()`.
        pub static quit() {
            Mix_Quit()
        }

        /// Open the mixer's audio device.
        ///
        /// **Args:**
        /// - `frequency` — sample rate (Hz).
        /// - `format` — sample format (`AudioFormat.*`).
        /// - `channels` — `1` mono, `2` stereo.
        /// - `chunksize` — internal scratch buffer in samples per
        ///   channel; smaller = lower latency, higher
        ///   CPU.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub static openAudio(
            frequency: i32, format: AudioFormat,
            channels: i32, chunksize: i32
        ): i32 {
            Mix_OpenAudio(frequency, format as u16, channels, chunksize)
        }

        /// Open the device with the `Mixer.default*` settings —
        /// 44.1 kHz, `AudioFormat.s16Lsb`, stereo, 2048-sample
        /// chunks. Convenience wrapper around `openAudio(...)`.
        /// (Separate name because static methods cannot be
        /// overloaded.)
        pub static openAudioDefault(): i32 {
            Mix_OpenAudio(
                Mixer.defaultFrequency as i32, AudioFormat.s16Lsb as u16,
                Mixer.defaultChannels as i32, Mixer.defaultChunksize as i32
            )
        }

        pub static closeAudio() {
            Mix_CloseAudio()
        }

        /// Allocate / resize the channel pool (default is 8).
        /// Pass `-1` to query without resizing.
        ///
        /// **Returns:**
        ///   the resulting channel count.
        pub static allocateChannels(numchans: i32): i32 {
            Mix_AllocateChannels(numchans)
        }

        /// Query the open device's spec.
        ///
        /// **Returns:**
        ///   `Some(MixerSpec)` with the negotiated frequency /
        ///   format / channels when the device is open, or `none`
        ///   if the mixer hasn't been opened yet.
        pub static querySpec(): MixerSpec? {
            let spec = new MixerSpec()
            // Mix_QuerySpec writes format as a raw u16; bridge it
            // into the AudioFormat enum after the call. The other
            // two fields are i32, matching `MixerSpec`'s layout
            // directly.
            let fmt: u16 = 0 as u16
            let opened = Mix_QuerySpec(&spec.frequency, &fmt, &spec.channels)
            if opened == 0 { return none }
            spec.format = fmt as AudioFormat
            some(spec)
        }

        pub static getError(): string {
            stringFromCstr(Mix_GetError())
        }

        // ─── per-channel control ─────────────────────────────────
        //
        // Channels are i32 ids handed out by the pool allocated in
        // `allocateChannels`. There's no per-channel object — the
        // helpers below take the id and forward to the matching
        // SDL2_mixer fn. `-1` for `channel` usually means "all
        // channels".

        /// Set the volume of `channel` (`-1` = all channels).
        /// `volume = -1` queries without changing.
        ///
        /// **Returns:**
        ///   the volume in effect after the call (or the average
        ///   when querying with `channel = -1`).
        pub static setChannelVolume(channel: i32, volume: i32): i32 {
            Mix_Volume(channel, volume)
        }

        pub static pauseChannel(channel: i32) {
            Mix_Pause(channel)
        }

        pub static resumeChannel(channel: i32) {
            Mix_Resume(channel)
        }

        /// Stop playback on `channel` (`-1` = all channels).
        pub static haltChannel(channel: i32): i32 {
            Mix_HaltChannel(channel)
        }

        pub static fadeOutChannel(channel: i32, ms: i32): i32 {
            Mix_FadeOutChannel(channel, ms)
        }

        pub static isChannelPlaying(channel: i32): bool {
            Mix_Playing(channel) != 0
        }

        pub static isChannelPaused(channel: i32): bool {
            Mix_Paused(channel) != 0
        }
    }

    /// ARC-managed `Mix_Chunk`. Construct via `new Chunk(path)`;
    /// `deinit` calls `Mix_FreeChunk`.
    pub class Chunk {
        /// Raw `Mix_Chunk *` cast to i64. `0` when loading failed.
        pub handle: i64

        /// Load a short audio file (WAV / AIFF / OGG / VOC) as a
        /// `Mix_Chunk`. Loaded fully into memory — meant for SE,
        /// not BGM.
        pub init(path: string) {
            this.handle = Mix_LoadWAV(cstrFromString(path)) as i64
        }

        deinit() {
            if this.handle != 0 {
                Mix_FreeChunk(this.handle as *Mix_Chunk)
            }
        }

        /// Play the chunk on `channel` (`-1` = first available).
        /// `loops = 0` plays once; `loops = N > 0` plays N+1 times;
        /// `loops = -1` loops forever.
        ///
        /// **Returns:**
        ///   the channel actually used (`>= 0`), or `-1` on
        ///   failure.
        pub play(channel: i32, loops: i32): i32 {
            if this.handle == 0 { return -1 }
            Mix_PlayChannelTimed(channel, this.handle as *Mix_Chunk, loops, -1)
        }

        /// Variant of `play` that auto-stops after `durationMs`
        /// milliseconds, regardless of `loops`.
        pub playTimed(channel: i32, loops: i32, durationMs: i32): i32 {
            if this.handle == 0 { return -1 }
            Mix_PlayChannelTimed(channel, this.handle as *Mix_Chunk, loops, durationMs)
        }

        /// Fade-in playback over `ms` milliseconds.
        ///
        /// **Returns:**
        ///   chosen channel, or `-1` on failure.
        pub fadeIn(channel: i32, loops: i32, ms: i32): i32 {
            if this.handle == 0 { return -1 }
            Mix_FadeInChannelTimed(
                channel, this.handle as *Mix_Chunk, loops, ms, -1
            )
        }

        /// Per-chunk volume (`0..Mixer.maxVolume`). Multiplies
        /// against the channel volume at mix time.
        ///
        /// **Args:**
        /// - `volume` — `-1` to query without changing.
        ///
        /// **Returns:**
        ///   the volume in effect after the call.
        pub volume(volume: i32): i32 {
            if this.handle == 0 { return 0 }
            Mix_VolumeChunk(this.handle as *Mix_Chunk, volume)
        }
    }

    /// ARC-managed `Mix_Music`. Construct via `new Music(path)`;
    /// `deinit` calls `Mix_FreeMusic`. Only one music track plays
    /// at a time (SDL2_mixer limitation).
    pub class Music {
        /// Raw `Mix_Music *` cast to i64. `0` when loading failed.
        pub handle: i64

        /// Load a music file (OGG / MP3 / FLAC / MOD / etc. —
        /// formats compiled into your SDL2_mixer build).
        pub init(path: string) {
            this.handle = Mix_LoadMUS(cstrFromString(path)) as i64
        }

        deinit() {
            if this.handle != 0 {
                Mix_FreeMusic(this.handle as *Mix_Music)
            }
        }

        /// Start playback. Replaces any music currently playing.
        ///
        /// **Args:**
        /// - `loops` — `0` plays once, `-1` loops forever.
        ///
        /// **Returns:**
        ///   `0` on success, `< 0` on failure.
        pub play(loops: i32): i32 {
            if this.handle == 0 { return -1 }
            Mix_PlayMusic(this.handle as *Mix_Music, loops)
        }

        /// Fade-in over `ms` milliseconds.
        pub fadeIn(loops: i32, ms: i32): i32 {
            if this.handle == 0 { return -1 }
            Mix_FadeInMusic(this.handle as *Mix_Music, loops, ms)
        }

        // ─── global music slot (only one music plays at a time) ──
        //
        // SDL2_mixer maintains a single "current music" slot that
        // any `instance.play(...)` replaces. The control statics
        // below operate on that slot rather than on a particular
        // `Music` instance — pausing the music is necessarily
        // pausing whatever's currently playing.

        /// Set the global music volume (`0..Mixer.maxVolume`).
        /// Passing `-1` queries without changing.
        pub static setVolume(volume: i32): i32 {
            Mix_VolumeMusic(volume)
        }

        pub static pause() {
            Mix_PauseMusic()
        }

        pub static resume() {
            Mix_ResumeMusic()
        }

        pub static rewind() {
            Mix_RewindMusic()
        }

        /// Stop music playback immediately.
        pub static halt(): i32 {
            Mix_HaltMusic()
        }

        /// Fade out the currently-playing music over `ms` ms.
        pub static fadeOut(ms: i32): i32 {
            Mix_FadeOutMusic(ms)
        }

        pub static isPlaying(): bool {
            Mix_PlayingMusic() != 0
        }

        pub static isPaused(): bool {
            Mix_PausedMusic() != 0
        }
    }
}
