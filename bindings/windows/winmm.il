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
}
