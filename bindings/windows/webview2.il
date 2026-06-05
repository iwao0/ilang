/// Microsoft Edge WebView2 — COM API bindings.
///
/// Only the surface used by `libs/gui/win32/webview.il` is bound:
///   - The runtime entry point `CreateWebViewEnvironmentWithOptionsInternal`
///     exported from EmbeddedBrowserWebView.dll (not the SDK's
///     `WebView2Loader.dll`, which we deliberately do not depend on).
///   - The COM interfaces of objects we consume — environment,
///     controller, webview, and the args object passed to navigation
///     event handlers.
///
/// COM handler interfaces that we IMPLEMENT (env-completed /
/// controller-completed / nav-starting / nav-completed) are not
/// declared here — they are hand-built as raw fn-pointer vtables
/// in the gui-win32 backend.
///
/// Slot numbering on every interface matches the order in
/// WebView2.h verbatim, with placeholder `_slotN()` entries for
/// methods we do not call. Re-ordering or omitting slots silently
/// corrupts the vtable dispatch — the slot numbers come from
/// declaration order plus the inherited `IUnknown` base (slots 0-2).

use windef { RECT }
use ole32 { IUnknown }

@extern(C) {

    // ─── ICoreWebView2Environment ───────────────────────────────
    /// Returned by `CreateWebViewEnvironmentWithOptionsInternal`
    /// once the env-completed handler fires. Owns the browser
    /// process lifetime; create one per user-data-folder.
    @com pub interface ICoreWebView2Environment : IUnknown {
        CreateCoreWebView2Controller(
            parentWindow: i64,            // HWND
            handler:      *void            // completed handler vtable*
        ): i32
        _slot4(): i32                      // CreateWebResourceResponse
        _slot5(): i32                      // get_BrowserVersionString
        _slot6(): i32                      // add_NewBrowserVersionAvailable
        _slot7(): i32                      // remove_NewBrowserVersionAvailable
    }

    // ─── ICoreWebView2Controller ────────────────────────────────
    /// The HWND-hosted side of a webview — handles visibility,
    /// bounds, focus, and the `Close()` that tears down the
    /// browser-side state. Returned via the controller-completed
    /// handler that env.CreateCoreWebView2Controller dispatches.
    @com pub interface ICoreWebView2Controller : IUnknown {
        get_IsVisible(value: *i32): i32                 // 3
        put_IsVisible(value: i32): i32                  // 4
        get_Bounds(value: *RECT): i32                   // 5
        /// `value` is the new client-area rect, in pixels relative
        /// to the parent HWND. Passed by pointer per the Microsoft
        /// x64 ABI (struct > 8 bytes → hidden pointer).
        put_Bounds(value: *const RECT): i32             // 6
        get_ZoomFactor(value: *f64): i32                // 7
        put_ZoomFactor(value: f64): i32                 // 8
        _slot9():  i32                                  // add_ZoomFactorChanged
        _slot10(): i32                                  // remove_ZoomFactorChanged
        _slot11(): i32                                  // SetBoundsAndZoomFactor
        _slot12(): i32                                  // MoveFocus
        _slot13(): i32                                  // add_MoveFocusRequested
        _slot14(): i32                                  // remove_MoveFocusRequested
        _slot15(): i32                                  // add_GotFocus
        _slot16(): i32                                  // remove_GotFocus
        _slot17(): i32                                  // add_LostFocus
        _slot18(): i32                                  // remove_LostFocus
        _slot19(): i32                                  // add_AcceleratorKeyPressed
        _slot20(): i32                                  // remove_AcceleratorKeyPressed
        _slot21(): i32                                  // get_ParentWindow
        _slot22(): i32                                  // put_ParentWindow
        _slot23(): i32                                  // NotifyParentWindowPositionChanged
        Close(): i32                                    // 24
        get_CoreWebView2(value: **void): i32            // 25
    }

    // ─── ICoreWebView2 ──────────────────────────────────────────
    /// The browser-side webview itself. Retrieved by calling
    /// `controller.get_CoreWebView2(&out)`. Slot ordering follows
    /// the original (pre-extension) `ICoreWebView2` from WebView2.h;
    /// later interfaces (`ICoreWebView2_2`, `_3`, ...) extend rather
    /// than reorder, so this layout is stable.
    @com pub interface ICoreWebView2 : IUnknown {
        _slot3():  i32                                  // get_Settings
        get_Source(value: **u16): i32                   // 4
        Navigate(uri: *const u16): i32                  // 5
        _slot6():  i32                                  // NavigateToString
        add_NavigationStarting(
            handler: *void,                              // 7
            token:   *i64                                // EventRegistrationToken
        ): i32
        remove_NavigationStarting(token: i64): i32      // 8
        _slot9():  i32                                  // add_ContentLoading
        _slot10(): i32                                  // remove_ContentLoading
        _slot11(): i32                                  // add_SourceChanged
        _slot12(): i32                                  // remove_SourceChanged
        _slot13(): i32                                  // add_HistoryChanged
        _slot14(): i32                                  // remove_HistoryChanged
        add_NavigationCompleted(
            handler: *void,                              // 15
            token:   *i64
        ): i32
        remove_NavigationCompleted(token: i64): i32     // 16
        _slot17(): i32                                  // add_FrameNavigationStarting
        _slot18(): i32                                  // remove_FrameNavigationStarting
        _slot19(): i32                                  // add_FrameNavigationCompleted
        _slot20(): i32                                  // remove_FrameNavigationCompleted
        _slot21(): i32                                  // add_ScriptDialogOpening
        _slot22(): i32                                  // remove_ScriptDialogOpening
        _slot23(): i32                                  // add_PermissionRequested
        _slot24(): i32                                  // remove_PermissionRequested
        _slot25(): i32                                  // add_ProcessFailed
        _slot26(): i32                                  // remove_ProcessFailed
        _slot27(): i32                                  // AddScriptToExecuteOnDocumentCreated
        _slot28(): i32                                  // RemoveScriptToExecuteOnDocumentCreated
        _slot29(): i32                                  // ExecuteScript
        _slot30(): i32                                  // CapturePreview
        Reload(): i32                                   // 31
        _slot32(): i32                                  // PostWebMessageAsJson
        _slot33(): i32                                  // PostWebMessageAsString
        _slot34(): i32                                  // add_WebMessageReceived
        _slot35(): i32                                  // remove_WebMessageReceived
        _slot36(): i32                                  // CallDevToolsProtocolMethod
        _slot37(): i32                                  // get_BrowserProcessId
        get_CanGoBack(value: *i32): i32                 // 38
        get_CanGoForward(value: *i32): i32              // 39
        GoBack(): i32                                   // 40
        GoForward(): i32                                // 41
    }

    // ─── ICoreWebView2NavigationStartingEventArgs ───────────────
    /// Passed to `Invoke` on a NavigationStarting handler. The
    /// URI is returned as a wide string allocated with
    /// `CoTaskMemAlloc`; the caller must `CoTaskMemFree` it.
    @com pub interface ICoreWebView2NavigationStartingEventArgs : IUnknown {
        get_Uri(value: **u16): i32                      // 3
        _slot4(): i32                                   // get_IsUserInitiated
        _slot5(): i32                                   // get_IsRedirected
        _slot6(): i32                                   // get_RequestHeaders
        _slot7(): i32                                   // get_Cancel
        _slot8(): i32                                   // put_Cancel
        _slot9(): i32                                   // get_NavigationId
    }

    // ─── ICoreWebView2NavigationCompletedEventArgs ──────────────
    @com pub interface ICoreWebView2NavigationCompletedEventArgs : IUnknown {
        get_IsSuccess(value: *i32): i32                 // 3
        /// `value` receives a `COREWEBVIEW2_WEB_ERROR_STATUS` enum.
        get_WebErrorStatus(value: *i32): i32            // 4
        _slot5(): i32                                   // get_NavigationId
    }
}
