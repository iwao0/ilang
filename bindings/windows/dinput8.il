/// dinput8.dll — DirectInput 8.
///
/// Legacy controller/keyboard/mouse input API.  For modern code on
/// Xbox-style gamepads, prefer XInput (xinput1_4.il).  DirectInput
/// is still useful for non-XInput devices (flight sticks, racing
/// wheels, custom HID controllers).
///
/// All real work is done through COM interfaces (IDirectInput8,
/// IDirectInputDevice8). The flat function `DirectInput8Create`
/// returns an IDirectInput8 interface pointer to bootstrap.

// ─── DI_ HRESULT codes (selected) ────────────────────────────────
pub enum DiResult: i32 {
    ok             = 0
    polledDevice   = 0x00000002
    notAcquired    = -2147024891   // 0x80070005 E_ACCESSDENIED-ish
    inputLost      = -2005401590   // 0x8007001E (DIERR_INPUTLOST)
    notInitialized = -2147467259   // E_FAIL-ish
    invalidParam   = -2147024809   // E_INVALIDARG
    outOfMemory    = -2147024882   // E_OUTOFMEMORY
    unsupported    = -2147467263   // E_NOTIMPL
}

// ─── Cooperative-level flags (DISCL_*) ───────────────────────────
@flags
pub enum DiCooperative: u32 {
    exclusive    = 0x00000001
    nonexclusive = 0x00000002
    foreground   = 0x00000004
    background   = 0x00000008
    noWinkey     = 0x00000010
}

// ─── EnumDevices device-type filters (DI8DEVCLASS_*) ─────────────
pub enum DiDeviceClass: u32 {
    all          = 0
    device       = 1   // generic
    pointer      = 2   // mouse / tablet
    keyboard     = 3
    gameCtrl     = 4
}

// ─── EnumDevices dwFlags (DIEDFL_*) ──────────────────────────────
@flags
pub enum DiEnumFlags: u32 {
    allDevices       = 0x00000000
    attachedOnly     = 0x00000001
    forceFeedback    = 0x00000100
    includeAliases   = 0x00010000
    includePhantoms  = 0x00020000
    includeHidden    = 0x00040000
}

// ─── Joystick state object offsets for DIJOYSTATE2 ───────────────
// Use these constants as the `dwObj` parameter to GetObjectInfo:
//   DIJOFS_X      = 0
//   DIJOFS_Y      = 4
//   DIJOFS_Z      = 8
//   DIJOFS_RX     = 12
//   DIJOFS_RY     = 16
//   DIJOFS_RZ     = 20
//   DIJOFS_SLIDER(n) = 24 + 4*n
//   DIJOFS_POV(n)    = 32 + 4*n
//   DIJOFS_BUTTON(n) = 48 + n

@extern(C, "dinput8") {
    /// Top-level DirectInput8 interface (IDirectInput8A).
    pub struct IDirectInput8 {}

    /// Per-device interface (IDirectInputDevice8A).
    pub struct IDirectInputDevice8 {}

    // ── DIJOYSTATE2 — 272 bytes ──────────────────────────────────

    /// Generic joystick state used with `c_dfDIJoystick2`.
    /// Axis values: signed integers, range depends on per-axis
    /// `SetProperty(DIPROP_RANGE)` (default −32768..+32767).
    pub struct DIJOYSTATE2 {
        lX:           i32
        lY:           i32
        lZ:           i32
        lRx:          i32
        lRy:          i32
        lRz:          i32
        rglSlider0:   i32
        rglSlider1:   i32
        rgdwPOV0:     u32      // 0xFFFFFFFF = centred, else 0–35999 (×100°)
        rgdwPOV1:     u32
        rgdwPOV2:     u32
        rgdwPOV3:     u32
        // 128 buttons, one u8 each (high bit = pressed)
        rgbButtons0:  u64
        rgbButtons1:  u64
        rgbButtons2:  u64
        rgbButtons3:  u64
        rgbButtons4:  u64
        rgbButtons5:  u64
        rgbButtons6:  u64
        rgbButtons7:  u64
        rgbButtons8:  u64
        rgbButtons9:  u64
        rgbButtonsa:  u64
        rgbButtonsb:  u64
        rgbButtonsc:  u64
        rgbButtonsd:  u64
        rgbButtonse:  u64
        rgbButtonsf:  u64
        // Acceleration / force feedback fields
        lVX:          i32
        lVY:          i32
        lVZ:          i32
        lVRx:         i32
        lVRy:         i32
        lVRz:         i32
        rglVSlider0:  i32
        rglVSlider1:  i32
        lAX:          i32
        lAY:          i32
        lAZ:          i32
        lARx:         i32
        lARy:         i32
        lARz:         i32
        rglASlider0:  i32
        rglASlider1:  i32
        lFX:          i32
        lFY:          i32
        lFZ:          i32
        lFRx:         i32
        lFRy:         i32
        lFRz:         i32
        rglFSlider0:  i32
        rglFSlider1:  i32
    }

    // ── DIMOUSESTATE2 — 20 bytes ─────────────────────────────────

    /// Mouse state for the standard mouse2 format (8 buttons).
    pub struct DIMOUSESTATE2 {
        lX:        i32
        lY:        i32
        lZ:        i32         // wheel delta
        rgbButtons0: u8
        rgbButtons1: u8
        rgbButtons2: u8
        rgbButtons3: u8
        rgbButtons4: u8
        rgbButtons5: u8
        rgbButtons6: u8
        rgbButtons7: u8
    }

    // ── Bootstrap ─────────────────────────────────────────────────

    /// Creates the DirectInput8 interface.
    /// `hInstance`: module handle of the calling EXE.
    /// `dwVersion`: 0x0800 for DirectInput 8.
    /// `riidltf`: IID_IDirectInput8A.
    /// `punkOuter`: NULL.
    @lib pub fn DirectInput8Create(
        hInstance:   *void,    // *HINSTANCE
        dwVersion:   u32,
        riidltf:     *const void,    // *IID
        ppvOut:      **IDirectInput8,
        punkOuter:   *void
    ): i32
}
