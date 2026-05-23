/// uiautomationcore.dll — UI Automation (UIA) accessibility +
/// test-automation surface.
///
/// Modern replacement for MSAA (oleacc). Used by:
///   * Screen readers (Narrator / NVDA / JAWS)
///   * End-to-end UI test frameworks (WinAppDriver / FlaUI)
///   * Custom controls that need to expose their own a11y tree
///
/// Client-side bring-up (consume UIA):
///   CoCreateInstance(&CLSID_CUIAutomation, NULL,
///                    CLSCTX_INPROC_SERVER, &IID_IUIAutomation,
///                    &auto)
///   auto.GetRootElement(&root)
///   root.FindAll(scope, condition, &found)
///
/// Provider-side bring-up (expose UIA from a custom control):
///   UiaReturnRawElementProvider(hwnd, wParam, lParam, &provider)
///   in WM_GETOBJECT handler.

// ─── UiaPropertyId (subset of well-known property GUIDs) ─────────
pub enum UiaPropertyId: i32 {
    runtimeId          = 30000
    boundingRectangle  = 30001
    processId          = 30002
    controlType        = 30003
    localizedControlType = 30004
    name               = 30005
    acceleratorKey     = 30006
    accessKey          = 30007
    hasKeyboardFocus   = 30008
    isKeyboardFocusable = 30009
    isEnabled          = 30010
    automationId       = 30011
    className          = 30012
    helpText           = 30013
    clickablePoint     = 30014
    culture            = 30015
    isControlElement   = 30016
    isContentElement   = 30017
    labeledBy          = 30018
    isPassword         = 30019
    nativeWindowHandle = 30020
    itemType           = 30021
    isOffscreen        = 30022
    orientation        = 30023
    frameworkId        = 30024
}

// ─── UiaControlTypeId ────────────────────────────────────────────
pub enum UiaControlType: i32 {
    button     = 50000
    calendar   = 50001
    checkBox   = 50002
    comboBox   = 50003
    edit       = 50004
    hyperlink  = 50005
    image      = 50006
    listItem   = 50007
    list       = 50008
    menu       = 50009
    menuBar    = 50010
    menuItem   = 50011
    progressBar = 50012
    radioButton = 50013
    scrollBar   = 50014
    slider      = 50015
    spinner     = 50016
    statusBar   = 50017
    tab         = 50018
    tabItem     = 50019
    text        = 50020
    toolBar     = 50021
    toolTip     = 50022
    tree        = 50023
    treeItem    = 50024
    custom      = 50025
    group       = 50026
    thumb       = 50027
    dataGrid    = 50028
    dataItem    = 50029
    document    = 50030
    splitButton = 50031
    window      = 50032
    pane        = 50033
    header      = 50034
    headerItem  = 50035
    table       = 50036
    titleBar    = 50037
    separator   = 50038
}

// ─── UiaTreeScope ────────────────────────────────────────────────
@flags
pub enum UiaTreeScope: i32 {
    element     = 0x1
    children    = 0x2
    descendants = 0x4
    parent      = 0x8
    ancestors   = 0x10
    subtree     = 0x7        // element | children | descendants
}

use windef { HWND }
@extern(C, "UIAutomationCore") {
    /// Provider-side return helper. Call from WM_GETOBJECT.
    /// Returns the LRESULT to pass back from your WndProc.
    @lib pub fn UiaReturnRawElementProvider(
        hwnd:     HWND,
        wParam:   u64,
        lParam:   i64,
        el:       *void          // *IRawElementProviderSimple
    ): i64

    /// Whether a client is connected — if no, skip producing the
    /// provider tree for performance.
    @lib pub fn UiaClientsAreListening(): i32

    /// Fire an event to all listening clients. `pProvider` is a
    /// `*IRawElementProviderSimple`.
    @lib pub fn UiaRaiseAutomationEvent(
        pProvider: *void,
        id:        i32                  // UIA_*_EventId
    ): i32

    @lib pub fn UiaRaisePropertyChangedEvent(
        pProvider: *void,
        id:        i32,                 // UiaPropertyId
        oldValue:  *const void,         // *VARIANT
        newValue:  *const void
    ): i32

    @lib pub fn UiaRaiseStructureChangedEvent(
        pProvider: *void,
        structureChangeType: i32,
        pRuntimeId: *const i32,
        cRuntimeIdLen: i32
    ): i32

    @lib pub fn UiaRaiseNotificationEvent(
        provider: *void,
        notificationKind: i32,
        notificationProcessing: i32,
        displayString: *const u16,      // BSTR
        activityId: *const u16
    ): i32

    /// Disconnect a provider so subsequent client calls return
    /// UIA_E_ELEMENTNOTAVAILABLE. Call this when your HWND is
    /// destroyed.
    @lib pub fn UiaDisconnectProvider(pProvider: *void): i32

    @lib pub fn UiaDisconnectAllProviders(): i32

    /// Element from a screen point — used by accessibility tools.
    @lib pub fn UiaNodeFromPoint(
        x:         f64,
        y:         f64,
        pRequest:  *const void,
        ppNode:    **void
    ): i32

    /// Element from an HWND.
    @lib pub fn UiaNodeFromHandle(
        hwnd:    HWND,
        ppNode:  **void
    ): i32

    /// Element from a focused window / control.
    @lib pub fn UiaNodeFromFocus(
        pRequest: *const void,
        ppNode:   **void
    ): i32

    /// `UiaHostProviderFromHwnd(hwnd, &provider)` — system-supplied
    /// fragment root for a vanilla HWND.
    @lib pub fn UiaHostProviderFromHwnd(
        hwnd:        HWND,
        ppProvider:  **void
    ): i32

    @lib pub fn UiaGetReservedNotSupportedValue(ppUnkNotSupportedValue: **void): i32

    @lib pub fn UiaGetReservedMixedAttributeValue(ppUnkMixedAttributeValue: **void): i32

    /// Find a `propertyId` -> friendly programmatic name string.
    @lib pub fn UiaLookupId(type_: i32, pGuid: *const void): i32
}
