/// winmm.dll — Windows Multimedia: simple audio playback,
/// waveform I/O, MIDI, joysticks, and high-resolution timers.

// ─── PlaySound flags (SND_*) ─────────────────────────────────────
@flags
pub enum PlaySoundFlags: u32 {
    sync        = 0x00000000   // play synchronously (default)
    async       = 0x00000001   // play asynchronously
    noDefault   = 0x00000002   // no default sound if not found
    memory      = 0x00000004   // pszSound is a pointer to in-memory sound
    loop_       = 0x00000008   // loop until next PlaySound(NULL,...)
    noStop      = 0x00000010   // do not stop currently playing sound
    nowait      = 0x00002000   // return immediately if driver busy
    filename    = 0x00020000   // pszSound is a filename
    resource    = 0x00040004   // pszSound is a resource in hmod
    purge       = 0x00000040   // stop playing / discard cache
    application = 0x00000080   // look in app section of win.ini
    alias       = 0x00010000   // pszSound is a system-event alias
    aliasId     = 0x00110000   // pszSound is a predefined alias ID
    unicode     = 0x00020000   // pszSound (W variant) is Unicode
    sentry      = 0x00080000   // play system sentry sound
}

// ─── waveOut / waveIn error codes ────────────────────────────────
pub enum WaveError: u32 {
    noError        = 0
    badDeviceId    = 2
    driverFailed   = 4
    invalidHandle  = 5
    noDriver       = 6
    noMem          = 7
    badFormat      = 32
    stillPlaying   = 33
    unprepared     = 34
    sync           = 35
}

// ─── waveOut / waveIn open flags (WAVE_*) ────────────────────────
@flags
pub enum WaveOpenFlags: u32 {
    formatQuery  = 0x0001   // query only — don't open
    allowSync    = 0x0002
    mapped       = 0x0004   // reserved for memory-mapped wave
    formatDirect = 0x0008   // don't pass to ACM format converters
    formatQuerySupported = 0x0010
    callbackNull    = 0x00000000
    callbackWindow  = 0x00010000   // dwCallback is HWND
    callbackThread  = 0x00020000   // dwCallback is thread ID
    callbackFunction = 0x00030000  // dwCallback is FARPROC
    callbackEvent   = 0x00050000   // dwCallback is EVENT handle
}

// ─── WAVEHDR flags (WHDR_*) ──────────────────────────────────────
@flags
pub enum WaveHdrFlags: u32 {
    done       = 0x00000001   // set by driver when buffer is finished
    prepared   = 0x00000002   // set by waveOutPrepareHeader
    beginLoop  = 0x00000004
    endLoop    = 0x00000008
    inQueue    = 0x00000010
}

// ─── Multimedia timer (timeBeginPeriod etc.) ─────────────────────
pub enum TimerError: u32 {
    noError   = 0
    base      = 96
    nocando   = 97   // TIMERR_NOCANDO — resolution out of range
    struct_   = 129  // TIMERR_STRUCT  — bad TIMECAPS pointer
}

// ─── Joystick return codes (JOYERR_*) ────────────────────────────
pub enum JoystickError: u32 {
    noError    = 0
    parms      = 165
    noCanDo    = 166
    unplugged  = 167
}

// ─── Joystick button flags (JOY_BUTTON*) ─────────────────────────
@flags
pub enum JoystickButtons: u32 {
    button1  = 0x0001
    button2  = 0x0002
    button3  = 0x0004
    button4  = 0x0008
}

// ─── MMRESULT — generic return code for the wave / mixer / midi
// entries (MMSYSERR_*). Higher-level wrappers translate them; the
// codes shared between subsystems live here.
pub enum MmResult: u32 {
    noError      = 0
    error        = 1
    badDeviceId  = 2
    notEnabled   = 3
    allocated    = 4
    invalHandle  = 5
    noDriver     = 6
    noMem        = 7
    notSupported = 8
    badErrNum    = 9
    invalFlag    = 10
    invalParam   = 11
    handleBusy   = 12
    invalidAlias = 13
    badDb        = 14
    keyNotFound  = 15
    readError    = 16
    writeError   = 17
    deleteError  = 18
    valNotFound  = 19
    noDriverCb   = 20
}

// ─── MIDI flags for MIDIHDR ──────────────────────────────────────
@flags
pub enum MidiHdrFlags: u32 {
    done       = 0x00000001
    prepared   = 0x00000002
    inQueue    = 0x00000004
    isStream   = 0x00000008
}

// ─── Mixer constants ─────────────────────────────────────────────
@flags
pub enum MixerObjectFlag: u32 {
    handle           = 0x80000000
    mixer            = 0x00000000
    waveOut          = 0x10000000
    waveIn           = 0x20000000
    midiOut          = 0x30000000
    midiIn           = 0x40000000
    aux              = 0x50000000
}

@flags
pub enum MixerLineFlags: u32 {
    active     = 0x00000001
    disconnected = 0x00008000
    source     = 0x80000000
}

@extern(C, "winmm") {
    // ── Opaque handles ────────────────────────────────────────────

    /// Waveform audio output handle.
    pub struct HWAVEOUT {}

    /// Waveform audio input handle.
    pub struct HWAVEIN {}

    // ── Audio format ──────────────────────────────────────────────

    /// `WAVEFORMATEX` — 18 bytes.
    /// For PCM audio set `wFormatTag` = 1 (WAVE_FORMAT_PCM) and
    /// `cbSize` = 0.  Extra format data follows immediately after in
    /// extended variants (WAVEFORMATEXTENSIBLE), but those are
    /// rare for basic playback.
    pub struct WAVEFORMATEX {
        wFormatTag:      u16   // 1 = WAVE_FORMAT_PCM
        nChannels:       u16   // 1 = mono, 2 = stereo
        nSamplesPerSec:  u32   // e.g. 44100
        nAvgBytesPerSec: u32   // nSamplesPerSec × nBlockAlign
        nBlockAlign:     u16   // nChannels × (wBitsPerSample / 8)
        wBitsPerSample:  u16   // 8 or 16
        cbSize:          u16   // extra info after struct; 0 for PCM
    }

    // ── Waveform buffer header ────────────────────────────────────

    /// `WAVEHDR` — 32 bytes on x64. Describes one audio buffer.
    /// Must be prepared with `waveOutPrepareHeader` before use and
    /// unprepared with `waveOutUnprepareHeader` before freeing.
    pub struct WAVEHDR {
        lpData:          *char   // pointer to audio data
        dwBufferLength:  u32
        dwBytesRecorded: u32
        dwUser:          u64     // user data
        dwFlags:         u32     // WaveHdrFlags
        dwLoops:         u32     // loop count
        lpNext:          *void   // reserved — internal linked-list
        reserved:        u64
    }

    // ── Joystick info ─────────────────────────────────────────────

    /// `JOYINFOEX` — 52 bytes. Filled by `joyGetPosEx`.
    /// Set `dwSize` = 52 and `dwFlags` before calling.
    pub struct JOYINFOEX {
        dwSize:     u32
        dwFlags:    u32
        dwXpos:     u32
        dwYpos:     u32
        dwZpos:     u32
        dwRpos:     u32
        dwUpos:     u32
        dwVpos:     u32
        dwButtons:  u32   // JoystickButtons
        dwButtonNumber: u32
        dwPOV:      u32
        dwReserved1: u32
        dwReserved2: u32
    }

    // ── Simple sound playback ─────────────────────────────────────

    /// Plays a sound file, resource, or system-event alias.
    /// `pszSound`: filename, alias name, or NULL to stop.
    /// `hmod`: module containing the resource (0 for file/alias).
    /// `fdwSound`: PlaySoundFlags.*.
    @lib pub fn PlaySoundA(
        pszSound: *const char,
        hmod:     *void,      // HMODULE, or 0
        fdwSound: u32
    ): i32

    // ── waveOut ───────────────────────────────────────────────────

    /// Returns the number of waveform output devices.
    @lib pub fn waveOutGetNumDevs(): u32

    /// Opens a waveform output device. `uDeviceID`: device index or
    /// WAVE_MAPPER (0xFFFFFFFF) to let Windows choose.
    /// `dwCallback`/`dwInstance`/`fdwOpen`: set per WaveOpenFlags.
    @lib pub fn waveOutOpen(
        phwo:       **HWAVEOUT,
        uDeviceID:  u32,
        pwfx:       *const WAVEFORMATEX,
        dwCallback: u64,
        dwInstance: u64,
        fdwOpen:    u32
    ): u32

    @lib pub fn waveOutClose(hwo: *HWAVEOUT): u32

    @lib pub fn waveOutPrepareHeader(
        hwo:       *HWAVEOUT,
        pwh:       *WAVEHDR,
        cbwh:      u32
    ): u32

    @lib pub fn waveOutUnprepareHeader(
        hwo:  *HWAVEOUT,
        pwh:  *WAVEHDR,
        cbwh: u32
    ): u32

    /// Queues a prepared buffer for playback.
    @lib pub fn waveOutWrite(
        hwo:  *HWAVEOUT,
        pwh:  *WAVEHDR,
        cbwh: u32
    ): u32

    @lib pub fn waveOutPause(hwo: *HWAVEOUT): u32
    @lib pub fn waveOutRestart(hwo: *HWAVEOUT): u32
    @lib pub fn waveOutReset(hwo: *HWAVEOUT): u32

    /// Sets the playback volume. `dwVolume` = `(right << 16) | left`
    /// where each channel is 0x0000 (silence) – 0xFFFF (max).
    @lib pub fn waveOutSetVolume(hwo: *HWAVEOUT, dwVolume: u32): u32
    @lib pub fn waveOutGetVolume(hwo: *HWAVEOUT, pdwVolume: *u32): u32

    // ── Multimedia timer ──────────────────────────────────────────

    /// Sets the minimum timer resolution to `uPeriod` milliseconds.
    /// Always call `timeEndPeriod` with the same value when done.
    /// Lower values increase timer accuracy but raise CPU usage.
    @lib pub fn timeBeginPeriod(uPeriod: u32): u32

    @lib pub fn timeEndPeriod(uPeriod: u32): u32

    /// Returns the current time in milliseconds. More accurate than
    /// `GetTickCount` after `timeBeginPeriod(1)`.
    @lib pub fn timeGetTime(): u32

    // ── Joystick ──────────────────────────────────────────────────

    /// Returns the number of joysticks supported (not necessarily
    /// plugged in). Typically 2 (JOYSTICKID1 and JOYSTICKID2).
    @lib pub fn joyGetNumDevs(): u32

    /// Fills `pji` with the extended position of joystick `uJoyID`
    /// (0-based). Returns JoystickError.noError on success.
    @lib pub fn joyGetPosEx(uJoyID: u32, pji: *JOYINFOEX): u32

    // ── Additional handles ────────────────────────────────────────

    pub struct HMIDIOUT {}
    pub struct HMIDIIN {}
    pub struct HMIXER {}

    // ── Device caps (WAVEOUTCAPSA / WAVEINCAPSA / MIDIOUTCAPSA) ───
    //
    // Full SDK caps blocks carry a 32-char name array; we expose the
    // numeric fields and treat the name slot as an opaque 32-byte
    // run that the caller copies out via raw pointer if it needs to
    // display the device label.

    /// `WAVEOUTCAPSA` — driver capability block for waveOutGetDevCapsA.
    /// Size = 84 bytes on x64.
    pub struct WAVEOUTCAPSA {
        wMid:         u16
        wPid:         u16
        vDriverVer:   u32
        szPname0:     u64       // 32-byte device name (ANSI) — read
        szPname1:     u64       //   the ANSI string out of these
        szPname2:     u64       //   four u64s after the call.
        szPname3:     u64
        dwFormats:    u32       // bitmask of WAVE_FORMAT_* the driver supports
        wChannels:    u16
        wReserved:    u16
        dwSupport:    u32
    }

    /// `WAVEINCAPSA` — mirror struct for the input side, same shape
    /// minus `dwSupport`. Size = 80 bytes on x64.
    pub struct WAVEINCAPSA {
        wMid:         u16
        wPid:         u16
        vDriverVer:   u32
        szPname0:     u64
        szPname1:     u64
        szPname2:     u64
        szPname3:     u64
        dwFormats:    u32
        wChannels:    u16
        wReserved:    u16
    }

    /// `MIDIOUTCAPSA` — 50 bytes on x64. Same layout convention as
    /// `WAVEOUTCAPSA`.
    pub struct MIDIOUTCAPSA {
        wMid:         u16
        wPid:         u16
        vDriverVer:   u32
        szPname0:     u64
        szPname1:     u64
        szPname2:     u64
        szPname3:     u64
        wTechnology:  u16
        wVoices:      u16
        wNotes:       u16
        wChannelMask: u16
        dwSupport:    u32
    }

    /// `TIMECAPS` — multimedia-timer resolution range.
    pub struct TIMECAPS {
        wPeriodMin: u32
        wPeriodMax: u32
    }

    // ── WAVE input / output (extended) ────────────────────────────

    @lib pub fn waveOutGetDevCapsA(
        uDeviceID: u64,
        pwoc:      *WAVEOUTCAPSA,
        cbwoc:     u32
    ): u32

    @lib pub fn waveOutGetID(hwo: *HWAVEOUT, puDeviceID: *u32): u32

    /// Returns the playback position of `hwo` into `pmmt` — the
    /// caller fills `wType` (TIME_BYTES / TIME_SAMPLES / TIME_MS /
    /// TIME_SMPTE / TIME_MIDI) before the call.
    @lib pub fn waveOutGetPosition(
        hwo:   *HWAVEOUT,
        pmmt:  *void,            // MMTIME
        cbmmt: u32
    ): u32

    /// Returns the playback pitch as a 32-bit fixed-point
    /// `whole.fraction` (high u16 = whole, low u16 = fraction).
    /// Not supported on every driver; check WAVECAPS_PITCH.
    @lib pub fn waveOutGetPitch(hwo: *HWAVEOUT, pdwPitch: *u32): u32
    @lib pub fn waveOutSetPitch(hwo: *HWAVEOUT, dwPitch: u32): u32

    @lib pub fn waveOutGetPlaybackRate(hwo: *HWAVEOUT, pdwRate: *u32): u32
    @lib pub fn waveOutSetPlaybackRate(hwo: *HWAVEOUT, dwRate: u32): u32

    @lib pub fn waveOutBreakLoop(hwo: *HWAVEOUT): u32

    /// Number of waveform input devices.
    @lib pub fn waveInGetNumDevs(): u32

    @lib pub fn waveInGetDevCapsA(
        uDeviceID: u64,
        pwic:      *WAVEINCAPSA,
        cbwic:     u32
    ): u32

    /// Opens a waveform input (capture) device — mirror of
    /// `waveOutOpen`.
    @lib pub fn waveInOpen(
        phwi:       **HWAVEIN,
        uDeviceID:  u32,
        pwfx:       *const WAVEFORMATEX,
        dwCallback: u64,
        dwInstance: u64,
        fdwOpen:    u32
    ): u32
    @lib pub fn waveInClose(hwi: *HWAVEIN): u32
    @lib pub fn waveInStart(hwi: *HWAVEIN): u32
    @lib pub fn waveInStop(hwi: *HWAVEIN): u32
    @lib pub fn waveInReset(hwi: *HWAVEIN): u32
    @lib pub fn waveInPrepareHeader(hwi: *HWAVEIN, pwh: *WAVEHDR, cbwh: u32): u32
    @lib pub fn waveInUnprepareHeader(hwi: *HWAVEIN, pwh: *WAVEHDR, cbwh: u32): u32
    @lib pub fn waveInAddBuffer(hwi: *HWAVEIN, pwh: *WAVEHDR, cbwh: u32): u32

    // ── MIDI output ────────────────────────────────────────────────

    @lib pub fn midiOutGetNumDevs(): u32
    @lib pub fn midiOutGetDevCapsA(
        uDeviceID: u64,
        pmoc:      *MIDIOUTCAPSA,
        cbmoc:     u32
    ): u32
    @lib pub fn midiOutOpen(
        phmo:        **HMIDIOUT,
        uDeviceID:   u32,
        dwCallback:  u64,
        dwInstance:  u64,
        fdwOpen:     u32
    ): u32
    @lib pub fn midiOutClose(hmo: *HMIDIOUT): u32
    @lib pub fn midiOutShortMsg(hmo: *HMIDIOUT, dwMsg: u32): u32
    @lib pub fn midiOutLongMsg(hmo: *HMIDIOUT, pmh: *void, cbmh: u32): u32
    @lib pub fn midiOutReset(hmo: *HMIDIOUT): u32
    @lib pub fn midiOutGetVolume(hmo: *HMIDIOUT, pdwVolume: *u32): u32
    @lib pub fn midiOutSetVolume(hmo: *HMIDIOUT, dwVolume: u32): u32

    // ── MIDI input ─────────────────────────────────────────────────

    @lib pub fn midiInGetNumDevs(): u32
    @lib pub fn midiInOpen(
        phmi:        **HMIDIIN,
        uDeviceID:   u32,
        dwCallback:  u64,
        dwInstance:  u64,
        fdwOpen:     u32
    ): u32
    @lib pub fn midiInClose(hmi: *HMIDIIN): u32
    @lib pub fn midiInStart(hmi: *HMIDIIN): u32
    @lib pub fn midiInStop(hmi: *HMIDIIN): u32
    @lib pub fn midiInReset(hmi: *HMIDIIN): u32

    // ── Mixer ──────────────────────────────────────────────────────

    @lib pub fn mixerGetNumDevs(): u32
    @lib pub fn mixerOpen(
        phmx:        **HMIXER,
        uMxId:       u32,
        dwCallback:  u64,
        dwInstance:  u64,
        fdwOpen:     u32
    ): u32
    @lib pub fn mixerClose(hmx: *HMIXER): u32

    // ── MCI (Media Control Interface) ──────────────────────────────

    /// Send a string command to the MCI command set, e.g.
    /// `mciSendStringA("open foo.mp3 alias bgm", NULL, 0, NULL)`
    /// then `mciSendStringA("play bgm", NULL, 0, NULL)`. Returns
    /// 0 on success or an MCI error code.
    @lib pub fn mciSendStringA(
        lpszCommand:        *const char,
        lpszReturnString:   *char,
        cchReturn:          u32,
        hwndCallback:       *void
    ): u32

    /// Looks up a string for the most recent MCI error.
    @lib pub fn mciGetErrorStringA(
        fdwError: u32,
        lpszErrorText: *char,
        cchErrorText: u32
    ): i32

    // ── Multimedia timer caps ──────────────────────────────────────

    @lib pub fn timeGetDevCaps(ptc: *TIMECAPS, cbtc: u32): u32
}
