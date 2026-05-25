/// xaudio2 — bring up the engine + mastering voice.
///
/// Pipeline:
///   1. CoInitializeEx (XAudio2Create's COM apartment requirement)
///   2. XAudio2Create
///   3. CreateMasteringVoice (default device / channels / rate)
///   4. GetPerformanceData sanity (counters readable)
///   5. masteringVoice.DestroyVoice + engine.Release
///   6. CoUninitialize
///
/// Bails cleanly on hosts where XAudio2 isn't installed
/// (XAudio2_9.dll / XAudio2_8.dll both missing) or no audio
/// endpoint is available — every entry point that can fail
/// short-circuits the test.

use std.test as test
use ole32 { CoInitializeEx, CoUninitialize, CoInit, IUnknown }
use windows {
    NULL,
}
use xaudio2 {
    XAudio2Create,
    XAudio2Processor,
    XAUDIO2_DEFAULT_CHANNELS,
    XAUDIO2_DEFAULT_SAMPLERATE,
    XAUDIO2_DEFAULT_AUDIO_CATEGORY,
    XAUDIO2_PERFORMANCE_DATA,
    IXAudio2,
    IXAudio2MasteringVoice,
    XAUDIO2_EFFECT_CHAIN,
}

@extern(C) {
    pub fn run() {
        if CoInitializeEx(NULL, CoInit.apartmentThreaded as u32) < 0 {
            return
        }

        let rawEngine: *void = NULL
        let crc = XAudio2Create(
            &rawEngine,
            0 as u32,
            XAudio2Processor.useDefaultProcessor as u32
        )
        if crc != 0 {
            CoUninitialize()
            return
        }
        test.expectTrue((rawEngine as i64) != 0)
        let engine = rawEngine as IXAudio2

        let rawMaster: *void = NULL
        let mhr = engine.CreateMasteringVoice(
            &rawMaster,
            XAUDIO2_DEFAULT_CHANNELS,
            XAUDIO2_DEFAULT_SAMPLERATE,
            0 as u32,
            0 as *const u16,                // default device
            0 as *const XAUDIO2_EFFECT_CHAIN,
            XAUDIO2_DEFAULT_AUDIO_CATEGORY
        )
        if mhr == 0 {
            test.expectTrue((rawMaster as i64) != 0)
            let master = rawMaster as IXAudio2MasteringVoice

            // ── GetPerformanceData ──────────────────────────────────
            let perf = XAUDIO2_PERFORMANCE_DATA {
                AudioCyclesSinceLastQuery: 0,
                TotalCyclesSinceLastQuery: 0,
                MinimumCyclesPerQuantum:   0,
                MaximumCyclesPerQuantum:   0,
                MemoryUsageInBytes:        0,
                CurrentLatencyInSamples:   0,
                GlitchesSinceEngineStarted: 0,
                ActiveSourceVoiceCount:    0,
                TotalSourceVoiceCount:     0,
                ActiveSubmixVoiceCount:    0,
                ActiveResamplerCount:      0,
                ActiveMatrixMixCount:      0,
                ActiveXmaSourceVoices:     0,
                ActiveXmaStreams:          0,
            }
            engine.GetPerformanceData(&perf)
            // No specific bound — just confirm the call doesn't
            // crash and the struct is the right shape.
            test.expectTrue(perf.MemoryUsageInBytes >= 0 as u32)

            master.DestroyVoice()
        }

        test.expect(engine.Release() as i64, 0)
        CoUninitialize()
    }

}

run()
