// Short PCM beep generators, the BGM arpeggio loop, and the
// `AudioSession` class that owns the SDL devices + pre-rendered
// buffers. All audio buffers are i16 mono at `consts.sampleRate`.

use sdl2
use std.math as math
use consts

// Short sine-wave beep with a 5ms attack/release ramp so the start
// and end don't click.
pub fn makeBeep(freqHz: f64, ms: i32): i16[] {
    let n = (consts.sampleRate * ms) / 1000
    let buf: i16[] = []
    let twoPi: f64 = 2.0 * math.pi
    let i: i32 = 0
    while i < n {
        let ramp = consts.sampleRate / 200
        let env: f64 = if i < ramp {
            (i as f64) / (ramp as f64)
        } elif i > n - ramp {
            ((n - i) as f64) / (ramp as f64)
        } else {
            1.0
        }
        let t = (i as f64) / (consts.sampleRate as f64)
        let sample = math.sin(twoPi * freqHz * t) * 8000.0 * env
        buf.push(sample as i16)
        i += 1
    }
    buf
}

// Single chiptune note (square wave) of `ms` milliseconds. Used as
// the building block for the BGM loop.
pub fn makeNote(freqHz: f64, ms: i32, vol: f64): i16[] {
    let n = (consts.sampleRate * ms) / 1000
    let buf: i16[] = []
    let i: i32 = 0
    while i < n {
        let ramp = consts.sampleRate / 100   // 10ms attack/decay
        let env: f64 = if i < ramp {
            (i as f64) / (ramp as f64)
        } elif i > n - ramp {
            ((n - i) as f64) / (ramp as f64)
        } else {
            1.0
        }
        let t = (i as f64) / (consts.sampleRate as f64)
        // Square wave from sin sign — gives the classic chip-style
        // bite without needing a phase accumulator.
        let s = math.sin(2.0 * math.pi * freqHz * t)
        let sq: f64 = if s >= 0.0 { 1.0 } else { -1.0 }
        let sample = sq * vol * 5500.0 * env
        buf.push(sample as i16)
        i += 1
    }
    buf
}

// 16-step arpeggio over Am → F → C → G (4 bars of 4 notes each at
// ~180ms per step → ≈2.9s loop). Returns one full loop ready to be
// queued into an AudioDevice; the main loop refills the queue when
// it gets low.
pub fn makeBgmLoop(): i16[] {
    let buf: i16[] = []
    let beatMs: i32 = 180
    let notes: f64[] = [
        // Am: A2 E3 A3 E3
        110.0,  164.81, 220.0,  164.81,
        // F:  F2 C3 F3 C3
         87.31, 130.81, 174.61, 130.81,
        // C:  C3 G3 C4 G3
        130.81, 196.0,  261.63, 196.0,
        // G:  G2 D3 G3 D3
         98.0,  146.83, 196.0,  146.83
    ]
    let i: i32 = 0
    let n: i32 = notes.length as i32
    while i < n {
        let note = makeNote(notes[i], beatMs, 0.16)
        let ji: i32 = 0
        let jn: i32 = note.length as i32
        while ji < jn {
            buf.push(note[ji])
            ji += 1
        }
        i += 1
    }
    buf
}

/// SFX + BGM device pair. SDL allows multiple simultaneous open
/// devices, so the music loop keeps playing on `bgm` while short
/// sound effects (wall / paddle / brick) queue and dequeue rapidly
/// on `sfx`. The session also owns the pre-rendered beep buffers
/// so callers don't have to thread them around.
pub class AudioSession {
    pub sfx: sdl2.AudioDevice
    pub bgm: sdl2.AudioDevice
    /// Pre-rendered BGM loop (one full Am→F→C→G arpeggio).
    pub bgmLoop: i16[]
    /// Cached byte length of `bgmLoop` (samples × 2). Used by
    /// `refillBgm` to know when the queued buffer has drained
    /// below one full loop.
    pub bgmBytes: u32
    /// Pre-rendered SFX waveforms — built once on construction.
    pub beepWall: i16[]
    pub beepPaddle: i16[]
    pub beepBrick: i16[]

    pub init() {
        this.sfx = new sdl2.AudioDevice(consts.sampleRate, sdl2.AudioFormat.s16Lsb, 1 as u8, 4096 as u16)
        if this.sfx.id != 0 {
            this.sfx.play()
        }

        this.bgm = new sdl2.AudioDevice(consts.sampleRate, sdl2.AudioFormat.s16Lsb, 1 as u8, 4096 as u16)
        let bgmBuf = makeBgmLoop()
        this.bgmLoop = bgmBuf
        this.bgmBytes = (bgmBuf.length * 2) as u32
        if this.bgm.id != 0 {
            // Pre-buffer two loops so the first refill check has slack.
            let _ = this.bgm.queue(bgmBuf)
            let _ = this.bgm.queue(bgmBuf)
            this.bgm.play()
        }

        this.beepWall = makeBeep(440.0, 40)
        this.beepPaddle = makeBeep(660.0, 50)
        this.beepBrick = makeBeep(880.0, 70)
        return
    }

    /// True when `init` opened the SFX device successfully.
    pub sfxOk(): bool {
        this.sfx.id != 0
    }

    pub playWall() {
        let _ = this.sfx.queue(this.beepWall)
        return
    }
    pub playPaddle() {
        let _ = this.sfx.queue(this.beepPaddle)
        return
    }
    pub playBrick() {
        let _ = this.sfx.queue(this.beepBrick)
        return
    }

    /// Append another copy of the BGM loop when the BGM device's
    /// queued bytes have drained below one full loop. Call once
    /// per frame.
    pub refillBgm() {
        if this.bgm.id != 0 && this.bgm.queuedBytes < this.bgmBytes {
            let _ = this.bgm.queue(this.bgmLoop)
        }
    }
}
