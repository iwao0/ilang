// Short PCM beep generators + the BGM arpeggio loop. All buffers
// are i16 mono at SAMPLE_RATE.

use math

const SAMPLE_RATE: i32 = 44100

// Short sine-wave beep with a 5ms attack/release ramp so the start
// and end don't click.
fn makeBeep(freq_hz: f64, ms: i32): i16[] {
    let n = (SAMPLE_RATE * ms) / 1000
    let buf: i16[] = []
    let two_pi: f64 = 2.0 * math.pi
    let i: i32 = 0
    while i < n {
        let ramp = SAMPLE_RATE / 200
        let env: f64 = if i < ramp {
            (i as f64) / (ramp as f64)
        } elif i > n - ramp {
            ((n - i) as f64) / (ramp as f64)
        } else {
            1.0
        }
        let t = (i as f64) / (SAMPLE_RATE as f64)
        let sample = math.sin(two_pi * freq_hz * t) * 8000.0 * env
        buf.push(sample as i16)
        i += 1
    }
    buf
}

// Single chiptune note (square wave) of `ms` milliseconds. Used as
// the building block for the BGM loop.
fn makeNote(freq_hz: f64, ms: i32, vol: f64): i16[] {
    let n = (SAMPLE_RATE * ms) / 1000
    let buf: i16[] = []
    let i: i32 = 0
    while i < n {
        let ramp = SAMPLE_RATE / 100   // 10ms attack/decay
        let env: f64 = if i < ramp {
            (i as f64) / (ramp as f64)
        } elif i > n - ramp {
            ((n - i) as f64) / (ramp as f64)
        } else {
            1.0
        }
        let t = (i as f64) / (SAMPLE_RATE as f64)
        // Square wave from sin sign — gives the classic chip-style
        // bite without needing a phase accumulator.
        let s = math.sin(2.0 * math.pi * freq_hz * t)
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
fn makeBgmLoop(): i16[] {
    let buf: i16[] = []
    let beat_ms: i32 = 180
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
        let note = makeNote(notes[i], beat_ms, 0.16)
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
