// user32.dll — window classes, window management, painting,
// input, messaging, timers, dialogs, and resources.
//
// ANSI (`A`) variants only for now: arguments stay as
// `*const char`, no UTF-16 conversion. Switch to `*W` once
// a UTF-16 path lands.
//
// Handle types are opaque empty structs so the type system
// catches accidental mix-ups between e.g. HWND and HBRUSH.
// WPARAM = `u64` (UINT_PTR), LPARAM = `i64` (LONG_PTR),
// LRESULT = `i64` (LONG_PTR), ATOM = `u16`.

// ─── Window class styles (CS_*) ───────────────────────────────
@flags
pub enum ClassStyle: u32 {
    vredraw         = 0x0001
    hredraw         = 0x0002
    dblclks         = 0x0008
    ownDC           = 0x0020
    classDC         = 0x0040
    parentDC        = 0x0080
    noClose         = 0x0200
    saveBits        = 0x0800
    byteAlignClient = 0x1000
    byteAlignWindow = 0x2000
    /// Makes the class available to all processes in the session.
    globalClass     = 0x4000
    ime             = 0x00010000
    dropShadow      = 0x00020000
}

// ─── Window styles (WS_*) ─────────────────────────────────────
@flags
pub enum WindowStyle: u32 {
    overlapped   = 0x00000000
    popup        = 0x80000000
    child        = 0x40000000
    minimize     = 0x20000000   // WS_ICONIC — start minimised
    visible      = 0x10000000
    disabled     = 0x08000000
    clipSiblings = 0x04000000
    clipChildren = 0x02000000
    maximize     = 0x01000000
    caption      = 0x00C00000   // = border | dlgFrame
    border       = 0x00800000
    dlgFrame     = 0x00400000
    vScroll      = 0x00200000
    hScroll      = 0x00100000
    sysmenu      = 0x00080000
    thickframe   = 0x00040000   // WS_SIZEBOX
    minimizeBox  = 0x00020000
    maximizeBox  = 0x00010000
    /// `WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME
    /// | WS_MINIMIZEBOX | WS_MAXIMIZEBOX` — standard top-level window.
    overlappedWindow = 0x00CF0000
    /// `WS_POPUP | WS_BORDER | WS_SYSMENU`
    popupWindow      = 0x80880000
}

// ─── Extended window styles (WS_EX_*) ────────────────────────
@flags
pub enum WindowStyleEx: u32 {
    dlgModalFrame       = 0x00000001
    noParentNotify      = 0x00000004
    topmost             = 0x00000008
    acceptFiles         = 0x00000010   // enables WM_DROPFILES
    transparent         = 0x00000020
    mdiChild            = 0x00000040
    toolWindow          = 0x00000080   // no taskbar button
    windowEdge          = 0x00000100
    clientEdge          = 0x00000200
    contextHelp         = 0x00000400
    right               = 0x00001000
    rtlReading          = 0x00002000
    leftScrollBar       = 0x00004000
    controlParent       = 0x00010000
    staticEdge          = 0x00020000
    appWindow           = 0x00040000   // force taskbar button
    layered             = 0x00080000
    noInheritLayout     = 0x00100000
    noRedirectionBitmap = 0x00200000
    layoutRtl           = 0x00400000
    composited          = 0x02000000
    noActivate          = 0x08000000
    /// `WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE`
    overlappedWindow    = 0x00000300
    /// `WS_EX_WINDOWEDGE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST`
    paletteWindow       = 0x00000188
}

// ─── ShowWindow nCmdShow (SW_*) ───────────────────────────────
pub enum ShowCmd: i32 {
    hide            = 0
    showNormal      = 1
    showMinimized   = 2
    showMaximized   = 3
    showNoActivate  = 4
    show            = 5
    minimize        = 6
    showMinNoActive = 7
    showNA          = 8
    restore         = 9
    showDefault     = 10
    /// Minimises even if the owning thread is not responding.
    forceMinimize   = 11
}

// ─── Window messages (WM_*) ───────────────────────────────────
pub enum WindowMessage: u32 {
    // ── Lifecycle / painting ──────────────────────────────────
    null            = 0x0000
    create          = 0x0001
    destroy         = 0x0002
    move            = 0x0003
    size            = 0x0005
    activate        = 0x0006   // wParam low-word: ActivateCode
    setFocus        = 0x0007
    killFocus       = 0x0008
    enable          = 0x000A
    setRedraw       = 0x000B
    paint           = 0x000F
    close           = 0x0010
    queryEndSession = 0x0011
    quit            = 0x0012
    eraseBkgnd      = 0x0014
    sysColorChange  = 0x0015
    endSession      = 0x0016
    showWindow      = 0x0018
    activateApp     = 0x001C
    cancelMode      = 0x001F
    setCursor       = 0x0020
    mouseActivate   = 0x0021
    childActivate   = 0x0022
    getMinMaxInfo   = 0x0024   // lParam: *MINMAXINFO
    notify          = 0x004E   // WM_NOTIFY (common controls)
    contextMenu     = 0x007B
    styleChanging   = 0x007C
    styleChanged    = 0x007D
    displayChange   = 0x007E
    getIcon         = 0x007F
    setIcon         = 0x0080

    // ── Non-client area ───────────────────────────────────────
    ncCreate        = 0x0081
    ncDestroy       = 0x0082
    ncCalcSize      = 0x0083
    ncHitTest       = 0x0084   // return HitTest value
    ncPaint         = 0x0085
    ncActivate      = 0x0086
    getDlgCode      = 0x0087
    ncMouseMove     = 0x00A0
    ncLButtonDown   = 0x00A1
    ncLButtonUp     = 0x00A2
    ncLButtonDblClk = 0x00A3
    ncRButtonDown   = 0x00A4
    ncRButtonUp     = 0x00A5
    ncRButtonDblClk = 0x00A6
    ncMButtonDown   = 0x00A7
    ncMButtonUp     = 0x00A8
    ncMButtonDblClk = 0x00A9

    // ── Keyboard ──────────────────────────────────────────────
    keydown         = 0x0100   // wParam: VirtualKey
    keyup           = 0x0101   // wParam: VirtualKey
    char            = 0x0102
    deadChar        = 0x0103
    sysKeyDown      = 0x0104
    sysKeyUp        = 0x0105
    sysChar         = 0x0106
    sysDeadChar     = 0x0107
    unichar         = 0x0109

    // ── Dialog / system ───────────────────────────────────────
    initDialog      = 0x0110
    command         = 0x0111   // menu / accelerator / control notification
    sysCommand      = 0x0112   // wParam: SysCommand (mask 0xFFF0)
    timer           = 0x0113
    hScroll         = 0x0114
    vScroll         = 0x0115
    initMenu        = 0x0116
    initMenuPopup   = 0x0117
    menuSelect      = 0x011F
    menuChar        = 0x0120
    enterIdle       = 0x0121
    ctlColorEdit    = 0x0133
    ctlColorListBox = 0x0134
    ctlColorBtn     = 0x0135
    ctlColorDlg     = 0x0136
    ctlColorStatic  = 0x0138

    // ── Mouse ─────────────────────────────────────────────────
    mouseMove       = 0x0200
    lbuttondown     = 0x0201
    lbuttonup       = 0x0202
    lButtonDblClk   = 0x0203
    rButtonDown     = 0x0204
    rButtonUp       = 0x0205
    rButtonDblClk   = 0x0206
    mButtonDown     = 0x0207
    mButtonUp       = 0x0208
    mButtonDblClk   = 0x0209
    mouseWheel      = 0x020A   // HIWORD(wParam): signed delta / WHEEL_DELTA
    xButtonDown     = 0x020B
    xButtonUp       = 0x020C
    xButtonDblClk   = 0x020D
    mouseHWheel     = 0x020E

    // ── Window management ─────────────────────────────────────
    enterSizeMove      = 0x0231
    exitSizeMove       = 0x0232
    dropFiles          = 0x0233   // requires DragAcceptFiles
    windowPosChanging  = 0x0046   // lParam: *WINDOWPOS
    windowPosChanged   = 0x0047   // lParam: *WINDOWPOS

    // ── Mouse tracking (TrackMouseEvent) ──────────────────────
    mouseHover      = 0x02A1
    mouseLeave      = 0x02A3
    ncMouseHover    = 0x02A0
    ncMouseLeave    = 0x02A2

    // ── Clipboard ─────────────────────────────────────────────
    cut             = 0x0300
    copy            = 0x0301
    paste           = 0x0302
    clear           = 0x0303
    undo            = 0x0304
    hotkey          = 0x0312
    clipboardUpdate = 0x031D

    // ── Rendering / DWM ───────────────────────────────────────
    print           = 0x0317
    printClient     = 0x0318
    appCommand      = 0x0319
    themeChanged    = 0x031A
    dwmCompositionChanged       = 0x031E
    dwmNcRenderingChanged       = 0x031F
    dwmColorizationColorChanged = 0x0320
    dwmWindowMaximizedChange    = 0x0321

    // ── Application-defined range ─────────────────────────────
    /// WM_USER — first app-private message. Range: user..user+0x7FFF.
    user = 0x0400
    /// WM_APP — first library-private message. Range: app..0xBFFF.
    app  = 0x8000
}

// ─── Virtual key codes (VK_*) ─────────────────────────────────
// wParam of WM_KEYDOWN / WM_KEYUP / WM_SYSKEYDOWN / WM_SYSKEYUP.
pub enum VirtualKey: u32 {
    lButton     = 0x01   // left mouse button
    rButton     = 0x02   // right mouse button
    cancel      = 0x03   // Ctrl+Break
    mButton     = 0x04   // middle mouse button
    xButton1    = 0x05
    xButton2    = 0x06
    back        = 0x08   // Backspace
    tab         = 0x09
    clear       = 0x0C
    enter       = 0x0D   // Return (VK_RETURN)
    shift       = 0x10
    control     = 0x11
    alt         = 0x12   // VK_MENU
    pause       = 0x13
    capsLock    = 0x14
    kana        = 0x15   // IME Kana / Hangul mode
    imeOn       = 0x16
    junja       = 0x17
    final       = 0x18
    hanja       = 0x19   // VK_KANJI
    imeOff      = 0x1A
    escape      = 0x1B
    convert     = 0x1C
    nonconvert  = 0x1D
    accept      = 0x1E
    modeChange  = 0x1F
    space       = 0x20
    pageUp      = 0x21   // VK_PRIOR
    pageDown    = 0x22   // VK_NEXT
    end         = 0x23
    home        = 0x24
    left        = 0x25
    up          = 0x26
    right       = 0x27
    down        = 0x28
    select      = 0x29
    print       = 0x2A
    execute     = 0x2B
    printScreen = 0x2C   // VK_SNAPSHOT
    insert      = 0x2D
    delete      = 0x2E
    help        = 0x2F
    // Digit row — ASCII '0'–'9'
    d0 = 0x30
    d1 = 0x31
    d2 = 0x32
    d3 = 0x33
    d4 = 0x34
    d5 = 0x35
    d6 = 0x36
    d7 = 0x37
    d8 = 0x38
    d9 = 0x39
    // Letter keys — ASCII 'A'–'Z' (case-insensitive in WM_KEYDOWN)
    a = 0x41
    b = 0x42
    c = 0x43
    d = 0x44
    e = 0x45
    f = 0x46
    g = 0x47
    h = 0x48
    i = 0x49
    j = 0x4A
    k = 0x4B
    l = 0x4C
    m = 0x4D
    n = 0x4E
    o = 0x4F
    p = 0x50
    q = 0x51
    r = 0x52
    s = 0x53
    t = 0x54
    u = 0x55
    v = 0x56
    w = 0x57
    x = 0x58
    y = 0x59
    z = 0x5A
    lWin        = 0x5B
    rWin        = 0x5C
    apps        = 0x5D   // context-menu key
    sleep       = 0x5F
    // Numpad
    numpad0   = 0x60
    numpad1   = 0x61
    numpad2   = 0x62
    numpad3   = 0x63
    numpad4   = 0x64
    numpad5   = 0x65
    numpad6   = 0x66
    numpad7   = 0x67
    numpad8   = 0x68
    numpad9   = 0x69
    multiply  = 0x6A
    add       = 0x6B
    separator = 0x6C
    subtract  = 0x6D
    decimal   = 0x6E
    divide    = 0x6F
    // Function keys
    f1  = 0x70
    f2  = 0x71
    f3  = 0x72
    f4  = 0x73
    f5  = 0x74
    f6  = 0x75
    f7  = 0x76
    f8  = 0x77
    f9  = 0x78
    f10 = 0x79
    f11 = 0x7A
    f12 = 0x7B
    f13 = 0x7C
    f14 = 0x7D
    f15 = 0x7E
    f16 = 0x7F
    f17 = 0x80
    f18 = 0x81
    f19 = 0x82
    f20 = 0x83
    f21 = 0x84
    f22 = 0x85
    f23 = 0x86
    f24 = 0x87
    numLock    = 0x90
    scrollLock = 0x91
    lShift     = 0xA0
    rShift     = 0xA1
    lControl   = 0xA2
    rControl   = 0xA3
    lAlt       = 0xA4   // VK_LMENU
    rAlt       = 0xA5   // VK_RMENU
    browserBack      = 0xA6
    browserForward   = 0xA7
    browserRefresh   = 0xA8
    browserStop      = 0xA9
    browserSearch    = 0xAA
    browserFavorites = 0xAB
    browserHome      = 0xAC
    volumeMute       = 0xAD
    volumeDown       = 0xAE
    volumeUp         = 0xAF
    mediaNextTrack   = 0xB0
    mediaPrevTrack   = 0xB1
    mediaStop        = 0xB2
    mediaPlayPause   = 0xB3
    // OEM keys (US keyboard layout labels)
    oem1      = 0xBA   // ;:
    oemPlus   = 0xBB   // =+
    oemComma  = 0xBC   // ,<
    oemMinus  = 0xBD   // -_
    oemPeriod = 0xBE   // .>
    oem2      = 0xBF   // /?
    oem3      = 0xC0   // `~
    oem4      = 0xDB   // [{
    oem5      = 0xDC   // \|
    oem6      = 0xDD   // ]}
    oem7      = 0xDE   // '"
    oem8      = 0xDF
    oem102    = 0xE2   // extra key on ISO keyboards
    processKey = 0xE5  // IME
    attn      = 0xF6
    crsel     = 0xF7
    exsel     = 0xF8
    ereof     = 0xF9
    play      = 0xFA
    zoom      = 0xFB
    pa1       = 0xFD
    oemClear  = 0xFE
}

// ─── System command IDs (SC_*) ────────────────────────────────
// Mask wParam with `0xFFF0` before comparing:
//   `(msg.wParam & 0xFFF0) == SysCommand.close as u64`
pub enum SysCommand: u32 {
    size         = 0xF000
    move         = 0xF010
    minimize     = 0xF020
    maximize     = 0xF030
    nextWindow   = 0xF040
    prevWindow   = 0xF050
    close        = 0xF060
    vScroll      = 0xF070
    hScroll      = 0xF080
    mouseMenu    = 0xF090
    keyMenu      = 0xF100
    arrange      = 0xF110
    restore      = 0xF120
    taskList     = 0xF130
    screensave   = 0xF140
    hotkey       = 0xF150
    default      = 0xF160
    monitorPower = 0xF170   // lParam: -1=on, 1=low, 2=off
    contextHelp  = 0xF180
    separator    = 0xF00F
}

// ─── Hit-test return values (HT*) ────────────────────────────
// Return from a WM_NCHITTEST handler, or read from wParam in
// WM_NCLBUTTONxxx messages.
pub enum HitTest: i32 {
    error       = -2   // HTERROR — invalid prior SetCapture
    transparent = -1   // pass hit-test to the window beneath
    nowhere     = 0
    client      = 1
    caption     = 2
    sysMenu     = 3
    growBox     = 4    // HTSIZE — resize grip
    menu        = 5
    hScroll     = 6
    vScroll     = 7
    minButton   = 8    // HTREDUCE
    maxButton   = 9    // HTZOOM
    left        = 10
    right       = 11
    top         = 12
    topLeft     = 13
    topRight    = 14
    bottom      = 15
    bottomLeft  = 16
    bottomRight = 17
    border      = 18
    object      = 19
    close       = 20
    help        = 21
}

// ─── WM_ACTIVATE wParam low-word (WA_*) ──────────────────────
pub enum ActivateCode: u32 {
    inactive    = 0   // window being deactivated
    active      = 1   // activated (keyboard / programmatic)
    clickActive = 2   // activated by mouse click
}

// ─── MessageBox type flags (MB_*) ────────────────────────────
@flags
pub enum MessageBoxType: u32 {
    // Button layouts
    ok                = 0x00000000
    okCancel          = 0x00000001
    abortRetryIgnore  = 0x00000002
    yesNoCancel       = 0x00000003
    yesNo             = 0x00000004
    retryCancel       = 0x00000005
    cancelTryContinue = 0x00000006
    // Icons
    iconError       = 0x00000010   // MB_ICONHAND / MB_ICONSTOP
    iconQuestion    = 0x00000020
    iconWarning     = 0x00000030   // MB_ICONEXCLAMATION
    iconInformation = 0x00000040   // MB_ICONASTERISK
    // Default button
    defButton2 = 0x00000100
    defButton3 = 0x00000200
    defButton4 = 0x00000300
    // Modality
    systemModal = 0x00001000
    taskModal   = 0x00002000
    // Misc
    setForeground     = 0x00010000
    topmost           = 0x00040000
    right             = 0x00080000
    rtlReading        = 0x00100000
}

// ─── MessageBox return values (IDOK, IDCANCEL …) ─────────────
pub enum MessageBoxResult: i32 {
    ok       = 1
    cancel   = 2
    abort    = 3
    retry    = 4
    ignore   = 5
    yes      = 6
    no       = 7
    tryAgain = 10
    continue = 11
}

// ─── SetWindowPos flags (SWP_*) ──────────────────────────────
@flags
pub enum SetWindowPosFlags: u32 {
    noSize         = 0x0001
    noMove         = 0x0002
    noZOrder       = 0x0004
    noRedraw       = 0x0008
    noActivate     = 0x0010
    frameChanged   = 0x0020   // SWP_DRAWFRAME — sends WM_NCCALCSIZE
    showWindow     = 0x0040
    hideWindow     = 0x0080
    noCopyBits     = 0x0100
    noOwnerZOrder  = 0x0200
    noSendChanging = 0x0400
    deferErase     = 0x2000
    asyncWindowPos = 0x4000
}

// ─── Predefined cursor resource IDs (IDC_*) ───────────────────
// Usage: `LoadCursorA(0 as *HINSTANCE, SystemCursor.arrow as u32 as *const char)`
pub enum SystemCursor: u32 {
    arrow       = 32512
    ibeam       = 32513
    wait        = 32514
    cross       = 32515
    upArrow     = 32516
    sizeNWSE    = 32642
    sizeNESW    = 32643
    sizeWE      = 32644
    sizeNS      = 32645
    sizeAll     = 32646
    no          = 32648
    hand        = 32649
    appStarting = 32650
    help        = 32651
}

// ─── Predefined icon resource IDs (IDI_*) ────────────────────
// Usage: `LoadIconA(0 as *HINSTANCE, SystemIcon.application as u32 as *const char)`
pub enum SystemIcon: u32 {
    application = 32512
    error       = 32513   // IDI_HAND
    question    = 32514
    warning     = 32515   // IDI_EXCLAMATION
    information = 32516   // IDI_ASTERISK
    winLogo     = 32517
    shield      = 32518
}

@extern(C, "user32") {
    // ── Opaque handle types ───────────────────────────────────

    /// Opaque window handle. Used as `*HWND` in `@lib` API;
    /// use `i64` in `pub fn` signatures (same register on Win64).
    pub struct HWND {}

    /// Executable / DLL image handle. Obtain via
    /// `GetModuleHandleA(0 as *const char) as *HINSTANCE`.
    pub struct HINSTANCE {}

    /// Icon handle — returned by `LoadIconA` / `LoadImageA`.
    pub struct HICON {}

    /// Cursor handle — returned by `LoadCursorA` / `LoadImageA`.
    pub struct HCURSOR {}

    /// GDI brush handle. Stock brush: `(COLOR_WINDOW + 1) as *HBRUSH`.
    pub struct HBRUSH {}

    /// Menu handle — returned by `CreateMenu` / `LoadMenuA`.
    pub struct HMENU {}

    /// Device-context handle for GDI drawing operations.
    pub struct HDC {}

    // ── Geometry structs ──────────────────────────────────────

    /// `POINT { LONG x; LONG y; }` — 8 bytes.
    pub struct POINT {
        x: i32
        y: i32
    }

    /// `RECT { LONG left, top, right, bottom; }` — 16 bytes.
    pub struct RECT {
        left:   i32
        top:    i32
        right:  i32
        bottom: i32
    }

    // ── Message structs ───────────────────────────────────────

    /// `MSG` returned by `GetMessageA` / posted by Windows.
    /// 48 bytes on x64.  `message` is `u32` rather than
    /// `WindowMessage` because `RegisterWindowMessage` can
    /// deliver values in 0xC000–0xFFFF at runtime.
    pub struct MSG {
        hwnd:     *HWND
        message:  u32
        // 4 bytes implicit padding so wParam aligns to 8.
        wParam:   u64
        lParam:   i64
        time:     u32
        pt:       POINT
        lPrivate: u32
    }

    /// `PAINTSTRUCT` — 72 bytes on x64. Pass to `BeginPaint` /
    /// `EndPaint` to describe the area needing repainting.
    /// `hdc` is valid only between BeginPaint and EndPaint.
    pub struct PAINTSTRUCT {
        hdc:        *HDC
        fErase:     i32
        rcPaint:    RECT
        fRestore:   i32
        fIncUpdate: i32
        // rgbReserved[32] — internal use, 8 × u32 = 32 bytes.
        _r0: u32
        _r1: u32
        _r2: u32
        _r3: u32
        _r4: u32
        _r5: u32
        _r6: u32
        _r7: u32
    }

    /// `MINMAXINFO` — 40 bytes. lParam of `WM_GETMINMAXINFO`.
    /// Set `ptMinTrackSize` / `ptMaxTrackSize` to constrain
    /// how small or large the user can resize the window.
    pub struct MINMAXINFO {
        ptReserved:     POINT
        ptMaxSize:      POINT
        ptMaxPosition:  POINT
        ptMinTrackSize: POINT
        ptMaxTrackSize: POINT
    }

    /// `CREATESTRUCTA` — lParam of `WM_CREATE`. Reflects the
    /// arguments that were passed to `CreateWindowExA`.
    pub struct CREATESTRUCTA {
        lpCreateParams: i64
        hInstance:      *HINSTANCE
        hMenu:          *HMENU
        hwndParent:     *HWND
        cy:             i32
        cx:             i32
        y:              i32
        x:              i32
        style:          i32
        // 4 bytes implicit padding before the pointer fields.
        lpszName:       *const char
        lpszClass:      *const char
        dwExStyle:      u32
    }

    /// `WINDOWPOS` — lParam of `WM_WINDOWPOSCHANGED` /
    /// `WM_WINDOWPOSCHANGING`.
    pub struct WINDOWPOS {
        hwnd:            *HWND
        hwndInsertAfter: *HWND
        x:               i32
        y:               i32
        cx:              i32
        cy:              i32
        flags:           u32
    }

    // ── Window class / creation ───────────────────────────────

    /// `WNDCLASSEXA` — 80 bytes on x64. Set `cbSize` to 80.
    pub struct WNDCLASSEXA {
        cbSize:        u32
        style:         u32
        /// First param is HWND. Declared `i64` so the user-side
        /// `wndProc` can be a `pub fn` without raw pointer args.
        lpfnWndProc:   fn(i64, u32, u64, i64): i64
        cbClsExtra:    i32
        cbWndExtra:    i32
        // 4 bytes implicit padding before hInstance.
        hInstance:     *HINSTANCE
        hIcon:         *HICON
        hCursor:       *HCURSOR
        hbrBackground: *HBRUSH
        lpszMenuName:  *const char
        lpszClassName: *const char
        hIconSm:       *HICON
    }

    /// Registers a window class. Returns 0 on failure.
    @lib pub fn RegisterClassExA(lpwcx: *const WNDCLASSEXA): u16

    /// Creates a window. Returns NULL on failure.
    @lib pub fn CreateWindowExA(
        dwExStyle:    u32,
        lpClassName:  *const char,
        lpWindowName: *const char,
        dwStyle:      u32,
        X:            i32,
        Y:            i32,
        nWidth:       i32,
        nHeight:      i32,
        hWndParent:   *HWND,
        hMenu:        *HMENU,
        hInstance:    *HINSTANCE,
        lpParam:      *void
    ): *HWND

    // ── Default message handling ──────────────────────────────

    /// Default window procedure; call for every unhandled message.
    @lib pub fn DefWindowProcA(
        hwnd: *HWND, msg: u32, wParam: u64, lParam: i64
    ): i64

    // ── Visibility / state ────────────────────────────────────

    @lib pub fn ShowWindow(hwnd: *HWND, nCmdShow: i32): i32
    @lib pub fn UpdateWindow(hwnd: *HWND): i32
    @lib pub fn DestroyWindow(hwnd: *HWND): i32
    @lib pub fn IsWindow(hwnd: *HWND): i32
    @lib pub fn IsWindowVisible(hwnd: *HWND): i32
    @lib pub fn EnableWindow(hwnd: *HWND, bEnable: i32): i32
    @lib pub fn BringWindowToTop(hwnd: *HWND): i32

    // ── Position / size ───────────────────────────────────────

    @lib pub fn MoveWindow(
        hwnd:    *HWND,
        X:       i32,
        Y:       i32,
        nWidth:  i32,
        nHeight: i32,
        bRepaint: i32
    ): i32

    /// `hwndInsertAfter` special values (cast from integer):
    ///   HWND_TOP     = `0 as *HWND`
    ///   HWND_BOTTOM  = `1 as *HWND`
    ///   HWND_TOPMOST = `-1 as i64 as *HWND`
    ///   HWND_NOTOPMOST = `-2 as i64 as *HWND`
    @lib pub fn SetWindowPos(
        hwnd:            *HWND,
        hwndInsertAfter: *HWND,
        X:               i32,
        Y:               i32,
        cx:              i32,
        cy:              i32,
        uFlags:          u32
    ): i32

    @lib pub fn GetWindowRect(hwnd: *HWND, lpRect: *RECT): i32
    @lib pub fn GetClientRect(hwnd: *HWND, lpRect: *RECT): i32

    /// Computes the required window rect for a desired client rect.
    @lib pub fn AdjustWindowRectEx(
        lpRect:    *RECT,
        dwStyle:   u32,
        bMenu:     i32,
        dwExStyle: u32
    ): i32

    // ── Window text ───────────────────────────────────────────

    @lib pub fn SetWindowTextA(hwnd: *HWND, lpString: *const char): i32
    /// Returns the number of characters copied (excluding NUL).
    @lib pub fn GetWindowTextA(
        hwnd:       *HWND,
        lpString:   *char,
        nMaxCount:  i32
    ): i32
    @lib pub fn GetWindowTextLengthA(hwnd: *HWND): i32

    // ── Z-order / focus / parent ──────────────────────────────

    @lib pub fn SetFocus(hwnd: *HWND): *HWND
    @lib pub fn GetFocus(): *HWND
    @lib pub fn GetForegroundWindow(): *HWND
    @lib pub fn SetForegroundWindow(hwnd: *HWND): i32
    @lib pub fn GetDesktopWindow(): *HWND
    @lib pub fn GetParent(hwnd: *HWND): *HWND
    @lib pub fn SetParent(hwndChild: *HWND, hwndNewParent: *HWND): *HWND
    @lib pub fn FindWindowA(
        lpClassName:  *const char,
        lpWindowName: *const char
    ): *HWND

    // ── Message loop ─────────────────────────────────────────

    /// Blocking message fetch. Returns >0 (message), 0 (WM_QUIT),
    /// or -1 (error).
    @lib pub fn GetMessageA(
        lpMsg:         *MSG,
        hwnd:          *HWND,
        wMsgFilterMin: u32,
        wMsgFilterMax: u32
    ): i32

    /// Non-blocking message check. `wRemoveMsg`: 0=peek only,
    /// 1=remove (PM_REMOVE).
    @lib pub fn PeekMessageA(
        lpMsg:         *MSG,
        hwnd:          *HWND,
        wMsgFilterMin: u32,
        wMsgFilterMax: u32,
        wRemoveMsg:    u32
    ): i32

    /// Synthesises WM_CHAR from key events.
    @lib pub fn TranslateMessage(lpMsg: *const MSG): i32
    /// Invokes the target window's wndproc.
    @lib pub fn DispatchMessageA(lpMsg: *const MSG): i64
    /// Posts WM_QUIT to the current thread's queue.
    @lib pub fn PostQuitMessage(nExitCode: i32)

    /// Sends a message synchronously; blocks until the wndproc returns.
    @lib pub fn SendMessageA(
        hwnd:   *HWND,
        msg:    u32,
        wParam: u64,
        lParam: i64
    ): i64

    /// Posts a message asynchronously; returns immediately.
    @lib pub fn PostMessageA(
        hwnd:   *HWND,
        msg:    u32,
        wParam: u64,
        lParam: i64
    ): i32

    // ── Painting ─────────────────────────────────────────────

    /// Begins painting; fills `lpPaint` and returns a DC.
    /// Must be paired with `EndPaint`.
    @lib pub fn BeginPaint(hwnd: *HWND, lpPaint: *PAINTSTRUCT): *HDC
    @lib pub fn EndPaint(hwnd: *HWND, lpPaint: *const PAINTSTRUCT): i32

    /// Returns a DC for the client area. Release with `ReleaseDC`.
    @lib pub fn GetDC(hwnd: *HWND): *HDC
    @lib pub fn ReleaseDC(hwnd: *HWND, hDC: *HDC): i32

    /// Marks `lpRect` (or the whole client area if NULL) dirty.
    @lib pub fn InvalidateRect(
        hwnd:   *HWND,
        lpRect: *const RECT,
        bErase: i32
    ): i32
    @lib pub fn ValidateRect(hwnd: *HWND, lpRect: *const RECT): i32

    /// Fills `lprc` with `hbr`. `hbr` can be a colour index + 1
    /// cast to `*HBRUSH`, e.g. `6 as *HBRUSH` for COLOR_WINDOW+1.
    @lib pub fn FillRect(hDC: *HDC, lprc: *const RECT, hbr: *HBRUSH): i32

    // ── Timer ─────────────────────────────────────────────────

    /// Sets a timer; fires WM_TIMER with `nIDEvent` as wParam.
    /// Pass `0 as *void` for `lpTimerFunc` to use the message queue.
    /// Returns the timer ID, or 0 on failure.
    @lib pub fn SetTimer(
        hwnd:          *HWND,
        nIDEvent:      u64,
        uElapse:       u32,
        lpTimerFunc:   *void
    ): u64
    @lib pub fn KillTimer(hwnd: *HWND, uIDEvent: u64): i32

    // ── Keyboard input ────────────────────────────────────────

    /// Tests the asynchronous state of a virtual key.
    /// Bit 15 (0x8000) set = key is down; bit 0 = was toggled.
    @lib pub fn GetAsyncKeyState(vKey: i32): i16

    /// Tests the queue-synchronised state of a virtual key.
    @lib pub fn GetKeyState(nVirtKey: i32): i16

    // ── Mouse / cursor ────────────────────────────────────────

    /// Fills `lpPoint` with the cursor position in screen coords.
    @lib pub fn GetCursorPos(lpPoint: *POINT): i32
    @lib pub fn SetCursorPos(X: i32, Y: i32): i32
    @lib pub fn ShowCursor(bShow: i32): i32

    /// Converts screen coordinates to client-area coordinates.
    @lib pub fn ScreenToClient(hwnd: *HWND, lpPoint: *POINT): i32
    /// Converts client-area coordinates to screen coordinates.
    @lib pub fn ClientToScreen(hwnd: *HWND, lpPoint: *POINT): i32

    /// Captures all mouse input to `hwnd` until `ReleaseCapture`.
    @lib pub fn SetCapture(hwnd: *HWND): *HWND
    @lib pub fn ReleaseCapture(): i32
    @lib pub fn GetCapture(): *HWND

    // ── Resources ────────────────────────────────────────────

    /// Loads a predefined or application icon.
    /// For system icons pass `0 as *HINSTANCE` and
    /// `SystemIcon.application as u32 as *const char`.
    @lib pub fn LoadIconA(
        hInstance:  *HINSTANCE,
        lpIconName: *const char
    ): *HICON

    /// Loads a predefined or application cursor.
    /// For system cursors pass `0 as *HINSTANCE` and
    /// `SystemCursor.arrow as u32 as *const char`.
    @lib pub fn LoadCursorA(
        hInstance:    *HINSTANCE,
        lpCursorName: *const char
    ): *HCURSOR

    // ── Dialogs ──────────────────────────────────────────────

    /// Shows a modal message box. Returns a `MessageBoxResult`.
    @lib pub fn MessageBoxA(
        hwnd:      *HWND,
        lpText:    *const char,
        lpCaption: *const char,
        uType:     u32
    ): i32
}
