// Visual effects spawned on brick break / paddle catch:
//   • Particle — small spinning fragment that flies off and fades
//   • Popup    — floating "+100" text that drifts up and fades
//
// Both live as their own arrays in the main loop and are stepped /
// drawn every frame.

use math
use rng

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

/// Burst 14 colour-tinted particles outward from `(cx, cy)`, mixing
/// random angles, speeds and spins so the cloud doesn't move
/// uniformly.
fn spawnParticles(particles: Particle[], cx: f64, cy: f64,
                  r: u8, g: u8, b: u8) {
    let n: i32 = 14
    let i: i32 = 0
    while i < n {
        let ang = rng.randf() * 6.2831853
        let speed = 1.5 + rng.randf() * 4.0
        let vx = math.cos(ang) * speed
        let vy = math.sin(ang) * speed - 1.5
        let life = 24 + (rng.randf() * 16.0) as i32
        // Right-going particles spin clockwise (positive degrees per
        // frame), left-going counter-clockwise. Magnitude random so
        // the cloud doesn't rotate uniformly.
        let dir: f64 = if vx >= 0.0 { 1.0 } else { -1.0 }
        let spin = dir * (4.0 + rng.randf() * 12.0)
        let startAngle = rng.randf() * 360.0
        particles.push(
            new Particle(cx, cy, vx, vy, startAngle, spin, life, r, g, b)
        )
        i += 1
    }
}
