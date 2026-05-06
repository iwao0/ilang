// Falls from a broken brick at low probability. The paddle catches
// it on contact, applying a temporary effect: kind 0 = wider
// paddle, 1 = multi-ball (immediate, one-shot), 2 = slow motion.
class PowerUp {
    x: f64
    y: f64
    vy: f64
    kind: i32
    init(x: f64, y: f64, kind: i32) {
        this.x = x
        this.y = y
        this.vy = 1.8
        this.kind = kind
    }
}
