// xinput1_4.dll — XInput 1.4: Xbox One / Xbox 360 gamepad input.
// Bound to xinput1_4.dll for Windows 8+. For Windows 7 use xinput9_1_0.dll
// (same exports, smaller surface).

// ─── XINPUT_GAMEPAD wButtons flags (XINPUT_GAMEPAD_*) ────────────
@flags
pub enum GamepadButton: u16 {
    dpadUp          = 0x0001
    dpadDown        = 0x0002
    dpadLeft        = 0x0004
    dpadRight       = 0x0008
    start           = 0x0010
    back            = 0x0020
    leftThumb       = 0x0040
    rightThumb      = 0x0080
    leftShoulder    = 0x0100
    rightShoulder   = 0x0200
    a               = 0x1000
    b               = 0x2000
    x               = 0x4000
    y               = 0x8000
}

// ─── XInputGetCapabilities flags / device type ───────────────────
pub enum XInputDeviceType: u8 {
    gamepad = 1
}

@flags
pub enum XInputDeviceSubtype: u8 {
    unknown        = 0x00
    gamepad        = 0x01
    wheel          = 0x02
    arcadeStick    = 0x03
    flightStick    = 0x04
    dancePad       = 0x05
    guitar         = 0x06
    guitarAlternate = 0x07
    drumKit        = 0x08
    guitarBass     = 0x0B
    arcadePad      = 0x13
}

// ─── XInputGetBatteryInformation device type ─────────────────────
pub enum XInputBatteryDevice: u8 {
    gamepad  = 0x00
    headset  = 0x01
}

pub enum XInputBatteryType: u8 {
    disconnected = 0x00
    wired        = 0x01
    alkaline     = 0x02
    nimh         = 0x03
    unknown      = 0xFF
}

pub enum XInputBatteryLevel: u8 {
    empty    = 0x00
    low      = 0x01
    medium   = 0x02
    full     = 0x03
}

// ─── Recommended deadzone constants ──────────────────────────────
// XINPUT_GAMEPAD_LEFT_THUMB_DEADZONE  = 7849
// XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE = 8689
// XINPUT_GAMEPAD_TRIGGER_THRESHOLD    = 30

// The Windows SDK ships `xinput.lib` as the import library, which
// forwards to `xinput1_4.dll` on Windows 8+ (or to `xinput9_1_0.dll`
// for the legacy fallback).  Link against "xinput" rather than the
// versioned DLL name so the SDK's import library is picked up.
@extern(C, "xinput") {
    // ── XINPUT_GAMEPAD — 12 bytes ────────────────────────────────

    /// Raw pad state. Thumb-stick axes are i16 (–32768..+32767).
    /// Triggers are u8 (0..255). Apply a deadzone before reading sticks.
    pub struct XINPUT_GAMEPAD {
        wButtons:      u16    // GamepadButton.*
        bLeftTrigger:  u8
        bRightTrigger: u8
        sThumbLX:      i16
        sThumbLY:      i16
        sThumbRX:      i16
        sThumbRY:      i16
    }

    // ── XINPUT_STATE — 16 bytes ──────────────────────────────────

    /// Result of `XInputGetState`. `dwPacketNumber` only increments
    /// when the pad state changes; useful to detect "no input since last poll".
    pub struct XINPUT_STATE {
        dwPacketNumber: u32
        Gamepad:        XINPUT_GAMEPAD
    }

    // ── XINPUT_VIBRATION — 4 bytes ───────────────────────────────

    /// Motor speeds: 0 = off, 0xFFFF = full speed.
    /// Left motor = low-frequency rumble; right motor = high-frequency.
    pub struct XINPUT_VIBRATION {
        wLeftMotorSpeed:  u16
        wRightMotorSpeed: u16
    }

    // ── XINPUT_CAPABILITIES — 20 bytes ───────────────────────────

    pub struct XINPUT_CAPABILITIES {
        Type:      u8     // XInputDeviceType.gamepad
        SubType:   u8     // XInputDeviceSubtype.*
        Flags:     u16    // XINPUT_CAPS_* flags
        Gamepad:   XINPUT_GAMEPAD     // 12 bytes
        Vibration: XINPUT_VIBRATION   // 4 bytes
    }

    // ── XINPUT_BATTERY_INFORMATION — 2 bytes ─────────────────────

    pub struct XINPUT_BATTERY_INFORMATION {
        BatteryType:  u8   // XInputBatteryType.*
        BatteryLevel: u8   // XInputBatteryLevel.*
    }

    // ── XINPUT_KEYSTROKE — 8 bytes ───────────────────────────────

    /// Keyboard-style virtual key events from the gamepad.
    pub struct XINPUT_KEYSTROKE {
        VirtualKey: u16
        Unicode:    u16
        Flags:      u16    // XINPUT_KEYSTROKE_KEYDOWN/UP/REPEAT
        UserIndex:  u8
        HidCode:    u8
    }

    // ── State / vibration ─────────────────────────────────────────

    /// Reads the current state of controller `dwUserIndex` (0..3).
    /// Returns 0 (ERROR_SUCCESS) on success or 1167 (ERROR_DEVICE_NOT_CONNECTED).
    @lib pub fn XInputGetState(
        dwUserIndex: u32,
        pState:      *XINPUT_STATE
    ): u32

    /// Sets the vibration motor speeds of controller `dwUserIndex`.
    @lib pub fn XInputSetState(
        dwUserIndex: u32,
        pVibration:  *XINPUT_VIBRATION
    ): u32

    /// Returns the static capabilities of the connected controller.
    /// `dwFlags`: 0 = any device, 1 (XINPUT_FLAG_GAMEPAD) = gamepads only.
    @lib pub fn XInputGetCapabilities(
        dwUserIndex:  u32,
        dwFlags:      u32,
        pCapabilities: *XINPUT_CAPABILITIES
    ): u32

    /// Returns battery info for either the gamepad or its headset.
    @lib pub fn XInputGetBatteryInformation(
        dwUserIndex:    u32,
        devType:        u8,    // XInputBatteryDevice.*
        pBatteryInformation: *XINPUT_BATTERY_INFORMATION
    ): u32

    /// Returns a single keystroke event (controller-button as keyboard key).
    /// Returns ERROR_EMPTY (4306) when no event is queued.
    @lib pub fn XInputGetKeystroke(
        dwUserIndex: u32,
        dwReserved:  u32,
        pKeystroke:  *XINPUT_KEYSTROKE
    ): u32

    /// Suspends or resumes reporting from all XInput devices.
    /// `enable`: 0 = disable, non-zero = enable. Disable when the
    /// app loses focus to free up bandwidth.
    @lib pub fn XInputEnable(enable: i32)
}
