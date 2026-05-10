// Background twinkle. Stars scroll downward and vary their alpha
// over time so the field feels alive. `BgStars` owns the array and
// the per-frame step; rendering stays in the main loop because it
// needs the renderer + neon helpers anyway.

use rng

pub class Star {
    pub x: f64
    pub y: f64
    pub speed: f64
    pub phase: f64
    pub twinkle: f64
    pub init(x: f64, y: f64, speed: f64, phase: f64, twinkle: f64) {
        this.x = x
        this.y = y
        this.speed = speed
        this.phase = phase
        this.twinkle = twinkle
    }
}

pub class BgStars {
    pub stars: Star[]

    /// Build a fresh field of `count` randomly-positioned stars.
    pub init(count: i32, winW: i32, winH: i32) {
        let arr: Star[] = []
        let i: i32 = 0
        while i < count {
            arr.push(new Star(
                rng.randf() * (winW as f64),
                rng.randf() * (winH as f64),
                0.4 + rng.randf() * 1.6,
                rng.randf() * 6.2831853,
                0.04 + rng.randf() * 0.06
            ))
            i = i + 1
        }
        this.stars = arr
    }

    /// Scroll every star downward and advance its twinkle phase.
    /// Stars exiting the bottom respawn at the top with a fresh
    /// random x.
    pub step(winW: i32, winH: i32) {
        let i: i64 = 0
        let n: i64 = this.stars.length
        while i < n {
            let s = this.stars[i]
            s.y = s.y + s.speed
            s.phase = s.phase + s.twinkle
            if s.y > winH as f64 {
                s.y = -2.0
                s.x = rng.randf() * (winW as f64)
            }
            i = i + 1
        }
    }
}
