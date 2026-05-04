// SDL2 audio: queue-based playback. Open a device, queue raw PCM
// samples, hit `play()`. The device class wraps the lifecycle so
// `deinit` closes the device automatically.
//
// Format: signed 16-bit little-endian (AUDIO_S16LSB). Use mono
// (channels = 1) or stereo (channels = 2). For stereo the sample
// stream interleaves L, R, L, R, ...

@extern(C) {
    // Mirror of C `SDL_AudioSpec` (32 B on 64-bit). We never pass
    // a callback (queue-based playback), so `callback` and
    // `userdata` stay zero.
    struct SDL_AudioSpec {
        freq: i32
        format: u16
        channels: u8
        silence: u8
        samples: u16
        padding: u16
        size: u32
        callback: i64    // fn pointer — NULL means "use the queue API"
        userdata: i64    // void * — NULL
    }

    @lib("SDL2") fn SDL_OpenAudioDevice(
        device: *const char, iscapture: i32,
        desired: *SDL_AudioSpec, obtained: *SDL_AudioSpec,
        allowed_changes: i32
    ): u32
    @lib("SDL2") fn SDL_CloseAudioDevice(dev: u32)
    @lib("SDL2") fn SDL_QueueAudio(dev: u32, data: *const void, len: u32): i32
    @lib("SDL2") fn SDL_PauseAudioDevice(dev: u32, pause_on: i32)
    @lib("SDL2") fn SDL_GetQueuedAudioSize(dev: u32): u32
    @lib("SDL2") fn SDL_ClearQueuedAudio(dev: u32)

    class AudioDevice {
        id: u32     // SDL device id (1+ on success, 0 on failure)

        // Open the default device with the requested sample rate
        // (e.g. 44100) and channel count (1 = mono, 2 = stereo).
        // After construction check `id == 0` to detect failure.
        init(freq: i32, channels: u8) {
            let desired = new SDL_AudioSpec()
            desired.freq = freq
            desired.format = 0x8010              // AUDIO_S16LSB
            desired.channels = channels
            desired.samples = 4096               // device buffer size
            // callback / userdata default to 0 (NULL) — queue API.

            let obtained = new SDL_AudioSpec()
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

        // Queue 16-bit signed PCM samples for playback. The length
        // of `data` is the number of *samples* (so for stereo, two
        // entries per frame). Returns 0 on success, < 0 on failure.
        queue(data: i16[]): i32 {
            // i16[] auto-coerces to *i16; reinterpret as *const void
            // for the C API. byte count = sample count × sizeof(i16).
            let p: *i16 = data
            SDL_QueueAudio(this.id, p as *const void, (data.length * 2) as u32)
        }

        // Start (or resume) playback. Devices come up paused.
        play() {
            SDL_PauseAudioDevice(this.id, 0)
        }

        pause() {
            SDL_PauseAudioDevice(this.id, 1)
        }

        // Bytes still waiting in the queue (decreases as the device
        // plays). Useful for "wait until the buffer drains".
        queuedBytes(): u32 {
            SDL_GetQueuedAudioSize(this.id)
        }

        // Drop everything still in the queue.
        clear() {
            SDL_ClearQueuedAudio(this.id)
        }
    }
}
