// Background twinkle. Stars scroll downward and vary their alpha
// over time so the field feels alive.

class Star {
    x: f64
    y: f64
    speed: f64
    phase: f64
    twinkle: f64
    init(x: f64, y: f64, speed: f64, phase: f64, twinkle: f64) {
        this.x = x
        this.y = y
        this.speed = speed
        this.phase = phase
        this.twinkle = twinkle
    }
}
