/// oleacc.dll — MSAA (Microsoft Active Accessibility).
///
/// Predecessor to UI Automation (uiautomationcore). Still widely
/// used — every HWND control has an MSAA `IAccessible` interface
/// fallback even when no UIA provider is registered.
///
/// Common pattern:
///   let acc: *void = 0 as *void
///   AccessibleObjectFromWindow(hwnd, OBJID_CLIENT,
///                              &IID_IAccessible, &acc)
///   // walk children, query properties via IAccessible vtable

// ─── Well-known OBJID_* values for AccessibleObjectFromWindow ────
pub enum AccObjId: i32 {
    window        = 0
    sysmenu       = 0xFFFFFFFF
    titlebar      = 0xFFFFFFFE
    menu          = 0xFFFFFFFD
    client        = 0xFFFFFFFC
    vscroll       = 0xFFFFFFFB
    hscroll       = 0xFFFFFFFA
    sizegrip      = 0xFFFFFFF9
    caret         = 0xFFFFFFF8
    cursor        = 0xFFFFFFF7
    alert         = 0xFFFFFFF6
    sound         = 0xFFFFFFF5
    queryClassNameIdx = 0xFFFFFFF4
    nativeObj     = 0xFFFFFFF0
}

// ─── Role IDs (ROLE_SYSTEM_*) ────────────────────────────────────
pub enum AccRole: u32 {
    titlebar       = 0x1
    menubar        = 0x2
    scrollbar      = 0x3
    grip           = 0x4
    sound          = 0x5
    cursor         = 0x6
    caret          = 0x7
    alert          = 0x8
    window         = 0x9
    client         = 0xA
    menuPopup      = 0xB
    menuItem       = 0xC
    tooltip        = 0xD
    application    = 0xE
    document       = 0xF
    pane           = 0x10
    chart          = 0x11
    dialog         = 0x12
    border         = 0x13
    grouping       = 0x14
    separator      = 0x15
    toolbar        = 0x16
    statusbar      = 0x17
    table          = 0x18
    columnHeader   = 0x19
    rowHeader      = 0x1A
    column         = 0x1B
    row            = 0x1C
    cell           = 0x1D
    link           = 0x1E
    helpBalloon    = 0x1F
    character      = 0x20
    list           = 0x21
    listItem       = 0x22
    outline        = 0x23
    outlineItem    = 0x24
    pageTab        = 0x25
    propertyPage   = 0x26
    indicator      = 0x27
    graphic        = 0x28
    statictext     = 0x29
    text           = 0x2A
    pushButton     = 0x2B
    checkButton    = 0x2C
    radioButton    = 0x2D
    comboBox       = 0x2E
    dropList       = 0x2F
    progressBar    = 0x30
    dial           = 0x31
    hotkeyField    = 0x32
    slider         = 0x33
    spinButton     = 0x34
    diagram        = 0x35
    animation      = 0x36
    equation       = 0x37
    buttonDropdown = 0x38
    buttonMenu     = 0x39
    buttonDropdownGrid = 0x3A
    whitespace     = 0x3B
    pageTabList    = 0x3C
    clock          = 0x3D
    splitButton    = 0x3E
    ipAddress      = 0x3F
    outlineButton  = 0x40
}

// ─── State flags (STATE_SYSTEM_*) ────────────────────────────────
@flags
pub enum AccState: u32 {
    unavailable    = 0x00000001
    selected       = 0x00000002
    focused        = 0x00000004
    pressed        = 0x00000008
    checked        = 0x00000010
    mixed          = 0x00000020
    readOnly       = 0x00000040
    hotTracked     = 0x00000080
    default_       = 0x00000100
    expanded       = 0x00000200
    collapsed      = 0x00000400
    busy           = 0x00000800
    floating       = 0x00001000
    marquee        = 0x00002000
    animated       = 0x00004000
    invisible      = 0x00008000
    offscreen      = 0x00010000
    sizeable       = 0x00020000
    moveable       = 0x00040000
    selfVoicing    = 0x00080000
    focusable      = 0x00100000
    selectable     = 0x00200000
    linked         = 0x00400000
    traversed      = 0x00800000
    multiSelectable = 0x01000000
    extSelectable  = 0x02000000
    alertLow       = 0x04000000
    alertMedium    = 0x08000000
    alertHigh      = 0x10000000
    protected      = 0x20000000
    hasPopup       = 0x40000000
}

use user32 { HWND }

@extern(C, "oleacc") {
    /// Get an `IAccessible` for `hwnd`. `dwObjectID` is `AccObjId.*`.
    /// `riid` is `IID_IAccessible` or `IID_IDispatch`. Returns S_OK.
    @lib pub fn AccessibleObjectFromWindow(
        hwnd:        HWND,
        dwObjectID:  u32,
        riid:        *const void,        // *GUID
        ppvObject:   **void
    ): i32

    /// `IAccessible` for the object at the screen point.
    @lib pub fn AccessibleObjectFromPoint(
        ptScreenX:   i32,
        ptScreenY:   i32,
        ppacc:       **void,
        pvarChild:   *void               // *VARIANT
    ): i32

    /// `IAccessible` from an `IAccessible.accNavigate` event payload.
    @lib pub fn AccessibleObjectFromEvent(
        hwnd:        HWND,
        dwObjectID:  u32,
        dwChildID:   u32,
        ppacc:       **void,
        pvarChild:   *void
    ): i32

    /// Enumerate children. `rgVarChildren` holds at least
    /// `cMaxChildren` `VARIANT` slots; `*pcObtained` is filled on
    /// return with the actual count.
    @lib pub fn AccessibleChildren(
        paccContainer: *void,
        iChildStart:   i64,
        cMaxChildren:  i64,
        rgVarChildren: *void,             // *VARIANT[]
        pcObtained:    *i32
    ): i32

    /// `GetRoleTextA(role, buf, cch)` — friendly role string.
    /// Returns the actual length; 0 if `role` is invalid.
    @lib pub fn GetRoleTextA(dwRole: u32, lpszRole: *char, cchRoleMax: u32): u32
    @lib pub fn GetRoleTextW(dwRole: u32, lpszRole: *u16, cchRoleMax: u32): u32

    @lib pub fn GetStateTextA(dwStateBit: u32, lpszStateBit: *char, cchStateBitMax: u32): u32
    @lib pub fn GetStateTextW(dwStateBit: u32, lpszStateBit: *u16, cchStateBitMax: u32): u32

    /// Resolve the HWND that owns `pacc`.
    @lib pub fn WindowFromAccessibleObject(
        pacc: *void,
        phwnd: *i64
    ): i32

    /// Helper for legacy controls — wraps `OBJID_NATIVEOM` for
    /// objects that store private data outside MSAA.
    @lib pub fn CreateStdAccessibleObject(
        hwnd: HWND,
        idObject: i32,
        riid: *const void,
        ppvObject: **void
    ): i32

    @lib pub fn LresultFromObject(
        riid:    *const void,
        wParam:  u64,
        punk:    *void
    ): i64

    @lib pub fn ObjectFromLresult(
        lResult: i64,
        riid:    *const void,
        wParam:  u64,
        ppvObject: **void
    ): i32

    /// `NotifyWinEvent(event, hwnd, idObject, idChild)` —
    /// also exported by user32, mirrored here for completeness.
    @lib pub fn NotifyWinEvent(
        event:    u32,
        hwnd:     HWND,
        idObject: i32,
        idChild:  i32
    )
}
