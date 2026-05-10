// Falls from a broken brick at low probability. The paddle catches
// it on contact, applying a temporary effect: kind 0 = wider
// paddle, 1 = multi-ball (immediate, one-shot), 2 = slow motion.
pub class PowerUp {
    pub x: f64
    pub y: f64
    pub vy: f64
    pub kind: i32
    pub init(x: f64, y: f64, kind: i32) {
        this.x = x
        this.y = y
        this.vy = 1.8
        this.kind = kind
    }
}
