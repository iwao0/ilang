// Per-frame input snapshot + a small `Input` class that polls
// keyboard / gamepad once and exposes the result through plain
// fields. Pulling everything through a snapshot means the rest of
// the loop reads input once per frame regardless of how many sites
// reference each button — and keeps `Game.run()` from having to
// thread `gamepad` / `sdl.Keyboard` calls all over the place.

use sdl

/// One frame's input. Booleans are true when the key / button is
/// currently held; `padLx` is the raw left-stick X axis value
/// (`[-32768, 32767]`).
class InputSnapshot {
    padLx: i32
    padDpadL: bool
    padDpadR: bool
    padA: bool
    padStart: bool
    padBack: bool
    keyEsc: bool
    keyF: bool
    keyR: bool
    keyLeft: bool
    keyRight: bool
    keyA: bool
    keyD: bool
    keySpace: bool
}

class Input {
    gamepad: sdl.Gamepad
    init() {
        // Open the first attached gamepad if any. `gamepad.handle == 0`
        // when nothing is plugged in — every poll degrades to
        // returning 0/false, so the keyboard path keeps working.
        this.gamepad = new sdl.Gamepad()
        return
    }

    /// Read keyboard + gamepad state for the current frame.
    poll(): InputSnapshot {
        let s = new InputSnapshot()
        s.padLx = this.gamepad.getAxis(sdl.Axis.leftX) as i32
        s.padDpadL = this.gamepad.isPressed(sdl.Button.dpadLeft)
        s.padDpadR = this.gamepad.isPressed(sdl.Button.dpadRight)
        s.padA = this.gamepad.isPressed(sdl.Button.a)
        s.padStart = this.gamepad.isPressed(sdl.Button.start)
        s.padBack = this.gamepad.isPressed(sdl.Button.back)
        s.keyEsc = sdl.Keyboard.isPressed(sdl.Scancode.escape)
        s.keyF = sdl.Keyboard.isPressed(sdl.Scancode.f)
        s.keyR = sdl.Keyboard.isPressed(sdl.Scancode.r)
        s.keyLeft = sdl.Keyboard.isPressed(sdl.Scancode.left)
        s.keyRight = sdl.Keyboard.isPressed(sdl.Scancode.right)
        s.keyA = sdl.Keyboard.isPressed(sdl.Scancode.a)
        s.keyD = sdl.Keyboard.isPressed(sdl.Scancode.d)
        s.keySpace = sdl.Keyboard.isPressed(sdl.Scancode.space)
        s
    }
}
