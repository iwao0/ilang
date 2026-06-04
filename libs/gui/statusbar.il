/// Status bar — a horizontal strip docked along the bottom edge
/// of a `Window` and split into one or more text segments.
/// Install via `window.setStatusBar(statusBar)`; the backend
/// resizes / repositions it automatically as the window changes
/// shape, so the caller doesn't manage geometry.
///
///   let s = new gui.StatusBar(["Ready"])
///   window.setStatusBar(s)
///   ...
///   s.setText(0, "Saved 42 changes")
///
/// Backends:
///   - Win32 : `msctls_statusbar32` common control.
///   - macOS : NSTextField row inside a flipped NSView, since
///             AppKit has no native status bar.
///   - GTK 4 : horizontal GtkBox of GtkLabels.

use gui_impl {
    createNativeStatusBar, setStatusBarSegments, setStatusBarSegmentText,
}

pub class StatusBar {
    pub handle: i64
    pub _segments: string[]

    pub init() {
        this.handle    = -1
        let empty: string[] = []
        this._segments = empty
    }
    pub init(segments: string[]) {
        this.handle    = -1
        this._segments = segments
    }

    /// Current segment texts. Mutating the returned array does
    /// NOT update the bar — call `setSegments` for that.
    pub get segments(): string[] { this._segments }

    /// Replace every segment in one go. The new list's length
    /// becomes the bar's part count; if any part already had text
    /// it's overwritten by the corresponding entry.
    pub setSegments(segments: string[]) {
        this._segments = segments
        if this.handle >= 0 {
            setStatusBarSegments(this.handle, segments)
        }
    }

    /// Update one segment's text in place. Out-of-range indices
    /// are no-ops (mirrors the OS controls' tolerant behaviour
    /// rather than throwing).
    pub setText(index: i32, text: string) {
        let i = index as i64
        if i < 0 as i64 || i >= this._segments.length { return }
        this._segments[i] = text
        if this.handle >= 0 {
            setStatusBarSegmentText(this.handle, index, text)
        }
    }

    /// Realise the bar inside `window`. Called by
    /// `Window.setStatusBar`; user code shouldn't hit this
    /// directly. The handle returned by the native side becomes
    /// the StatusBar's permanent identity for later setters.
    pub attachTo(windowHandle: i64) {
        if this.handle >= 0 { return }
        this.handle = createNativeStatusBar(windowHandle, this._segments)
    }
}
