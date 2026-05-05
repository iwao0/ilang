// SDL2 keyboard: state polling, modifier state, name <-> code
// lookups, and text-input mode control. Mirrors SDL_keyboard.h
// + the SDL_Scancode / SDL_Keycode / SDL_Keymod enums.
//
// The state polled by `Keyboard.isPressed(scancode)` reflects
// SDL's internal keyboard array — call `sdl.pumpEvents()` once
// per frame before querying so newly pressed keys show up.

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

    //     sdl.Keyboard.pump()
    //     if sdl.Keyboard.isPressed(sdl.SCANCODE_LEFT) { ... }
    //
    // Static-only — `Keyboard` exists just to namespace the
    // polling helpers. `state_ptr` is a class-static i64 holding
    // the pointer SDL handed back from `SDL_GetKeyboardState`
    // (the same buffer, with stable address, is mutated by SDL
    // itself); `pump()` refreshes it. `isPressed` reads a single
    // byte at `state_ptr + scancode` via `readU8` — no allocation
    // per call.
    class Keyboard {
        static state_ptr: i64 = 0

        // Refresh the cached pointer. Call once per frame after
        // `sdl.pumpEvents()`. (The address SDL hands back is
        // actually static, so calling once at startup also works,
        // but pumping each frame keeps the contract symmetric
        // with stateful APIs.)
        static pump() {
            Keyboard.state_ptr = SDL_GetKeyboardState(0 as *i32) as i64
        }

        // ─── live state ──────────────────────────────────────────
        static isPressed(scancode: i32): bool {
            if Keyboard.state_ptr == 0 || scancode < 0 || scancode >= NUM_SCANCODES {
                return false
            }
            readU8(Keyboard.state_ptr as *const void, scancode as i64) != 0 as u8
        }

        // ─── modifiers ───────────────────────────────────────────
        static modState(): u16 {
            SDL_GetModState()
        }
        static setModState(modstate: u16) {
            SDL_SetModState(modstate)
        }

        // ─── scancode <-> keycode ────────────────────────────────
        static keyFromScancode(scancode: i32): i32 {
            SDL_GetKeyFromScancode(scancode)
        }
        static scancodeFromKey(key: i32): i32 {
            SDL_GetScancodeFromKey(key)
        }

        // ─── name lookups ────────────────────────────────────────
        static scancodeName(scancode: i32): string {
            stringFromCstr(SDL_GetScancodeName(scancode))
        }
        static scancodeFromName(name: string): i32 {
            SDL_GetScancodeFromName(cstrFromString(name))
        }
        static keyName(key: i32): string {
            stringFromCstr(SDL_GetKeyName(key))
        }
        static keyFromName(name: string): i32 {
            SDL_GetKeyFromName(cstrFromString(name))
        }

        // ─── text input mode ─────────────────────────────────────
        static startTextInput() {
            SDL_StartTextInput()
        }
        static stopTextInput() {
            SDL_StopTextInput()
        }
        static isTextInputActive(): bool {
            SDL_IsTextInputActive() != 0
        }
        static clearComposition() {
            SDL_ClearComposition()
        }
        static isTextInputShown(): bool {
            SDL_IsTextInputShown() != 0
        }
        static hasScreenKeyboardSupport(): bool {
            SDL_HasScreenKeyboardSupport() != 0
        }

        static reset() {
            SDL_ResetKeyboard()
        }
    }
}

// ─── SDL_Scancode (matches the C enum) ────────────────────────────
const SCANCODE_UNKNOWN: i32 = 0

// Letters
const SCANCODE_A: i32 = 4
const SCANCODE_B: i32 = 5
const SCANCODE_C: i32 = 6
const SCANCODE_D: i32 = 7
const SCANCODE_E: i32 = 8
const SCANCODE_F: i32 = 9
const SCANCODE_G: i32 = 10
const SCANCODE_H: i32 = 11
const SCANCODE_I: i32 = 12
const SCANCODE_J: i32 = 13
const SCANCODE_K: i32 = 14
const SCANCODE_L: i32 = 15
const SCANCODE_M: i32 = 16
const SCANCODE_N: i32 = 17
const SCANCODE_O: i32 = 18
const SCANCODE_P: i32 = 19
const SCANCODE_Q: i32 = 20
const SCANCODE_R: i32 = 21
const SCANCODE_S: i32 = 22
const SCANCODE_T: i32 = 23
const SCANCODE_U: i32 = 24
const SCANCODE_V: i32 = 25
const SCANCODE_W: i32 = 26
const SCANCODE_X: i32 = 27
const SCANCODE_Y: i32 = 28
const SCANCODE_Z: i32 = 29

// Number row
const SCANCODE_1: i32 = 30
const SCANCODE_2: i32 = 31
const SCANCODE_3: i32 = 32
const SCANCODE_4: i32 = 33
const SCANCODE_5: i32 = 34
const SCANCODE_6: i32 = 35
const SCANCODE_7: i32 = 36
const SCANCODE_8: i32 = 37
const SCANCODE_9: i32 = 38
const SCANCODE_0: i32 = 39

// Whitespace / control
const SCANCODE_RETURN: i32    = 40
const SCANCODE_ESCAPE: i32    = 41
const SCANCODE_BACKSPACE: i32 = 42
const SCANCODE_TAB: i32       = 43
const SCANCODE_SPACE: i32     = 44

// Punctuation
const SCANCODE_MINUS: i32        = 45
const SCANCODE_EQUALS: i32       = 46
const SCANCODE_LEFTBRACKET: i32  = 47
const SCANCODE_RIGHTBRACKET: i32 = 48
const SCANCODE_BACKSLASH: i32    = 49
const SCANCODE_NONUSHASH: i32    = 50
const SCANCODE_SEMICOLON: i32    = 51
const SCANCODE_APOSTROPHE: i32   = 52
const SCANCODE_GRAVE: i32        = 53
const SCANCODE_COMMA: i32        = 54
const SCANCODE_PERIOD: i32       = 55
const SCANCODE_SLASH: i32        = 56

const SCANCODE_CAPSLOCK: i32 = 57

// Function keys
const SCANCODE_F1: i32  = 58
const SCANCODE_F2: i32  = 59
const SCANCODE_F3: i32  = 60
const SCANCODE_F4: i32  = 61
const SCANCODE_F5: i32  = 62
const SCANCODE_F6: i32  = 63
const SCANCODE_F7: i32  = 64
const SCANCODE_F8: i32  = 65
const SCANCODE_F9: i32  = 66
const SCANCODE_F10: i32 = 67
const SCANCODE_F11: i32 = 68
const SCANCODE_F12: i32 = 69

// Navigation cluster
const SCANCODE_PRINTSCREEN: i32 = 70
const SCANCODE_SCROLLLOCK: i32  = 71
const SCANCODE_PAUSE: i32       = 72
const SCANCODE_INSERT: i32      = 73
const SCANCODE_HOME: i32        = 74
const SCANCODE_PAGEUP: i32      = 75
const SCANCODE_DELETE: i32      = 76
const SCANCODE_END: i32         = 77
const SCANCODE_PAGEDOWN: i32    = 78
const SCANCODE_RIGHT: i32       = 79
const SCANCODE_LEFT: i32        = 80
const SCANCODE_DOWN: i32        = 81
const SCANCODE_UP: i32          = 82

// Numpad
const SCANCODE_NUMLOCKCLEAR: i32 = 83
const SCANCODE_KP_DIVIDE: i32    = 84
const SCANCODE_KP_MULTIPLY: i32  = 85
const SCANCODE_KP_MINUS: i32     = 86
const SCANCODE_KP_PLUS: i32      = 87
const SCANCODE_KP_ENTER: i32     = 88
const SCANCODE_KP_1: i32         = 89
const SCANCODE_KP_2: i32         = 90
const SCANCODE_KP_3: i32         = 91
const SCANCODE_KP_4: i32         = 92
const SCANCODE_KP_5: i32         = 93
const SCANCODE_KP_6: i32         = 94
const SCANCODE_KP_7: i32         = 95
const SCANCODE_KP_8: i32         = 96
const SCANCODE_KP_9: i32         = 97
const SCANCODE_KP_0: i32         = 98
const SCANCODE_KP_PERIOD: i32    = 99

const SCANCODE_NONUSBACKSLASH: i32 = 100
const SCANCODE_APPLICATION: i32    = 101
const SCANCODE_POWER: i32          = 102
const SCANCODE_KP_EQUALS: i32      = 103

// Extended function keys
const SCANCODE_F13: i32 = 104
const SCANCODE_F14: i32 = 105
const SCANCODE_F15: i32 = 106
const SCANCODE_F16: i32 = 107
const SCANCODE_F17: i32 = 108
const SCANCODE_F18: i32 = 109
const SCANCODE_F19: i32 = 110
const SCANCODE_F20: i32 = 111
const SCANCODE_F21: i32 = 112
const SCANCODE_F22: i32 = 113
const SCANCODE_F23: i32 = 114
const SCANCODE_F24: i32 = 115

const SCANCODE_EXECUTE: i32    = 116
const SCANCODE_HELP: i32       = 117
const SCANCODE_MENU: i32       = 118
const SCANCODE_SELECT: i32     = 119
const SCANCODE_STOP: i32       = 120
const SCANCODE_AGAIN: i32      = 121
const SCANCODE_UNDO: i32       = 122
const SCANCODE_CUT: i32        = 123
const SCANCODE_COPY: i32       = 124
const SCANCODE_PASTE: i32      = 125
const SCANCODE_FIND: i32       = 126
const SCANCODE_MUTE: i32       = 127
const SCANCODE_VOLUMEUP: i32   = 128
const SCANCODE_VOLUMEDOWN: i32 = 129

// Modifier keys
const SCANCODE_LCTRL: i32  = 224
const SCANCODE_LSHIFT: i32 = 225
const SCANCODE_LALT: i32   = 226
const SCANCODE_LGUI: i32   = 227
const SCANCODE_RCTRL: i32  = 228
const SCANCODE_RSHIFT: i32 = 229
const SCANCODE_RALT: i32   = 230
const SCANCODE_RGUI: i32   = 231
const SCANCODE_MODE: i32   = 257

// Media / browser keys (high range, partial)
const SCANCODE_AUDIONEXT: i32   = 258
const SCANCODE_AUDIOPREV: i32   = 259
const SCANCODE_AUDIOSTOP: i32   = 260
const SCANCODE_AUDIOPLAY: i32   = 261
const SCANCODE_AUDIOMUTE: i32   = 262
const SCANCODE_MEDIASELECT: i32 = 263
const SCANCODE_WWW: i32         = 264
const SCANCODE_MAIL: i32        = 265
const SCANCODE_CALCULATOR: i32  = 266
const SCANCODE_COMPUTER: i32    = 267
const SCANCODE_AC_SEARCH: i32   = 268
const SCANCODE_AC_HOME: i32     = 269
const SCANCODE_AC_BACK: i32     = 270
const SCANCODE_AC_FORWARD: i32  = 271
const SCANCODE_AC_STOP: i32     = 272
const SCANCODE_AC_REFRESH: i32  = 273
const SCANCODE_AC_BOOKMARKS: i32 = 274

const NUM_SCANCODES: i32 = 512

// ─── SDL_Keycode (SDLK_* values) ──────────────────────────────────
//
// SDLK values for printable ASCII characters are the character
// itself. Function-keyish ones (F1, arrows, modifiers) are formed
// as `SCANCODE_MASK | scancode`. SDLK_SCANCODE_MASK = 1 << 30 =
// 0x40000000, so e.g. SDLK_F1 = 0x40000000 | 58 = 0x4000003A.

const SDLK_SCANCODE_MASK: i32 = 0x40000000

const SDLK_UNKNOWN: i32   = 0
const SDLK_RETURN: i32    = 13      // '\r'
const SDLK_ESCAPE: i32    = 27      // '\033'
const SDLK_BACKSPACE: i32 = 8       // '\b'
const SDLK_TAB: i32       = 9       // '\t'
const SDLK_SPACE: i32     = 32      // ' '
const SDLK_EXCLAIM: i32      = 33   // '!'
const SDLK_QUOTEDBL: i32     = 34   // '"'
const SDLK_HASH: i32         = 35   // '#'
const SDLK_DOLLAR: i32       = 36   // '$'
const SDLK_PERCENT: i32      = 37   // '%'
const SDLK_AMPERSAND: i32    = 38   // '&'
const SDLK_QUOTE: i32        = 39   // '\''
const SDLK_LEFTPAREN: i32    = 40   // '('
const SDLK_RIGHTPAREN: i32   = 41   // ')'
const SDLK_ASTERISK: i32     = 42   // '*'
const SDLK_PLUS: i32         = 43   // '+'
const SDLK_COMMA: i32        = 44   // ','
const SDLK_MINUS: i32        = 45   // '-'
const SDLK_PERIOD: i32       = 46   // '.'
const SDLK_SLASH: i32        = 47   // '/'
const SDLK_0: i32 = 48
const SDLK_1: i32 = 49
const SDLK_2: i32 = 50
const SDLK_3: i32 = 51
const SDLK_4: i32 = 52
const SDLK_5: i32 = 53
const SDLK_6: i32 = 54
const SDLK_7: i32 = 55
const SDLK_8: i32 = 56
const SDLK_9: i32 = 57
const SDLK_COLON: i32        = 58   // ':'
const SDLK_SEMICOLON: i32    = 59   // ';'
const SDLK_LESS: i32         = 60   // '<'
const SDLK_EQUALS: i32       = 61   // '='
const SDLK_GREATER: i32      = 62   // '>'
const SDLK_QUESTION: i32     = 63   // '?'
const SDLK_AT: i32           = 64   // '@'
const SDLK_LEFTBRACKET: i32  = 91   // '['
const SDLK_BACKSLASH: i32    = 92   // '\\'
const SDLK_RIGHTBRACKET: i32 = 93   // ']'
const SDLK_CARET: i32        = 94   // '^'
const SDLK_UNDERSCORE: i32   = 95   // '_'
const SDLK_BACKQUOTE: i32    = 96   // '`'
// Lowercase letters — SDLK is the typed character.
const SDLK_a: i32 = 97
const SDLK_b: i32 = 98
const SDLK_c: i32 = 99
const SDLK_d: i32 = 100
const SDLK_e: i32 = 101
const SDLK_f: i32 = 102
const SDLK_g: i32 = 103
const SDLK_h: i32 = 104
const SDLK_i: i32 = 105
const SDLK_j: i32 = 106
const SDLK_k: i32 = 107
const SDLK_l: i32 = 108
const SDLK_m: i32 = 109
const SDLK_n: i32 = 110
const SDLK_o: i32 = 111
const SDLK_p: i32 = 112
const SDLK_q: i32 = 113
const SDLK_r: i32 = 114
const SDLK_s: i32 = 115
const SDLK_t: i32 = 116
const SDLK_u: i32 = 117
const SDLK_v: i32 = 118
const SDLK_w: i32 = 119
const SDLK_x: i32 = 120
const SDLK_y: i32 = 121
const SDLK_z: i32 = 122
const SDLK_DELETE: i32 = 127

// Non-printable keys = SDLK_SCANCODE_MASK | scancode.
//   SDLK_F1     = 0x40000000 | 58  = 0x4000003A = 1073741882
//   SDLK_RIGHT  = 0x40000000 | 79  = 0x4000004F = 1073741903
const SDLK_CAPSLOCK: i32   = 1073741881   // SCANCODE_CAPSLOCK
const SDLK_F1: i32         = 1073741882
const SDLK_F2: i32         = 1073741883
const SDLK_F3: i32         = 1073741884
const SDLK_F4: i32         = 1073741885
const SDLK_F5: i32         = 1073741886
const SDLK_F6: i32         = 1073741887
const SDLK_F7: i32         = 1073741888
const SDLK_F8: i32         = 1073741889
const SDLK_F9: i32         = 1073741890
const SDLK_F10: i32        = 1073741891
const SDLK_F11: i32        = 1073741892
const SDLK_F12: i32        = 1073741893
const SDLK_PRINTSCREEN: i32 = 1073741894
const SDLK_SCROLLLOCK: i32 = 1073741895
const SDLK_PAUSE: i32      = 1073741896
const SDLK_INSERT: i32     = 1073741897
const SDLK_HOME: i32       = 1073741898
const SDLK_PAGEUP: i32     = 1073741899
const SDLK_END: i32        = 1073741901
const SDLK_PAGEDOWN: i32   = 1073741902
const SDLK_RIGHT: i32      = 1073741903
const SDLK_LEFT: i32       = 1073741904
const SDLK_DOWN: i32       = 1073741905
const SDLK_UP: i32         = 1073741906
const SDLK_LCTRL: i32      = 1073742048   // SCANCODE_LCTRL = 224
const SDLK_LSHIFT: i32     = 1073742049
const SDLK_LALT: i32       = 1073742050
const SDLK_LGUI: i32       = 1073742051
const SDLK_RCTRL: i32      = 1073742052
const SDLK_RSHIFT: i32     = 1073742053
const SDLK_RALT: i32       = 1073742054
const SDLK_RGUI: i32       = 1073742055

// ─── SDL_Keymod (modifier-key bitmask) ────────────────────────────
const KMOD_NONE: u16     = 0x0000
const KMOD_LSHIFT: u16   = 0x0001
const KMOD_RSHIFT: u16   = 0x0002
const KMOD_LCTRL: u16    = 0x0040
const KMOD_RCTRL: u16    = 0x0080
const KMOD_LALT: u16     = 0x0100
const KMOD_RALT: u16     = 0x0200
const KMOD_LGUI: u16     = 0x0400
const KMOD_RGUI: u16     = 0x0800
const KMOD_NUM: u16      = 0x1000
const KMOD_CAPS: u16     = 0x2000
const KMOD_MODE: u16     = 0x4000
const KMOD_SCROLL: u16   = 0x8000
const KMOD_CTRL: u16     = 0x00C0   // LCTRL | RCTRL
const KMOD_SHIFT: u16    = 0x0003   // LSHIFT | RSHIFT
const KMOD_ALT: u16      = 0x0300   // LALT | RALT
const KMOD_GUI: u16      = 0x0C00   // LGUI | RGUI
