// Visual effects spawned on brick break / paddle catch:
//   • Particle — small spinning fragment that flies off and fades
//   • Popup    — floating "+100" text that drifts up and fades
//
// Both live as their own arrays in the main loop and are stepped /
// drawn every frame.

use std.math
use rng

pub class Particle {
    pub x: f64
    pub y: f64
    pub vx: f64
    pub vy: f64
    pub angle: f64
    pub spin: f64
    pub life: i32
    pub r: u8
    pub g: u8
    pub b: u8
    pub init(x: f64, y: f64, vx: f64, vy: f64, angle: f64, spin: f64,
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
pub class Popup {
    pub x: f64
    pub y: f64
    pub vy: f64
    pub life: i32
    pub text: string
    pub r: u8
    pub g: u8
    pub b: u8
    pub init(x: f64, y: f64, text: string, r: u8, g: u8, b: u8) {
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

/// Owns the `particles` and `popups` arrays plus the per-frame
/// step (advance position / spin / life, prune dead entries).
/// Spawning / drawing the colour-tinted particles still goes
/// through `spawnParticles` / the renderer in main.il.
pub class ParticleSystem {
    pub particles: Particle[]
    pub popups: Popup[]

    pub init() {
        let p: Particle[] = []
        let pp: Popup[] = []
        this.particles = p
        this.popups = pp
    }

    /// Spawn a 14-particle burst at `(cx, cy)` tinted `(r, g, b)`.
    pub spawn(cx: f64, cy: f64, r: u8, g: u8, b: u8) {
        spawnParticles(this.particles, cx, cy, r, g, b)
    }

    /// Push a "+100"-style floating text popup.
    pub spawnPopup(x: f64, y: f64, text: string, r: u8, g: u8, b: u8) {
        this.popups.push(new Popup(x, y, text, r, g, b))
    }

    /// Advance every particle / popup by one frame and prune the
    /// dead. Particles fall under a gentle gravity; popups drift
    /// upward and decelerate.
    pub step() {
        let aliveP: Particle[] = []
        let i: i64 = 0
        let n: i64 = this.particles.length
        while i < n {
            let p = this.particles[i]
            p.x = p.x + p.vx
            p.y = p.y + p.vy
            p.vy = p.vy + 0.18    // gentle gravity
            p.angle = p.angle + p.spin
            p.life = p.life - 1
            if p.life > 0 {
                aliveP.push(p)
            }
            i = i + 1
        }
        this.particles = aliveP

        let alivePop: Popup[] = []
        let j: i64 = 0
        let m: i64 = this.popups.length
        while j < m {
            let pp = this.popups[j]
            pp.y = pp.y + pp.vy
            pp.vy = pp.vy * 0.96       // ease toward stop
            pp.life = pp.life - 1
            if pp.life > 0 {
                alivePop.push(pp)
            }
            j = j + 1
        }
        this.popups = alivePop
    }

    /// Drop everything (used on game restart).
    pub reset() {
        let p: Particle[] = []
        let pp: Popup[] = []
        this.particles = p
        this.popups = pp
    }
}

/// Burst 14 colour-tinted particles outward from `(cx, cy)`, mixing
/// random angles, speeds and spins so the cloud doesn't move
/// uniformly.
pub fn spawnParticles(particles: Particle[], cx: f64, cy: f64,
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
