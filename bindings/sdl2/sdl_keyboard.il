// SDL2 keyboard: state polling, modifier state, name <-> code
// lookups, and text-input mode control. Mirrors SDL_keyboard.h
// + the SDL_Scancode / SDL_Keycode / SDL_Keymod enums.
//
// The state polled by `Keyboard.isPressed(scancode)` reflects
// SDL's internal keyboard array — call `sdl.pumpEvents()` once
// per frame before querying so newly pressed keys show up.

/// `SDL_Scancode` — physical key, layout-independent.
/// Number-row keys use the `digit0..digit9` form (idents can't
/// start with a digit).
enum Scancode: i32 {
    unknown                     = 0
    a                           = 4
    b                           = 5
    c                           = 6
    d                           = 7
    e                           = 8
    f                           = 9
    g                           = 10
    h                           = 11
    i                           = 12
    j                           = 13
    k                           = 14
    l                           = 15
    m                           = 16
    n                           = 17
    o                           = 18
    p                           = 19
    q                           = 20
    r                           = 21
    s                           = 22
    t                           = 23
    u                           = 24
    v                           = 25
    w                           = 26
    x                           = 27
    y                           = 28
    z                           = 29
    digit1                      = 30
    digit2                      = 31
    digit3                      = 32
    digit4                      = 33
    digit5                      = 34
    digit6                      = 35
    digit7                      = 36
    digit8                      = 37
    digit9                      = 38
    digit0                      = 39
    return                      =40
    escape                      = 41
    backspace                   = 42
    tab                         = 43
    space                       = 44
    minus                       = 45
    equals                      = 46
    leftbracket                 = 47
    rightbracket                = 48
    backslash                   = 49
    nonushash                   = 50
    semicolon                   = 51
    apostrophe                  = 52
    grave                       = 53
    comma                       = 54
    period                      = 55
    slash                       = 56
    capslock                    = 57
    f1                          = 58
    f2                          = 59
    f3                          = 60
    f4                          = 61
    f5                          = 62
    f6                          = 63
    f7                          = 64
    f8                          = 65
    f9                          = 66
    f10                         = 67
    f11                         = 68
    f12                         = 69
    printscreen                 = 70
    scrolllock                  = 71
    pause                       = 72
    insert                      = 73
    home                        = 74
    pageup                      = 75
    delete                      = 76
    end                         = 77
    pagedown                    = 78
    right                       = 79
    left                        = 80
    down                        = 81
    up                          = 82
    numlockclear                = 83
    kpDivide                    = 84
    kpMultiply                  = 85
    kpMinus                     = 86
    kpPlus                      = 87
    kpEnter                     = 88
    kp1                         = 89
    kp2                         = 90
    kp3                         = 91
    kp4                         = 92
    kp5                         = 93
    kp6                         = 94
    kp7                         = 95
    kp8                         = 96
    kp9                         = 97
    kp0                         = 98
    kpPeriod                    = 99
    nonusbackslash              = 100
    application                 = 101
    power                       = 102
    kpEquals                    = 103
    f13                         = 104
    f14                         = 105
    f15                         = 106
    f16                         = 107
    f17                         = 108
    f18                         = 109
    f19                         = 110
    f20                         = 111
    f21                         = 112
    f22                         = 113
    f23                         = 114
    f24                         = 115
    execute                     = 116
    help                        = 117
    menu                        = 118
    select                      = 119
    stop                        = 120
    again                       = 121
    undo                        = 122
    cut                         = 123
    copy                        = 124
    paste                       = 125
    find                        = 126
    mute                        = 127
    volumeup                    = 128
    volumedown                  = 129
    lctrl                       = 224
    lshift                      = 225
    lalt                        = 226
    lgui                        = 227
    rctrl                       = 228
    rshift                      = 229
    ralt                        = 230
    rgui                        = 231
    mode                        = 257
    audionext                   = 258
    audioprev                   = 259
    audiostop                   = 260
    audioplay                   = 261
    audiomute                   = 262
    mediaselect                 = 263
    www                         = 264
    mail                        = 265
    calculator                  = 266
    computer                    = 267
    acSearch                    = 268
    acHome                      = 269
    acBack                      = 270
    acForward                   = 271
    acStop                      = 272
    acRefresh                   = 273
    acBookmarks                 = 274
}

/// `SDL_Keycode` — virtual key (current keyboard layout).
/// Same renaming rule as `Scancode` (`digit0..9`).
enum Keycode: i32 {
    scancodeMask                = 0x40000000
    unknown                     = 0
    return                      =13  // '\r'
    escape                      = 27  // '\033'
    backspace                   = 8  // '\b'
    tab                         = 9  // '\t'
    space                       = 32  // ' '
    exclaim                     = 33  // '!'
    quotedbl                    = 34  // '"'
    hash                        = 35  // '#'
    dollar                      = 36  // '$'
    percent                     = 37  // '%'
    ampersand                   = 38  // '&'
    quote                       = 39  // '\''
    leftparen                   = 40  // '('
    rightparen                  = 41  // ')'
    asterisk                    = 42  // '*'
    plus                        = 43  // '+'
    comma                       = 44  // ','
    minus                       = 45  // '-'
    period                      = 46  // '.'
    slash                       = 47  // '/'
    digit0                      = 48
    digit1                      = 49
    digit2                      = 50
    digit3                      = 51
    digit4                      = 52
    digit5                      = 53
    digit6                      = 54
    digit7                      = 55
    digit8                      = 56
    digit9                      = 57
    colon                       = 58  // ':'
    semicolon                   = 59  // ';'
    less                        = 60  // '<'
    equals                      = 61  // '='
    greater                     = 62  // '>'
    question                    = 63  // '?'
    at                          = 64  // '@'
    leftbracket                 = 91  // '['
    backslash                   = 92  // '\\'
    rightbracket                = 93  // ']'
    caret                       = 94  // '^'
    underscore                  = 95  // '_'
    backquote                   = 96  // '`'
    a                           = 97
    b                           = 98
    c                           = 99
    d                           = 100
    e                           = 101
    f                           = 102
    g                           = 103
    h                           = 104
    i                           = 105
    j                           = 106
    k                           = 107
    l                           = 108
    m                           = 109
    n                           = 110
    o                           = 111
    p                           = 112
    q                           = 113
    r                           = 114
    s                           = 115
    t                           = 116
    u                           = 117
    v                           = 118
    w                           = 119
    x                           = 120
    y                           = 121
    z                           = 122
    delete                      = 127
    capslock                    = 1073741881  // SCANCODE_CAPSLOCK
    f1                          = 1073741882
    f2                          = 1073741883
    f3                          = 1073741884
    f4                          = 1073741885
    f5                          = 1073741886
    f6                          = 1073741887
    f7                          = 1073741888
    f8                          = 1073741889
    f9                          = 1073741890
    f10                         = 1073741891
    f11                         = 1073741892
    f12                         = 1073741893
    printscreen                 = 1073741894
    scrolllock                  = 1073741895
    pause                       = 1073741896
    insert                      = 1073741897
    home                        = 1073741898
    pageup                      = 1073741899
    end                         = 1073741901
    pagedown                    = 1073741902
    right                       = 1073741903
    left                        = 1073741904
    down                        = 1073741905
    up                          = 1073741906
    lctrl                       = 1073742048  // SCANCODE_LCTRL = 224
    lshift                      = 1073742049
    lalt                        = 1073742050
    lgui                        = 1073742051
    rctrl                       = 1073742052
    rshift                      = 1073742053
    ralt                        = 1073742054
    rgui                        = 1073742055
}

/// `SDL_Keymod` — modifier-key bitmask.
@flags
enum KeyMod: u16 {
    none    = 0x0000
    lshift  = 0x0001
    rshift  = 0x0002
    lctrl   = 0x0040
    rctrl   = 0x0080
    lalt    = 0x0100
    ralt    = 0x0200
    lgui    = 0x0400
    rgui    = 0x0800
    num     = 0x1000
    caps    = 0x2000
    mode    = 0x4000
    scroll  = 0x8000
    ctrl    = 0x00C0  // LCTRL | RCTRL
    shift   = 0x0003  // LSHIFT | RSHIFT
    alt     = 0x0300  // LALT | RALT
    gui     = 0x0C00  // LGUI | RGUI
}

@extern(C) {
    @lib("SDL2") fn SDL_GetKeyboardState(numkeys: *i32): *const u8
    @lib("SDL2") fn SDL_GetModState(): u16
    @lib("SDL2") fn SDL_SetModState(modstate: u16)
    @lib("SDL2") fn SDL_GetKeyFromScancode(scancode: i32): i32
    @lib("SDL2") fn SDL_GetScancodeFromKey(key: i32): i32
    @lib("SDL2") fn SDL_GetScancodeName(scancode: i32): *const char
    @lib("SDL2") fn SDL_GetScancodeFromName(name: *const char): i32
    @lib("SDL2") fn SDL_GetKeyName(key: i32): *const char
    @lib("SDL2") fn SDL_GetKeyFromName(name: *const char): i32
    @lib("SDL2") fn SDL_StartTextInput()
    @lib("SDL2") fn SDL_IsTextInputActive(): i32
    @lib("SDL2") fn SDL_StopTextInput()
    @lib("SDL2") fn SDL_ClearComposition()
    @lib("SDL2") fn SDL_IsTextInputShown(): i32
    @lib("SDL2") fn SDL_HasScreenKeyboardSupport(): i32
    @lib("SDL2") fn SDL_ResetKeyboard()
    @lib("SDL2") fn SDL_SetTextInputRect(rect: *sdl.Rect)
    @lib("SDL2") fn SDL_IsScreenKeyboardShown(window: *void): i32
    @lib("SDL2") fn SDL_GetKeyboardFocus(): *void

    /// Static-only namespace for keyboard state. Typical usage:
    ///
    ///     sdl.Keyboard.pump()
    ///     if sdl.Keyboard.isPressed(sdl.Scancode.left) { ... }
    ///
    /// `state_ptr` is a class-static `i64` holding the pointer SDL
    /// handed back from `SDL_GetKeyboardState` (the same buffer,
    /// with stable address, is mutated by SDL itself); `pump()`
    /// refreshes it. `isPressed` reads a single byte at
    /// `state_ptr + scancode` via `readU8` — no allocation per
    /// call.
    class Keyboard {
        static state_ptr: i64 = 0

        /// Refresh the cached state pointer. Call once per frame
        /// after `sdl.pumpEvents()`. (The address SDL hands back is
        /// actually static, so calling once at startup also works,
        /// but pumping each frame keeps the contract symmetric
        /// with stateful APIs.)
        static pump() {
            Keyboard.state_ptr = SDL_GetKeyboardState(0 as *i32) as i64
        }

        // ─── live state ──────────────────────────────────────────

        /// Args:
        ///   scancode — physical key code.
        ///
        /// Returns:
        ///   true while the key is held this frame. `false` when
        ///   `pump()` hasn't been called yet or when `scancode` is
        ///   out of range.
        static isPressed(scancode: Scancode): bool {
            let s: i32 = scancode as i32
            // SDL_NUM_SCANCODES = 512. Inlined because module-level
            // consts aren't visible from inside this @extern block
            // once the file is imported through the umbrella.
            if Keyboard.state_ptr == 0 || s < 0 || s >= 512 {
                return false
            }
            readU8(Keyboard.state_ptr as *const void, s as i64) != 0 as u8
        }

        // ─── modifiers ───────────────────────────────────────────

        /// Returns:
        ///   bitmask of currently-held modifier keys.
        static modState(): KeyMod {
            SDL_GetModState() as KeyMod
        }

        /// Force-set the modifier state. Useful for emulating
        /// modifiers in tests.
        static setModState(modstate: KeyMod) {
            SDL_SetModState(modstate as u16)
        }

        // ─── scancode <-> keycode ────────────────────────────────

        /// Returns:
        ///   the keycode (layout-dependent) corresponding to a
        ///   physical scancode in the current layout.
        static keyFromScancode(scancode: Scancode): Keycode {
            SDL_GetKeyFromScancode(scancode as i32) as Keycode
        }

        /// Returns:
        ///   the scancode that produces `key` in the current
        ///   layout.
        static scancodeFromKey(key: Keycode): Scancode {
            SDL_GetScancodeFromKey(key as i32) as Scancode
        }

        // ─── name lookups ────────────────────────────────────────

        /// Returns:
        ///   human-readable name (e.g. `"Left Shift"`).
        static scancodeName(scancode: Scancode): string {
            stringFromCstr(SDL_GetScancodeName(scancode as i32))
        }

        /// Args:
        ///   name — name as returned by `scancodeName`.
        ///
        /// Returns:
        ///   the matching scancode, or `Scancode.unknown` for
        ///   unrecognised names.
        static scancodeFromName(name: string): Scancode {
            SDL_GetScancodeFromName(cstrFromString(name)) as Scancode
        }

        /// Returns:
        ///   localised key name (depends on layout — `"Q"` on
        ///   QWERTY but `"A"` on AZERTY).
        static keyName(key: Keycode): string {
            stringFromCstr(SDL_GetKeyName(key as i32))
        }

        /// Args:
        ///   name — name as returned by `keyName`.
        ///
        /// Returns:
        ///   the matching keycode, or `Keycode.unknown`.
        static keyFromName(name: string): Keycode {
            SDL_GetKeyFromName(cstrFromString(name)) as Keycode
        }

        // ─── text input mode ─────────────────────────────────────

        /// Begin receiving `textInput` events; opens the on-screen
        /// keyboard on platforms that have one.
        static startTextInput() {
            SDL_StartTextInput()
        }

        /// Pair to `startTextInput`.
        static stopTextInput() {
            SDL_StopTextInput()
        }

        /// Returns:
        ///   true if `textInput` events are currently flowing.
        static isTextInputActive(): bool {
            SDL_IsTextInputActive() != 0
        }

        /// Drop any in-progress IME composition without committing.
        static clearComposition() {
            SDL_ClearComposition()
        }

        /// Returns:
        ///   true when the on-screen text input UI is visible.
        static isTextInputShown(): bool {
            SDL_IsTextInputShown() != 0
        }

        /// Returns:
        ///   true when the platform offers an on-screen keyboard
        ///   (mobile / tablet / game console).
        static hasScreenKeyboardSupport(): bool {
            SDL_HasScreenKeyboardSupport() != 0
        }

        /// Clear all "key currently held" state — useful after
        /// regaining focus to avoid spurious held keys.
        static reset() {
            SDL_ResetKeyboard()
        }

        /// Hint to IMEs / on-screen keyboards where the user is
        /// typing. `(x, y)` is in window pixels.
        ///
        /// Args:
        ///   x, y — top-left corner of the input rect.
        ///   w, h — rectangle size in pixels.
        static setTextInputRect(x: i32, y: i32, w: i32, h: i32) {
            let r = new sdl.Rect()
            r.x = x; r.y = y; r.w = w; r.h = h
            SDL_SetTextInputRect(r)
        }

        /// Args:
        ///   window — window to query.
        ///
        /// Returns:
        ///   true when the on-screen keyboard is currently visible
        ///   for that window.
        static isScreenKeyboardShown(window: sdl.Window): bool {
            SDL_IsScreenKeyboardShown(window.handle as *void) != 0
        }

        /// Returns:
        ///   raw handle of the window that currently owns keyboard
        ///   focus, or `0` if no SDL window owns it. Compare against
        ///   `myWindow.handle` to test focus.
        static focusHandle(): i64 {
            SDL_GetKeyboardFocus() as i64
        }
    }
}
