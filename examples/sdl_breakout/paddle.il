// The player-controlled paddle. Owns the horizontal position, the
// optional WIDER timer, and small helpers for input / clamping. The
// renderer keeps drawing it through a `NeonRenderer.fillStadium`
// call in main.il — that part doesn't fit cleanly as a method (it
// needs both the paddle and the renderer alongside the screen-shake
// offsets, all main-loop state), so we leave drawing where it is.

pub class Paddle {
    // Left-edge x in window pixels. Stored as f64 because analog
    // stick input nudges sub-pixel.
    pub x: f64
    pub y: i32
    // Default ("non-widened") width. The visible width is queried
    // through `width()` and depends on `widerTimer`.
    pub baseW: i32
    pub h: i32
    pub speed: f64
    // Frames remaining for the WIDER power-up. While > 0 the paddle
    // renders 1.5× wide; the previous-width tracking re-anchors `x`
    // around the centre so the paddle grows / shrinks symmetrically
    // rather than growing rightward.
    pub widerTimer: i32
    // Last-frame width — used by `tick` to detect the wider toggle
    // and re-anchor `x`.
    pub prevW: i32

    pub init(x: f64, y: i32, baseW: i32, h: i32, speed: f64) {
        this.x = x
        this.y = y
        this.baseW = baseW
        this.h = h
        this.speed = speed
        this.widerTimer = 0
        this.prevW = baseW
    }

    /// Visible paddle width on this frame (`baseW * 1.5` while
    /// WIDER is active, `baseW` otherwise).
    pub width(): i32 {
        if this.widerTimer > 0 {
            this.baseW + this.baseW / (2 as i32)
        } else {
            this.baseW
        }
    }

    /// Activate / extend the WIDER power-up. `frames` is added to
    /// the existing timer so back-to-back catches stack the effect.
    pub widen(frames: i32) {
        this.widerTimer = this.widerTimer + frames
    }

    /// Per-frame bookkeeping: tick the WIDER timer down, and re-
    /// anchor `x` around the paddle centre when the width changes
    /// so the visible centre doesn't jump.
    pub tick() {
        if this.widerTimer > 0 {
            this.widerTimer = this.widerTimer - 1
        }
        let w = this.width()
        if w != this.prevW {
            let delta = w - this.prevW
            this.x = this.x - (delta as f64) / 2.0
            this.prevW = w
        }
    }

    pub moveLeft() {
        this.x = this.x - this.speed
    }
    pub moveRight() {
        this.x = this.x + this.speed
    }

    /// Apply analog-stick lateral input. `stickX` is SDL's raw
    /// `[-32768, 32767]` value; the standard 24% deadzone applies.
    pub moveAnalog(stickX: i32) {
        let abs: i32 = if stickX < 0 { -stickX } else { stickX }
        if abs > 8000 {
            let norm = (stickX as f64) / 32767.0
            this.x = this.x + norm * this.speed
        }
    }

    /// Clamp `x` so the paddle stays inside `[0, winW - width()]`.
    pub clamp(winW: i32) {
        if this.x < 0.0 {
            this.x = 0.0
        }
        let maxX: f64 = (winW - this.width()) as f64
        if this.x > maxX {
            this.x = maxX
        }
    }

    pub centerX(): f64 {
        this.x + (this.width() as f64) / 2.0
    }
}
