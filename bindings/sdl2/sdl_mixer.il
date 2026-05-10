// SDL2_mixer — multi-channel audio mixer for short SE (`Chunk`)
// and streaming BGM (`Music`). Separate library
// (`libSDL2_mixer`); not re-exported from `sdl.il` so programs
// that only need raw queue-based playback through `sdl_audio.il`
// don't have to dlopen it.
//
// Lifecycle:
//   sdl.init(InitFlag.audio)
//   sdl_mixer.mixInit(MixInitFlag.ogg | MixInitFlag.mp3)
//   sdl_mixer.mixOpenAudio(44100, 0x8010, 2, 2048)  // S16LSB stereo
//   ... game loop ...
//   sdl_mixer.mixCloseAudio()
//   sdl_mixer.mixQuit()
//   sdl.quit()

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

/// Default `freq` argument for `mixOpenAudio` — 44.1 kHz.
pub const MIX_DEFAULT_FREQUENCY: i32 = 44100
/// Default `format` for `mixOpenAudio` — signed 16-bit LE
/// (matches `AUDIO_S16LSB` in `sdl_audio.il`).
pub const MIX_DEFAULT_FORMAT:    u16 = 0x8010
/// Default channel count — stereo.
pub const MIX_DEFAULT_CHANNELS:  i32 = 2
/// Default mixer sample buffer (in samples per channel).
pub const MIX_DEFAULT_CHUNKSIZE: i32 = 2048
/// Volume range: 0 = silent, 128 = full.
pub const MIX_MAX_VOLUME:        i32 = 128

@extern(C) {
    /// Opaque `Mix_Chunk *` — short PCM buffer (sound effects).
    pub struct Mix_Chunk {}
    /// Opaque `Mix_Music *` — streaming track (BGM).
    pub struct Mix_Music {}

    // ─── lifecycle ───────────────────────────────────────────────
    @lib("SDL2_mixer") pub fn Mix_Init(flags: i32): i32
    @lib("SDL2_mixer") pub fn Mix_Quit()
    @lib("SDL2_mixer") pub fn Mix_OpenAudio(
        frequency: i32, format: u16, channels: i32, chunksize: i32
    ): i32
    @lib("SDL2_mixer") pub fn Mix_CloseAudio()
    @lib("SDL2_mixer") pub fn Mix_AllocateChannels(numchans: i32): i32
    @lib("SDL2_mixer") pub fn Mix_QuerySpec(
        frequency: *i32, format: *u16, channels: *i32
    ): i32

    // ─── chunk (sound-effect) lifecycle + playback ───────────────
    @lib("SDL2_mixer") pub fn Mix_LoadWAV(file: *const char): *Mix_Chunk
    @lib("SDL2_mixer") pub fn Mix_FreeChunk(chunk: *Mix_Chunk)
    @lib("SDL2_mixer") pub fn Mix_PlayChannelTimed(
        channel: i32, chunk: *Mix_Chunk, loops: i32, ticks: i32
    ): i32
    @lib("SDL2_mixer") pub fn Mix_FadeInChannelTimed(
        channel: i32, chunk: *Mix_Chunk, loops: i32,
        ms: i32, ticks: i32
    ): i32
    @lib("SDL2_mixer") pub fn Mix_VolumeChunk(
        chunk: *Mix_Chunk, volume: i32
    ): i32

    // ─── per-channel control ─────────────────────────────────────
    @lib("SDL2_mixer") pub fn Mix_Volume(channel: i32, volume: i32): i32
    @lib("SDL2_mixer") pub fn Mix_Pause(channel: i32)
    @lib("SDL2_mixer") pub fn Mix_Resume(channel: i32)
    @lib("SDL2_mixer") pub fn Mix_HaltChannel(channel: i32): i32
    @lib("SDL2_mixer") pub fn Mix_FadeOutChannel(channel: i32, ms: i32): i32
    @lib("SDL2_mixer") pub fn Mix_Playing(channel: i32): i32
    @lib("SDL2_mixer") pub fn Mix_Paused(channel: i32): i32

    // ─── music (streaming) lifecycle + playback ──────────────────
    @lib("SDL2_mixer") pub fn Mix_LoadMUS(file: *const char): *Mix_Music
    @lib("SDL2_mixer") pub fn Mix_FreeMusic(music: *Mix_Music)
    @lib("SDL2_mixer") pub fn Mix_PlayMusic(music: *Mix_Music, loops: i32): i32
    @lib("SDL2_mixer") pub fn Mix_FadeInMusic(
        music: *Mix_Music, loops: i32, ms: i32
    ): i32
    @lib("SDL2_mixer") pub fn Mix_FadeOutMusic(ms: i32): i32
    @lib("SDL2_mixer") pub fn Mix_VolumeMusic(volume: i32): i32
    @lib("SDL2_mixer") pub fn Mix_PauseMusic()
    @lib("SDL2_mixer") pub fn Mix_ResumeMusic()
    @lib("SDL2_mixer") pub fn Mix_RewindMusic()
    @lib("SDL2_mixer") pub fn Mix_HaltMusic(): i32
    @lib("SDL2_mixer") pub fn Mix_PlayingMusic(): i32
    @lib("SDL2_mixer") pub fn Mix_PausedMusic(): i32

    // ─── error reporting ─────────────────────────────────────────
    // SDL2_mixer's Mix_GetError macro calls SDL_GetError; declare
    // our own thin wrapper so the namespaced form stays available
    // alongside the existing `sdl.getError()`.
    @lib("SDL2_mixer") @symbol("SDL_GetError") pub fn Mix_GetError(): *const char

    // ─── friendlier ilang wrappers ───────────────────────────────

    /// Initialise SDL2_mixer with the requested decoder set.
    ///
    /// Args:
    ///   flags — bitwise OR of `MixInitFlag.*` values.
    ///
    /// Returns:
    ///   bitmask of decoders that actually loaded. Mismatched bits
    ///   indicate missing optional dependencies (call
    ///   `mixGetError()`).
    pub fn mixInit(flags: MixInitFlag): i32 {
        Mix_Init(flags as i32)
    }

    /// Tear down SDL2_mixer. Call after `mixCloseAudio()` and
    /// before `sdl.quit()`.
    pub fn mixQuit() {
        Mix_Quit()
    }

    /// Open the mixer's audio device. Defaults are `(44100,
    /// AUDIO_S16LSB, 2, 2048)` — use the `MIX_DEFAULT_*` consts.
    ///
    /// Args:
    ///   frequency — sample rate (Hz).
    ///   format    — `AUDIO_*` format constant.
    ///   channels  — `1` mono, `2` stereo.
    ///   chunksize — internal scratch buffer in samples per
    ///               channel; smaller = lower latency, higher CPU.
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure.
    pub fn mixOpenAudio(
        frequency: i32, format: u16, channels: i32, chunksize: i32
    ): i32 {
        Mix_OpenAudio(frequency, format, channels, chunksize)
    }

    pub fn mixCloseAudio() {
        Mix_CloseAudio()
    }

    /// Allocate / resize the channel pool (default is 8). Pass
    /// `-1` to query without resizing.
    ///
    /// Returns:
    ///   the resulting channel count.
    pub fn mixAllocateChannels(numchans: i32): i32 {
        Mix_AllocateChannels(numchans)
    }

    /// Returns:
    ///   `(frequency, format, channels)` triple of the open mixer
    ///   device packed in i64 — caller unpacks via the lower 32 /
    ///   middle 16 / upper 16 bits as needed. Mostly useful for
    ///   debug prints; for game logic the values are usually
    ///   whatever the caller passed to `mixOpenAudio`.
    pub fn mixQuerySpec(): i32 {
        let f: i32[] = [0]
        let fmt: u16[] = [0 as u16]
        let ch: i32[] = [0]
        let pf: *i32 = f
        let pfmt: *u16 = fmt
        let pch: *i32 = ch
        Mix_QuerySpec(pf, pfmt, pch)
    }

    pub fn mixGetError(): string {
        stringFromCstr(Mix_GetError())
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

        pub deinit() {
            if this.handle != 0 {
                Mix_FreeChunk(this.handle as *Mix_Chunk)
            }
        }

        /// Play the chunk on `channel` (`-1` = first available).
        /// `loops = 0` plays once; `loops = N > 0` plays N+1 times;
        /// `loops = -1` loops forever.
        ///
        /// Returns:
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
        /// Returns:
        ///   chosen channel, or `-1` on failure.
        pub fadeIn(channel: i32, loops: i32, ms: i32): i32 {
            if this.handle == 0 { return -1 }
            Mix_FadeInChannelTimed(
                channel, this.handle as *Mix_Chunk, loops, ms, -1
            )
        }

        /// Per-chunk volume (`0..MIX_MAX_VOLUME`). Multiplies
        /// against the channel volume at mix time.
        ///
        /// Args:
        ///   volume — `-1` to query without changing.
        ///
        /// Returns:
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

        pub deinit() {
            if this.handle != 0 {
                Mix_FreeMusic(this.handle as *Mix_Music)
            }
        }

        /// Start playback. Replaces any music currently playing.
        ///
        /// Args:
        ///   loops — `0` plays once, `-1` loops forever.
        ///
        /// Returns:
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
    }

    // ─── module-level music control (only one music at a time) ──

    /// Set the global music volume (`0..MIX_MAX_VOLUME`). Passing
    /// `-1` queries without changing.
    pub fn setMusicVolume(volume: i32): i32 {
        Mix_VolumeMusic(volume)
    }

    pub fn pauseMusic() {
        Mix_PauseMusic()
    }

    pub fn resumeMusic() {
        Mix_ResumeMusic()
    }

    pub fn rewindMusic() {
        Mix_RewindMusic()
    }

    /// Stop music playback immediately.
    pub fn haltMusic(): i32 {
        Mix_HaltMusic()
    }

    /// Fade out the currently-playing music over `ms` ms.
    pub fn fadeOutMusic(ms: i32): i32 {
        Mix_FadeOutMusic(ms)
    }

    pub fn isMusicPlaying(): bool {
        Mix_PlayingMusic() != 0
    }

    pub fn isMusicPaused(): bool {
        Mix_PausedMusic() != 0
    }

    // ─── module-level channel control ────────────────────────────

    /// Set the volume of `channel` (`-1` = all channels).
    /// `volume = -1` queries without changing.
    ///
    /// Returns:
    ///   the volume in effect after the call (or the average when
    ///   querying with `channel = -1`).
    pub fn setChannelVolume(channel: i32, volume: i32): i32 {
        Mix_Volume(channel, volume)
    }

    pub fn pauseChannel(channel: i32) {
        Mix_Pause(channel)
    }

    pub fn resumeChannel(channel: i32) {
        Mix_Resume(channel)
    }

    /// Stop playback on `channel` (`-1` = all channels).
    pub fn haltChannel(channel: i32): i32 {
        Mix_HaltChannel(channel)
    }

    pub fn fadeOutChannel(channel: i32, ms: i32): i32 {
        Mix_FadeOutChannel(channel, ms)
    }

    pub fn isChannelPlaying(channel: i32): bool {
        Mix_Playing(channel) != 0
    }

    pub fn isChannelPaused(channel: i32): bool {
        Mix_Paused(channel) != 0
    }
}
