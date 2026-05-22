/// imm32.dll — Input Method Manager: IME composition and candidate
/// list interaction. Used to handle CJK / complex-script input in
/// custom windows that don't get default IME behaviour.

// ─── ImmGetCompositionString index (GCS_*) ───────────────────────
@flags
pub enum CompositionStringIndex: u32 {
    compReadStr      = 0x0001
    compReadAttr     = 0x0002
    compReadClause   = 0x0004
    compStr          = 0x0008
    compAttr         = 0x0010
    compClause       = 0x0020
    cursorPos        = 0x0080
    deltaStart       = 0x0100
    resultReadStr    = 0x0200
    resultReadClause = 0x0400
    resultStr        = 0x0800
    resultClause     = 0x1000
}

// ─── ImmSetCompositionWindow style (CFS_*) ───────────────────────
pub enum CompositionWindowStyle: u32 {
    default      = 0
    rect         = 1
    point        = 2
    forcePos     = 4
    candidatepos = 0x40
    excludeRect  = 0x10
}

// ─── ImmSetConversionStatus IME modes (IME_CMODE_*) ──────────────
@flags
pub enum ImeConversionMode: u32 {
    alphanumeric = 0x0000
    native       = 0x0001     // Kana / Hangul mode
    chinese      = 0x0001
    hangul       = 0x0001
    japanese     = 0x0001
    katakana     = 0x0002
    language     = 0x0003
    fullshape    = 0x0008
    roman        = 0x0010
    charcode     = 0x0020
    handWriting  = 0x0040
    softKbd      = 0x0080
    noConversion = 0x0100
    eudc         = 0x0200
    symbol       = 0x0400
    fixed        = 0x0800
}

// ─── ImeSentenceMode (IME_SMODE_*) ───────────────────────────────
@flags
pub enum ImeSentenceMode: u32 {
    none           = 0x0000
    plauralClause  = 0x0001
    singleConvert  = 0x0002
    automatic      = 0x0004
    phrasePredict  = 0x0008
    conversation   = 0x0010
}

// ─── ImmNotifyIME action (NI_*) ──────────────────────────────────
pub enum ImeNotifyAction: u32 {
    openCandidate     = 0x0010
    closeCandidate    = 0x0011
    selectCandidate   = 0x0012
    changeCandidateList = 0x0013
    finalizeConvert   = 0x0014
    compositionStr    = 0x0015
    setCandidatePagestart = 0x0016
    setCandidatePagesize = 0x0017
    imeMenuSelected   = 0x0018
}

use user32 { HWND }

@extern(C, "imm32") {
    // ── Opaque handle ─────────────────────────────────────────────

    /// IME context handle (HIMC).
    @handle pub struct HIMC {}

    // ── COMPOSITIONFORM — 28 bytes (4-byte aligned) ──────────────

    pub struct COMPOSITIONFORM {
        dwStyle:      u32        // CompositionWindowStyle.*
        ptCurrentPos_x: i32      // inline POINT
        ptCurrentPos_y: i32
        rcArea_left:  i32        // inline RECT
        rcArea_top:   i32
        rcArea_right: i32
        rcArea_bottom: i32
    }

    // ── CANDIDATEFORM — 36 bytes ─────────────────────────────────

    pub struct CANDIDATEFORM {
        dwIndex:      u32
        dwStyle:      u32
        ptCurrentPos_x: i32
        ptCurrentPos_y: i32
        rcArea_left:  i32
        rcArea_top:   i32
        rcArea_right: i32
        rcArea_bottom: i32
        _pad:         u32
    }

    // ── Context ───────────────────────────────────────────────────

    /// Returns the IME context for `hwnd`.  Always pair with
    /// `ImmReleaseContext`.
    @lib pub fn ImmGetContext(hwnd: HWND): HIMC

    @lib pub fn ImmReleaseContext(hwnd: HWND, himc: HIMC): i32

    /// Creates a new IME context; rarely needed (default contexts
    /// are created automatically per window).
    @lib pub fn ImmCreateContext(): HIMC

    @lib pub fn ImmDestroyContext(himc: HIMC): i32

    /// Associates `himc` with `hwnd`.  Returns the previously
    /// associated context.
    @lib pub fn ImmAssociateContext(hwnd: HWND, himc: HIMC): HIMC

    // ── Composition string ────────────────────────────────────────

    /// Reads the current composition / result string from the IME.
    /// `dwIndex`: CompositionStringIndex.*.
    /// Returns the byte count (or character count for some indices).
    @lib pub fn ImmGetCompositionStringA(
        himc:    HIMC,
        dwIndex: u32,
        lpBuf:   *void,
        dwBufLen: u32
    ): i32

    @lib pub fn ImmGetCompositionStringW(
        himc:    HIMC,
        dwIndex: u32,
        lpBuf:   *void,
        dwBufLen: u32
    ): i32

    @lib pub fn ImmSetCompositionStringA(
        himc:        HIMC,
        dwIndex:     u32,
        lpComp:      *const void,
        dwCompLen:   u32,
        lpRead:      *const void,
        dwReadLen:   u32
    ): i32
    @lib pub fn ImmSetCompositionStringW(
        himc:        HIMC,
        dwIndex:     u32,
        lpComp:      *const void,
        dwCompLen:   u32,
        lpRead:      *const void,
        dwReadLen:   u32
    ): i32

    // ── IME state / conversion mode ──────────────────────────────

    /// Returns 1 if the IME for `hwnd` is currently open (composing).
    @lib pub fn ImmGetOpenStatus(himc: HIMC): i32

    /// Opens or closes the IME.
    @lib pub fn ImmSetOpenStatus(himc: HIMC, fOpen: i32): i32

    @lib pub fn ImmGetConversionStatus(
        himc:        HIMC,
        lpdwConversion: *u32,    // ImeConversionMode.*
        lpdwSentence:   *u32     // ImeSentenceMode.*
    ): i32

    @lib pub fn ImmSetConversionStatus(
        himc:        HIMC,
        dwConversion: u32,
        dwSentence:   u32
    ): i32

    // ── Window placement ─────────────────────────────────────────

    @lib pub fn ImmSetCompositionWindow(himc: HIMC, lpCompForm: *COMPOSITIONFORM): i32

    @lib pub fn ImmSetCandidateWindow(himc: HIMC, lpCandidate: *CANDIDATEFORM): i32

    // ── Notification ──────────────────────────────────────────────

    /// Sends a notification to the IME (e.g. close candidate window).
    @lib pub fn ImmNotifyIME(
        himc:    HIMC,
        dwAction: u32,           // ImeNotifyAction.*
        dwIndex: u32,
        dwValue: u32
    ): i32

    // ── Hot-key registration ──────────────────────────────────────

    /// Sets an IME hot-key for the calling thread.
    /// `dwHotKeyID`: identifier from the IME hot-key range (0x10..0x6F).
    /// `uModifiers`: MOD_* (1=Alt, 2=Ctrl, 4=Shift, 8=Win).
    /// `uVKey`: virtual-key code.  Pass 0 to disable.
    /// `hKL`: keyboard-layout handle (typically 0 for current).
    @lib pub fn ImmSetHotKey(
        dwHotKeyID: u32,
        uModifiers: u32,
        uVKey:      u32,
        hKL:        i64
    ): i32

    @lib pub fn ImmGetHotKey(
        dwHotKeyID: u32,
        lpuModifiers: *u32,
        lpuVKey:      *u32,
        lphKL:        *i64
    ): i32

    // ── IME description / properties ──────────────────────────────

    /// Returns the IME's user-friendly description (e.g. "Microsoft IME").
    @lib pub fn ImmGetDescriptionA(
        hkl:     i64,
        lpsz:    *char,
        uBufLen: u32
    ): u32
    @lib pub fn ImmGetDescriptionW(
        hkl:     i64,
        lpsz:    *u16,
        uBufLen: u32
    ): u32

    @lib pub fn ImmGetIMEFileNameA(
        hkl:     i64,
        lpsz:    *char,
        uBufLen: u32
    ): u32
    @lib pub fn ImmGetIMEFileNameW(
        hkl:     i64,
        lpsz:    *u16,
        uBufLen: u32
    ): u32

    /// Returns a flag combination describing the IME's capabilities
    /// (IME_PROP_*, e.g. AT_CARET = 0x10000).
    @lib pub fn ImmGetProperty(hkl: i64, fdwIndex: u32): u32

    @lib pub fn ImmIsIME(hkl: i64): i32

    // ── Conversion list (for IME apps; rarely used) ───────────────

    /// Asks the IME for a candidate list given a reading string.
    @lib pub fn ImmGetConversionListA(
        hkl:      i64,
        himc:     HIMC,
        lpsrc:    *const char,
        lpDst:    *void,
        dwBufLen: u32,
        uFlag:    u32
    ): u32
    @lib pub fn ImmGetConversionListW(
        hkl:      i64,
        himc:     HIMC,
        lpsrc:    *const u16,
        lpDst:    *void,
        dwBufLen: u32,
        uFlag:    u32
    ): u32
}
