// SFX + BGM device pair. SDL allows multiple simultaneous open
// devices, so the music loop keeps playing on `bgm` while short
// sound effects (wall / paddle / brick) queue and dequeue rapidly
// on `sfx`. The session also owns the pre-rendered beep buffers
// so callers don't have to thread them around.

use sdl
use audio
use consts

class AudioSession {
    sfx: sdl.AudioDevice
    bgm: sdl.AudioDevice
    /// Pre-rendered BGM loop (one full Am→F→C→G arpeggio).
    bgmLoop: i16[]
    /// Cached byte length of `bgmLoop` (samples × 2). Used by
    /// `refillBgm` to know when the queued buffer has drained
    /// below one full loop.
    bgmBytes: u32
    /// Pre-rendered SFX waveforms — built once on construction.
    beepWall: i16[]
    beepPaddle: i16[]
    beepBrick: i16[]

    init() {
        this.sfx = new sdl.AudioDevice(consts.sampleRate, 1)
        if this.sfx.id != 0 {
            this.sfx.play()
        }

        this.bgm = new sdl.AudioDevice(consts.sampleRate, 1)
        let bgmBuf = audio.makeBgmLoop()
        this.bgmLoop = bgmBuf
        this.bgmBytes = (bgmBuf.length * 2) as u32
        if this.bgm.id != 0 {
            // Pre-buffer two loops so the first refill check has slack.
            let _ = this.bgm.queue(bgmBuf)
            let _ = this.bgm.queue(bgmBuf)
            this.bgm.play()
        }

        this.beepWall = audio.makeBeep(440.0, 40)
        this.beepPaddle = audio.makeBeep(660.0, 50)
        this.beepBrick = audio.makeBeep(880.0, 70)
        return
    }

    /// True when `init` opened the SFX device successfully.
    sfxOk(): bool {
        this.sfx.id != 0
    }

    playWall() {
        let _ = this.sfx.queue(this.beepWall)
        return
    }
    playPaddle() {
        let _ = this.sfx.queue(this.beepPaddle)
        return
    }
    playBrick() {
        let _ = this.sfx.queue(this.beepBrick)
        return
    }

    /// Append another copy of the BGM loop when the BGM device's
    /// queued bytes have drained below one full loop. Call once
    /// per frame.
    refillBgm() {
        if this.bgm.id != 0 && this.bgm.queuedBytes() < this.bgmBytes {
            let _ = this.bgm.queue(this.bgmLoop)
        }
    }
}
