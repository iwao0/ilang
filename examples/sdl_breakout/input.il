// Per-frame input snapshot + a small `Input` class that polls
// keyboard / gamepad once and exposes the result through plain
// fields. Pulling everything through a snapshot means the rest of
// the loop reads input once per frame regardless of how many sites
// reference each button — and keeps `Game.run()` from having to
// thread `gamepad` / `Keyboard` calls all over the place.

use sdl

/// One frame's input. `key*Held` are true while the key is held;
/// `key*Down` fire once on the rising edge (rendered + consumed
/// the same frame); `padLx` is the raw left-stick X axis value
/// (`[-32768, 32767]`).
class InputSnapshot {
    padLx: i32
    padDpadL: bool
    padDpadR: bool
    padA: bool
    padStart: bool
    padBack: bool
    keyEscDown: bool
    keyFDown: bool
    keyRDown: bool
    keyLeft: bool
    keyRight: bool
    keyA: bool
    keyD: bool
    keySpaceDown: bool
}

class Input {
    gamepad: sdl.Gamepad
    keyboard: sdl.Keyboard
    init() {
        // Open the first attached gamepad if any. `gamepad.handle == 0`
        // when nothing is plugged in — every poll degrades to
        // returning 0/false, so the keyboard path keeps working.
        this.gamepad = new sdl.Gamepad()
        this.keyboard = new sdl.Keyboard()
        return
    }

    /// Advance the OS event queue, snapshot the previous-frame
    /// keyboard state, and read keyboard + gamepad state for the
    /// current frame. Call once per frame — Game.run() should not
    /// call `sdl.pumpEvents()` separately.
    poll(): InputSnapshot {
        // Snapshot previous-frame keyboard state BEFORE
        // `sdl.pumpEvents()` mutates SDL's internal buffer; then
        // pump events so subsequent reads see the new state.
        this.keyboard.pump()
        sdl.pumpEvents()
        let s = new InputSnapshot()
        s.padLx = this.gamepad.getAxis(sdl.Axis.leftX) as i32
        s.padDpadL = this.gamepad.isPressed(sdl.Button.dpadLeft)
        s.padDpadR = this.gamepad.isPressed(sdl.Button.dpadRight)
        s.padA = this.gamepad.isPressed(sdl.Button.a)
        s.padStart = this.gamepad.isPressed(sdl.Button.start)
        s.padBack = this.gamepad.isPressed(sdl.Button.back)
        s.keyEscDown = this.keyboard.justPressed(sdl.Scancode.escape)
        s.keyFDown = this.keyboard.justPressed(sdl.Scancode.f)
        s.keyRDown = this.keyboard.justPressed(sdl.Scancode.r)
        s.keyLeft = this.keyboard.isPressed(sdl.Scancode.left)
        s.keyRight = this.keyboard.isPressed(sdl.Scancode.right)
        s.keyA = this.keyboard.isPressed(sdl.Scancode.a)
        s.keyD = this.keyboard.isPressed(sdl.Scancode.d)
        s.keySpaceDown = this.keyboard.justPressed(sdl.Scancode.space)
        s
    }
}
