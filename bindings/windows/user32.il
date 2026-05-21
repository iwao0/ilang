/// user32.dll — window classes, window management, painting,
/// input, messaging, timers, dialogs, and resources.
///
/// ANSI (`A`) variants only for now: arguments stay as
/// `*const char`, no UTF-16 conversion. Switch to `*W` once
/// a UTF-16 path lands.
///
/// Handle types are opaque empty structs so the type system
/// catches accidental mix-ups between e.g. HWND and HBRUSH.
/// WPARAM = `u64` (UINT_PTR), LPARAM = `i64` (LONG_PTR),
/// LRESULT = `i64` (LONG_PTR), ATOM = `u16`.

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
// Usage: `LoadCursorA(0 as HINSTANCE, SystemCursor.arrow as u32 as *const char)`
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
// Usage: `LoadIconA(0 as HINSTANCE, SystemIcon.application as u32 as *const char)`
pub enum SystemIcon: u32 {
    application = 32512
    error       = 32513   // IDI_HAND
    question    = 32514
    warning     = 32515   // IDI_EXCLAMATION
    information = 32516   // IDI_ASTERISK
    winLogo     = 32517
    shield      = 32518
}

// ─── GetSystemMetrics indices (SM_*) ─────────────────────────────
pub enum SystemMetric: i32 {
    cxScreen        = 0    // primary monitor width in pixels
    cyScreen        = 1    // primary monitor height in pixels
    cxVScroll       = 2    // vertical scroll bar width
    cyHScroll       = 3    // horizontal scroll bar height
    cyCaption       = 4    // title bar height (includes border)
    cxBorder        = 5
    cyBorder        = 6
    cxDlgFrame      = 7    // dialog frame width (= cxFixedFrame)
    cyDlgFrame      = 8
    cyCursor        = 13   // cursor height
    cxCursor        = 14   // cursor width  (same index on most OSes)
    cyMenu          = 15   // menu bar height
    cxFullScreen    = 16   // full-screen client area width
    cyFullScreen    = 17
    cyKanjiWindow   = 18
    mousePresent    = 19   // 0 = no mouse
    cyVScroll       = 20   // vertical scroll bar arrow height
    cxHScroll       = 21   // horizontal scroll bar arrow width
    debug           = 22
    swapButton      = 23   // left/right mouse buttons swapped?
    cxMin           = 28   // minimum window width
    cyMin           = 29   // minimum window height
    cxSize          = 30   // caption-button width
    cySize          = 31   // caption-button height
    cxFrame         = 32   // resizable window border width (= cxSizeFrame)
    cyFrame         = 33
    cxMinTrack      = 34   // minimum tracking size
    cyMinTrack      = 35
    cxDoubleClk     = 36   // double-click region width
    cyDoubleClk     = 37
    cxIconSpacing   = 38
    cyIconSpacing   = 39
    menudropalignment = 40 // 0=left-align, 1=right-align pop-ups
    penwindows      = 41
    dbcsenabled     = 42
    cmousebuttons   = 43
    secure          = 44
    cxEdge          = 45
    cyEdge          = 46
    cxMinSpacing    = 47
    cyMinSpacing    = 48
    cxSmIcon        = 49   // small icon width
    cySmIcon        = 50
    cySmCaption     = 51
    cxSmSize        = 52
    cySmSize        = 53
    cxMenuSize      = 54
    cyMenuSize      = 55
    arrange         = 56
    cxMinimized     = 57
    cyMinimized     = 58
    cxMaxTrack      = 59
    cyMaxTrack      = 60
    cxMaximized     = 61
    cyMaximized     = 62
    network         = 63
    cleanboot       = 67
    cxDrag          = 68
    cyDrag          = 69
    showsounds      = 70
    cxMenuCheck     = 71
    cyMenuCheck     = 72
    slowmachine     = 73
    mideastenabled  = 74
    mousewheelpresent = 75
    xVirtualScreen  = 76
    yVirtualScreen  = 77
    cxVirtualScreen = 78
    cyVirtualScreen = 79
    cMonitors       = 80
    samedisplayformat = 81
    immenabled      = 82
    cxFocusBorder   = 83
    cyFocusBorder   = 84
    tabletpc        = 86
    mediacenter     = 87
    starter         = 88
    serverr2        = 89
    mousehorizontalwheelpresent = 91
    cxPaddedBorder  = 92
    digitizer       = 94
    maximumtouches  = 95
}

// ─── GetWindowLongPtrA / SetWindowLongPtrA index values ──────────
pub enum WindowLongIndex: i32 {
    wndproc    = -4    // GWLP_WNDPROC — window procedure
    hinstance  = -6    // GWLP_HINSTANCE
    hwndparent = -8    // GWLP_HWNDPARENT
    id         = -12   // GWLP_ID — control identifier
    userdata   = -21   // GWLP_USERDATA
    style      = -16   // GWL_STYLE — window style (32-bit)
    exstyle    = -20   // GWL_EXSTYLE — extended style (32-bit)
    dlgmsgresult = 0   // DWLP_MSGRESULT — dialog message return
    dlgdlgproc = 8     // DWLP_DLGPROC — dialog procedure (x64 offset)
    dlguser    = 16    // DWLP_USER — dialog user data (x64 offset)
}

// ─── GetClassLongPtrA / SetClassLongPtrA index values ────────────
pub enum ClassLongIndex: i32 {
    menuName      = -8    // GCLP_MENUNAME
    hbrBackground = -10   // GCLP_HBRBACKGROUND
    hcursor       = -12   // GCLP_HCURSOR
    hicon         = -14   // GCLP_HICON
    hmodule       = -16   // GCLP_HMODULE
    cbWndExtra    = -18   // GCL_CBWNDEXTRA
    cbClsExtra    = -20   // GCL_CBCLSEXTRA
    wndproc       = -24   // GCLP_WNDPROC
    style         = -26   // GCL_STYLE
    hiconsm       = -34   // GCLP_HICONSM
    atomName      = -32   // GCL_ATOMNAME — WCHAR class name
}

// ─── TrackMouseEvent dwFlags (TME_*) ─────────────────────────────
@flags
pub enum TrackMouseFlags: u32 {
    hover    = 0x00000001   // generate WM_MOUSEHOVER
    leave    = 0x00000002   // generate WM_MOUSELEAVE
    nonclient = 0x00000010  // track non-client area
    query    = 0x40000000   // query existing tracking
    cancel   = 0x80000000   // cancel existing tracking
}

// ─── SetLayeredWindowAttributes flags (LWA_*) ────────────────────
@flags
pub enum LayeredWindowFlags: u32 {
    colorKey = 0x00000001   // use crKey as transparency color
    alpha    = 0x00000002   // use bAlpha as opacity (0=transparent)
}

// ─── AnimateWindow flags (AW_*) ──────────────────────────────────
@flags
pub enum AnimateWindowFlags: u32 {
    horPositive = 0x00000001
    horNegative = 0x00000002
    verPositive = 0x00000004
    verNegative = 0x00000008
    center      = 0x00000010
    hide        = 0x00010000
    activate    = 0x00020000
    slide       = 0x00040000
    blend       = 0x00080000
}

// ─── FlashWindow / FlashWindowEx flags (FLASHW_*) ────────────────
@flags
pub enum FlashWindowFlags: u32 {
    stop      = 0   // stop flashing
    caption   = 0x00000001
    tray      = 0x00000002
    all       = 0x00000003   // = caption | tray
    timer     = 0x00000004   // keep flashing until FLASHW_STOP
    timernofg = 0x0000000C   // flash until foregrounded
}

// ─── MouseEvent flags for INPUT_MOUSE (MOUSEEVENTF_*) ────────────
@flags
pub enum MouseEventFlags: u32 {
    move          = 0x0001
    leftDown      = 0x0002
    leftUp        = 0x0004
    rightDown     = 0x0008
    rightUp       = 0x0010
    middleDown    = 0x0020
    middleUp      = 0x0040
    xDown         = 0x0080
    xUp           = 0x0100
    wheel         = 0x0800
    hWheel        = 0x1000
    moveNocoalesce = 0x2000
    virtualDesk   = 0x4000
    absolute      = 0x8000
}

// ─── KeyboardEvent flags for INPUT_KEYBOARD (KEYEVENTF_*) ────────
@flags
pub enum KeyEventFlags: u32 {
    extendedKey = 0x0001
    keyUp       = 0x0002
    unicode     = 0x0004
    scanCode    = 0x0008
}

// ─── SendInput type constants ─────────────────────────────────────
pub enum InputType: u32 {
    mouse    = 0
    keyboard = 1
    hardware = 2
}

// ─── AppendMenuA fFlags (MF_*) ───────────────────────────────────
@flags
pub enum MenuFlags: u32 {
    insert      = 0x00000000
    change      = 0x00000080
    append      = 0x00000100
    delete      = 0x00000200
    remove      = 0x00001000
    bycommand   = 0x00000000
    byposition  = 0x00000400
    separator   = 0x00000800
    enabled     = 0x00000000
    grayed      = 0x00000001
    disabled    = 0x00000002
    unchecked   = 0x00000000
    checked     = 0x00000008
    usecheckbitmaps = 0x00000200
    string      = 0x00000000
    bitmap      = 0x00000004
    ownerdraw   = 0x00000100
    popup       = 0x00000010
    menubarbreak = 0x00000020
    menubreak   = 0x00000040
    unhilite    = 0x00000000
    hilite      = 0x00000080
    default_    = 0x00001000
    sysmenu     = 0x00002000
    help        = 0x00004000
    rightjustify = 0x00004000
    mouseselect = 0x00008000
}

// ─── TrackPopupMenu flags (TPM_*) ────────────────────────────────
@flags
pub enum TrackPopupMenuFlags: u32 {
    leftButton     = 0x0000
    rightButton    = 0x0002
    leftAlign      = 0x0000
    centerAlign    = 0x0004
    rightAlign     = 0x0008
    topAlign       = 0x0000
    vcenterAlign   = 0x0010
    bottomAlign    = 0x0020
    horizontal     = 0x0000
    vertical       = 0x0040
    noNotify       = 0x0080   // don't send WM_COMMAND to owner
    returnCmd      = 0x0100   // return selected command ID
    recurse        = 0x0001
    horPosAnimation = 0x0400
    horNegAnimation = 0x0800
    verPosAnimation = 0x1000
    verNegAnimation = 0x2000
    noAnimation     = 0x4000
    layoutRtl       = 0x8000
    workArea        = 0x10000
}

// ─── LoadImage type constants (IMAGE_*) ──────────────────────────
pub enum ImageType: u32 {
    bitmap  = 0
    icon    = 1
    cursor  = 2
    enhMetafile = 3
}

// ─── LoadImage flags (LR_*) ──────────────────────────────────────
@flags
pub enum LoadImageFlags: u32 {
    defaultColor  = 0x00000000
    monochrome    = 0x00000001
    color         = 0x00000002
    copyReturnOrg = 0x00000004
    copyDeleteOrg = 0x00000008
    loadFromFile  = 0x00000010
    loadTransparent = 0x00000020
    defaultSize   = 0x00000040
    vgaColor      = 0x00000080
    loadMap3dColors = 0x00001000
    createdibsection = 0x00002000
    copyfromresource = 0x00004000
    shared        = 0x00008000
}

// ─── SystemParametersInfoA (SPI_*) uiAction values (selection) ───
pub enum SpiAction: u32 {
    getBeep           = 1
    setBeep           = 2
    getMouseSpeed     = 112
    setMouseSpeed     = 113
    getWorkArea       = 48    // lpvParam: *RECT — usable desktop area
    setWorkArea       = 47
    getScreenSaveTimeout = 14
    setScreenSaveTimeout = 15
    getScreenSaveActive = 16
    setScreenSaveActive = 17
    getDragFullWindows = 38
    setDragFullWindows = 37
    getWheelScrollLines = 104
    setWheelScrollLines = 105
    getWheelScrollChars = 0x006C
    setWheelScrollChars = 0x006D
    getFontSmoothing    = 74
    setFontSmoothing    = 75
    getFontSmoothingType = 0x200A
    setFontSmoothingType = 0x200B
    getCursorShadow     = 0x101A
    setCursorShadow     = 0x101B
    getDropShadow       = 0x1024
    setDropShadow       = 0x1025
}

@extern(C, "user32") {
    // ── Opaque handle types ───────────────────────────────────

    /// Opaque window handle (`HWND` in win32). Pointer-sized, no
    /// in-memory representation beyond the bare value — write
    /// `hwnd: HWND` directly (no `*`) at every API boundary.
    @handle pub struct HWND {}

    /// Executable / DLL image handle. Obtain via
    /// `GetModuleHandleA(0 as *const char) as HINSTANCE`.
    @handle pub struct HINSTANCE {}

    /// Icon handle — returned by `LoadIconA` / `LoadImageA`.
    @handle pub struct HICON {}

    /// Cursor handle — returned by `LoadCursorA` / `LoadImageA`.
    @handle pub struct HCURSOR {}

    /// GDI brush handle. Stock brush: `(COLOR_WINDOW + 1) as HBRUSH`.
    @handle pub struct HBRUSH {}

    /// Menu handle — returned by `CreateMenu` / `LoadMenuA`.
    @handle pub struct HMENU {}

    /// Device-context handle for GDI drawing operations.
    @handle pub struct HDC {}

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
        hwnd:     HWND
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
        hdc:         HDC
        fErase:      i32
        rcPaint:     RECT
        fRestore:    i32
        fIncUpdate:  i32
        rgbReserved: u8[32]   // internal use
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
        hInstance:      HINSTANCE
        hMenu:          HMENU
        hwndParent:     HWND
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
        hwnd:            HWND
        hwndInsertAfter: HWND
        x:               i32
        y:               i32
        cx:              i32
        cy:              i32
        flags:           u32
    }

    // ── Window class / creation ───────────────────────────────

    /// `WNDCLASSEXA` — 80 bytes on x64. Set `cbSize` to 80.
    pub struct WNDCLASSEXA {
        /// `sizeof(WNDCLASSEXA)`. Must be `80` on x64; if the value
        /// doesn't match what user32 expects, `RegisterClassExA`
        /// silently fails (returns 0; check `GetLastError()`).
        cbSize:        u32
        /// Class-style bitmask. Build with `ClassStyle` flags
        /// (`ClassStyle.hredraw | ClassStyle.vredraw | ...` then
        /// `as u32`).
        style:         u32
        /// First param is HWND. Declared `i64` so the user-side
        /// `wndProc` can be a `pub fn` without raw pointer args.
        lpfnWndProc:   fn(i64, u32, u64, i64): i64
        /// Extra per-class bytes the window manager allocates next
        /// to the class. Most apps leave this `0`.
        cbClsExtra:    i32
        /// Extra per-window bytes attached to every window of this
        /// class (accessed via `GetWindowLongPtr` / `SetWindowLongPtr`
        /// with positive offsets). Most apps leave this `0`.
        cbWndExtra:    i32
        // 4 bytes implicit padding before hInstance.
        /// EXE / DLL handle this class belongs to. Typically
        /// `GetModuleHandleA(0 as *const char)` for the main EXE.
        hInstance:     HINSTANCE
        /// Large window icon. `NULL` for the default app icon.
        hIcon:         HICON
        /// Mouse cursor used inside the client area. `NULL` leaves
        /// it undefined (cursor flickers); usually set to
        /// `LoadCursorA(NULL, IDC_ARROW)`.
        hCursor:       HCURSOR
        /// Background brush. Pass `NULL` to handle `WM_ERASEBKGND`
        /// yourself, or `(COLOR_WINDOW + 1) as HBRUSH` (i.e.
        /// `6 as HBRUSH`) to fill with the system window colour.
        hbrBackground: HBRUSH
        /// Default menu resource name (`MAKEINTRESOURCEA(id)`).
        /// `NULL` when the window has no class-level menu.
        lpszMenuName:  *const char
        /// Class name passed back to `CreateWindowExA`. Use
        /// `cstrFromString(name)` to convert an ilang `string`.
        lpszClassName: *const char
        /// Small (taskbar / titlebar) icon. `NULL` falls back to
        /// `hIcon`.
        hIconSm:       HICON
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
        hWndParent:   HWND,
        hMenu:        HMENU,
        hInstance:    HINSTANCE,
        lpParam:      *void
    ): HWND

    // ── Default message handling ──────────────────────────────

    /// Default window procedure; call for every unhandled message.
    @lib pub fn DefWindowProcA(
        hwnd: HWND, msg: u32, wParam: u64, lParam: i64
    ): i64

    // ── Visibility / state ────────────────────────────────────

    @lib pub fn ShowWindow(hwnd: HWND, nCmdShow: i32): i32
    @lib pub fn UpdateWindow(hwnd: HWND): i32
    @lib pub fn DestroyWindow(hwnd: HWND): i32
    @lib pub fn IsWindow(hwnd: HWND): i32
    @lib pub fn IsWindowVisible(hwnd: HWND): i32
    @lib pub fn EnableWindow(hwnd: HWND, bEnable: i32): i32
    @lib pub fn BringWindowToTop(hwnd: HWND): i32

    // ── Position / size ───────────────────────────────────────

    @lib pub fn MoveWindow(
        hwnd:    HWND,
        X:       i32,
        Y:       i32,
        nWidth:  i32,
        nHeight: i32,
        bRepaint: i32
    ): i32

    /// `hwndInsertAfter` special values (cast from integer):
    ///   HWND_TOP     = `0 as HWND`
    ///   HWND_BOTTOM  = `1 as HWND`
    ///   HWND_TOPMOST = `-1 as i64 as HWND`
    ///   HWND_NOTOPMOST = `-2 as i64 as HWND`
    @lib pub fn SetWindowPos(
        hwnd:            HWND,
        hwndInsertAfter: HWND,
        X:               i32,
        Y:               i32,
        cx:              i32,
        cy:              i32,
        uFlags:          u32
    ): i32

    @lib pub fn GetWindowRect(hwnd: HWND, lpRect: *RECT): i32
    @lib pub fn GetClientRect(hwnd: HWND, lpRect: *RECT): i32

    /// Computes the required window rect for a desired client rect.
    @lib pub fn AdjustWindowRectEx(
        lpRect:    *RECT,
        dwStyle:   u32,
        bMenu:     i32,
        dwExStyle: u32
    ): i32

    // ── Window text ───────────────────────────────────────────

    @lib pub fn SetWindowTextA(hwnd: HWND, lpString: *const char): i32
    /// Returns the number of characters copied (excluding NUL).
    @lib pub fn GetWindowTextA(
        hwnd:       HWND,
        lpString:   *char,
        nMaxCount:  i32
    ): i32
    @lib pub fn GetWindowTextLengthA(hwnd: HWND): i32

    // ── Z-order / focus / parent ──────────────────────────────

    @lib pub fn SetFocus(hwnd: HWND): HWND
    @lib pub fn GetFocus(): HWND
    @lib pub fn GetForegroundWindow(): HWND
    @lib pub fn SetForegroundWindow(hwnd: HWND): i32
    @lib pub fn GetDesktopWindow(): HWND
    @lib pub fn GetParent(hwnd: HWND): HWND
    @lib pub fn SetParent(hwndChild: HWND, hwndNewParent: HWND): HWND
    @lib pub fn FindWindowA(
        lpClassName:  *const char,
        lpWindowName: *const char
    ): HWND

    /// Returns the thread that owns `hwnd` and writes its process ID
    /// to `lpdwProcessId` (pass `0 as *u32` to skip).
    @lib pub fn GetWindowThreadProcessId(hwnd: HWND, lpdwProcessId: *u32): u32

    // ── Message loop ─────────────────────────────────────────

    /// Blocking message fetch. Returns >0 (message), 0 (WM_QUIT),
    /// or -1 (error).
    @lib pub fn GetMessageA(
        lpMsg:         *MSG,
        hwnd:          HWND,
        wMsgFilterMin: u32,
        wMsgFilterMax: u32
    ): i32

    /// Non-blocking message check. `wRemoveMsg`: 0=peek only,
    /// 1=remove (PM_REMOVE).
    @lib pub fn PeekMessageA(
        lpMsg:         *MSG,
        hwnd:          HWND,
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
        hwnd:   HWND,
        msg:    u32,
        wParam: u64,
        lParam: i64
    ): i64

    /// Posts a message asynchronously; returns immediately.
    @lib pub fn PostMessageA(
        hwnd:   HWND,
        msg:    u32,
        wParam: u64,
        lParam: i64
    ): i32

    // ── Painting ─────────────────────────────────────────────

    /// Begins painting; fills `lpPaint` and returns a DC.
    /// Must be paired with `EndPaint`.
    @lib pub fn BeginPaint(hwnd: HWND, lpPaint: *PAINTSTRUCT): HDC
    @lib pub fn EndPaint(hwnd: HWND, lpPaint: *const PAINTSTRUCT): i32

    /// Returns a DC for the client area. Release with `ReleaseDC`.
    @lib pub fn GetDC(hwnd: HWND): HDC
    @lib pub fn ReleaseDC(hwnd: HWND, hDC: HDC): i32

    /// Marks `lpRect` (or the whole client area if NULL) dirty.
    @lib pub fn InvalidateRect(
        hwnd:   HWND,
        lpRect: *const RECT,
        bErase: i32
    ): i32
    @lib pub fn ValidateRect(hwnd: HWND, lpRect: *const RECT): i32

    /// Fills `lprc` with `hbr`. `hbr` can be a colour index + 1
    /// cast to `HBRUSH`, e.g. `6 as HBRUSH` for COLOR_WINDOW+1.
    @lib pub fn FillRect(hDC: HDC, lprc: *const RECT, hbr: HBRUSH): i32

    // ── Timer ─────────────────────────────────────────────────

    /// Sets a timer; fires WM_TIMER with `nIDEvent` as wParam.
    /// Pass `0 as *void` for `lpTimerFunc` to use the message queue.
    /// Returns the timer ID, or 0 on failure.
    @lib pub fn SetTimer(
        hwnd:          HWND,
        nIDEvent:      u64,
        uElapse:       u32,
        lpTimerFunc:   *void
    ): u64
    @lib pub fn KillTimer(hwnd: HWND, uIDEvent: u64): i32

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
    @lib pub fn ScreenToClient(hwnd: HWND, lpPoint: *POINT): i32
    /// Converts client-area coordinates to screen coordinates.
    @lib pub fn ClientToScreen(hwnd: HWND, lpPoint: *POINT): i32

    /// Captures all mouse input to `hwnd` until `ReleaseCapture`.
    @lib pub fn SetCapture(hwnd: HWND): HWND
    @lib pub fn ReleaseCapture(): i32
    @lib pub fn GetCapture(): HWND

    // ── Resources ────────────────────────────────────────────

    /// Loads a predefined or application icon.
    /// For system icons pass `0 as HINSTANCE` and
    /// `SystemIcon.application as u32 as *const char`.
    @lib pub fn LoadIconA(
        hInstance:  HINSTANCE,
        lpIconName: *const char
    ): HICON

    /// Loads a predefined or application cursor.
    /// For system cursors pass `0 as HINSTANCE` and
    /// `SystemCursor.arrow as u32 as *const char`.
    @lib pub fn LoadCursorA(
        hInstance:    HINSTANCE,
        lpCursorName: *const char
    ): HCURSOR

    // ── Dialogs ──────────────────────────────────────────────

    /// Shows a modal message box. Returns a `MessageBoxResult`.
    @lib pub fn MessageBoxA(
        hwnd:      HWND,
        lpText:    *const char,
        lpCaption: *const char,
        uType:     u32
    ): i32

    // ── Multi-monitor ─────────────────────────────────────────────

    /// Opaque monitor handle. Returned by MonitorFromWindow etc.
    @handle pub struct HMONITOR {}

    /// `MONITORINFO` — 40 bytes. Fill `cbSize` = 40 before calling
    /// `GetMonitorInfoA`.
    pub struct MONITORINFO {
        cbSize:    u32
        rcMonitor: RECT   // full monitor rect in virtual-screen coords
        rcWork:    RECT   // work area (excludes taskbar)
        dwFlags:   u32    // 1 = MONITORINFOF_PRIMARY
    }

    /// Returns the monitor that contains `hwnd`, or the nearest.
    /// `dwFlags`: 0=MONITOR_DEFAULTTONULL, 1=NEAREST, 2=PRIMARY.
    @lib pub fn MonitorFromWindow(hwnd: HWND, dwFlags: u32): HMONITOR

    @lib pub fn MonitorFromPoint(pt: i64, dwFlags: u32): HMONITOR   // pt is packed POINT (lo=x, hi=y)
    @lib pub fn MonitorFromRect(lprc: *const RECT, dwFlags: u32): HMONITOR

    @lib pub fn GetMonitorInfoA(hMonitor: HMONITOR, lpmi: *MONITORINFO): i32

    /// Calls `lpfnEnum(hMonitor, hdcMonitor, lprcMonitor, dwData)`
    /// for every monitor; stops when the callback returns FALSE.
    @lib pub fn EnumDisplayMonitors(
        hdc:       *void,      // HDC filter, or NULL for all
        lprcClip:  *const RECT,
        lpfnEnum:  fn(i64, i64, i64, i64): i32,  // MONITORENUMPROC
        dwData:    i64
    ): i32

    // ── Window enumeration ────────────────────────────────────────

    /// Calls `lpEnumFunc(hwnd, lParam)` for every top-level window.
    @lib pub fn EnumWindows(
        lpEnumFunc: fn(i64, i64): i32,
        lParam:     i64
    ): i32

    @lib pub fn EnumChildWindows(
        hwndParent: HWND,
        lpEnumFunc: fn(i64, i64): i32,
        lParam:     i64
    ): i32

    // ── Window / class long ───────────────────────────────────────

    /// Reads a window attribute by index (WindowLongIndex.*).
    /// Use this for pointer-sized values; use GetWindowLongA for u32.
    @lib pub fn GetWindowLongPtrA(hwnd: HWND, nIndex: i32): i64

    @lib pub fn SetWindowLongPtrA(hwnd: HWND, nIndex: i32, dwNewLong: i64): i64

    @lib pub fn GetWindowLongA(hwnd: HWND, nIndex: i32): i32
    @lib pub fn SetWindowLongA(hwnd: HWND, nIndex: i32, dwNewLong: i32): i32

    @lib pub fn GetClassLongPtrA(hwnd: HWND, nIndex: i32): u64
    @lib pub fn SetClassLongPtrA(hwnd: HWND, nIndex: i32, dwNewLong: i64): u64

    // ── System metrics ────────────────────────────────────────────

    /// Returns a system metric. `nIndex`: SystemMetric.*.
    @lib pub fn GetSystemMetrics(nIndex: i32): i32

    // ── Mouse tracking ────────────────────────────────────────────

    /// `TRACKMOUSEEVENT` — 24 bytes (with trailing padding).
    /// Set `cbSize` = 24, `dwFlags` = TrackMouseFlags.*, `hwndTrack`,
    /// and optionally `dwHoverTime` (HOVER_DEFAULT = 0xFFFFFFFF).
    pub struct TRACKMOUSEEVENT {
        cbSize:      u32
        dwFlags:     u32
        hwndTrack:   HWND
        dwHoverTime: u32
    }

    /// Requests WM_MOUSEHOVER / WM_MOUSELEAVE notifications.
    @lib pub fn TrackMouseEvent(lpEventTrack: *TRACKMOUSEEVENT): i32

    // ── Layered windows ───────────────────────────────────────────

    /// Sets per-pixel or colour-key transparency for a layered window
    /// (must have WS_EX_LAYERED set with SetWindowLongPtrA first).
    /// `crKey`: transparent colour (COLORREF). `bAlpha`: 0=transparent.
    /// `dwFlags`: LayeredWindowFlags.*.
    @lib pub fn SetLayeredWindowAttributes(
        hwnd:    HWND,
        crKey:   u32,
        bAlpha:  u8,
        dwFlags: u32
    ): i32

    @lib pub fn GetLayeredWindowAttributes(
        hwnd:    HWND,
        pcrKey:  *u32,
        pbAlpha: *u8,
        pdwFlags: *u32
    ): i32

    // ── Window animation ──────────────────────────────────────────

    @lib pub fn AnimateWindow(hwnd: HWND, dwTime: u32, dwFlags: u32): i32

    // ── Flashing ─────────────────────────────────────────────────

    /// `FLASHWINFO` — 32 bytes.
    /// Set `cbSize` = 32, `hwnd`, `dwFlags` (FlashWindowFlags.*),
    /// `uCount` (flash count), and `dwTimeout` (ms, 0=cursor blink rate).
    pub struct FLASHWINFO {
        cbSize:    u32
        // 4 bytes implicit padding before hwnd
        hwnd:      HWND
        dwFlags:   u32
        uCount:    u32
        dwTimeout: u32
    }

    @lib pub fn FlashWindow(hwnd: HWND, bInvert: i32): i32
    @lib pub fn FlashWindowEx(pfwi: *const FLASHWINFO): i32

    // ── Hot keys ─────────────────────────────────────────────────

    /// Registers a global hot key. `id`: app-defined identifier (1–49151).
    /// `fsModifiers`: MOD_ALT=1, MOD_CONTROL=2, MOD_SHIFT=4, MOD_WIN=8.
    @lib pub fn RegisterHotKey(hwnd: HWND, id: i32, fsModifiers: u32, vk: u32): i32
    @lib pub fn UnregisterHotKey(hwnd: HWND, id: i32): i32

    // ── Virtual key mapping ───────────────────────────────────────

    /// Translates a virtual key code to a scan code, or vice-versa.
    /// `uMapType`: 0=VK→scan, 1=scan→VK, 2=VK→char, 3=VK→unshifted char.
    @lib pub fn MapVirtualKeyA(uCode: u32, uMapType: u32): u32

    // ── SendInput ─────────────────────────────────────────────────

    /// `INPUT_MOUSE` — 40 bytes. Set `type_` = 0 (InputType.mouse).
    /// `mouseFlags`: MouseEventFlags.*; `dwExtraInfo`: 0 for normal input.
    pub struct INPUT_MOUSE {
        type_:       u32          // = 0
        _pad0:       u32          // alignment gap before union
        dx:          i32
        dy:          i32
        mouseData:   u32          // wheel delta × WHEEL_DELTA or X-button ID
        mouseFlags:  u32
        time:        u32
        _pad1:       u32          // alignment gap before extraInfo
        dwExtraInfo: u64
    }

    /// `INPUT_KEYBOARD` — 40 bytes. Set `type_` = 1 (InputType.keyboard).
    /// `kbdFlags`: KeyEventFlags.*; set keyUp to simulate key release.
    pub struct INPUT_KEYBOARD {
        type_:       u32          // = 1
        _pad0:       u32          // alignment gap before union
        wVk:         u16          // VirtualKey value (0 for scan-code only)
        wScan:       u16          // hardware scan code
        kbdFlags:    u32
        time:        u32
        _pad1:       u32
        dwExtraInfo: u64
        _tail:       u64          // padding to match MOUSEINPUT union size
    }

    /// Synthesises input events. `nInputs`: number of INPUT_* structs.
    /// `cbSize`: sizeof the struct (40). Returns number of events inserted.
    @lib pub fn SendInput(nInputs: u32, pInputs: *void, cbSize: i32): u32

    // ── Window info ───────────────────────────────────────────────

    /// `WINDOWINFO` — 60 bytes. Set `cbSize` = 60.
    pub struct WINDOWINFO {
        cbSize:          u32
        rcWindow:        RECT   // window rect in screen coords
        rcClient:        RECT   // client rect in screen coords
        dwStyle:         u32
        dwExStyle:       u32
        dwWindowStatus:  u32   // WS_ACTIVECAPTION = 1 if active
        cxWindowBorders: u32
        cyWindowBorders: u32
        atomWindowType:  u16
        wCreatorVersion: u16
    }

    @lib pub fn GetWindowInfo(hwnd: HWND, pwi: *WINDOWINFO): i32

    // ── Window placement ─────────────────────────────────────────

    /// `WINDOWPLACEMENT` — 44 bytes. Set `length` = 44.
    pub struct WINDOWPLACEMENT {
        length:           u32
        flags:            u32
        showCmd:          u32
        ptMinPosition:    POINT
        ptMaxPosition:    POINT
        rcNormalPosition: RECT
    }

    @lib pub fn GetWindowPlacement(hwnd: HWND, lpwndpl: *WINDOWPLACEMENT): i32
    @lib pub fn SetWindowPlacement(hwnd: HWND, lpwndpl: *const WINDOWPLACEMENT): i32

    // ── Menu ──────────────────────────────────────────────────────

    /// Returns the window's menu handle (NULL if none).
    @lib pub fn GetMenu(hwnd: HWND): HMENU

    /// Attaches `hMenu` to `hwnd` as the menu bar.
    @lib pub fn SetMenu(hwnd: HWND, hMenu: HMENU): i32

    @lib pub fn CreateMenu(): HMENU
    @lib pub fn CreatePopupMenu(): HMENU
    @lib pub fn DestroyMenu(hMenu: HMENU): i32

    /// Appends a menu item. `uFlags`: MenuFlags.*; `uIDNewItem`:
    /// command ID (for commands) or popup HMENU (for sub-menus);
    /// `lpNewItem`: label string (or bitmap/null per flags).
    @lib pub fn AppendMenuA(
        hMenu:       HMENU,
        uFlags:      u32,
        uIDNewItem:  u64,    // UINT_PTR — command ID or HMENU
        lpNewItem:   *const char
    ): i32

    @lib pub fn InsertMenuA(
        hMenu:       HMENU,
        uPosition:   u32,
        uFlags:      u32,
        uIDNewItem:  u64,
        lpNewItem:   *const char
    ): i32

    @lib pub fn DeleteMenu(hMenu: HMENU, uPosition: u32, uFlags: u32): i32
    @lib pub fn RemoveMenu(hMenu: HMENU, uPosition: u32, uFlags: u32): i32
    @lib pub fn ModifyMenuA(
        hMenu:       HMENU,
        uPosition:   u32,
        uFlags:      u32,
        uIDNewItem:  u64,
        lpNewItem:   *const char
    ): i32

    @lib pub fn EnableMenuItem(hMenu: HMENU, uIDEnableItem: u32, uEnable: u32): i32
    @lib pub fn CheckMenuItem(hMenu: HMENU, uIDCheckItem: u32, uCheck: u32): u32
    @lib pub fn CheckMenuRadioItem(
        hMenu:  HMENU,
        first:  u32,
        last:   u32,
        check:  u32,
        flags:  u32
    ): i32
    @lib pub fn GetMenuItemCount(hMenu: HMENU): i32
    @lib pub fn GetMenuItemID(hMenu: HMENU, nPos: i32): u32
    @lib pub fn GetSubMenu(hMenu: HMENU, nPos: i32): HMENU
    @lib pub fn DrawMenuBar(hwnd: HWND): i32

    /// Shows a floating popup menu. `hwnd` receives WM_COMMAND.
    /// `uFlags`: TrackPopupMenuFlags.*. Returns selected cmd or 0.
    @lib pub fn TrackPopupMenu(
        hMenu:  HMENU,
        uFlags: u32,
        x:      i32,
        y:      i32,
        nReserved: i32,
        hwnd:   HWND,
        prcRect: *const RECT
    ): i32

    // ── LoadImage ─────────────────────────────────────────────────

    /// Loads a bitmap, icon, or cursor from a file or resource.
    /// `hInst`: 0 for file-based; `image`: filename or resource ID;
    /// `type_`: ImageType.*; `cx`/`cy`: 0 = use embedded size;
    /// `fuLoad`: LoadImageFlags.*.
    @lib pub fn LoadImageA(
        hInst:  *void,   // HINSTANCE
        image:  *const char,
        type_:  u32,
        cx:     i32,
        cy:     i32,
        fuLoad: u32
    ): *void

    // ── Registered window messages ────────────────────────────────

    /// Registers a unique message for inter-application communication.
    /// Returns a value in 0xC000–0xFFFF, unique to the string.
    @lib pub fn RegisterWindowMessageA(lpString: *const char): u32

    // ── Character conversion ──────────────────────────────────────

    @lib pub fn CharToOemA(pSrc: *const char, pDst: *char): i32
    @lib pub fn OemToCharA(pSrc: *const char, pDst: *char): i32
    @lib pub fn CharUpperA(lpsz: *char): *char
    @lib pub fn CharLowerA(lpsz: *char): *char

    // ── Window procedure calling ──────────────────────────────────

    /// Passes a message to the window procedure at `lpPrevWndFunc`
    /// (typically the original procedure saved before subclassing).
    @lib pub fn CallWindowProcA(
        lpPrevWndFunc: fn(i64, u32, u64, i64): i64,
        hwnd:          HWND,
        msg:           u32,
        wParam:        u64,
        lParam:        i64
    ): i64

    // ── System parameters ─────────────────────────────────────────

    /// Gets or sets a system-wide parameter. `uiAction`: SpiAction.*.
    /// `lpvParam`: action-specific pointer; `fWinIni`: 0 = no broadcast.
    @lib pub fn SystemParametersInfoA(
        uiAction:  u32,
        uiParam:   u32,
        pvParam:   *void,
        fWinIni:   u32
    ): i32

    // ── Cursor confinement ────────────────────────────────────────

    /// Confines the cursor to `lpRect` (screen coords).
    /// Pass `0 as *const RECT` to release confinement.
    @lib pub fn ClipCursor(lpRect: *const RECT): i32

    /// Retrieves the current cursor clip rect (full screen if unclipped).
    @lib pub fn GetClipCursor(lpRect: *RECT): i32

    // ── Clipboard ─────────────────────────────────────────────────

    /// Opens the clipboard for exclusive access by the caller.
    /// `hWndNewOwner`: 0 to associate with the calling task only.
    /// Pair with `CloseClipboard`.
    @lib pub fn OpenClipboard(hWndNewOwner: HWND): i32
    @lib pub fn CloseClipboard(): i32
    @lib pub fn EmptyClipboard(): i32

    /// Retrieves clipboard data of the given format (CF_TEXT=1,
    /// CF_BITMAP=2, CF_OEMTEXT=7, CF_DIB=8, CF_UNICODETEXT=13,
    /// CF_HDROP=15). Returns a GMEM handle — call `GlobalLock`
    /// (kernel32) to access the bytes.
    @lib pub fn GetClipboardData(uFormat: u32): *void

    /// Places data on the clipboard. `hMem` must be a GMEM_MOVEABLE
    /// allocation (from `GlobalAlloc` with flags 0x42). After this
    /// call, the system owns the handle.
    @lib pub fn SetClipboardData(uFormat: u32, hMem: *void): *void

    @lib pub fn IsClipboardFormatAvailable(format: u32): i32

    /// Enumerates registered clipboard formats. Pass 0 to start;
    /// each subsequent call returns the next format, 0 at end.
    @lib pub fn EnumClipboardFormats(format: u32): u32

    /// Registers a private clipboard format and returns a unique
    /// id in range 0xC000..0xFFFF.
    @lib pub fn RegisterClipboardFormatA(lpszFormat: *const char): u32

    /// Returns the registered name of a private clipboard format.
    @lib pub fn GetClipboardFormatNameA(
        format:      u32,
        lpszFormatName: *char,
        cchMaxCount: i32
    ): i32

    @lib pub fn CountClipboardFormats(): i32
    @lib pub fn GetClipboardOwner(): HWND
    @lib pub fn GetOpenClipboardWindow(): HWND

    // ── Dialog API ────────────────────────────────────────────────

    /// Creates and shows a modal dialog from a resource template.
    /// `lpTemplateName`: MAKEINTRESOURCE(id) cast to `*const char`.
    /// `lpDialogFunc`: dialog procedure (returns INT_PTR / i64).
    /// `dwInitParam`: passed to the dialog as the lParam of WM_INITDIALOG.
    /// Returns the nResult passed to `EndDialog`.
    @lib pub fn DialogBoxParamA(
        hInstance:      HINSTANCE,
        lpTemplateName: *const char,
        hWndParent:     HWND,
        lpDialogFunc:   fn(i64, u32, u64, i64): i64,
        dwInitParam:    i64
    ): i64

    /// Creates a modeless dialog.  Free with `DestroyWindow`.
    @lib pub fn CreateDialogParamA(
        hInstance:      HINSTANCE,
        lpTemplateName: *const char,
        hWndParent:     HWND,
        lpDialogFunc:   fn(i64, u32, u64, i64): i64,
        dwInitParam:    i64
    ): HWND

    /// Closes a modal dialog. The value is returned from `DialogBoxParamA`.
    @lib pub fn EndDialog(hDlg: HWND, nResult: i64): i32

    /// Returns the HWND of a control by ID. Returns NULL if not found.
    @lib pub fn GetDlgItem(hDlg: HWND, nIDDlgItem: i32): HWND

    @lib pub fn GetDlgCtrlID(hwndCtl: HWND): i32

    /// Sends a message to the named control.
    @lib pub fn SendDlgItemMessageA(
        hDlg:       HWND,
        nIDDlgItem: i32,
        Msg:        u32,
        wParam:     u64,
        lParam:     i64
    ): i64

    @lib pub fn SetDlgItemTextA(hDlg: HWND, nIDDlgItem: i32, lpString: *const char): i32
    @lib pub fn GetDlgItemTextA(
        hDlg:       HWND,
        nIDDlgItem: i32,
        lpString:   *char,
        nMaxCount:  i32
    ): u32

    @lib pub fn SetDlgItemInt(
        hDlg:       HWND,
        nIDDlgItem: i32,
        uValue:     u32,
        bSigned:    i32
    ): i32

    @lib pub fn GetDlgItemInt(
        hDlg:        HWND,
        nIDDlgItem:  i32,
        lpTranslated: *i32,
        bSigned:     i32
    ): u32

    @lib pub fn CheckDlgButton(hDlg: HWND, nIDButton: i32, uCheck: u32): i32
    @lib pub fn IsDlgButtonChecked(hDlg: HWND, nIDButton: i32): u32

    @lib pub fn CheckRadioButton(
        hDlg:           HWND,
        nIDFirstButton: i32,
        nIDLastButton:  i32,
        nIDCheckButton: i32
    ): i32

    /// Returns the next/previous tab-stop control in a dialog.
    @lib pub fn GetNextDlgTabItem(
        hDlg:        HWND,
        hCtl:        HWND,
        bPrevious:   i32
    ): HWND

    @lib pub fn IsDialogMessageA(hDlg: HWND, lpMsg: *MSG): i32

    // ── Hooks ─────────────────────────────────────────────────────

    /// Opaque hook handle.
    @handle pub struct HHOOK {}

    /// Installs a hook procedure that monitors system events.
    /// `idHook`: WH_KEYBOARD_LL=13, WH_MOUSE_LL=14, WH_CALLWNDPROC=4,
    /// WH_GETMESSAGE=3, WH_CBT=5, etc.
    /// `lpfn`: callback (nCode, wParam, lParam) → i64.
    /// `hMod`: module containing the procedure (NULL for current).
    /// `dwThreadId`: 0 = global hook (low-level only); else thread id.
    @lib pub fn SetWindowsHookExA(
        idHook:     i32,
        lpfn:       fn(i32, u64, i64): i64,
        hMod:       HINSTANCE,
        dwThreadId: u32
    ): HHOOK

    /// Removes a previously installed hook.
    @lib pub fn UnhookWindowsHookEx(hhk: HHOOK): i32

    /// Passes a hook event to the next hook in the chain.
    /// Every hook procedure should call this for unhandled events.
    @lib pub fn CallNextHookEx(
        hhk:    HHOOK,
        nCode:  i32,
        wParam: u64,
        lParam: i64
    ): i64

    // ── Raw input ─────────────────────────────────────────────────

    /// `RAWINPUTDEVICE` — 16 bytes on x64.
    /// `usUsagePage` / `usUsage`: HID usage page + usage to register
    /// (e.g. 0x01/0x06 = generic keyboard, 0x01/0x02 = generic mouse,
    ///       0x01/0x04 = joystick, 0x01/0x05 = gamepad).
    /// `dwFlags`: RIDEV_REMOVE=0x1, RIDEV_NOLEGACY=0x30, RIDEV_INPUTSINK=0x100.
    /// `hwndTarget`: window receiving WM_INPUT (0 for the focused window).
    pub struct RAWINPUTDEVICE {
        usUsagePage: u16
        usUsage:     u16
        dwFlags:     u32
        hwndTarget:  HWND
    }

    /// `RAWINPUTHEADER` — 24 bytes. Common header at the start of
    /// each RAWINPUT structure.
    pub struct RAWINPUTHEADER {
        dwType:    u32      // 0=mouse, 1=keyboard, 2=HID
        dwSize:    u32
        hDevice:   *void    // HANDLE
        wParam:    u64
    }

    /// Registers raw-input devices to deliver WM_INPUT to `hwndTarget`.
    /// `cbSize`: must be sizeof(RAWINPUTDEVICE) = 16.
    @lib pub fn RegisterRawInputDevices(
        pRawInputDevices: *const RAWINPUTDEVICE,
        uiNumDevices:     u32,
        cbSize:           u32
    ): i32

    /// Reads the RAWINPUT for a WM_INPUT message.
    /// `hRawInput`: the lParam of WM_INPUT cast to *void.
    /// `uiCommand`: 0x10000003 (RID_INPUT) for data, 0x10000005 (RID_HEADER) for header only.
    /// Pass `0 as *void` for `pData` to query the size.
    @lib pub fn GetRawInputData(
        hRawInput: *void,
        uiCommand: u32,
        pData:     *void,
        pcbSize:   *u32,
        cbSizeHeader: u32
    ): u32

    @lib pub fn GetRawInputDeviceList(
        pRawInputDeviceList: *void,
        puiNumDevices:       *u32,
        cbSize:              u32
    ): u32

    @lib pub fn GetRawInputDeviceInfoA(
        hDevice: *void,
        uiCommand: u32,
        pData:   *void,
        pcbSize: *u32
    ): u32

    // ── Scroll bar ────────────────────────────────────────────────

    /// `SCROLLINFO` — 28 bytes.  Set `cbSize` = 28 and `fMask` to
    /// indicate which fields are valid (SIF_RANGE=1, SIF_PAGE=2,
    /// SIF_POS=4, SIF_DISABLENOSCROLL=8, SIF_TRACKPOS=0x10,
    /// SIF_ALL=0x17).
    pub struct SCROLLINFO {
        cbSize:    u32
        fMask:     u32
        nMin:      i32
        nMax:      i32
        nPage:     u32
        nPos:      i32
        nTrackPos: i32
    }

    /// `fnBar`: 0=SB_HORZ, 1=SB_VERT, 2=SB_CTL, 3=SB_BOTH.
    @lib pub fn GetScrollInfo(hwnd: HWND, nBar: i32, lpsi: *SCROLLINFO): i32
    @lib pub fn SetScrollInfo(hwnd: HWND, nBar: i32, lpsi: *const SCROLLINFO, redraw: i32): i32

    @lib pub fn ScrollWindow(
        hwnd:     HWND,
        xAmount:  i32,
        yAmount:  i32,
        prcScroll: *const RECT,
        prcClip:   *const RECT
    ): i32

    @lib pub fn ScrollWindowEx(
        hwnd:     HWND,
        dx:       i32,
        dy:       i32,
        prcScroll: *const RECT,
        prcClip:   *const RECT,
        hrgnUpdate: *void,
        prcUpdate: *RECT,
        flags:    u32
    ): i32

    @lib pub fn ShowScrollBar(hwnd: HWND, wBar: i32, bShow: i32): i32

    // ── Misc utility ──────────────────────────────────────────────

    /// Plays a system sound. `uType`: 0=default beep, 0x10=hand,
    /// 0x20=question, 0x30=exclamation, 0x40=asterisk, 0xFFFFFFFF=simple beep.
    @lib pub fn MessageBeep(uType: u32): i32

    /// Returns the current cursor info: state and screen position.
    @lib pub fn GetCursor(): HCURSOR

    @lib pub fn SetCursor(hCursor: HCURSOR): HCURSOR

    /// Returns 1 if the user is currently active (typing/moving mouse).
    @lib pub fn GetLastInputInfo(plii: *void): i32

    // ── Per-window DPI (Windows 10+) ─────────────────────────────

    /// Returns the DPI of `hwnd`. 96 = 100%.
    @lib pub fn GetDpiForWindow(hwnd: HWND): u32

    /// Returns the DPI of the system as a whole.
    @lib pub fn GetDpiForSystem(): u32

    /// Sets the DPI awareness context for the calling thread.
    /// Returns the previous context, or NULL if `dpiContext` is invalid.
    @lib pub fn SetThreadDpiAwarenessContext(dpiContext: *void): *void

    @lib pub fn GetThreadDpiAwarenessContext(): *void

    /// Scales `value` from `dpiFrom` to `dpiTo` (e.g. system → window DPI).
    @lib pub fn AdjustWindowRectExForDpi(
        lpRect:   *RECT,
        dwStyle:  u32,
        bMenu:    i32,
        dwExStyle: u32,
        dpi:      u32
    ): i32

    @lib pub fn GetSystemMetricsForDpi(nIndex: i32, dpi: u32): i32

    // ── GUI thread info ──────────────────────────────────────────

    /// `GUITHREADINFO` — 48 bytes.  Set `cbSize` = 48.
    pub struct GUITHREADINFO {
        cbSize:        u32
        flags:         u32     // GUI_* state flags
        hwndActive:    HWND
        hwndFocus:     HWND
        hwndCapture:   HWND
        hwndMenuOwner: HWND
        hwndMoveSize:  HWND
        hwndCaret:     HWND
        rcCaret:       RECT
    }

    /// `idThread`: 0 = current thread.
    @lib pub fn GetGUIThreadInfo(idThread: u32, pgui: *GUITHREADINFO): i32

    // ── Window broadcasting ───────────────────────────────────────

    /// Broadcasts a message to system components.
    /// `dwFlags`: BSF_QUERY=1, BSF_IGNORECURRENTTASK=2, BSF_FLUSHDISK=4,
    /// BSF_NOHANG=8, BSF_POSTMESSAGE=0x10, BSF_FORCEIFHUNG=0x20,
    /// BSF_NOTIMEOUTIFNOTHUNG=0x40, BSF_ALLOWSFW=0x80, BSF_SENDNOTIFYMESSAGE=0x100.
    /// `lpInfo`: BSM_* recipient bitmask — pass 0 (BSM_ALLCOMPONENTS).
    @lib pub fn BroadcastSystemMessageA(
        dwFlags:    u32,
        lpInfo:     *u32,
        Msg:        u32,
        wParam:     u64,
        lParam:     i64
    ): i64

    // ── Device-change notifications ──────────────────────────────

    /// Registers `hRecipient` to receive WM_DEVICECHANGE messages
    /// for the device-interface class GUID in `NotificationFilter`.
    /// `Flags`: DEVICE_NOTIFY_WINDOW_HANDLE=0, DEVICE_NOTIFY_SERVICE_HANDLE=1,
    /// DEVICE_NOTIFY_ALL_INTERFACE_CLASSES=4.
    @lib pub fn RegisterDeviceNotificationA(
        hRecipient:        i64,
        NotificationFilter: *const void,    // *DEV_BROADCAST_DEVICEINTERFACE_A
        Flags:             u32
    ): *void

    @lib pub fn UnregisterDeviceNotification(Handle: *void): i32

    // ── Idle / waits ─────────────────────────────────────────────

    /// Waits until the calling thread has no input to process.
    /// Useful at startup so dependent code waits for the main window
    /// to finish its WM_CREATE handler.
    @lib pub fn WaitForInputIdle(hProcess: HANDLE, dwMilliseconds: u32): u32

    /// Waits for one of `pHandles` to signal OR for input on the
    /// calling thread's message queue.  `dwWakeMask`: QS_* flags.
    @lib pub fn MsgWaitForMultipleObjects(
        nCount:      u32,
        pHandles:    *const HANDLE,
        fWaitAll:    i32,
        dwMilliseconds: u32,
        dwWakeMask:  u32
    ): u32

    // ── Foreground / lock ────────────────────────────────────────

    /// Locks `lpfLockSetForegroundWindow`: 1=lock (LSFW_LOCK), 2=unlock (LSFW_UNLOCK).
    /// Prevents background apps from stealing focus.
    @lib pub fn LockSetForegroundWindow(uLockCode: u32): i32

    /// Allows another process to call SetForegroundWindow.
    /// `dwProcessId`: target pid, or 0xFFFFFFFF (ASFW_ANY).
    @lib pub fn AllowSetForegroundWindow(dwProcessId: u32): i32
}
