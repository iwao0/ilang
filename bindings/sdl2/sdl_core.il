// SDL2 core: subsystem init / quit, error reporting, hints, log,
// version, platform. Mirrors the headers SDL_init.h, SDL_error.h,
// SDL_hints.h, SDL_log.h, SDL_version.h, SDL_platform.h.
//
// Variadic SDL functions (SDL_SetError, the SDL_Log* family) are
// wrapped with single-string entry points — pass a pre-built
// message; printf-style formatting happens in ilang code.

// ─── SDL_INIT_* flags ─────────────────────────────────────────────

/// Subsystem bitflags passed to `SDL_Init` / `init` /
/// `initSubSystem` / `wasInit`.
@flags
pub enum InitFlag: u32 {
    timer          = 0x00000001
    audio          = 0x00000010
    video          = 0x00000020   // implies events
    joystick       = 0x00000200   // implies events
    haptic         = 0x00001000
    gameController = 0x00002000   // implies joystick
    events         = 0x00004000
    sensor         = 0x00008000
    noParachute    = 0x00100000   // legacy / no-op
    /// Bitwise-OR of all the practical subsystems above.
    everything     = 0x0000F231
}

// ─── SDL_LogPriority ──────────────────────────────────────────────

/// Severity passed to the `Log*` family.
pub enum LogPriority: i32 {
    verbose  = 1
    debug    = 2
    info     = 3
    warn     = 4
    error    = 5
    critical = 6
}

// ─── SDL_LogCategory ──────────────────────────────────────────────

/// Built-in log categories. User-defined categories start at
/// `custom` (= 19, matching `SDL_LOG_CATEGORY_CUSTOM`).
pub enum LogCategory: i32 {
    application = 0
    error       = 1
    assert      = 2
    system      = 3
    audio       = 4
    video       = 5
    render      = 6
    input       = 7
    test        = 8
}

// ─── SDL_HintPriority ─────────────────────────────────────────────

/// Hint priority passed to `setHintWithPriority`.
pub enum HintPriority: i32 {
    default  = 0
    normal   = 1
    override = 2
}

// ─── SDL_PowerState (SDL_power.h) ─────────────────────────────────

/// Battery / power-source state returned by `getPowerInfo`.
pub enum PowerState: i32 {
    unknown    = 0   // can't determine
    onBattery  = 1   // plugged out, running on battery
    noBattery  = 2   // plugged in, no battery present
    charging   = 3   // plugged in, battery charging
    charged    = 4   // plugged in, battery full
}

// ─── SDL_HINT_* names ────────────────────────────────────────────
// String-repr enum mirroring the C `SDL_HINT_*` macros. Pass a
// `Hint.x` to `setHint(...)` / `setHintWithPriority(...)` /
// `resetHint(...)` via `Hint.x as string`. The list below covers
// audio, video/render, input, mouse, gamepad, window management,
// and miscellaneous behaviour controls — the hints most apps
// reach for. Anything not enumerated here can still be set with
// `setHint("SDL_THE_NAME", value)`.
pub enum Hint: string {
    // Audio
    audioResamplingMode               = "SDL_AUDIO_RESAMPLING_MODE"
    audioCategory                     = "SDL_AUDIO_CATEGORY"
    audioDeviceAppName                = "SDL_AUDIO_DEVICE_APP_NAME"
    audioDeviceStreamName             = "SDL_AUDIO_DEVICE_STREAM_NAME"

    // Video / Render
    framebufferAcceleration           = "SDL_FRAMEBUFFER_ACCELERATION"
    renderDriver                      = "SDL_RENDER_DRIVER"
    renderOpenGLShaders               = "SDL_RENDER_OPENGL_SHADERS"
    renderDirect3DThreadsafe          = "SDL_RENDER_DIRECT3D_THREADSAFE"
    renderDirect3D11Debug             = "SDL_RENDER_DIRECT3D11_DEBUG"
    renderLogicalSizeMode             = "SDL_RENDER_LOGICAL_SIZE_MODE"
    renderScaleQuality                = "SDL_RENDER_SCALE_QUALITY"
    renderVsync                       = "SDL_RENDER_VSYNC"
    renderBatching                    = "SDL_RENDER_BATCHING"
    videoAllowScreensaver             = "SDL_VIDEO_ALLOW_SCREENSAVER"
    videoHighDPIDisabled              = "SDL_VIDEO_HIGHDPI_DISABLED"
    videoMacFullscreenSpaces          = "SDL_VIDEO_MAC_FULLSCREEN_SPACES"
    videoWinD3DCompiler               = "SDL_VIDEO_WIN_D3DCOMPILER"
    videoX11NetWMPing                 = "SDL_VIDEO_X11_NET_WM_PING"
    videoX11NetWMBypassCompositor     = "SDL_VIDEO_X11_NET_WM_BYPASS_COMPOSITOR"
    videoX11Xinerama                  = "SDL_VIDEO_X11_XINERAMA"
    videoX11Xrandr                    = "SDL_VIDEO_X11_XRANDR"
    videoX11Xvidmode                  = "SDL_VIDEO_X11_XVIDMODE"
    openGLESDriver                    = "SDL_OPENGL_ES_DRIVER"

    // Window
    windowsDisableThreadNaming        = "SDL_WINDOWS_DISABLE_THREAD_NAMING"
    windowsEnableMessageloop          = "SDL_WINDOWS_ENABLE_MESSAGELOOP"
    windowFrameUsableWhileCursorHidden = "SDL_WINDOW_FRAME_USABLE_WHILE_CURSOR_HIDDEN"

    // Mouse / touch
    mouseNormalSpeedScale             = "SDL_MOUSE_NORMAL_SPEED_SCALE"
    mouseRelativeSpeedScale           = "SDL_MOUSE_RELATIVE_SPEED_SCALE"
    mouseRelativeModeWarp             = "SDL_MOUSE_RELATIVE_MODE_WARP"
    mouseFocusClickthrough            = "SDL_MOUSE_FOCUS_CLICKTHROUGH"
    touchMouseEvents                  = "SDL_TOUCH_MOUSE_EVENTS"
    mouseTouchEvents                  = "SDL_MOUSE_TOUCH_EVENTS"
    mouseDoubleClickTime              = "SDL_MOUSE_DOUBLE_CLICK_TIME"
    mouseDoubleClickRadius            = "SDL_MOUSE_DOUBLE_CLICK_RADIUS"

    // Keyboard
    grabKeyboard                      = "SDL_GRAB_KEYBOARD"
    keyboardElement                   = "SDL_KEYBOARD_ELEMENT"
    noSignalHandlers                  = "SDL_NO_SIGNAL_HANDLERS"

    // Joystick / gamepad
    joystickAllowBackgroundEvents     = "SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS"
    gameControllerConfig              = "SDL_GAMECONTROLLERCONFIG"
    gameControllerConfigFile          = "SDL_GAMECONTROLLERCONFIG_FILE"
    gameControllerUseButtonLabels     = "SDL_GAMECONTROLLER_USE_BUTTON_LABELS"
    gameControllerIgnoreDevices       = "SDL_GAMECONTROLLER_IGNORE_DEVICES"
    gameControllerIgnoreDevicesExcept = "SDL_GAMECONTROLLER_IGNORE_DEVICES_EXCEPT"

    // Apple / iOS / accelerometer
    accelerometerAsJoystick           = "SDL_ACCELEROMETER_AS_JOYSTICK"
    appleTVControllerUIEvents         = "SDL_APPLE_TV_CONTROLLER_UI_EVENTS"
    appleTVRemoteAllowRotation        = "SDL_APPLE_TV_REMOTE_ALLOW_ROTATION"
    iosIdleTimerDisabled              = "SDL_IOS_IDLE_TIMER_DISABLED"
    iosOrientations                   = "SDL_IOS_ORIENTATIONS"

    // Android
    androidApkExpansionMainFileVersion  = "SDL_ANDROID_APK_EXPANSION_MAIN_FILE_VERSION"
    androidApkExpansionPatchFileVersion = "SDL_ANDROID_APK_EXPANSION_PATCH_FILE_VERSION"
    androidTrapBackButton             = "SDL_ANDROID_TRAP_BACK_BUTTON"

    // XInput / Wii / RPI
    xinputEnabled                     = "SDL_XINPUT_ENABLED"
    xinputUseOldJoystickMapping       = "SDL_XINPUT_USE_OLD_JOYSTICK_MAPPING"
    rpiVideoLayer                     = "SDL_RPI_VIDEO_LAYER"

    // Threads / behaviour
    threadStackSize                   = "SDL_THREAD_STACK_SIZE"
    threadPriorityPolicy              = "SDL_THREAD_PRIORITY_POLICY"
    timerResolution                   = "SDL_TIMER_RESOLUTION"
    bmpSaveLegacyFormat               = "SDL_BMP_SAVE_LEGACY_FORMAT"
}

@extern(C, "SDL2") {
    /// SDL library version triple. Mirrors C `SDL_version` (3 u8
    /// bytes, 1 byte tail padding). Filled in by `getVersion()`.
    pub struct Version {
        major: u8
        minor: u8
        patch: u8
    }

    /// 16 B rectangle, passed by pointer to most C routines.
    /// Lives here so every module (renderer / window / keyboard /
    /// mouse / …) can share `sdl.Rect` instead of declaring
    /// per-module duplicates. Mirrors C `SDL_Rect`.
    pub struct Rect {
        /// Top-left x in pixels.
        x: i32
        /// Top-left y in pixels.
        y: i32
        /// Width in pixels.
        w: i32
        /// Height in pixels.
        h: i32
    }

    /// 2D integer point. Shared geometry type used by `Window`
    /// (`position`) and the renderer's draw-points API. Mirrors C
    /// `SDL_Point`.
    pub struct Point {
        x: i32
        y: i32
    }

    /// 2D float point. Shared by the renderer's float-coordinate
    /// API (`SDL_FPoint`) and convenience getters that return a
    /// pair of floats (`getScale`, `windowToLogical`).
    pub struct FPoint {
        x: f32
        y: f32
    }

    /// `(w, h)` integer size. Shared by `Window.size` /
    /// `Renderer.outputSize` / `Renderer.getLogicalSize` and other
    /// `(w, h)` returning APIs.
    pub struct Size {
        w: i32
        h: i32
    }

    /// RGBA8 colour. Mirrors C `SDL_Color`. Lives here (rather
    /// than per-feature) so renderer / surface / shape / ttf /
    /// etc. can all share `sdl.Color` without circular imports.
    pub struct Color {
        r: u8
        g: u8
        b: u8
        a: u8
    }

    // ─── SDL_init.h ──────────────────────────────────────────────
    @lib pub fn SDL_Init(flags: u32): i32
    @lib pub fn SDL_InitSubSystem(flags: u32): i32
    @lib pub fn SDL_QuitSubSystem(flags: u32)
    @lib pub fn SDL_WasInit(flags: u32): u32
    @lib pub fn SDL_Quit()

    // ─── SDL_error.h ─────────────────────────────────────────────
    @lib pub fn SDL_GetError(): *const char
    @lib pub fn SDL_ClearError()
    // SDL_SetError is C-variadic; accepting a single pre-formatted
    // string covers the common case (no format specifiers in the
    // payload).
    @lib pub fn SDL_SetError(fmt: *const char): i32

    // ─── SDL_hints.h ─────────────────────────────────────────────
    @lib pub fn SDL_SetHint(name: *const char, value: *const char): i32
    @lib pub fn SDL_SetHintWithPriority(
        name: *const char, value: *const char, priority: i32
    ): i32
    @lib pub fn SDL_GetHint(name: *const char): *const char
    @lib pub fn SDL_GetHintBoolean(
        name: *const char, default_value: i32
    ): i32
    @lib pub fn SDL_ResetHint(name: *const char): i32
    @lib pub fn SDL_ResetHints()
    @lib pub fn SDL_ClearHints()

    // ─── SDL_log.h (non-variadic subset) ─────────────────────────
    @lib pub fn SDL_LogSetAllPriority(priority: i32)
    @lib pub fn SDL_LogSetPriority(category: i32, priority: i32)
    @lib pub fn SDL_LogGetPriority(category: i32): i32
    @lib pub fn SDL_LogResetPriorities()
    @lib pub fn SDL_LogMessage(
        category: i32, priority: i32, fmt: *const char
    )

    // ─── SDL_version.h ───────────────────────────────────────────
    @lib pub fn SDL_GetVersion(ver: *Version)
    @lib pub fn SDL_GetRevision(): *const char

    // ─── SDL_platform.h ──────────────────────────────────────────
    @lib pub fn SDL_GetPlatform(): *const char

    // ─── SDL_stdinc.h (allocator the C side hands ownership of) ─
    /// Some SDL APIs return a `char *` / `void *` SDL malloc'd and
    /// expects the caller to free. We mostly hide this in wrappers
    /// (copy via `stringFromCstr`, then `SDL_free`), but expose the
    /// raw fn for callers that need it directly.
    @lib pub fn SDL_free(mem: *const void)

    // ─── SDL_clipboard.h ─────────────────────────────────────────
    @lib pub fn SDL_HasClipboardText(): u8
    @lib pub fn SDL_GetClipboardText(): *const char
    @lib pub fn SDL_SetClipboardText(text: *const char): i32

    // ─── SDL_cpuinfo.h (subset) ──────────────────────────────────
    @lib pub fn SDL_GetCPUCount(): i32
    @lib pub fn SDL_GetSystemRAM(): i32
    @lib pub fn SDL_GetCPUCacheLineSize(): i32

    // ─── SDL_rect.h utilities ────────────────────────────────────
    @lib pub fn SDL_HasIntersection(
        a: *const Rect, b: *const Rect
    ): i32
    @lib pub fn SDL_IntersectRect(
        a: *const Rect, b: *const Rect, result: *Rect
    ): i32
    @lib pub fn SDL_UnionRect(
        a: *const Rect, b: *const Rect, result: *Rect
    )
    // SDL_RectEquals and SDL_PointInRect are `static SDL_FORCE_INLINE`
    // in SDL_rect.h with no real exported symbol — we reimplement
    // them in ilang below as `rectsEqual` / `pointInRect`.
    @lib pub fn SDL_EnclosePoints(
        points: *const Point, count: i32,
        clip: *const Rect, result: *Rect
    ): i32

    // ─── SDL_power.h ─────────────────────────────────────────────
    @lib pub fn SDL_GetPowerInfo(secs: *i32, pct: *i32): i32

    // ─── SDL_misc.h ──────────────────────────────────────────────
    @lib pub fn SDL_OpenURL(url: *const char): i32

    // ─── SDL_locale.h ────────────────────────────────────────────
    /// SDL_Locale entry. SDL hands back a malloc'd null-terminated
    /// array of these (caller frees with `SDL_free`). `language`
    /// is always non-null; `country` may be null.
    pub struct SDL_Locale {
        language: i64   // *const char
        country:  i64   // *const char (may be 0)
    }
    @lib pub fn SDL_GetPreferredLocales(): *SDL_Locale

    // ─── friendlier ilang wrappers ───────────────────────────────

    /// Initialise the named subsystems. Must be called once before
    /// any other SDL routine in those subsystems.
    ///
    /// Args:
    ///   flags — bitwise-OR of `InitFlag.*` (e.g.
    ///           `InitFlag.video | InitFlag.audio`).
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure (call `getError()`).
    pub fn init(flags: InitFlag): i32 {
        SDL_Init(flags as u32)
    }

    /// Initialise additional subsystems after the first `init` call.
    ///
    /// Args:
    ///   flags — subsystems to add.
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure.
    pub fn initSubSystem(flags: InitFlag): i32 {
        SDL_InitSubSystem(flags as u32)
    }

    /// Tear down a subset of subsystems while leaving the rest
    /// running.
    pub fn quitSubSystem(flags: InitFlag) {
        SDL_QuitSubSystem(flags as u32)
    }

    /// Args:
    ///   flags — mask to filter (`InitFlag.everything` for "all").
    ///
    /// Returns:
    ///   bitmask of subsystems currently initialised among `flags`.
    pub fn wasInit(flags: InitFlag): u32 {
        SDL_WasInit(flags as u32)
    }

    /// Tear down every initialised subsystem and free SDL's global
    /// state. Call exactly once on program exit.
    pub fn quit() {
        SDL_Quit()
    }

    /// Returns:
    ///   the latest SDL error message (or empty string when no
    ///   error is recorded for this thread).
    pub fn getError(): string {
        stringFromCstr(SDL_GetError())
    }

    /// Reset the per-thread error string to empty.
    pub fn clearError() {
        SDL_ClearError()
    }

    /// Set the per-thread error string.
    ///
    /// Args:
    ///   msg — message to record.
    ///
    /// Returns:
    ///   always `-1` (matches SDL's convention of "error path"
    ///   return values).
    pub fn setError(msg: string): i32 {
        SDL_SetError(cstrFromString(msg))
    }

    /// Set a hint at `Normal` priority — overrides defaults but
    /// not user-set values.
    ///
    /// Args:
    ///   name  — `"SDL_…"` hint name (use the `HINT_*` constants).
    ///   value — hint value as a string.
    ///
    /// Returns:
    ///   true when the hint was applied.
    pub fn setHint(name: string, value: string): bool {
        SDL_SetHint(cstrFromString(name), cstrFromString(value)) != 0
    }

    /// Variant of `setHint` with explicit priority.
    pub fn setHintWithPriority(name: string, value: string, priority: HintPriority): bool {
        SDL_SetHintWithPriority(
            cstrFromString(name), cstrFromString(value), priority as i32
        ) != 0
    }

    /// Args:
    ///   name — hint name.
    ///
    /// Returns:
    ///   the current value, or empty when the hint isn't set.
    pub fn getHint(name: string): string {
        stringFromCstr(SDL_GetHint(cstrFromString(name)))
    }

    /// Boolean view of a hint.
    ///
    /// Args:
    ///   name          — hint name.
    ///   defaultValue — fallback when the hint is unset / unparsed.
    pub fn getHintBoolean(name: string, defaultValue: bool): bool {
        let dv: i32 = if defaultValue { 1 } else { 0 }
        SDL_GetHintBoolean(cstrFromString(name), dv) != 0
    }

    /// Drop a single hint — restores the SDL default.
    pub fn resetHint(name: string): bool {
        SDL_ResetHint(cstrFromString(name)) != 0
    }

    /// Reset every hint to its SDL default.
    pub fn resetHints() {
        SDL_ResetHints()
    }

    /// Discard every hint set so far. Stronger than `resetHints` —
    /// also forgets that the hint was ever touched.
    pub fn clearHints() {
        SDL_ClearHints()
    }

    /// Set the global log priority filter.
    pub fn logSetAllPriority(priority: LogPriority) {
        SDL_LogSetAllPriority(priority as i32)
    }

    /// Set the priority filter for a single category.
    pub fn logSetPriority(category: LogCategory, priority: LogPriority) {
        SDL_LogSetPriority(category as i32, priority as i32)
    }

    /// Returns:
    ///   the priority threshold currently set for `category`.
    pub fn logGetPriority(category: LogCategory): LogPriority {
        SDL_LogGetPriority(category as i32) as LogPriority
    }

    /// Restore the per-category priority defaults.
    pub fn logResetPriorities() {
        SDL_LogResetPriorities()
    }

    /// Emit a single log message through SDL's logger.
    pub fn logMessage(category: LogCategory, priority: LogPriority, msg: string) {
        SDL_LogMessage(category as i32, priority as i32, cstrFromString(msg))
    }

    /// Returns:
    ///   the SDL revision string (e.g. `"SDL2-2.30.1"`).
    pub fn getRevision(): string {
        stringFromCstr(SDL_GetRevision())
    }

    /// Returns:
    ///   the platform name (`"Mac OS X"`, `"Linux"`, `"Windows"`,
    ///   `"iOS"`, …).
    pub fn getPlatform(): string {
        stringFromCstr(SDL_GetPlatform())
    }

    /// Returns:
    ///   the linked SDL2 library's version (major.minor.patch).
    pub fn getVersion(): Version {
        let v = new Version()
        SDL_GetVersion(&v)
        v
    }

    /// Returns:
    ///   true when the OS clipboard contains text. Doesn't allocate
    ///   so it's cheap to poll.
    pub fn hasClipboardText(): bool {
        SDL_HasClipboardText() != 0 as u8
    }

    /// Read the OS clipboard as text.
    ///
    /// Returns:
    ///   clipboard contents, or empty string when the clipboard is
    ///   empty or holds non-text data.
    pub fn getClipboardText(): string {
        // SDL_GetClipboardText returns a pointer SDL owns; we copy
        // through stringFromCstr so the caller never frees it.
        stringFromCstr(SDL_GetClipboardText())
    }

    /// Write `text` to the OS clipboard.
    ///
    /// Args:
    ///   text — UTF-8 string to copy.
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure (call `getError()`).
    pub fn setClipboardText(text: string): i32 {
        SDL_SetClipboardText(cstrFromString(text))
    }

    /// Returns:
    ///   number of CPU cores SDL sees. Includes hyperthreaded
    ///   logical cores.
    pub fn getCPUCount(): i32 {
        SDL_GetCPUCount()
    }

    /// Returns:
    ///   physical RAM in MiB.
    pub fn getSystemRAM(): i32 {
        SDL_GetSystemRAM()
    }

    /// Returns:
    ///   L1 cache line size in bytes — useful for laying out hot
    ///   structs to avoid false sharing.
    pub fn getCPUCacheLineSize(): i32 {
        SDL_GetCPUCacheLineSize()
    }

    /// Test whether two rectangles overlap.
    pub fn rectsIntersect(a: Rect, b: Rect): bool {
        SDL_HasIntersection(&a, &b) != 0
    }

    /// Compute `a ∩ b`. The intersection is written into a fresh
    /// `Rect` returned by the helper.
    ///
    /// Returns:
    ///   `(rect, hadIntersection)` packed as a one-element array
    ///   plus a bool isn't ergonomic in ilang yet — instead the
    ///   helper returns the rect with `(w, h) == (0, 0)` when there
    ///   was no overlap.
    pub fn intersectRects(a: Rect, b: Rect): Rect {
        let r = new Rect()
        SDL_IntersectRect(&a, &b, &r)
        r
    }

    /// Compute the smallest rectangle that contains both `a` and
    /// `b`.
    pub fn unionRects(a: Rect, b: Rect): Rect {
        let r = new Rect()
        SDL_UnionRect(&a, &b, &r)
        r
    }

    pub fn rectsEqual(a: Rect, b: Rect): bool {
        a.x == b.x && a.y == b.y && a.w == b.w && a.h == b.h
    }

    /// True when `(p.x, p.y)` lies inside `r` (left/top inclusive,
    /// right/bottom exclusive — same convention as SDL).
    pub fn pointInRect(p: Point, r: Rect): bool {
        p.x >= r.x && p.x < (r.x + r.w)
            && p.y >= r.y && p.y < (r.y + r.h)
    }

    /// Compute the smallest rect enclosing `points`. When `clip`
    /// is non-null, only points falling inside `clip` are
    /// considered.
    ///
    /// Returns:
    ///   the enclosing rect, or `(0, 0, 0, 0)` when no points
    ///   matched.
    pub fn enclosePoints(points: Point[], clip: Rect): Rect {
        let r = new Rect()
        let p: *Point = points
        SDL_EnclosePoints(p, points.length as i32, &clip, &r)
        r
    }

    /// Battery / power-source snapshot.
    ///
    /// Returns:
    ///   `state` — one of `PowerState.*`.
    ///   Pass `seconds` and `percent` as 1-element arrays to
    ///   receive the remaining battery time (seconds, `-1` on
    ///   unknown / no battery) and charge percent (0..100, `-1`
    ///   when unknown).
    pub fn getPowerInfo(seconds: i32[], percent: i32[]): PowerState {
        let ps: *i32 = seconds
        let pc: *i32 = percent
        SDL_GetPowerInfo(ps, pc) as PowerState
    }

    /// Convenience: `getPowerInfo` that just returns the state.
    pub fn getPowerState(): PowerState {
        let s: i32 = 0
        let p: i32 = 0
        SDL_GetPowerInfo(&s, &p) as PowerState
    }

    /// Hand a URL to the OS — opens the user's default browser
    /// (or mailto / file handler, depending on the scheme).
    ///
    /// Returns:
    ///   `0` on success, `< 0` on failure (call `getError()`).
    pub fn openURL(url: string): i32 {
        SDL_OpenURL(cstrFromString(url))
    }

    /// Read the user's preferred UI locales, in priority order.
    ///
    /// Returns:
    ///   array of `"lang"` or `"lang_country"` strings (e.g.
    ///   `"en_US"`, `"ja"`). Empty when SDL can't determine the
    ///   locale.
    pub fn getPreferredLocales(): string[] {
        let head = SDL_GetPreferredLocales()
        let out: string[] = []
        if head as i64 == 0 { return out }
        // Walk the SDL_Locale array; the terminator has both
        // `language` and `country` set to NULL (i64 0).
        let i: i64 = 0
        let base: i64 = head as i64
        loop {
            let lang_ptr = readU64(base as *const void, i * 16) as i64
            let country_ptr = readU64(base as *const void, i * 16 + 8) as i64
            if lang_ptr == 0 && country_ptr == 0 { break }
            let lang = if lang_ptr != 0 {
                stringFromCstr(lang_ptr as *const char)
            } else { "" }
            let country = if country_ptr != 0 {
                stringFromCstr(country_ptr as *const char)
            } else { "" }
            let entry = if country != "" {
                lang + "_" + country
            } else {
                lang
            }
            out.push(entry)
            i = i + 1
        }
        SDL_free(head as *const void)
        out
    }
}
