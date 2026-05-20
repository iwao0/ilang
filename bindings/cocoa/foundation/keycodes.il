// Virtual key codes — what `NSEvent.keyCode` returns. Mirrors the
// `kVK_*` constants from `<HIToolbox/Events.h>`. These are
// **physical-key**, layout-independent codes: pressing the key in
// the bottom-left of a US QWERTY (`Z`) gives `keyZ` whether the
// system layout is QWERTY, Dvorak, or AZERTY.
//
// Use against an `NSEvent`:
//
//     scene.keyDown(fn(e) {
//         if e.keyCode == KeyCode.leftArrow {
//             player.x -= 20.0
//         }
//     })
//
// The values are stable across macOS releases (Carbon ABI).

// ─── Letters (ANSI QWERTY layout) ─────────────────────────────
//
/// Names map physical positions, NOT printed characters. `keyA` is
/// always the bottom-row leftmost letter; under a Dvorak layout it
/// still produces `keyA` (the *character* it emits would be 'a').
pub enum KeyCode: u16 {
    keyA = 0x00
    keyS = 0x01
    keyD = 0x02
    keyF = 0x03
    keyH = 0x04
    keyG = 0x05
    keyZ = 0x06
    keyX = 0x07
    keyC = 0x08
    keyV = 0x09
    keyB = 0x0B
    keyQ = 0x0C
    keyW = 0x0D
    keyE = 0x0E
    keyR = 0x0F
    keyY = 0x10
    keyT = 0x11
    keyO = 0x1F
    keyU = 0x20
    keyI = 0x22
    keyP = 0x23
    keyL = 0x25
    keyJ = 0x26
    keyK = 0x28
    keyN = 0x2D
    keyM = 0x2E

    // ─── Digits (top row, NOT keypad) ─────────────────────────
    key1 = 0x12
    key2 = 0x13
    key3 = 0x14
    key4 = 0x15
    key5 = 0x17
    key6 = 0x16
    key7 = 0x1A
    key8 = 0x1C
    key9 = 0x19
    key0 = 0x1D

    // ─── Punctuation / symbols (ANSI positions) ───────────────
    equal        = 0x18   // =/+
    minus        = 0x1B   // -/_
    rightBracket = 0x1E   // ]/}
    leftBracket  = 0x21   // [/{
    quote        = 0x27   // '/"
    semicolon    = 0x29   // ;/:
    backslash    = 0x2A   // \/|
    comma        = 0x2B   // ,/<
    slash        = 0x2C   // //?
    period       = 0x2F   // ./>
    grave        = 0x32   // `/~

    // ─── Whitespace / editing ─────────────────────────────────
    returnKey      = 0x24   // `return`/Enter on the main keyboard
    tab            = 0x30
    space          = 0x31
    delete         = 0x33   // Backspace
    forwardDelete  = 0x75   // fn+Delete on laptops, dedicated Delete key

    // ─── Navigation cluster ───────────────────────────────────
    escape   = 0x35
    home     = 0x73
    pageUp   = 0x74
    end      = 0x77
    pageDown = 0x79

    // ─── Arrow keys ───────────────────────────────────────────
    leftArrow  = 0x7B
    rightArrow = 0x7C
    downArrow  = 0x7D
    upArrow    = 0x7E

    // ─── Modifiers (sent on `flagsChanged:`, not `keyDown:`) ──
    command      = 0x37
    shift        = 0x38
    capsLock     = 0x39
    option       = 0x3A
    control      = 0x3B
    rightCommand = 0x36
    rightShift   = 0x3C
    rightOption  = 0x3D
    rightControl = 0x3E
    function     = 0x3F

    // ─── Function row ─────────────────────────────────────────
    f1  = 0x7A
    f2  = 0x78
    f3  = 0x63
    f4  = 0x76
    f5  = 0x60
    f6  = 0x61
    f7  = 0x62
    f8  = 0x64
    f9  = 0x65
    f10 = 0x6D
    f11 = 0x67
    f12 = 0x6F
    f13 = 0x69
    f14 = 0x6B
    f15 = 0x71
    f16 = 0x6A
    f17 = 0x40
    f18 = 0x4F
    f19 = 0x50
    f20 = 0x5A

    // ─── Media (Mac keyboards with a row of media keys) ───────
    volumeUp   = 0x48
    volumeDown = 0x49
    mute       = 0x4A

    // ─── Numeric keypad ───────────────────────────────────────
    keypad0        = 0x52
    keypad1        = 0x53
    keypad2        = 0x54
    keypad3        = 0x55
    keypad4        = 0x56
    keypad5        = 0x57
    keypad6        = 0x58
    keypad7        = 0x59
    keypad8        = 0x5B
    keypad9        = 0x5C
    keypadDecimal  = 0x41
    keypadMultiply = 0x43
    keypadPlus     = 0x45
    keypadClear    = 0x47   // shared with NumLock on PC keyboards
    keypadDivide   = 0x4B
    keypadEnter    = 0x4C
    keypadMinus    = 0x4E
    keypadEquals   = 0x51

    help = 0x72

    // ─── JIS keyboard extras ──────────────────────────────────
    //
    // Physical keys present on Apple JIS keyboards. `eisu` /
    // `kana` are the two keys flanking the spacebar that toggle
    // input mode (英数 → ASCII / latin, かな → kana). `yen` and
    // `underscore` are the JIS-only punctuation keys.
    jisYen          = 0x5D   // ¥ \ — to the left of `delete`
    jisUnderscore   = 0x5E   // _ — between right-Shift and the arrow cluster
    jisKeypadComma  = 0x5F
    jisEisu         = 0x66   // 英数 (left of space) — turn IME off
    jisKana         = 0x68   // かな (right of space) — turn IME on

    // ─── ISO keyboard extras ──────────────────────────────────
    //
    // The dedicated "section / paragraph" key that sits between
    // left-Shift and `Z` on European (ISO) layouts. North-American
    // ANSI keyboards don't have a physical equivalent.
    isoSection = 0x0A
}
