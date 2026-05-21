/// winmm — exercise the extended (second-pass) entry points:
///   * waveOutGetNumDevs / waveInGetNumDevs / midiOutGetNumDevs /
///     mixerGetNumDevs
///   * waveOutGetDevCapsA on device 0 (if any)
///   * waveInGetDevCapsA  on device 0 (if any)
///   * midiOutGetDevCapsA on device 0 (if any)
///   * timeGetDevCaps + timeBeginPeriod/timeEndPeriod
///
/// Every step is robust to "no device installed" — count==0 just
/// skips the corresponding caps probe.

use test
use windows {
    waveOutGetNumDevs, waveInGetNumDevs, midiOutGetNumDevs, mixerGetNumDevs,
    waveOutGetDevCapsA, waveInGetDevCapsA, midiOutGetDevCapsA,
    timeGetDevCaps, timeBeginPeriod, timeEndPeriod, timeGetTime,
    WAVEOUTCAPSA, WAVEINCAPSA, MIDIOUTCAPSA, TIMECAPS,
    MmResult,
}

@extern(C) {
    pub fn run() {
        // ── Device counts ─────────────────────────────────────────
        let nOut   = waveOutGetNumDevs()
        let nIn    = waveInGetNumDevs()
        let nMidi  = midiOutGetNumDevs()
        let nMixer = mixerGetNumDevs()
        // No assertion on exact counts — they vary by host. Just
        // confirm the entry points return a value (i.e. don't panic).
        test.expectTrue(nOut   < 256 as u32)
        test.expectTrue(nIn    < 256 as u32)
        test.expectTrue(nMidi  < 256 as u32)
        test.expectTrue(nMixer < 256 as u32)

        // ── waveOutGetDevCapsA on device 0 ────────────────────────
        if nOut > 0 as u32 {
            let caps = WAVEOUTCAPSA {
                wMid: 0, wPid: 0, vDriverVer: 0,
                szPname0: 0, szPname1: 0, szPname2: 0, szPname3: 0,
                dwFormats: 0, wChannels: 0, wReserved: 0, dwSupport: 0,
            }
            // sizeof(WAVEOUTCAPSA) = 84.
            let rc = waveOutGetDevCapsA(0 as u64, &caps, 84 as u32)
            test.expect(rc as i64, MmResult.noError as i64)
            test.expectTrue(caps.wChannels >= 1 as u16)
        }

        // ── waveInGetDevCapsA on device 0 ─────────────────────────
        if nIn > 0 as u32 {
            let caps = WAVEINCAPSA {
                wMid: 0, wPid: 0, vDriverVer: 0,
                szPname0: 0, szPname1: 0, szPname2: 0, szPname3: 0,
                dwFormats: 0, wChannels: 0, wReserved: 0,
            }
            // sizeof(WAVEINCAPSA) = 80.
            let rc = waveInGetDevCapsA(0 as u64, &caps, 80 as u32)
            test.expect(rc as i64, MmResult.noError as i64)
        }

        // ── midiOutGetDevCapsA on device 0 ────────────────────────
        if nMidi > 0 as u32 {
            let caps = MIDIOUTCAPSA {
                wMid: 0, wPid: 0, vDriverVer: 0,
                szPname0: 0, szPname1: 0, szPname2: 0, szPname3: 0,
                wTechnology: 0, wVoices: 0, wNotes: 0, wChannelMask: 0,
                dwSupport: 0,
            }
            let rc = midiOutGetDevCapsA(0 as u64, &caps, 50 as u32)
            test.expect(rc as i64, MmResult.noError as i64)
        }

        // ── Multimedia timer ──────────────────────────────────────
        let tc = TIMECAPS { wPeriodMin: 0, wPeriodMax: 0 }
        let trc = timeGetDevCaps(&tc, 8 as u32)
        test.expect(trc as i64, MmResult.noError as i64)
        test.expectTrue(tc.wPeriodMin >= 1 as u32)
        test.expectTrue(tc.wPeriodMax >= tc.wPeriodMin)

        // 1-ms tick promotion is always supported.
        test.expect(timeBeginPeriod(1 as u32) as i64, MmResult.noError as i64)
        let t0 = timeGetTime()
        // No sleep helper here — just confirm timeGetTime returns
        // something reasonable.
        test.expectTrue(t0 > 0 as u32)
        test.expect(timeEndPeriod(1 as u32) as i64, MmResult.noError as i64)
    }
}

run()
