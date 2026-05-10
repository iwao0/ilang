// One ball + the ring buffer of recent positions used by the
// renderer to draw a fading trail behind each ball. Multi-ball
// keeps a separate trail per Ball so trails stay individual.

use consts

// One sample of the ball's recent position.
pub class TrailPoint {
    pub x: f64
    pub y: f64
    pub init(x: f64, y: f64) {
        this.x = x
        this.y = y
    }
}

pub class Ball {
    pub x: f64
    pub y: f64
    pub vx: f64
    pub vy: f64
    // Ring buffer of recent positions. Initialised to the spawn
    // point so the trail starts collapsed onto the ball rather than
    // streaming from (0, 0).
    pub trail: TrailPoint[]
    pub trailHead: i32
    pub init(x: f64, y: f64, vx: f64, vy: f64) {
        this.x = x
        this.y = y
        this.vx = vx
        this.vy = vy
        let tr: TrailPoint[] = []
        let i: i32 = 0
        while i < consts.trailLen {
            tr.push(new TrailPoint(x, y))
            i = i + 1
        }
        this.trail = tr
        this.trailHead = 0
    }
}
