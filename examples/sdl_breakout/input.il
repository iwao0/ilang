// Per-frame input snapshot + a small `Input` class that polls
// keyboard / gamepad once and exposes the result through plain
// fields. Pulling everything through a snapshot means the rest of
// the loop reads input once per frame regardless of how many sites
// reference each button — and keeps `Game.run()` from having to
// thread `gamepad` / `Keyboard` calls all over the place.

use sdl

/// One frame's input. `*Held` are true while the key / button is
/// held; `*Down` fire once on the rising edge (rendered + consumed
/// the same frame); `padLx` is the raw left-stick X axis value
/// (`[-32768, 32767]`).
pub class InputSnapshot {
    pub padLx: i32
    pub padDpadL: bool
    pub padDpadR: bool
    pub padADown: bool
    pub padStartDown: bool
    pub padBackDown: bool
    pub keyEscDown: bool
    pub keyFDown: bool
    pub keyRDown: bool
    pub keyLeft: bool
    pub keyRight: bool
    pub keyA: bool
    pub keyD: bool
    pub keySpaceDown: bool
}

pub class Input {
    pub gamepad: sdl.Gamepad
    pub keyboard: sdl.Keyboard
    pub init() {
        // Open the first attached gamepad if any. `gamepad.handle == 0`
        // when nothing is plugged in — every poll degrades to
        // returning 0/false, so the keyboard path keeps working.
        this.gamepad = new sdl.Gamepad()
        this.keyboard = new sdl.Keyboard()
        return
    }

    /// Advance the OS event queue, snapshot the previous-frame
    /// keyboard / gamepad state, and read keyboard + gamepad state
    /// for the current frame. Call once per frame — Game.run()
    /// should not call `sdl.pumpEvents()` separately.
    pub poll(): InputSnapshot {
        // Snapshot previous-frame state BEFORE `sdl.pumpEvents()`
        // mutates SDL's internal buffers; then pump events so
        // subsequent reads see the new state.
        this.keyboard.pump()
        this.gamepad.pump()
        sdl.pumpEvents()
        let s = new InputSnapshot()
        s.padLx = this.gamepad.getAxis(sdl.Axis.leftX) as i32
        s.padDpadL = this.gamepad.isHeld(sdl.Button.dpadLeft)
        s.padDpadR = this.gamepad.isHeld(sdl.Button.dpadRight)
        s.padADown = this.gamepad.isPressed(sdl.Button.a)
        s.padStartDown = this.gamepad.isPressed(sdl.Button.start)
        s.padBackDown = this.gamepad.isPressed(sdl.Button.back)
        s.keyEscDown = this.keyboard.isPressed(sdl.Scancode.escape)
        s.keyFDown = this.keyboard.isPressed(sdl.Scancode.f)
        s.keyRDown = this.keyboard.isPressed(sdl.Scancode.r)
        s.keyLeft = this.keyboard.isHeld(sdl.Scancode.left)
        s.keyRight = this.keyboard.isHeld(sdl.Scancode.right)
        s.keyA = this.keyboard.isHeld(sdl.Scancode.a)
        s.keyD = this.keyboard.isHeld(sdl.Scancode.d)
        s.keySpaceDown = this.keyboard.isPressed(sdl.Scancode.space)
        s
    }
}
