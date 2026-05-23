/// WASAPI — Windows Audio Session API (Core Audio).
///
/// Modern Windows audio surface: enumerate endpoints, activate
/// an `IAudioClient` for shared / exclusive playback or capture,
/// pull buffers via `IAudioRenderClient` / `IAudioCaptureClient`.
///
/// COM-based: bring up via `CoInitializeEx` + `CoCreateInstance(
/// CLSID_MMDeviceEnumerator, ...)`.  Runtime lives in
/// `mmdevapi.dll` (`IMMDeviceEnumerator`) and `audioses.dll`
/// (everything else), but every entry below is reached through
/// the COM vtables — no flat `@lib pub fn` declarations needed.

use ole32 { GUID, IUnknown }
use winnt { HANDLE }
// ─── EDataFlow / ERole / DeviceState ─────────────────────────────

pub enum EDataFlow: u32 {
    render   = 0
    capture  = 1
    all      = 2
}

pub enum ERole: u32 {
    console        = 0
    multimedia     = 1
    communications = 2
}

@flags
pub enum DeviceState: u32 {
    active     = 0x00000001
    disabled   = 0x00000002
    notPresent = 0x00000004
    unplugged  = 0x00000008
    maskAll    = 0x0000000F
}

// ─── AudioClient share / stream / buffer flags ───────────────────

pub enum AudioClientShareMode: u32 {
    shared    = 0
    exclusive = 1
}

@flags
pub enum AudioClientStreamFlags: u32 {
    none                       = 0
    crossProcess               = 0x00010000
    loopback                   = 0x00020000
    eventCallback              = 0x00040000
    noPersist                  = 0x00080000
    rateAdjust                 = 0x00100000
    autoConvertPcm             = 0x80000000
    srcDefaultQuality          = 0x08000000
}

@flags
pub enum AudioClientBufferFlags: u32 {
    none                = 0
    dataDiscontinuity   = 0x1
    silent              = 0x2
    timestampError      = 0x4
}

pub enum AudioSessionState: u32 {
    inactive = 0
    active   = 1
    expired  = 2
}

// ─── AUDCLNT HRESULTs (commonly hit on first-run integration) ────
//
// Repr is `u32` so the bit pattern stays identical to the SDK
// header values (`AUDCLNT_E_*` are negative i32 HRESULTs at the
// ABI). Compare an HRESULT returned from a flat fn with
// `rc as u32 == AudClntResult.notInitialized as u32`.
pub enum AudClntResult: u32 {
    ok                        = 0
    notInitialized            = 0x88890001
    alreadyInitialized        = 0x88890002
    wrongEndpointType         = 0x88890003
    deviceInvalidated         = 0x88890004
    notStopped                = 0x88890005
    bufferTooLarge            = 0x88890006
    outOfOrder                = 0x88890007
    unsupportedFormat         = 0x88890008
    invalidSize               = 0x88890009
    deviceInUse               = 0x8889000A
    bufferOperationPending    = 0x8889000B
    threadNotRegistered       = 0x8889000C
    exclusiveModeNotAllowed   = 0x8889000E
    endpointCreateFailed      = 0x8889000F
    serviceNotRunning         = 0x88890010
    eventHandleNotExpected    = 0x88890011
    exclusiveModeOnly         = 0x88890012
    bufdurationPeriodNotEqual = 0x88890013
    eventHandleNotSet         = 0x88890014
    incorrectBufferSize       = 0x88890015
    bufferSizeError           = 0x88890016
    cpuUsageExceeded          = 0x88890017
}

// ─── REFERENCE_TIME — 100-ns ticks, signed 64-bit ────────────────
// (Used as buffer durations in IAudioClient.Initialize / GetDevicePeriod.)
//
// Common literals:
//   10 ms  =     100_000
//   100 ms =   1_000_000
//   1 s    =  10_000_000

@extern(C) {
    // ── WAVEFORMATEX subset (matches the winmm copy at the ABI) ──
    //
    // Re-declared here so call sites that only `use wasapi { ... }`
    // don't need to also pull in winmm.

    pub struct WAVEFORMATEX_WASAPI {
        wFormatTag:      u16
        nChannels:       u16
        nSamplesPerSec:  u32
        nAvgBytesPerSec: u32
        nBlockAlign:     u16
        wBitsPerSample:  u16
        cbSize:          u16
    }

    /// `WAVEFORMATEXTENSIBLE` — 40-byte extended format. Used for
    /// multi-channel layouts (>2 ch) and non-PCM containers.
    /// `Format.wFormatTag` must be `WAVE_FORMAT_EXTENSIBLE = 0xFFFE`
    /// and `Format.cbSize = 22`.
    pub struct WAVEFORMATEXTENSIBLE {
        Format:                WAVEFORMATEX_WASAPI
        _pad0:                 u16
        Samples_wValidBitsPerSample: u16
        _pad1:                 u16
        dwChannelMask:         u32
        SubFormat:             GUID
    }

    /// `PROPERTYKEY` — used by `IPropertyStore.GetValue`.
    ///
    /// The canonical friendly-name key is
    ///   `fmtid = a45c254e-df1c-4efd-8020-67d146a850e0`, `pid = 14`.
    /// Build it at the call site (or in a sibling .il alongside
    /// `wasapi_keys.il`) since ilang doesn't yet support struct-
    /// literal consts that reference a struct declared in the same
    /// `@extern(C)` block.
    pub struct PROPERTYKEY {
        fmtid: GUID
        pid:   u32
        _pad0: u32
    }

    /// `PROPVARIANT` — variant struct with a 4-byte type tag plus a
    /// 16-byte payload (LPWSTR / int / bytes / ...). Most fields
    /// land at offset 8 inside the payload. Use `pwszVal_lo`/`_hi`
    /// (= a `*u16` pointer when the type tag is `VT_LPWSTR = 31`)
    /// for the friendly-name read path.
    pub struct PROPVARIANT {
        vt:        u16
        _r0:       u16
        _r1:       u16
        _r2:       u16
        // 16-byte union payload, flattened.
        payload0:  u64    // pwszVal / lVal / ulVal / etc.
        payload1:  u64
    }
}

// ─── CLSID + IIDs ────────────────────────────────────────────────

/// CLSID_MMDeviceEnumerator = BCDE0395-E52F-467C-8E3D-C4579291692E
pub const CLSID_MMDeviceEnumerator: GUID = GUID {
    Data1: 0xbcde0395, Data2: 0xe52f, Data3: 0x467c,
    Data4lo: 0xc43d8e8e, Data4hi: 0x2e699179,
}

/// IID_IMMDeviceEnumerator = A95664D2-9614-4F35-A746-DE8DB63617E6
pub const IID_IMMDeviceEnumerator: GUID = GUID {
    Data1: 0xa95664d2, Data2: 0x9614, Data3: 0x4f35,
    Data4lo: 0x8d4d46a7, Data4hi: 0xe61736b6,
}

/// IID_IMMDeviceCollection = 0BD7A1BE-7A1A-44DB-8397-CC5392387B5E
pub const IID_IMMDeviceCollection: GUID = GUID {
    Data1: 0x0bd7a1be, Data2: 0x7a1a, Data3: 0x44db,
    Data4lo: 0xc5c39783, Data4hi: 0x5e7b3892,
}

/// IID_IMMDevice = D666063F-1587-4E43-81F1-B948E807363F
pub const IID_IMMDevice: GUID = GUID {
    Data1: 0xd666063f, Data2: 0x1587, Data3: 0x4e43,
    Data4lo: 0x48b9f181, Data4hi: 0x3f3607e8,
}

/// IID_IMMEndpoint = 1BE09788-6894-4089-8586-9A2A6C265AC5
pub const IID_IMMEndpoint: GUID = GUID {
    Data1: 0x1be09788, Data2: 0x6894, Data3: 0x4089,
    Data4lo: 0x2a9a8685, Data4hi: 0xc55a266c,
}

/// IID_IPropertyStore = 886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99
pub const IID_IPropertyStore: GUID = GUID {
    Data1: 0x886d8eeb, Data2: 0x8cf2, Data3: 0x4446,
    Data4lo: 0xba022d8d, Data4hi: 0x99cfbddb,
}

/// IID_IAudioClient = 1CB9AD4C-DBFA-4C32-B178-C2F568A703B2
pub const IID_IAudioClient: GUID = GUID {
    Data1: 0x1cb9ad4c, Data2: 0xdbfa, Data3: 0x4c32,
    Data4lo: 0xf5c278b1, Data4hi: 0xb203a768,
}

/// IID_IAudioClient2 = 726778CD-F60A-4EDA-82DE-E47610CD78AA
pub const IID_IAudioClient2: GUID = GUID {
    Data1: 0x726778cd, Data2: 0xf60a, Data3: 0x4eda,
    Data4lo: 0x76e4de82, Data4hi: 0xaa78cd10,
}

/// IID_IAudioClient3 = 7ED4EE07-8E67-4CD4-8C1A-2B7A5987AD42
pub const IID_IAudioClient3: GUID = GUID {
    Data1: 0x7ed4ee07, Data2: 0x8e67, Data3: 0x4cd4,
    Data4lo: 0x7a2b1a8c, Data4hi: 0x42ad8759,
}

/// IID_IAudioRenderClient = F294ACFC-3146-4483-A7BF-ADDCA7C260E2
pub const IID_IAudioRenderClient: GUID = GUID {
    Data1: 0xf294acfc, Data2: 0x3146, Data3: 0x4483,
    Data4lo: 0xdcadbfa7, Data4hi: 0xe260c2a7,
}

/// IID_IAudioCaptureClient = C8ADBD64-E71E-48A0-A4DE-185C395CD317
pub const IID_IAudioCaptureClient: GUID = GUID {
    Data1: 0xc8adbd64, Data2: 0xe71e, Data3: 0x48a0,
    Data4lo: 0x5c18dea4, Data4hi: 0x17d35c39,
}

/// IID_IAudioClock = CD63314F-3FBA-4A1B-812C-EF96358728E7
pub const IID_IAudioClock: GUID = GUID {
    Data1: 0xcd63314f, Data2: 0x3fba, Data3: 0x4a1b,
    Data4lo: 0xef2c2c81, Data4hi: 0xe7283596,
}

/// IID_IAudioStreamVolume = 93014887-242D-4068-8A15-CF5E93B90FE3
pub const IID_IAudioStreamVolume: GUID = GUID {
    Data1: 0x93014887, Data2: 0x242d, Data3: 0x4068,
    Data4lo: 0x5ecf158a, Data4hi: 0xe30fb993,
}

/// IID_IAudioEndpointVolume = 5CDF2C82-841E-4546-9722-0CF74078229A
pub const IID_IAudioEndpointVolume: GUID = GUID {
    Data1: 0x5cdf2c82, Data2: 0x841e, Data3: 0x4546,
    Data4lo: 0xf70c2297, Data4hi: 0x9a227840,
}

/// IID_ISimpleAudioVolume = 87CE5498-68D6-44E5-9215-6DA47EF883D8
pub const IID_ISimpleAudioVolume: GUID = GUID {
    Data1: 0x87ce5498, Data2: 0x68d6, Data3: 0x44e5,
    Data4lo: 0xa46d1592, Data4hi: 0xd883f87e,
}

// ─── COM interfaces (declarations) ───────────────────────────────

@extern(C) {
    /// `IMMDevice` — handle to a single audio endpoint.
    /// `Activate(IID, ClsCtx, params, ppInterface)` is the workhorse
    /// — pass `IID_IAudioClient` (or a relative) to bring up the
    /// shared/exclusive playback or capture pipeline against the
    /// endpoint.
    @com pub interface IMMDevice : IUnknown {
        Activate(
            iid:          *const GUID,
            dwClsCtx:     u32,
            pActivationParams: *void,
            ppInterface:  **void
        ): i32
        OpenPropertyStore(stgmAccess: u32, ppProperties: **void): i32
        GetId(ppstrId: **u16): i32
        GetState(pdwState: *u32): i32
    }

    /// `IMMEndpoint` — query-only side interface; returns the data
    /// flow direction of an `IMMDevice`. Obtain via QueryInterface.
    @com pub interface IMMEndpoint : IUnknown {
        GetDataFlow(pDataFlow: *u32): i32
    }

    /// `IMMDeviceCollection` — array view into the device list
    /// returned by `EnumAudioEndpoints`.
    @com pub interface IMMDeviceCollection : IUnknown {
        GetCount(pcDevices: *u32): i32
        Item(nDevice: u32, ppDevice: **void): i32
    }

    /// `IMMNotificationClient` — pure-callback interface; pass an
    /// implementation to `RegisterEndpointNotificationCallback`
    /// to receive device-state change events. Marked here so
    /// callers can hold an `*IMMNotificationClient` value safely
    /// even without implementing it from the ilang side.
    @com pub interface IMMNotificationClient : IUnknown {
        OnDeviceStateChanged(pwstrDeviceId: *const u16, dwNewState: u32): i32
        OnDeviceAdded(pwstrDeviceId: *const u16): i32
        OnDeviceRemoved(pwstrDeviceId: *const u16): i32
        OnDefaultDeviceChanged(flow: u32, role: u32, pwstrDefaultDeviceId: *const u16): i32
        OnPropertyValueChanged(pwstrDeviceId: *const u16, key: PROPERTYKEY): i32
    }

    /// `IMMDeviceEnumerator` — the entry point. CoCreate against
    /// `CLSID_MMDeviceEnumerator`, then enumerate or fetch the
    /// default device.
    @com pub interface IMMDeviceEnumerator : IUnknown {
        EnumAudioEndpoints(
            dataFlow:        u32,            // EDataFlow.*
            dwStateMask:     u32,            // DeviceState.*
            ppDevices:       **void
        ): i32
        GetDefaultAudioEndpoint(
            dataFlow:    u32,
            role:        u32,                // ERole.*
            ppEndpoint:  **void
        ): i32
        GetDevice(pwstrId: *const u16, ppDevice: **void): i32
        RegisterEndpointNotificationCallback(pClient: *IMMNotificationClient): i32
        UnregisterEndpointNotificationCallback(pClient: *IMMNotificationClient): i32
    }

    /// `IPropertyStore` — caller manages PROPVARIANT lifetime
    /// (`PropVariantClear` from ole32 after every successful read).
    @com pub interface IPropertyStore : IUnknown {
        GetCount(cProps: *u32): i32
        GetAt(iProp: u32, pkey: *PROPERTYKEY): i32
        GetValue(key: *const PROPERTYKEY, pv: *PROPVARIANT): i32
        SetValue(key: *const PROPERTYKEY, propvar: *const PROPVARIANT): i32
        Commit(): i32
    }

    /// `IAudioClient` — the device-bound playback / capture stream.
    @com pub interface IAudioClient : IUnknown {
        Initialize(
            ShareMode:          u32,                  // AudioClientShareMode
            StreamFlags:        u32,                  // AudioClientStreamFlags
            hnsBufferDuration:  i64,
            hnsPeriodicity:     i64,
            pFormat:            *const WAVEFORMATEX_WASAPI,
            AudioSessionGuid:   *const GUID
        ): i32
        GetBufferSize(pNumBufferFrames: *u32): i32
        GetStreamLatency(phnsLatency: *i64): i32
        GetCurrentPadding(pNumPaddingFrames: *u32): i32
        IsFormatSupported(
            ShareMode:           u32,
            pFormat:             *const WAVEFORMATEX_WASAPI,
            ppClosestMatch:      **void
        ): i32
        GetMixFormat(ppDeviceFormat: **WAVEFORMATEX_WASAPI): i32
        GetDevicePeriod(phnsDefaultDevicePeriod: *i64, phnsMinimumDevicePeriod: *i64): i32
        Start(): i32
        Stop(): i32
        Reset(): i32
        SetEventHandle(eventHandle: HANDLE): i32
        GetService(riid: *const GUID, ppv: **void): i32
    }

    /// `IAudioRenderClient` — push-side buffer accessor for
    /// playback streams. `GetBuffer(numFrames, &p)` returns a
    /// pointer to the next `numFrames * frameSize` bytes; fill
    /// them, then `ReleaseBuffer(numFrames, flags)`.
    @com pub interface IAudioRenderClient : IUnknown {
        GetBuffer(NumFramesRequested: u32, ppData: **u8): i32
        ReleaseBuffer(NumFramesWritten: u32, dwFlags: u32): i32
    }

    /// `IAudioCaptureClient` — pull-side buffer accessor for
    /// capture streams.
    @com pub interface IAudioCaptureClient : IUnknown {
        GetBuffer(
            ppData:             **u8,
            pNumFramesToRead:   *u32,
            pdwFlags:           *u32,
            pu64DevicePosition: *u64,
            pu64QPCPosition:    *u64
        ): i32
        ReleaseBuffer(NumFramesRead: u32): i32
        GetNextPacketSize(pNumFramesInNextPacket: *u32): i32
    }

    /// `IAudioClock` — sample-position clock for the stream.
    @com pub interface IAudioClock : IUnknown {
        GetFrequency(pu64Frequency: *u64): i32
        GetPosition(pu64Position: *u64, pu64QPCPosition: *u64): i32
        GetCharacteristics(pdwCharacteristics: *u32): i32
    }

    /// `IAudioStreamVolume` — per-channel gain on a session stream.
    @com pub interface IAudioStreamVolume : IUnknown {
        GetChannelCount(pdwCount: *u32): i32
        SetChannelVolume(dwIndex: u32, fLevel: f32): i32
        GetChannelVolume(dwIndex: u32, pfLevel: *f32): i32
        SetAllVolumes(dwCount: u32, pfVolumes: *const f32): i32
        GetAllVolumes(dwCount: u32, pfVolumes: *f32): i32
    }

    /// `IAudioEndpointVolume` — system-wide endpoint volume control.
    @com pub interface IAudioEndpointVolume : IUnknown {
        RegisterControlChangeNotify(pNotify: *void): i32
        UnregisterControlChangeNotify(pNotify: *void): i32
        GetChannelCount(pnChannelCount: *u32): i32
        SetMasterVolumeLevel(fLevelDB: f32, pguidEventContext: *const GUID): i32
        SetMasterVolumeLevelScalar(fLevel: f32, pguidEventContext: *const GUID): i32
        GetMasterVolumeLevel(pfLevelDB: *f32): i32
        GetMasterVolumeLevelScalar(pfLevel: *f32): i32
        SetChannelVolumeLevel(nChannel: u32, fLevelDB: f32, pguidEventContext: *const GUID): i32
        SetChannelVolumeLevelScalar(nChannel: u32, fLevel: f32, pguidEventContext: *const GUID): i32
        GetChannelVolumeLevel(nChannel: u32, pfLevelDB: *f32): i32
        GetChannelVolumeLevelScalar(nChannel: u32, pfLevel: *f32): i32
        SetMute(bMute: i32, pguidEventContext: *const GUID): i32
        GetMute(pbMute: *i32): i32
        GetVolumeStepInfo(pnStep: *u32, pnStepCount: *u32): i32
        VolumeStepUp(pguidEventContext: *const GUID): i32
        VolumeStepDown(pguidEventContext: *const GUID): i32
        QueryHardwareSupport(pdwHardwareSupportMask: *u32): i32
        GetVolumeRange(pflVolumeMindB: *f32, pflVolumeMaxdB: *f32, pflVolumeIncrementdB: *f32): i32
    }

    /// `ISimpleAudioVolume` — per-session volume (the slider the
    /// user sees in the volume mixer for a given app).
    @com pub interface ISimpleAudioVolume : IUnknown {
        SetMasterVolume(fLevel: f32, EventContext: *const GUID): i32
        GetMasterVolume(pfLevel: *f32): i32
        SetMute(bMute: i32, EventContext: *const GUID): i32
        GetMute(pbMute: *i32): i32
    }
}
