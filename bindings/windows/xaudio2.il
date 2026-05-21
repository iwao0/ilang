/// XAudio2 — Microsoft's low-latency, voice-graph audio API for games.
///
/// Modern entry-point convention:
///   1. `XAudio2Create(&engine, 0, XAudio2Processor.useDefaultProcessor)`
///   2. `engine.CreateMasteringVoice(&masterVoice, ...)` (a 2 ch / device-rate
///      mix output that owns the output device).
///   3. `engine.CreateSourceVoice(&srcVoice, &wfx, ...)` per playable audio
///      clip; submit PCM frames with `srcVoice.SubmitSourceBuffer(&buf)`
///      then `srcVoice.Start(0, 0)`.
///   4. Tear down with `srcVoice.DestroyVoice()` /
///      `masterVoice.DestroyVoice()` / `engine.Release()`.
///
/// Runtime DLL ships as `XAudio2_9.dll` on Windows 10+; older systems
/// expose `XAudio2_8.dll`. The `@extern(C, "xaudio2_9", "xaudio2_8")`
/// fallback below picks whichever loads.

use ole32 { IUnknown }

// ─── Flags ────────────────────────────────────────────────────────

@flags
pub enum XAudio2Flags: u32 {
    none                          = 0
    debugEngine                   = 0x0001
    voiceNoPitch                  = 0x0002
    voiceNoSrc                    = 0x0004
    voiceUseFilter                = 0x0020
    playTails                     = 0x0020
    endOfStream                   = 0x0040
    sendUseFilter                 = 0x0080
    voiceNoSampleRateConversion   = 0x0100
    stopEngineWhenIdle            = 0x2000
}

@flags
pub enum XAudio2BufferFlags: u32 {
    none        = 0
    endOfStream = 0x0040
}

@flags
pub enum XAudio2LogFlags: u32 {
    none        = 0
    errors      = 0x0001
    warnings    = 0x0002
    info        = 0x0004
    detail      = 0x0008
    apiCalls    = 0x0010
    funcCalls   = 0x0020
    timing      = 0x0040
    locks       = 0x0080
    memory      = 0x0100
    streaming   = 0x1000
}

/// `XAUDIO2_PROCESSOR` — bit mask of CPU cores to pin the audio
/// thread to. `useDefaultProcessor` (0xFFFFFFFF on Win10+) lets the
/// OS pick.
pub enum XAudio2Processor: u32 {
    processor1            = 0x00000001
    processor2            = 0x00000002
    processor3            = 0x00000004
    processor4            = 0x00000008
    anyProcessor          = 0xFFFFFFFF
    useDefaultProcessor   = 0xFFFFFFFF
}

/// `XAUDIO2_FILTER_TYPE` — picked by `XAUDIO2_FILTER_PARAMETERS.Type`.
pub enum XAudio2FilterType: u32 {
    lowPassFilter    = 0
    bandPassFilter   = 1
    highPassFilter   = 2
    notchFilter      = 3
    lowPassOnePole   = 4
    highPassOnePole  = 5
}

/// XAudio2 HRESULTs — most calls return `S_OK` (0) or one of these.
pub enum XAudio2Result: u32 {
    ok                  = 0
    invalidCall         = 0x88960001
    xmaDecoderError     = 0x88960002
    xapoCreationFailed  = 0x88960003
    deviceInvalidated   = 0x88960004
}

/// Sentinels.
///
/// `XAUDIO2_COMMIT_NOW = 0` — pass as `OperationSet` for state
/// changes that should apply immediately.
///
/// `XAUDIO2_MAX_QUEUED_BUFFERS = 64` — driver-side per-voice queue
/// limit. Submitting past this returns `INVALID_CALL`.
pub const XAUDIO2_COMMIT_NOW: u32 = 0
pub const XAUDIO2_MAX_QUEUED_BUFFERS: u32 = 64
pub const XAUDIO2_DEFAULT_CHANNELS: u32 = 0
pub const XAUDIO2_DEFAULT_SAMPLERATE: u32 = 0
pub const XAUDIO2_DEFAULT_AUDIO_CATEGORY: u32 = 6   // AudioCategory_GameEffects

// ─── IIDs ─────────────────────────────────────────────────────────

use ole32 { GUID }

/// IID_IXAudio2 = 2B02E3CF-2E0B-4ec3-BE45-1B2A3FE7210D
pub const IID_IXAudio2: GUID = GUID {
    Data1: 0x2b02e3cf, Data2: 0x2e0b, Data3: 0x4ec3,
    Data4lo: 0x2a1b45be, Data4hi: 0x0d21e73f,
}

@extern(C, "xaudio2_9", "xaudio2_8") {
    // ── WAVEFORMATEX mirror ───────────────────────────────────────
    //
    // Re-declared so call sites that only `use windows { ... }` and
    // pick up xaudio2's surface don't also need to drag in winmm's
    // copy. The two are layout-identical (18 bytes).
    pub struct WAVEFORMATEX_XAUDIO2 {
        wFormatTag:      u16
        nChannels:       u16
        nSamplesPerSec:  u32
        nAvgBytesPerSec: u32
        nBlockAlign:     u16
        wBitsPerSample:  u16
        cbSize:          u16
    }

    /// `XAUDIO2_BUFFER` — one playback buffer queued onto a source
    /// voice. `pAudioData` points at PCM frames; the voice doesn't
    /// copy the data, so it must stay alive until `OnBufferEnd`
    /// fires (or the voice is destroyed).
    pub struct XAUDIO2_BUFFER {
        Flags:      u32              // XAudio2BufferFlags
        AudioBytes: u32
        pAudioData: *const u8
        PlayBegin:  u32
        PlayLength: u32
        LoopBegin:  u32
        LoopLength: u32
        LoopCount:  u32
        pContext:   *void
    }

    /// `XAUDIO2_BUFFER_WMA` — companion buffer for WMA decode hints.
    pub struct XAUDIO2_BUFFER_WMA {
        pDecodedPacketCumulativeBytes: *const u32
        PacketCount:                   u32
    }

    /// `XAUDIO2_VOICE_DETAILS` — returned by `GetVoiceDetails`.
    pub struct XAUDIO2_VOICE_DETAILS {
        CreationFlags:   u32
        ActiveFlags:     u32
        InputChannels:   u32
        InputSampleRate: u32
    }

    /// `XAUDIO2_SEND_DESCRIPTOR` — one entry of a voice's output
    /// destination list. `pOutputVoice` is the receiver voice
    /// (a submix or the mastering voice).
    pub struct XAUDIO2_SEND_DESCRIPTOR {
        Flags:        u32           // XAudio2Flags.sendUseFilter etc.
        pOutputVoice: *void         // IXAudio2Voice*
    }

    /// `XAUDIO2_VOICE_SENDS` — array of send descriptors.
    pub struct XAUDIO2_VOICE_SENDS {
        SendCount: u32
        _pad0:     u32
        pSends:    *const XAUDIO2_SEND_DESCRIPTOR
    }

    /// `XAUDIO2_EFFECT_DESCRIPTOR` — one XAPO effect in a voice's
    /// effect chain.
    pub struct XAUDIO2_EFFECT_DESCRIPTOR {
        pEffect:        *void       // IUnknown* (the XAPO instance)
        InitialState:   i32
        OutputChannels: u32
    }

    /// `XAUDIO2_EFFECT_CHAIN`.
    pub struct XAUDIO2_EFFECT_CHAIN {
        EffectCount:        u32
        _pad0:              u32
        pEffectDescriptors: *const XAUDIO2_EFFECT_DESCRIPTOR
    }

    /// `XAUDIO2_FILTER_PARAMETERS` — set per-voice filter when the
    /// voice was created with `XAudio2Flags.voiceUseFilter`.
    pub struct XAUDIO2_FILTER_PARAMETERS {
        Type:      u32   // XAudio2FilterType
        Frequency: f32
        OneOverQ:  f32
    }

    /// `XAUDIO2_PERFORMANCE_DATA` — returned by
    /// `IXAudio2::GetPerformanceData`. Long but mostly counters.
    pub struct XAUDIO2_PERFORMANCE_DATA {
        AudioCyclesSinceLastQuery: u64
        TotalCyclesSinceLastQuery: u64
        MinimumCyclesPerQuantum:   u32
        MaximumCyclesPerQuantum:   u32
        MemoryUsageInBytes:        u32
        CurrentLatencyInSamples:   u32
        GlitchesSinceEngineStarted: u32
        ActiveSourceVoiceCount:    u32
        TotalSourceVoiceCount:     u32
        ActiveSubmixVoiceCount:    u32
        ActiveResamplerCount:      u32
        ActiveMatrixMixCount:      u32
        ActiveXmaSourceVoices:     u32
        ActiveXmaStreams:          u32
    }

    /// `XAUDIO2_DEBUG_CONFIGURATION` — pass to
    /// `SetDebugConfiguration` to route trace output.
    pub struct XAUDIO2_DEBUG_CONFIGURATION {
        TraceMask:       u32   // XAudio2LogFlags
        BreakMask:       u32
        LogThreadID:     i32
        LogFileline:     i32
        LogFunctionName: i32
        LogTiming:       i32
    }

    // ── Flat entry ────────────────────────────────────────────────

    /// Creates an XAudio2 engine instance.
    /// `Flags`: 0 for normal, `XAudio2Flags.debugEngine` to attach
    ///          to the debug runtime.
    /// `XAudio2Processor`: bit mask (or `useDefaultProcessor`).
    @lib pub fn XAudio2Create(
        ppXAudio2:         **void,        // IXAudio2*
        Flags:             u32,
        XAudio2Processor:  u32
    ): i32

    // ── COM / virtual interfaces ──────────────────────────────────
    //
    // `IXAudio2` is COM (inherits IUnknown). The voice interfaces
    // (`IXAudio2Voice` and its three concretes) are NOT COM — they
    // expose a vtable but no IUnknown chain; their lifetime is
    // explicit via `DestroyVoice`. ilang's @com lowering emits the
    // raw vtable dispatch in either case; we declare them with no
    // parent so the slot numbering starts at 0.

    /// `IXAudio2Voice` — abstract base for SourceVoice / SubmixVoice
    /// / MasteringVoice. Slot positions match the SDK header so
    /// concretes pick up the inherited methods at the right offsets.
    @com pub interface IXAudio2Voice {
        GetVoiceDetails(pVoiceDetails: *XAUDIO2_VOICE_DETAILS)
        SetOutputVoices(pSendList: *const XAUDIO2_VOICE_SENDS): i32
        SetEffectChain(pEffectChain: *const XAUDIO2_EFFECT_CHAIN): i32
        EnableEffect(EffectIndex: u32, OperationSet: u32): i32
        DisableEffect(EffectIndex: u32, OperationSet: u32): i32
        GetEffectState(EffectIndex: u32, pEnabled: *i32)
        SetEffectParameters(
            EffectIndex:  u32,
            pParameters:  *const void,
            ParametersByteSize: u32,
            OperationSet: u32
        ): i32
        GetEffectParameters(
            EffectIndex:  u32,
            pParameters:  *void,
            ParametersByteSize: u32
        ): i32
        SetFilterParameters(
            pParameters:  *const XAUDIO2_FILTER_PARAMETERS,
            OperationSet: u32
        ): i32
        GetFilterParameters(pParameters: *XAUDIO2_FILTER_PARAMETERS)
        SetOutputFilterParameters(
            pDestinationVoice: *void,
            pParameters:       *const XAUDIO2_FILTER_PARAMETERS,
            OperationSet:      u32
        ): i32
        GetOutputFilterParameters(
            pDestinationVoice: *void,
            pParameters:       *XAUDIO2_FILTER_PARAMETERS
        )
        SetVolume(Volume: f32, OperationSet: u32): i32
        GetVolume(pVolume: *f32)
        SetChannelVolumes(
            Channels:     u32,
            pVolumes:     *const f32,
            OperationSet: u32
        ): i32
        GetChannelVolumes(Channels: u32, pVolumes: *f32)
        SetOutputMatrix(
            pDestinationVoice:   *void,
            SourceChannels:      u32,
            DestinationChannels: u32,
            pLevelMatrix:        *const f32,
            OperationSet:        u32
        ): i32
        GetOutputMatrix(
            pDestinationVoice:   *void,
            SourceChannels:      u32,
            DestinationChannels: u32,
            pLevelMatrix:        *f32
        )
        /// Tear down the voice. Required even when the engine is
        /// being released — voices aren't refcounted.
        DestroyVoice()
    }

    /// `IXAudio2SourceVoice` — playback voice for one stream of PCM
    /// (or compressed) audio. Submit `XAUDIO2_BUFFER` chunks via
    /// `SubmitSourceBuffer`.
    @com pub interface IXAudio2SourceVoice : IXAudio2Voice {
        Start(Flags: u32, OperationSet: u32): i32
        Stop(Flags: u32, OperationSet: u32): i32
        SubmitSourceBuffer(
            pBuffer:    *const XAUDIO2_BUFFER,
            pBufferWMA: *const XAUDIO2_BUFFER_WMA
        ): i32
        FlushSourceBuffers(): i32
        Discontinuity(): i32
        ExitLoop(OperationSet: u32): i32
        GetState(pVoiceState: *void, Flags: u32)
        SetFrequencyRatio(Ratio: f32, OperationSet: u32): i32
        GetFrequencyRatio(pRatio: *f32)
        SetSourceSampleRate(NewSourceSampleRate: u32): i32
    }

    /// `IXAudio2SubmixVoice` — buses for grouping source voices
    /// before they reach the mastering voice. No own methods beyond
    /// the base.
    @com pub interface IXAudio2SubmixVoice : IXAudio2Voice {
    }

    /// `IXAudio2MasteringVoice` — the device-bound output voice.
    /// Exactly one per engine.
    @com pub interface IXAudio2MasteringVoice : IXAudio2Voice {
        GetChannelMask(pChannelmask: *u32): i32
    }

    /// `IXAudio2EngineCallback` — engine-wide event sink. Provide
    /// an implementation and register with
    /// `IXAudio2::RegisterForCallbacks`; the engine fires these on
    /// every audio quantum (≈ 10 ms).
    @com pub interface IXAudio2EngineCallback {
        OnProcessingPassStart()
        OnProcessingPassEnd()
        OnCriticalError(Error: i32)
    }

    /// `IXAudio2VoiceCallback` — per-voice event sink for source
    /// voices. Pass at voice creation; XAudio2 fires these on the
    /// audio thread.
    @com pub interface IXAudio2VoiceCallback {
        OnVoiceProcessingPassStart(BytesRequired: u32)
        OnVoiceProcessingPassEnd()
        OnStreamEnd()
        OnBufferStart(pBufferContext: *void)
        OnBufferEnd(pBufferContext: *void)
        OnLoopEnd(pBufferContext: *void)
        OnVoiceError(pBufferContext: *void, Error: i32)
    }

    /// `IXAudio2` — the engine. CoCreate-free: `XAudio2Create`
    /// returns it directly. Inherits from `IUnknown`.
    @com pub interface IXAudio2 : IUnknown {
        RegisterForCallbacks(pCallback: *IXAudio2EngineCallback): i32
        UnregisterForCallbacks(pCallback: *IXAudio2EngineCallback)
        CreateSourceVoice(
            ppSourceVoice:  **void,
            pSourceFormat:  *const WAVEFORMATEX_XAUDIO2,
            Flags:          u32,
            MaxFrequencyRatio: f32,
            pCallback:      *IXAudio2VoiceCallback,
            pSendList:      *const XAUDIO2_VOICE_SENDS,
            pEffectChain:   *const XAUDIO2_EFFECT_CHAIN
        ): i32
        CreateSubmixVoice(
            ppSubmixVoice:  **void,
            InputChannels:  u32,
            InputSampleRate: u32,
            Flags:          u32,
            ProcessingStage: u32,
            pSendList:      *const XAUDIO2_VOICE_SENDS,
            pEffectChain:   *const XAUDIO2_EFFECT_CHAIN
        ): i32
        CreateMasteringVoice(
            ppMasteringVoice: **void,
            InputChannels:    u32,
            InputSampleRate:  u32,
            Flags:            u32,
            szDeviceId:       *const u16,    // NULL = default device
            pEffectChain:     *const XAUDIO2_EFFECT_CHAIN,
            AudioCategory:    u32
        ): i32
        StartEngine(): i32
        StopEngine()
        CommitChanges(OperationSet: u32): i32
        GetPerformanceData(pPerfData: *XAUDIO2_PERFORMANCE_DATA)
        SetDebugConfiguration(
            pDebugConfiguration: *const XAUDIO2_DEBUG_CONFIGURATION,
            pReserved:           *void
        )
    }
}
