// SDL2 raw joystick API — for devices SDL doesn't recognise as a
// game controller (steering wheels, flight sticks, custom HID
// devices). Use `sdl_gamepad.il` instead for standard
// xbox/ps/switch-style pads; this file is the lower-level layer.
// Initialise SDL with `InitFlag.joystick` before use.

@extern(C) {
    /// Opaque SDL_Joystick — created by `Joystick.open(index)`,
    /// destroyed by `deinit`.
    pub struct SDL_Joystick {}

    /// Mirrors C `SDL_JoystickGUID` — 16 raw bytes that identify
    /// a joystick across reboots / OS reinstalls (USB VID/PID +
    /// driver-specific salt). Index `b[0..16]` to read individual
    /// bytes; usually you'll just round-trip through
    /// `Joystick.guidToString` / `Joystick.guidFromString` rather
    /// than touch the bytes directly.
    pub struct JoystickGUID {
        b: u8[16]
    }

    // (`SDL_NumJoysticks` is declared in `sdl_gamepad.il` so it
    // re-exports under the umbrella exactly once.)
    @lib("SDL2") pub fn SDL_JoystickOpen(device_index: i32): *SDL_Joystick
    @lib("SDL2") pub fn SDL_JoystickClose(joystick: *SDL_Joystick)
    @lib("SDL2") pub fn SDL_JoystickGetAttached(joystick: *SDL_Joystick): u8

    @lib("SDL2") pub fn SDL_JoystickName(joystick: *SDL_Joystick): *const char
    @lib("SDL2") pub fn SDL_JoystickNameForIndex(device_index: i32): *const char

    @lib("SDL2") pub fn SDL_JoystickNumAxes(joystick: *SDL_Joystick): i32
    @lib("SDL2") pub fn SDL_JoystickNumButtons(joystick: *SDL_Joystick): i32
    @lib("SDL2") pub fn SDL_JoystickNumHats(joystick: *SDL_Joystick): i32
    @lib("SDL2") pub fn SDL_JoystickNumBalls(joystick: *SDL_Joystick): i32

    @lib("SDL2") pub fn SDL_JoystickGetAxis(joystick: *SDL_Joystick, axis: i32): i16
    @lib("SDL2") pub fn SDL_JoystickGetButton(joystick: *SDL_Joystick, button: i32): u8
    @lib("SDL2") pub fn SDL_JoystickGetHat(joystick: *SDL_Joystick, hat: i32): u8
    @lib("SDL2") pub fn SDL_JoystickGetBall(
        joystick: *SDL_Joystick, ball: i32, dx: *i32, dy: *i32
    ): i32

    @lib("SDL2") pub fn SDL_JoystickEventState(state: i32): i32
    @lib("SDL2") pub fn SDL_JoystickUpdate()

    @lib("SDL2") pub fn SDL_JoystickRumble(
        joystick: *SDL_Joystick,
        low_frequency_rumble: u16, high_frequency_rumble: u16,
        duration_ms: u32
    ): i32

    // ─── GUID helpers ────────────────────────────────────────────
    @lib("SDL2") pub fn SDL_JoystickGetGUID(
        joystick: *SDL_Joystick
    ): JoystickGUID
    @lib("SDL2") pub fn SDL_JoystickGetDeviceGUID(
        device_index: i32
    ): JoystickGUID
    @lib("SDL2") pub fn SDL_JoystickGetGUIDString(
        guid: JoystickGUID, pszGUID: *char, cbGUID: i32
    )
    @lib("SDL2") pub fn SDL_JoystickGetGUIDFromString(
        pchGUID: *const char
    ): JoystickGUID

    /// ARC-managed wrapper around `SDL_Joystick`. `deinit` closes
    /// the underlying device handle.
    pub class Joystick {
        /// Raw `SDL_Joystick*` cast to i64. `0` when no device is
        /// open (e.g. opening failed).
        pub handle: i64

        /// Open the joystick at `deviceIndex`
        /// (`[0, sdl.numJoysticks())`). Use `nameForIndex` first
        /// to identify the device you want.
        pub init(deviceIndex: i32) {
            this.handle = SDL_JoystickOpen(deviceIndex) as i64
        }

        pub deinit() {
            if this.handle != 0 {
                SDL_JoystickClose(this.handle as *SDL_Joystick)
            }
        }

        /// Returns:
        ///   true while the device is still plugged in.
        pub isAttached(): bool {
            if this.handle == 0 { return false }
            SDL_JoystickGetAttached(this.handle as *SDL_Joystick) != 0 as u8
        }

        /// Returns:
        ///   the device's reported name, or empty when no device is
        ///   open.
        pub name(): string {
            if this.handle == 0 { return "" }
            stringFromCstr(SDL_JoystickName(this.handle as *SDL_Joystick))
        }

        /// Returns:
        ///   number of analog axes the device exposes.
        pub numAxes(): i32 {
            if this.handle == 0 { return 0 }
            SDL_JoystickNumAxes(this.handle as *SDL_Joystick)
        }

        /// Returns:
        ///   number of digital buttons the device exposes.
        pub numButtons(): i32 {
            if this.handle == 0 { return 0 }
            SDL_JoystickNumButtons(this.handle as *SDL_Joystick)
        }

        /// Returns:
        ///   number of D-pad / POV hats the device exposes.
        pub numHats(): i32 {
            if this.handle == 0 { return 0 }
            SDL_JoystickNumHats(this.handle as *SDL_Joystick)
        }

        /// Read one axis. Stick axes range `[-32768, 32767]`;
        /// trigger-style axes range `[0, 32767]`.
        pub getAxis(axis: i32): i16 {
            if this.handle == 0 { return 0 as i16 }
            SDL_JoystickGetAxis(this.handle as *SDL_Joystick, axis)
        }

        /// Read one button.
        ///
        /// Returns:
        ///   `1` while held, `0` otherwise.
        pub getButton(button: i32): u8 {
            if this.handle == 0 { return 0 as u8 }
            SDL_JoystickGetButton(this.handle as *SDL_Joystick, button)
        }

        /// Read one hat (D-pad). Returns one of the
        /// `SDL_HAT_CENTERED` (0) / `_UP` (1) / `_RIGHT` (2) /
        /// `_DOWN` (4) / `_LEFT` (8) values, or a bitwise OR of the
        /// diagonal pairs (e.g. `_RIGHTUP = 3`).
        pub getHat(hat: i32): u8 {
            if this.handle == 0 { return 0 as u8 }
            SDL_JoystickGetHat(this.handle as *SDL_Joystick, hat)
        }

        /// Trigger a rumble effect.
        ///
        /// Args:
        ///   lowFreq    — left motor magnitude, `0..65535`.
        ///   highFreq   — right motor magnitude, `0..65535`.
        ///   durationMs — effect length in milliseconds.
        ///
        /// Returns:
        ///   `0` on success, `< 0` if the device doesn't support
        ///   rumble.
        pub rumble(lowFreq: u16, highFreq: u16, durationMs: u32): i32 {
            if this.handle == 0 { return -1 }
            SDL_JoystickRumble(
                this.handle as *SDL_Joystick,
                lowFreq, highFreq, durationMs
            )
        }

        /// Returns:
        ///   the device's stable 16-byte GUID, or all-zero when no
        ///   device is open. Use `Joystick.guidToString(...)` to
        ///   serialise.
        pub guid(): JoystickGUID {
            if this.handle == 0 {
                return new JoystickGUID()
            }
            SDL_JoystickGetGUID(this.handle as *SDL_Joystick)
        }

        // ─── static helpers ──────────────────────────────────────

        /// Args:
        ///   deviceIndex — `[0, sdl.numJoysticks())`.
        ///
        /// Returns:
        ///   joystick name SDL would report at that index, or empty
        ///   when out of range.
        pub static nameForIndex(deviceIndex: i32): string {
            stringFromCstr(SDL_JoystickNameForIndex(deviceIndex))
        }

        /// Toggle whether `SDL_PollEvent` / `WaitEvent` deliver
        /// joystick events.
        pub static eventState(state: i32): i32 {
            SDL_JoystickEventState(state)
        }

        /// Force SDL to poll the joystick subsystem and refresh
        /// its view of every connected device. Normally
        /// `pumpEvents()` does this.
        pub static updateAll() {
            SDL_JoystickUpdate()
        }

        /// Returns:
        ///   the 33-byte (32 hex chars + NUL) string SDL
        ///   canonicalises `guid` to. Stable across runs /
        ///   platforms — safe to write to disk for "remember last
        ///   controller" features.
        pub static guidToString(guid: JoystickGUID): string {
            // SDL writes a NUL-terminated 33-byte string. Allocate
            // a 33-byte buffer, hand its pointer to SDL, then copy
            // out.
            let buf: u8[] = []
            let i: i32 = 0
            while i < 33 {
                buf.push(0 as u8)
                i = i + 1
            }
            let p: *u8 = buf
            SDL_JoystickGetGUIDString(guid, p as *char, 33)
            stringFromCstr(p as *const char)
        }

        /// Inverse of `guidToString`. Returns an all-zero GUID
        /// when `s` doesn't parse as 32 hex chars.
        pub static guidFromString(s: string): JoystickGUID {
            SDL_JoystickGetGUIDFromString(cstrFromString(s))
        }

        /// Args:
        ///   deviceIndex — `[0, sdl.numJoysticks())`.
        ///
        /// Returns:
        ///   the device's GUID *without* opening it. Pair with
        ///   `guidToString` to identify the pad before deciding
        ///   whether to call `new Joystick(deviceIndex)`.
        pub static deviceGUID(deviceIndex: i32): JoystickGUID {
            SDL_JoystickGetDeviceGUID(deviceIndex)
        }
    }
}
