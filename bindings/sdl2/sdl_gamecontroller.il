// SDL2 game-controller bindings: opens the first attached
// controller, exposes axis + button polling. Initialise SDL with
// `InitFlag.gameController` (and ideally `InitFlag.joystick`) before
// instantiating `GameController`.

use sdl_core { SDL_free }

/// `SDL_GameControllerButton` — face / shoulder / d-pad / stick-press
/// buttons that follow the standard layout.
pub enum Button: i32 {
    a              = 0
    b              = 1
    x              = 2
    y              = 3
    back           = 4
    guide          = 5
    start          = 6
    leftStick      = 7
    rightStick     = 8
    leftShoulder   = 9
    rightShoulder  = 10
    dpadUp         = 11
    dpadDown       = 12
    dpadLeft       = 13
    dpadRight      = 14
}

/// `SDL_GameControllerAxis`. Stick axes range `[-32768, 32767]`;
/// trigger axes range `[0, 32767]`.
pub enum Axis: i32 {
    leftX        = 0
    leftY        = 1
    rightX       = 2
    rightY       = 3
    triggerLeft  = 4
    triggerRight = 5
}

/// `SDL_GameControllerType` — what kind of pad SDL identified the
/// device as. Returned by `GameController.controllerType()`.
pub enum ControllerType: i32 {
    unknown                = 0
    xbox360                = 1
    xboxOne                = 2
    ps3                    = 3
    ps4                    = 4
    nintendoSwitchPro      = 5
    virtual                = 6
    ps5                    = 7
    amazonLuna             = 8
    googleStadia           = 9
    nvidiaShield           = 10
    nintendoSwitchJoyConLeft  = 11
    nintendoSwitchJoyConRight = 12
    nintendoSwitchJoyConPair  = 13
}

@extern(C) {
    /// Opaque SDL handle.
    pub struct SDL_GameController {}

    @lib("SDL2") pub fn SDL_NumJoysticks(): i32
    @lib("SDL2") pub fn SDL_IsGameController(joystick_index: i32): i32
    @lib("SDL2") pub fn SDL_GameControllerOpen(joystick_index: i32): *SDL_GameController
    @lib("SDL2") pub fn SDL_GameControllerClose(controller: *SDL_GameController)
    @lib("SDL2") pub fn SDL_GameControllerGetAxis(
        controller: *SDL_GameController, axis: i32
    ): i16
    @lib("SDL2") pub fn SDL_GameControllerGetButton(
        controller: *SDL_GameController, button: i32
    ): u8
    // ─── identification ──────────────────────────────────────────
    @lib("SDL2") pub fn SDL_GameControllerName(
        controller: *SDL_GameController
    ): *const char
    @lib("SDL2") pub fn SDL_GameControllerNameForIndex(
        joystick_index: i32
    ): *const char
    @lib("SDL2") pub fn SDL_GameControllerGetAttached(
        controller: *SDL_GameController
    ): u8
    /// Returns the underlying `SDL_Joystick *` as an opaque
    /// pointer — callers that need the raw joystick API cast it to
    /// `*sdl_joystick.SDL_Joystick`.
    @lib("SDL2") pub fn SDL_GameControllerGetJoystick(
        controller: *SDL_GameController
    ): *void
    @lib("SDL2") pub fn SDL_GameControllerGetType(
        controller: *SDL_GameController
    ): i32
    @lib("SDL2") pub fn SDL_GameControllerGetSerial(
        controller: *SDL_GameController
    ): *const char

    // ─── mappings ────────────────────────────────────────────────
    // The two `Mapping*` getters return malloc'd strings the caller
    // must `SDL_free` — wrappers below copy through `stringFromCstr`
    // and free the source so callers see plain ilang strings.
    @lib("SDL2") pub fn SDL_GameControllerMapping(
        controller: *SDL_GameController
    ): *const char
    @lib("SDL2") pub fn SDL_GameControllerMappingForIndex(
        joystick_index: i32
    ): *const char
    @lib("SDL2") pub fn SDL_GameControllerAddMapping(mapping: *const char): i32
    @lib("SDL2") pub fn SDL_GameControllerNumMappings(): i32

    // ─── capability probes ───────────────────────────────────────
    @lib("SDL2") pub fn SDL_GameControllerHasButton(
        controller: *SDL_GameController, button: i32
    ): u8
    @lib("SDL2") pub fn SDL_GameControllerHasAxis(
        controller: *SDL_GameController, axis: i32
    ): u8
    @lib("SDL2") pub fn SDL_GameControllerHasRumble(
        controller: *SDL_GameController
    ): u8

    // ─── event delivery + manual update ──────────────────────────
    @lib("SDL2") pub fn SDL_GameControllerEventState(state: i32): i32
    @lib("SDL2") pub fn SDL_GameControllerUpdate()

    // ─── rumble / LED ────────────────────────────────────────────
    @lib("SDL2") pub fn SDL_GameControllerRumble(
        controller: *SDL_GameController,
        low_frequency_rumble: u16, high_frequency_rumble: u16,
        duration_ms: u32
    ): i32
    @lib("SDL2") pub fn SDL_GameControllerRumbleTriggers(
        controller: *SDL_GameController,
        left_rumble: u16, right_rumble: u16,
        duration_ms: u32
    ): i32
    @lib("SDL2") pub fn SDL_GameControllerSetLED(
        controller: *SDL_GameController, red: u8, green: u8, blue: u8
    ): i32

    // ─── enum ↔ string helpers ───────────────────────────────────
    @lib("SDL2") pub fn SDL_GameControllerGetStringForAxis(
        axis: i32
    ): *const char
    @lib("SDL2") pub fn SDL_GameControllerGetStringForButton(
        button: i32
    ): *const char
    @lib("SDL2") pub fn SDL_GameControllerGetAxisFromString(
        s: *const char
    ): i32
    @lib("SDL2") pub fn SDL_GameControllerGetButtonFromString(
        s: *const char
    ): i32

    /// ARC-managed wrapper around the first available controller.
    /// `handle == 0` means no controller was attached at the time of
    /// construction; in that case every poll degrades to returning
    /// `0` / `false`.
    ///
    /// Edge detection (`isPressed` / `isReleased`) requires
    /// `pump()` to be called once per frame before the polling
    /// methods, so the previous-frame snapshot is up to date.
    pub class GameController {
        /// Pointer to the underlying `SDL_GameController`, cast to
        /// `i64`. `0` when no device is open.
        pub handle: i64
        /// Snapshot of every button's held / not-held state at the
        /// previous `pump()`. 15 slots — one per `Button` variant.
        pub prev: u8[]
        /// Snapshot at the most recent `pump()`. `isHeld` /
        /// `isPressed` / `isReleased` read this — never the live
        /// SDL state — so unrelated SDL calls between frames can't
        /// race with edge detection.
        pub curr: u8[]

        /// Open the first attached controller. Sets `handle` to `0`
        /// if none of the connected joysticks are recognised as game
        /// controllers (e.g. a steering wheel or a HID device with
        /// no SDL mapping).
        pub init() {
            this.handle = 0
            // SDL_CONTROLLER_BUTTON_MAX = 15 (matches the `Button`
            // enum, `a` … `dpadRight`).
            let p: u8[] = []
            let c: u8[] = []
            let i: i32 = 0
            while i < 15 {
                p.push(0)
                c.push(0)
                i += 1
            }
            this.prev = p
            this.curr = c
            let n = SDL_NumJoysticks()
            let j: i32 = 0
            while j < n {
                if SDL_IsGameController(j) != 0 {
                    let h = SDL_GameControllerOpen(j)
                    this.handle = h as i64
                    return
                }
                j += 1
            }
        }

        pub deinit() {
            if this.handle != 0 {
                SDL_GameControllerClose(this.handle as *SDL_GameController)
            }
        }

        /// Roll `curr → prev` and snapshot the latest button state
        /// into `curr`. Call once per frame after `sdl.pumpEvents()`
        /// (which keeps SDL's joystick state fresh); subsequent
        /// `isHeld` / `isPressed` / `isReleased` queries this
        /// snapshot, not SDL's live state.
        pub pump() {
            let i: i32 = 0
            while i < 15 {
                this.prev[i as i64] = this.curr[i as i64]
                i += 1
            }
            if this.handle == 0 {
                let z: i32 = 0
                while z < 15 {
                    this.curr[z as i64] = 0 as u8
                    z += 1
                }
                return
            }
            let j: i32 = 0
            while j < 15 {
                this.curr[j as i64] = SDL_GameControllerGetButton(
                    this.handle as *SDL_GameController, j
                )
                j += 1
            }
        }

        /// Read the current value of a stick / trigger axis.
        ///
        /// Args:
        ///   axis — the axis to read.
        ///
        /// Returns:
        ///   stick axes in `[-32768, 32767]`; trigger axes in
        ///   `[0, 32767]`. `0` when no controller is open.
        pub getAxis(axis: Axis): i16 {
            if this.handle == 0 { return 0 as i16 }
            SDL_GameControllerGetAxis(
                this.handle as *SDL_GameController, axis as i32
            )
        }

        /// Returns:
        ///   true while the button was held at the most recent
        ///   `pump()`. `false` before the first `pump()` or when
        ///   `button` is out of range.
        pub isHeld(button: Button): bool {
            let b: i32 = button as i32
            if b < 0 || b >= 15 {
                return false
            }
            this.curr[b as i64] != 0 as u8
        }

        /// Returns:
        ///   true on the single frame the button transitions from
        ///   "up" to "down" — rising edge.
        pub isPressed(button: Button): bool {
            let b: i32 = button as i32
            if b < 0 || b >= 15 {
                return false
            }
            this.curr[b as i64] != 0 as u8 && this.prev[b as i64] == 0 as u8
        }

        /// Returns:
        ///   true on the single frame the button transitions from
        ///   "down" to "up" — falling edge.
        pub isReleased(button: Button): bool {
            let b: i32 = button as i32
            if b < 0 || b >= 15 {
                return false
            }
            this.prev[b as i64] != 0 as u8 && this.curr[b as i64] == 0 as u8
        }

        /// Returns:
        ///   the controller name SDL has for this device (e.g.
        ///   `"Xbox 360 Controller"`), or empty when no device is
        ///   open.
        pub name(): string {
            if this.handle == 0 { return "" }
            stringFromCstr(SDL_GameControllerName(
                this.handle as *SDL_GameController
            ))
        }

        /// Returns:
        ///   true while the device is still plugged in. False after
        ///   the user unplugs the controller (the open handle becomes
        ///   stale, but `pump()` keeps returning zeros so it's safe
        ///   to keep calling).
        pub isAttached(): bool {
            if this.handle == 0 { return false }
            SDL_GameControllerGetAttached(
                this.handle as *SDL_GameController
            ) != 0 as u8
        }

        /// Returns:
        ///   true if SDL has a hardware mapping for this `button` on
        ///   this controller. Some pads (e.g. PS3 with no `guide`)
        ///   miss specific buttons — callers should gate visual
        ///   prompts on this.
        pub hasButton(button: Button): bool {
            if this.handle == 0 { return false }
            SDL_GameControllerHasButton(
                this.handle as *SDL_GameController, button as i32
            ) != 0 as u8
        }

        /// Returns:
        ///   true if SDL has a hardware mapping for this `axis`.
        pub hasAxis(axis: Axis): bool {
            if this.handle == 0 { return false }
            SDL_GameControllerHasAxis(
                this.handle as *SDL_GameController, axis as i32
            ) != 0 as u8
        }

        /// Returns:
        ///   true if the device supports rumble. Required for
        ///   `rumble()` to do anything visible.
        pub hasRumble(): bool {
            if this.handle == 0 { return false }
            SDL_GameControllerHasRumble(
                this.handle as *SDL_GameController
            ) != 0 as u8
        }

        /// Trigger a rumble effect.
        ///
        /// Args:
        ///   lowFreq      — left motor magnitude, `0..65535`.
        ///   highFreq     — right motor magnitude, `0..65535`.
        ///   durationMs   — effect length in milliseconds. `0` stops
        ///                  rumble immediately.
        ///
        /// Returns:
        ///   `0` on success, `< 0` if the device doesn't support
        ///   rumble or no controller is open.
        pub rumble(lowFreq: u16, highFreq: u16, durationMs: u32): i32 {
            if this.handle == 0 { return -1 }
            SDL_GameControllerRumble(
                this.handle as *SDL_GameController,
                lowFreq, highFreq, durationMs
            )
        }

        /// Trigger a trigger-rumble effect (only some pads support
        /// this — Xbox Series, DualSense, etc.).
        ///
        /// Args / Returns: same shape as `rumble`.
        pub rumbleTriggers(left: u16, right: u16, durationMs: u32): i32 {
            if this.handle == 0 { return -1 }
            SDL_GameControllerRumbleTriggers(
                this.handle as *SDL_GameController,
                left, right, durationMs
            )
        }

        /// Set the LED colour on devices that have one (DualShock 4,
        /// DualSense, etc.).
        ///
        /// Args:
        ///   r, g, b — colour components.
        ///
        /// Returns:
        ///   `0` on success, `< 0` if the device has no LED or no
        ///   controller is open.
        pub setLED(r: u8, g: u8, b: u8): i32 {
            if this.handle == 0 { return -1 }
            SDL_GameControllerSetLED(
                this.handle as *SDL_GameController, r, g, b
            )
        }

        /// Returns:
        ///   the SDL-identified hardware family. `unknown` when the
        ///   pad isn't in SDL's database.
        pub controllerType(): ControllerType {
            if this.handle == 0 { return ControllerType.unknown }
            let raw = SDL_GameControllerGetType(
                this.handle as *SDL_GameController
            )
            raw as ControllerType
        }

        /// Returns:
        ///   the device's serial number (typically empty unless the
        ///   pad reports one — DualSense / Switch Pro do).
        pub serial(): string {
            if this.handle == 0 { return "" }
            stringFromCstr(SDL_GameControllerGetSerial(
                this.handle as *SDL_GameController
            ))
        }

        /// Returns:
        ///   the SDL2_gamecontrollerdb-format mapping line for the
        ///   open device, or empty when no controller is attached.
        ///   The caller never frees — we copy through ilang and
        ///   `SDL_free` the source string.
        pub mapping(): string {
            if this.handle == 0 { return "" }
            let raw = SDL_GameControllerMapping(
                this.handle as *SDL_GameController
            )
            if raw as i64 == 0 { return "" }
            let copy = stringFromCstr(raw)
            SDL_free(raw as *const void)
            copy
        }

        // ─── static helpers ──────────────────────────────────────

        /// Args:
        ///   joystickIndex — `[0, sdl.numJoysticks())` index.
        ///
        /// Returns:
        ///   the controller name SDL would report for that index,
        ///   or empty when the index is out of range.
        pub static nameForIndex(joystickIndex: i32): string {
            stringFromCstr(SDL_GameControllerNameForIndex(joystickIndex))
        }

        /// Add a SDL2 controller mapping string. Format is
        /// described in the SDL_GameControllerDB project. Use it
        /// to inject custom pads at runtime when SDL's bundled
        /// database doesn't cover them.
        ///
        /// Args:
        ///   mapping — full mapping line, e.g.
        ///             `"<guid>,<name>,a:b0,b:b1,..."`.
        ///
        /// Returns:
        ///   `1` if a new mapping was added, `0` if an existing
        ///   mapping was replaced, `-1` on error.
        pub static addMapping(mapping: string): i32 {
            SDL_GameControllerAddMapping(cstrFromString(mapping))
        }

        /// Returns:
        ///   how many controller mappings are currently loaded
        ///   (built-in + any added via `addMapping`).
        pub static numMappings(): i32 {
            SDL_GameControllerNumMappings()
        }

        /// Read the mapping line SDL would use for the device at
        /// `joystickIndex` *without* opening it. Useful for
        /// picking a specific pad out of the attached set
        /// (`numJoysticks()` → inspect each → open the one you
        /// want).
        ///
        /// Returns:
        ///   mapping string, or empty when the index doesn't
        ///   match a game controller. Internally `SDL_free`'s the
        ///   malloc'd buffer SDL hands us.
        pub static mappingForIndex(joystickIndex: i32): string {
            let raw = SDL_GameControllerMappingForIndex(joystickIndex)
            if raw as i64 == 0 { return "" }
            let copy = stringFromCstr(raw)
            SDL_free(raw as *const void)
            copy
        }

        /// Toggle whether `SDL_PollEvent` / `WaitEvent` deliver
        /// controller events, or query the current state.
        ///
        /// Args:
        ///   state — `enable`, `ignore`, or `query` (use the
        ///           `EventState` enum from `sdl_time.il`).
        ///
        /// Returns:
        ///   the state in effect after the call.
        pub static eventState(state: i32): i32 {
            SDL_GameControllerEventState(state)
        }

        /// Force SDL to poll the joystick subsystem and refresh
        /// its view of every controller. Normally `pumpEvents()`
        /// does this — only call it when running with controller
        /// events disabled.
        pub static updateAll() {
            SDL_GameControllerUpdate()
        }

        /// Args:
        ///   axis — Axis variant.
        ///
        /// Returns:
        ///   SDL's canonical name for `axis` (`"leftx"`,
        ///   `"righty"`, …).
        pub static stringForAxis(axis: Axis): string {
            stringFromCstr(SDL_GameControllerGetStringForAxis(axis as i32))
        }

        /// Args:
        ///   button — Button variant.
        ///
        /// Returns:
        ///   SDL's canonical name for `button` (`"a"`, `"dpup"`, …).
        pub static stringForButton(button: Button): string {
            stringFromCstr(SDL_GameControllerGetStringForButton(button as i32))
        }

        /// Inverse of `stringForAxis`. Returns `-1` when `s`
        /// doesn't match any known axis.
        pub static axisFromString(s: string): i32 {
            SDL_GameControllerGetAxisFromString(cstrFromString(s))
        }

        /// Inverse of `stringForButton`. Returns `-1` when `s`
        /// doesn't match any known button.
        pub static buttonFromString(s: string): i32 {
            SDL_GameControllerGetButtonFromString(cstrFromString(s))
        }
    }
}
