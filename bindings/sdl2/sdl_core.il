// SDL2 core: subsystem init / quit, error reporting, hints, log,
// version, platform. Mirrors the headers SDL_init.h, SDL_error.h,
// SDL_hints.h, SDL_log.h, SDL_version.h, SDL_platform.h.
//
// Variadic SDL functions (SDL_SetError, the SDL_Log* family) are
// wrapped with single-string entry points — pass a pre-built
// message; printf-style formatting happens in ilang code.

@extern(C) {
    // SDL_version.h: filled in by SDL_GetVersion. Layout matches
    // C `SDL_version` exactly (3 u8 bytes, 1 byte tail padding).
    struct SDL_Version {
        major: u8
        minor: u8
        patch: u8
    }

    // SDL_rect.h: 16 B rectangle, passed by pointer to most C
    // routines. Lives here so every module (renderer / window /
    // keyboard / mouse / …) can refer to the same `sdl.SDL_Rect`
    // and avoid per-module duplicate definitions.
    struct SDL_Rect {
        x: i32
        y: i32
        w: i32
        h: i32
    }

    // ─── SDL_init.h ──────────────────────────────────────────────
    @lib("SDL2") fn SDL_Init(flags: u32): i32
    @lib("SDL2") fn SDL_InitSubSystem(flags: u32): i32
    @lib("SDL2") fn SDL_QuitSubSystem(flags: u32)
    @lib("SDL2") fn SDL_WasInit(flags: u32): u32
    @lib("SDL2") fn SDL_Quit()

    // ─── SDL_error.h ─────────────────────────────────────────────
    @lib("SDL2") fn SDL_GetError(): *const char
    @lib("SDL2") fn SDL_ClearError()
    // SDL_SetError is C-variadic; accepting a single pre-formatted
    // string covers the common case (no format specifiers in the
    // payload).
    @lib("SDL2") fn SDL_SetError(fmt: *const char): i32

    // ─── SDL_hints.h ─────────────────────────────────────────────
    @lib("SDL2") fn SDL_SetHint(name: *const char, value: *const char): i32
    @lib("SDL2") fn SDL_SetHintWithPriority(
        name: *const char, value: *const char, priority: i32
    ): i32
    @lib("SDL2") fn SDL_GetHint(name: *const char): *const char
    @lib("SDL2") fn SDL_GetHintBoolean(
        name: *const char, default_value: i32
    ): i32
    @lib("SDL2") fn SDL_ResetHint(name: *const char): i32
    @lib("SDL2") fn SDL_ResetHints()
    @lib("SDL2") fn SDL_ClearHints()

    // ─── SDL_log.h (non-variadic subset) ─────────────────────────
    @lib("SDL2") fn SDL_LogSetAllPriority(priority: i32)
    @lib("SDL2") fn SDL_LogSetPriority(category: i32, priority: i32)
    @lib("SDL2") fn SDL_LogGetPriority(category: i32): i32
    @lib("SDL2") fn SDL_LogResetPriorities()
    @lib("SDL2") fn SDL_LogMessage(
        category: i32, priority: i32, fmt: *const char
    )

    // ─── SDL_version.h ───────────────────────────────────────────
    @lib("SDL2") fn SDL_GetVersion(ver: *SDL_Version)
    @lib("SDL2") fn SDL_GetRevision(): *const char

    // ─── SDL_platform.h ──────────────────────────────────────────
    @lib("SDL2") fn SDL_GetPlatform(): *const char

    // ─── friendlier ilang wrappers ───────────────────────────────

    // Returns 0 on success, < 0 on failure (matches SDL_Init).
    fn init(flags: InitFlag): i32 {
        SDL_Init(flags as u32)
    }
    fn initSubSystem(flags: InitFlag): i32 {
        SDL_InitSubSystem(flags as u32)
    }
    fn quitSubSystem(flags: InitFlag) {
        SDL_QuitSubSystem(flags as u32)
    }
    // Returns the bitmask of subsystems currently initialised. Pass
    // `InitFlag.everything` (or any combo) to filter.
    fn wasInit(flags: InitFlag): u32 {
        SDL_WasInit(flags as u32)
    }
    fn quit() {
        SDL_Quit()
    }

    // Latest SDL error message (or empty string when none).
    fn getError(): string {
        stringFromCstr(SDL_GetError())
    }
    fn clearError() {
        SDL_ClearError()
    }
    fn setError(msg: string): i32 {
        SDL_SetError(cstrFromString(msg))
    }

    // SDL_bool returns are 0/1 in C; expose them as ilang bool.
    fn setHint(name: string, value: string): bool {
        SDL_SetHint(cstrFromString(name), cstrFromString(value)) != 0
    }
    fn setHintWithPriority(name: string, value: string, priority: HintPriority): bool {
        SDL_SetHintWithPriority(
            cstrFromString(name), cstrFromString(value), priority as i32
        ) != 0
    }
    fn getHint(name: string): string {
        stringFromCstr(SDL_GetHint(cstrFromString(name)))
    }
    fn getHintBoolean(name: string, default_value: bool): bool {
        let dv: i32 = if default_value { 1 } else { 0 }
        SDL_GetHintBoolean(cstrFromString(name), dv) != 0
    }
    fn resetHint(name: string): bool {
        SDL_ResetHint(cstrFromString(name)) != 0
    }
    fn resetHints() {
        SDL_ResetHints()
    }
    fn clearHints() {
        SDL_ClearHints()
    }

    fn logSetAllPriority(priority: LogPriority) {
        SDL_LogSetAllPriority(priority as i32)
    }
    fn logSetPriority(category: LogCategory, priority: LogPriority) {
        SDL_LogSetPriority(category as i32, priority as i32)
    }
    fn logGetPriority(category: LogCategory): i32 {
        SDL_LogGetPriority(category as i32)
    }
    fn logResetPriorities() {
        SDL_LogResetPriorities()
    }
    fn logMessage(category: LogCategory, priority: LogPriority, msg: string) {
        SDL_LogMessage(category as i32, priority as i32, cstrFromString(msg))
    }

    fn getRevision(): string {
        stringFromCstr(SDL_GetRevision())
    }

    fn getPlatform(): string {
        stringFromCstr(SDL_GetPlatform())
    }

    // Public version triple — plain data, no constructor. Filled
    // in by `getVersion()`.
    class Version {
        major: i32
        minor: i32
        patch: i32
    }

    fn getVersion(): Version {
        let raw = new SDL_Version()
        SDL_GetVersion(raw)
        let v = new Version()
        v.major = raw.major as i32
        v.minor = raw.minor as i32
        v.patch = raw.patch as i32
        v
    }
}

// ─── SDL_INIT_* flags (SDL_INIT_*) ────────────────────────────────

/// Subsystem bitflags passed to `SDL_Init` / `init` /
/// `initSubSystem` / `wasInit`.
@flags
enum InitFlag: u32 {
    timer          = 0x00000001
    audio          = 0x00000010
    video          = 0x00000020   // implies events
    joystick       = 0x00000200   // implies events
    haptic         = 0x00001000
    gameController = 0x00002000   // implies joystick
    events         = 0x00004000
    sensor         = 0x00008000
    noParachute    = 0x00100000   // legacy / no-op
    // Bitwise-OR of all the practical subsystems above.
    everything     = 0x0000F231
}

// ─── SDL_LogPriority ──────────────────────────────────────────────

/// Severity passed to the `Log*` family.
enum LogPriority: i32 {
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
enum LogCategory: i32 {
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

/// Hint priority passed to `setHintWithPriority`. `override_` is
/// renamed from `SDL_HINT_OVERRIDE` because `override` is reserved
/// in ilang.
enum HintPriority: i32 {
    default   = 0
    normal    = 1
    override_ = 2
}

// ─── SDL_HINT_* names ────────────────────────────────────────────
// String constants matching the C `SDL_HINT_*` macros. Pass these
// to `setHint` / `getHint`. The full list is large; the entries
// below cover audio, video/render, input, mouse, gamepad, window
// management, and miscellaneous behaviour controls — i.e. the
// hints most apps reach for. Anything not here can still be set
// with `setHint("SDL_THE_NAME", value)`.
//
// Audio
const HINT_AUDIO_RESAMPLING_MODE: string         = "SDL_AUDIO_RESAMPLING_MODE"
const HINT_AUDIO_CATEGORY: string                = "SDL_AUDIO_CATEGORY"
const HINT_AUDIO_DEVICE_APP_NAME: string         = "SDL_AUDIO_DEVICE_APP_NAME"
const HINT_AUDIO_DEVICE_STREAM_NAME: string      = "SDL_AUDIO_DEVICE_STREAM_NAME"

// Video / Render
const HINT_FRAMEBUFFER_ACCELERATION: string      = "SDL_FRAMEBUFFER_ACCELERATION"
const HINT_RENDER_DRIVER: string                 = "SDL_RENDER_DRIVER"
const HINT_RENDER_OPENGL_SHADERS: string         = "SDL_RENDER_OPENGL_SHADERS"
const HINT_RENDER_DIRECT3D_THREADSAFE: string    = "SDL_RENDER_DIRECT3D_THREADSAFE"
const HINT_RENDER_DIRECT3D11_DEBUG: string       = "SDL_RENDER_DIRECT3D11_DEBUG"
const HINT_RENDER_LOGICAL_SIZE_MODE: string      = "SDL_RENDER_LOGICAL_SIZE_MODE"
const HINT_RENDER_SCALE_QUALITY: string          = "SDL_RENDER_SCALE_QUALITY"
const HINT_RENDER_VSYNC: string                  = "SDL_RENDER_VSYNC"
const HINT_RENDER_BATCHING: string               = "SDL_RENDER_BATCHING"
const HINT_VIDEO_ALLOW_SCREENSAVER: string       = "SDL_VIDEO_ALLOW_SCREENSAVER"
const HINT_VIDEO_HIGHDPI_DISABLED: string        = "SDL_VIDEO_HIGHDPI_DISABLED"
const HINT_VIDEO_MAC_FULLSCREEN_SPACES: string   = "SDL_VIDEO_MAC_FULLSCREEN_SPACES"
const HINT_VIDEO_WIN_D3DCOMPILER: string         = "SDL_VIDEO_WIN_D3DCOMPILER"
const HINT_VIDEO_X11_NET_WM_PING: string         = "SDL_VIDEO_X11_NET_WM_PING"
const HINT_VIDEO_X11_NET_WM_BYPASS_COMPOSITOR: string =
    "SDL_VIDEO_X11_NET_WM_BYPASS_COMPOSITOR"
const HINT_VIDEO_X11_XINERAMA: string            = "SDL_VIDEO_X11_XINERAMA"
const HINT_VIDEO_X11_XRANDR: string              = "SDL_VIDEO_X11_XRANDR"
const HINT_VIDEO_X11_XVIDMODE: string            = "SDL_VIDEO_X11_XVIDMODE"
const HINT_OPENGL_ES_DRIVER: string              = "SDL_OPENGL_ES_DRIVER"

// Window
const HINT_WINDOWS_DISABLE_THREAD_NAMING: string = "SDL_WINDOWS_DISABLE_THREAD_NAMING"
const HINT_WINDOWS_ENABLE_MESSAGELOOP: string    = "SDL_WINDOWS_ENABLE_MESSAGELOOP"
const HINT_WINDOW_FRAME_USABLE_WHILE_CURSOR_HIDDEN: string =
    "SDL_WINDOW_FRAME_USABLE_WHILE_CURSOR_HIDDEN"

// Mouse / touch
const HINT_MOUSE_NORMAL_SPEED_SCALE: string         = "SDL_MOUSE_NORMAL_SPEED_SCALE"
const HINT_MOUSE_RELATIVE_SPEED_SCALE: string       = "SDL_MOUSE_RELATIVE_SPEED_SCALE"
const HINT_MOUSE_RELATIVE_MODE_WARP: string         = "SDL_MOUSE_RELATIVE_MODE_WARP"
const HINT_MOUSE_FOCUS_CLICKTHROUGH: string         = "SDL_MOUSE_FOCUS_CLICKTHROUGH"
const HINT_TOUCH_MOUSE_EVENTS: string               = "SDL_TOUCH_MOUSE_EVENTS"
const HINT_MOUSE_TOUCH_EVENTS: string               = "SDL_MOUSE_TOUCH_EVENTS"
const HINT_MOUSE_DOUBLE_CLICK_TIME: string          = "SDL_MOUSE_DOUBLE_CLICK_TIME"
const HINT_MOUSE_DOUBLE_CLICK_RADIUS: string        = "SDL_MOUSE_DOUBLE_CLICK_RADIUS"

// Keyboard
const HINT_GRAB_KEYBOARD: string                  = "SDL_GRAB_KEYBOARD"
const HINT_KEYBOARD_ELEMENT: string               = "SDL_KEYBOARD_ELEMENT"
const HINT_NO_SIGNAL_HANDLERS: string             = "SDL_NO_SIGNAL_HANDLERS"

// Joystick / gamepad
const HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS: string = "SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS"
const HINT_GAMECONTROLLERCONFIG: string             = "SDL_GAMECONTROLLERCONFIG"
const HINT_GAMECONTROLLERCONFIG_FILE: string        = "SDL_GAMECONTROLLERCONFIG_FILE"
const HINT_GAMECONTROLLER_USE_BUTTON_LABELS: string = "SDL_GAMECONTROLLER_USE_BUTTON_LABELS"
const HINT_GAMECONTROLLER_IGNORE_DEVICES: string    = "SDL_GAMECONTROLLER_IGNORE_DEVICES"
const HINT_GAMECONTROLLER_IGNORE_DEVICES_EXCEPT: string =
    "SDL_GAMECONTROLLER_IGNORE_DEVICES_EXCEPT"

// Apple / iOS / accelerometer
const HINT_ACCELEROMETER_AS_JOYSTICK: string     = "SDL_ACCELEROMETER_AS_JOYSTICK"
const HINT_APPLE_TV_CONTROLLER_UI_EVENTS: string = "SDL_APPLE_TV_CONTROLLER_UI_EVENTS"
const HINT_APPLE_TV_REMOTE_ALLOW_ROTATION: string = "SDL_APPLE_TV_REMOTE_ALLOW_ROTATION"
const HINT_IDLE_TIMER_DISABLED: string           = "SDL_IOS_IDLE_TIMER_DISABLED"
const HINT_ORIENTATIONS: string                  = "SDL_IOS_ORIENTATIONS"

// Android
const HINT_ANDROID_APK_EXPANSION_MAIN_FILE_VERSION: string =
    "SDL_ANDROID_APK_EXPANSION_MAIN_FILE_VERSION"
const HINT_ANDROID_APK_EXPANSION_PATCH_FILE_VERSION: string =
    "SDL_ANDROID_APK_EXPANSION_PATCH_FILE_VERSION"
const HINT_ANDROID_TRAP_BACK_BUTTON: string      = "SDL_ANDROID_TRAP_BACK_BUTTON"

// XInput / Wii / RPI
const HINT_XINPUT_ENABLED: string                = "SDL_XINPUT_ENABLED"
const HINT_XINPUT_USE_OLD_JOYSTICK_MAPPING: string = "SDL_XINPUT_USE_OLD_JOYSTICK_MAPPING"
const HINT_RPI_VIDEO_LAYER: string               = "SDL_RPI_VIDEO_LAYER"

// Threads / behaviour
const HINT_THREAD_STACK_SIZE: string             = "SDL_THREAD_STACK_SIZE"
const HINT_THREAD_PRIORITY_POLICY: string        = "SDL_THREAD_PRIORITY_POLICY"
const HINT_TIMER_RESOLUTION: string              = "SDL_TIMER_RESOLUTION"
const HINT_BMP_SAVE_LEGACY_FORMAT: string        = "SDL_BMP_SAVE_LEGACY_FORMAT"
