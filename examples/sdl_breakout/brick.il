// One coloured brick on the playfield. The death animation is held
// here too so the renderer can fade it out without separate state.

class Brick {
    x: i32
    y: i32
    r: u8
    g: u8
    b: u8
    alive: bool
    // Death animation timer. 0 = either alive or fully gone. > 0
    // means we're inside the puff-and-fade animation: 1.0 starts,
    // counts down to 0.
    dying: f64
    init(x: i32, y: i32, r: u8, g: u8, b: u8) {
        this.x = x
        this.y = y
        this.r = r
        this.g = g
        this.b = b
        this.alive = true
        this.dying = 0.0
    }
}
