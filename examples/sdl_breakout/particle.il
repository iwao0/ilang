// Visual effects spawned on brick break / paddle catch:
//   • Particle — small spinning fragment that flies off and fades
//   • Popup    — floating "+100" text that drifts up and fades
//
// Both live as their own arrays in the main loop and are stepped /
// drawn every frame.

class Particle {
    x: f64
    y: f64
    vx: f64
    vy: f64
    angle: f64
    spin: f64
    life: i32
    r: u8
    g: u8
    b: u8
    init(x: f64, y: f64, vx: f64, vy: f64, angle: f64, spin: f64,
         life: i32, r: u8, g: u8, b: u8) {
        this.x = x
        this.y = y
        this.vx = vx
        this.vy = vy
        this.angle = angle
        this.spin = spin
        this.life = life
        this.r = r
        this.g = g
        this.b = b
    }
}

// Floating "+100" text that drifts up from a brick break and
// fades to nothing.
class Popup {
    x: f64
    y: f64
    vy: f64
    life: i32
    text: string
    r: u8
    g: u8
    b: u8
    init(x: f64, y: f64, text: string, r: u8, g: u8, b: u8) {
        this.x = x
        this.y = y
        this.vy = -1.6
        this.life = 50
        this.text = text
        this.r = r
        this.g = g
        this.b = b
    }
}
