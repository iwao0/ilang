// 🌃 ilang × SDL2 — neon-style breakout (single stage).
//
// Controls (keyboard / gamepad):
//   ←/→ · A/D · D-pad ←/→ · left stick — move the paddle
//   Space · A button                   — launch the ball
//   F                                  — toggle fullscreen
//   R · START                          — restart after game over
//   ESC · BACK                         — quit
//
// Run (JIT only — `@lib(...)` extern fns need dlsym):
//   ilang run --jit examples/sdl_bouncing_ball/main.il
//
// The SDL2 bindings live under bindings/sdl2/; the `ilang.toml` in
// this directory points at them so `use sdl` resolves automatically.

use sdl
use math

const WIN_W: i32 = 900
const WIN_H: i32 = 700

// Brick grid.
const COLS: i32 = 9
const ROWS: i32 = 5
const BRICK_W: i32 = 80
const BRICK_H: i32 = 26
const BRICK_GAP: i32 = 8
const BRICK_TOP: i32 = 80

// Paddle / ball.
const PADDLE_W: i32 = 110
const PADDLE_H: i32 = 14
const PADDLE_SPEED: f64 = 9.0
const BALL_SIZE: i32 = 14

const FRAME_MS: u32 = 16
const SAMPLE_RATE: i32 = 44100

// Brick drop-in intro tuning. With STAGGER=4 and COLS=9 the last
// column starts at frame 32 and finishes 24 frames later → ~0.93s
// total, ≈0.5s "perceived" since most bricks are already settled
// by then.
const INTRO_STAGGER: i32 = 4
const INTRO_DURATION: i32 = 24
const INTRO_DROP_PX: f64 = 280.0

// ─── tiny utilities ───────────────────────────────────────────────

// ilang's `as` cast is numeric-only; this stringifies a counter for
// the on-screen text.
fn intToStr(n: i32): string {
    if n == 0 { return "0" }
    let neg = n < 0
    let v: i32 = if neg { -n } else { n }
    let digits = "0123456789"
    let result: string = ""
    while v > 0 {
        let d = v % 10
        result = digits.charAt(d as i64) + result
        v = v / 10
    }
    if neg {
        result = "-" + result
    }
    result
}

// Tiny LCG so particles can fly off in pseudo-random directions
// without pulling in a real RNG dependency. Top-level `let` isn't
// visible from fn bodies in ilang, so the state lives on a class
// static.
class Rng {
    static state: i64 = 0x5fe7c1a3
}

fn randNext(): i64 {
    Rng.state = Rng.state * 6364136223846793005 + 1442695040888963407
    Rng.state
}

// Returns a float in [0, 1).
fn randf(): f64 {
    let r = randNext()
    let bits = r & 0x7fffffff
    (bits as f64) / 2147483648.0
}

// ─── star sprite ──────────────────────────────────────────────────

// Render a 5-point white star into a 32×32 RGBA8 buffer suitable
// for `Texture.update`. The shape is computed as the union of the
// triangles between the centre, an outer point, and an inner point,
// alternating around 10 radial steps. Pixels are anti-aliased by
// coverage-fraction (the closer the centre, the brighter), giving
// a soft glow that drops to transparent at the edge. Color
// modulation at draw time tints it.
const STAR_TEX_SIZE: i32 = 32

fn make_star_sprite(): u8[] {
    let size: i32 = STAR_TEX_SIZE
    let buf: u8[] = []
    let cx = (size as f64) / 2.0
    let cy = (size as f64) / 2.0
    let r_outer = cx - 1.5
    let r_inner = r_outer * 0.42
    let two_pi: f64 = 2.0 * math.pi
    let y: i32 = 0
    while y < size {
        let x: i32 = 0
        while x < size {
            let dx = (x as f64) - cx + 0.5
            let dy = (y as f64) - cy + 0.5
            let d = math.sqrt(dx * dx + dy * dy)
            // Angle of this pixel from centre, normalised so 5 wedges
            // cover the circle. Within each wedge, distance from
            // centre is bounded by a linear interpolation between
            // r_outer and r_inner depending on local angle.
            let theta = math.atan2(dy, dx)
            // Shift so a point sits at the top.
            let t = theta + math.pi / 2.0
            let wedge = t / (two_pi / 10.0)
            let frac = wedge - (math.floor(wedge / 2.0) * 2.0)
            // Triangular wave 0→1→0 across two wedges.
            let tri: f64 = if frac < 1.0 { frac } else { 2.0 - frac }
            let r_local = r_inner + (r_outer - r_inner) * tri
            if d <= r_local {
                let alpha = 1.0 - d / r_local
                let a = (alpha * 255.0) as u8
                buf.push(255 as u8)
                buf.push(255 as u8)
                buf.push(255 as u8)
                buf.push(a)
            } else {
                buf.push(0 as u8)
                buf.push(0 as u8)
                buf.push(0 as u8)
                buf.push(0 as u8)
            }
            x += 1
        }
        y += 1
    }
    buf
}

// ─── glow sprite ──────────────────────────────────────────────────

// 64×64 soft white radial gradient (alpha falls smoothly from
// centre to edge). Tinted at draw time and stretched to whatever
// size is needed for a blurry bloom — used for the brick wave
// highlight so the glow doesn't look rectangular.
const GLOW_TEX_SIZE: i32 = 64

fn make_glow_sprite(): u8[] {
    let size: i32 = GLOW_TEX_SIZE
    let buf: u8[] = []
    let cx = (size as f64) / 2.0
    let cy = (size as f64) / 2.0
    let radius = cx
    let y: i32 = 0
    while y < size {
        let x: i32 = 0
        while x < size {
            let dx = (x as f64) - cx + 0.5
            let dy = (y as f64) - cy + 0.5
            let d = math.sqrt(dx * dx + dy * dy)
            // Smooth falloff: 1 at centre, 0 at and past `radius`.
            let t: f64 = if d >= radius { 0.0 } else { 1.0 - d / radius }
            // Gaussian-ish exponent for a softer bell.
            let a_norm = t * t
            let a = (a_norm * 255.0) as u8
            buf.push(255 as u8)
            buf.push(255 as u8)
            buf.push(255 as u8)
            buf.push(a)
            x += 1
        }
        y += 1
    }
    buf
}

// ─── CRT overlay ──────────────────────────────────────────────────

// Pre-render a full-window RGBA black overlay where the alpha
// encodes how much the underlying scene should be darkened at
// each pixel. Combines two CRT-style effects:
//   • scanlines — odd rows have a small fixed darken
//   • vignette  — soft radial darken toward the corners
// Drawn over everything (including HUD) at draw-time. Built once
// at startup so the per-frame cost is just one drawTexture call.
fn make_crt_overlay(w: i32, h: i32): u8[] {
    let buf: u8[] = []
    let cx = (w as f64) / 2.0
    let cy = (h as f64) / 2.0
    let max_d = math.sqrt(cx * cx + cy * cy)
    let y: i32 = 0
    while y < h {
        // 1.0 = pixel left untouched, lower = darken applied.
        let scan: f64 = if y % 2 == 1 { 0.82 } else { 1.0 }
        let x: i32 = 0
        while x < w {
            let dx = (x as f64) - cx
            let dy = (y as f64) - cy
            let d = math.sqrt(dx * dx + dy * dy)
            let v_norm = d / max_d
            // Vignette: 1.0 in centre, ~0.5 at corners.
            let vig = 1.0 - v_norm * v_norm * 0.55
            let combined = scan * vig
            let darken: f64 = if combined < 0.0 { 1.0 } else { 1.0 - combined }
            let alpha = (darken * 255.0) as u8
            buf.push(0 as u8)
            buf.push(0 as u8)
            buf.push(0 as u8)
            buf.push(alpha)
            x += 1
        }
        y += 1
    }
    buf
}

// ─── audio: short sine-wave beeps + BGM loop ──────────────────────

fn makeBeep(freq_hz: f64, ms: i32): i16[] {
    let n = (SAMPLE_RATE * ms) / 1000
    let buf: i16[] = []
    let two_pi: f64 = 2.0 * math.pi
    let i: i32 = 0
    while i < n {
        let ramp = SAMPLE_RATE / 200
        let env: f64 = if i < ramp {
            (i as f64) / (ramp as f64)
        } elif i > n - ramp {
            ((n - i) as f64) / (ramp as f64)
        } else {
            1.0
        }
        let t = (i as f64) / (SAMPLE_RATE as f64)
        let sample = math.sin(two_pi * freq_hz * t) * 8000.0 * env
        buf.push(sample as i16)
        i += 1
    }
    buf
}

// Single chiptune note (square wave) of `ms` milliseconds. Used as
// the building block for the BGM loop.
fn makeNote(freq_hz: f64, ms: i32, vol: f64): i16[] {
    let n = (SAMPLE_RATE * ms) / 1000
    let buf: i16[] = []
    let i: i32 = 0
    while i < n {
        let ramp = SAMPLE_RATE / 100   // 10ms attack/decay
        let env: f64 = if i < ramp {
            (i as f64) / (ramp as f64)
        } elif i > n - ramp {
            ((n - i) as f64) / (ramp as f64)
        } else {
            1.0
        }
        let t = (i as f64) / (SAMPLE_RATE as f64)
        // Square wave from sin sign — gives the classic chip-style
        // bite without needing a phase accumulator.
        let s = math.sin(2.0 * math.pi * freq_hz * t)
        let sq: f64 = if s >= 0.0 { 1.0 } else { -1.0 }
        let sample = sq * vol * 5500.0 * env
        buf.push(sample as i16)
        i += 1
    }
    buf
}

// 16-step arpeggio over Am → F → C → G (4 bars of 4 notes each at
// ~180ms per step → ≈2.9s loop). Returns one full loop ready to be
// queued into an AudioDevice; the main loop refills the queue when
// it gets low.
fn makeBgmLoop(): i16[] {
    let buf: i16[] = []
    let beat_ms: i32 = 180
    let notes: f64[] = [
        // Am: A2 E3 A3 E3
        110.0,  164.81, 220.0,  164.81,
        // F:  F2 C3 F3 C3
         87.31, 130.81, 174.61, 130.81,
        // C:  C3 G3 C4 G3
        130.81, 196.0,  261.63, 196.0,
        // G:  G2 D3 G3 D3
         98.0,  146.83, 196.0,  146.83
    ]
    let i: i32 = 0
    let n: i32 = notes.length as i32
    while i < n {
        let note = makeNote(notes[i], beat_ms, 0.16)
        let ji: i32 = 0
        let jn: i32 = note.length as i32
        while ji < jn {
            buf.push(note[ji])
            ji += 1
        }
        i += 1
    }
    buf
}

// ─── game data ────────────────────────────────────────────────────

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

// Stored per-ball so multi-ball can run multiple at once. The
// physics loop iterates `balls` each frame; balls that fall below
// the paddle drop out of the array.
class Ball {
    x: f64
    y: f64
    vx: f64
    vy: f64
    init(x: f64, y: f64, vx: f64, vy: f64) {
        this.x = x
        this.y = y
        this.vx = vx
        this.vy = vy
    }
}

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

// One sample of the ball's recent position, used to render a fading
// trail behind it.
class TrailPoint {
    x: f64
    y: f64
    init(x: f64, y: f64) {
        this.x = x
        this.y = y
    }
}

// One row → one neon hue. Five rows from top to bottom.
fn paletteR(row: i32): u8 {
    if row == 0 { return 255 as u8 }
    if row == 1 { return 255 as u8 }
    if row == 2 { return  80 as u8 }
    if row == 3 { return 120 as u8 }
    if row == 4 { return 255 as u8 }
    255 as u8
}

fn paletteG(row: i32): u8 {
    if row == 0 { return  80 as u8 }
    if row == 1 { return 200 as u8 }
    if row == 2 { return 255 as u8 }
    if row == 3 { return 120 as u8 }
    if row == 4 { return 255 as u8 }
    255 as u8
}

fn paletteB(row: i32): u8 {
    if row == 0 { return 200 as u8 }
    if row == 1 { return  80 as u8 }
    if row == 2 { return 220 as u8 }
    if row == 3 { return 255 as u8 }
    if row == 4 { return 120 as u8 }
    255 as u8
}

fn spawnParticles(particles: Particle[], cx: f64, cy: f64,
                  r: u8, g: u8, b: u8) {
    let n: i32 = 14
    let i: i32 = 0
    while i < n {
        let ang = randf() * 6.2831853
        let speed = 1.5 + randf() * 4.0
        let vx = math.cos(ang) * speed
        let vy = math.sin(ang) * speed - 1.5
        let life = 24 + (randf() * 16.0) as i32
        // Right-going particles spin clockwise (positive degrees per
        // frame), left-going counter-clockwise. Magnitude random so
        // the cloud doesn't rotate uniformly.
        let dir: f64 = if vx >= 0.0 { 1.0 } else { -1.0 }
        let spin = dir * (4.0 + randf() * 12.0)
        let start_angle = randf() * 360.0
        particles.push(
            new Particle(cx, cy, vx, vy, start_angle, spin, life, r, g, b)
        )
        i += 1
    }
}

// ─── main ─────────────────────────────────────────────────────────

// Wrapped in a fn so all ARC-managed locals (Window/Renderer/Audio
// etc.) drop BEFORE `sdl.quit()` runs at top level. If the bindings
// outlived the SDL subsystem teardown they'd call SDL_Destroy* on
// shut-down state and the OS would surface a crash dialog.
fn play() {
    let win = new sdl.Window("ilang × SDL2 — NEON BREAKOUT", WIN_W, WIN_H)
    if win.handle == 0 {
        console.log("SDL_CreateWindow failed: " + sdl.getError())
        return
    }

    let ren = new sdl.Renderer(win)
    if ren.handle == 0 {
        console.log("SDL_CreateRenderer failed: " + sdl.getError())
        return
    }

    // Pin the renderer to our virtual WIN_W×WIN_H so toggling
    // fullscreen-desktop just changes the window backing — the
    // game's coordinates stay the same and SDL letterboxes to fit.
    ren.setLogicalSize(WIN_W, WIN_H)

    // Filled rectangle with two semi-transparent halo passes around
    // it. The renderer's blend mode is BLEND so each halo layer
    // accumulates over the dark background, giving a neon-glow
    // look.
    let fillNeon = fn(x: i32, y: i32, w: i32, h: i32,
                      r: u8, g: u8, b: u8) {
        ren.setColor(r, g, b, 50)
        ren.fillRect(x - 6, y - 6, w + 12, h + 12)
        ren.setColor(r, g, b, 110)
        ren.fillRect(x - 3, y - 3, w + 6, h + 6)
        ren.setColor(r, g, b, 255)
        ren.fillRect(x, y, w, h)
        return
    }

    // Filled rectangle with small rounded corners (Pill/chiclet
    // shape). Three accumulating halo passes give the same neon
    // glow as fillNeon.
    let fillNeonRoundedRect = fn(x: i32, y: i32, w: i32, h: i32,
                                 radius: i32,
                                 r: u8, g: u8, b: u8) {
        let pass: i32 = 0
        while pass < 3 {
            let extra: i32 = if pass == 0 { 6 } elif pass == 1 { 3 } else { 0 }
            let alpha: u8 = if pass == 0 { 50 as u8 } elif pass == 1 { 110 as u8 } else { 255 as u8 }
            let xx = x - extra
            let yy = y - extra
            let ww = w + 2 * extra
            let hh = h + 2 * extra
            let rad = radius + extra
            ren.setColor(r, g, b, alpha)
            // Middle band: full width, between the rounded caps.
            ren.fillRect(xx, yy + rad, ww, hh - 2 * rad)
            // Top + bottom with corner inset computed from the arc.
            let dy: i32 = 0
            while dy < rad {
                let cy = (rad as f64) - 0.5 - (dy as f64)
                let radf = rad as f64
                let chord = math.sqrt(radf * radf - cy * cy)
                let inset = ((rad as f64) - chord) as i32
                ren.fillRect(xx + inset, yy + dy, ww - 2 * inset, 1)
                ren.fillRect(xx + inset, yy + hh - 1 - dy, ww - 2 * inset, 1)
                dy += 1
            }
            pass += 1
        }
        return
    }

    // Filled stadium (rectangle with semicircular ends), again with
    // three accumulating halo passes so the rounded paddle glows
    // like the rest of the scene.
    let fillNeonStadium = fn(x: i32, y: i32, w: i32, h: i32,
                             r: u8, g: u8, b: u8) {
        let pass: i32 = 0
        while pass < 3 {
            let extra: i32 = if pass == 0 { 6 } elif pass == 1 { 3 } else { 0 }
            let alpha: u8 = if pass == 0 { 50 as u8 } elif pass == 1 { 110 as u8 } else { 255 as u8 }
            let xx = x - extra
            let yy = y - extra
            let ww = w + 2 * extra
            let hh = h + 2 * extra
            let rad = hh / 2
            ren.setColor(r, g, b, alpha)
            // Centre rectangle (between the two end caps).
            ren.fillRect(xx + rad, yy, ww - 2 * rad, hh)
            // Two semicircle caps via scanlines.
            let dy: i32 = -rad
            while dy <= rad {
                let dyf = dy as f64
                let radf = rad as f64
                let half = (math.sqrt(radf * radf - dyf * dyf)) as i32
                // Left cap.
                ren.fillRect(xx + rad - half, yy + rad + dy, half + 1, 1)
                // Right cap.
                ren.fillRect(xx + ww - rad, yy + rad + dy, half + 1, 1)
                dy += 1
            }
            pass += 1
        }
        return
    }

    // Filled circle as a stack of 1-pixel-tall horizontal strips
    // (scanline rasteriser). Three passes at different radii + alphas
    // give the same neon halo as fillNeon.
    let fillNeonCircle = fn(cx: i32, cy: i32, radius: i32,
                            r: u8, g: u8, b: u8) {
        let pass: i32 = 0
        while pass < 3 {
            let extra: i32 = if pass == 0 { 6 } elif pass == 1 { 3 } else { 0 }
            let alpha: u8 = if pass == 0 { 50 as u8 } elif pass == 1 { 110 as u8 } else { 255 as u8 }
            let rad = radius + extra
            ren.setColor(r, g, b, alpha)
            let dy: i32 = -rad
            while dy <= rad {
                let dyf = dy as f64
                let radf = rad as f64
                let w = math.sqrt(radf * radf - dyf * dyf)
                let half = w as i32
                ren.fillRect(cx - half, cy + dy, half * 2 + 1, 1)
                dy += 1
            }
            pass += 1
        }
        return
    }

    let font = new sdl.Font(ren)

    let audio = new sdl.AudioDevice(SAMPLE_RATE, 1)
    if audio.id == 0 {
        console.log("SDL_OpenAudioDevice failed: " + sdl.getError())
        return
    }
    audio.play()

    // Second audio device dedicated to BGM. SDL allows multiple
    // simultaneous open devices, so the music keeps playing even
    // while SFX queue/dequeue rapidly on the first device.
    let audio_music = new sdl.AudioDevice(SAMPLE_RATE, 1)
    let bgm = makeBgmLoop()
    let bgm_bytes = (bgm.length * 2) as u32   // one loop in queued-bytes
    if audio_music.id != 0 {
        // Pre-buffer two loops so the first refill check has slack.
        audio_music.queue(bgm)
        audio_music.queue(bgm)
        audio_music.play()
    }

    // Open the first attached gamepad if any. `pad.handle == 0`
    // when nothing is plugged in — every poll degrades to
    // returning 0/false, so the keyboard path keeps working.
    let pad = new sdl.Gamepad()

    let beep_wall = makeBeep(440.0, 40)
    let beep_paddle = makeBeep(660.0, 50)
    let beep_brick = makeBeep(880.0, 70)

    // White star sprite for particle bursts. Tinted at draw time
    // via `setColorMod`, rotated via `drawTextureRot`.
    let star_tex = new sdl.Texture(ren, STAR_TEX_SIZE, STAR_TEX_SIZE, 1)
    if star_tex.handle == 0 {
        console.log("SDL_CreateTexture failed: " + sdl.getError())
        return
    }
    star_tex.update(make_star_sprite())

    // Soft radial-gradient sprite, tinted per-brick at draw time so
    // the wave highlight blooms as a blurry circle rather than a
    // rectangle.
    let glow_tex = new sdl.Texture(ren, GLOW_TEX_SIZE, GLOW_TEX_SIZE, 1)
    if glow_tex.handle == 0 {
        console.log("SDL_CreateTexture failed: " + sdl.getError())
        return
    }
    glow_tex.update(make_glow_sprite())

    // Full-window CRT-effect overlay — built once. The alpha
    // channel encodes scanlines + vignette, the colour is black,
    // so blending it on top darkens each pixel by the right
    // amount.
    let crt_tex = new sdl.Texture(ren, WIN_W, WIN_H, 1)
    if crt_tex.handle == 0 {
        console.log("SDL_CreateTexture failed: " + sdl.getError())
        return
    }
    crt_tex.update(make_crt_overlay(WIN_W, WIN_H))

    // Off-screen accumulator for the world. Drawn into each frame
    // (with a small per-frame fade applied first) and then composited
    // back to the window with chromatic-aberration channel offsets.
    // SDL_TEXTUREACCESS_TARGET = 2.
    let scene_tex = new sdl.Texture(ren, WIN_W, WIN_H, 2)
    if scene_tex.handle == 0 {
        console.log("SDL_CreateTexture(TARGET) failed: " + sdl.getError())
        return
    }
    // Linear filtering on the scene texture so that the bloom
    // pass's downscale + upscale produces a smooth blur instead of
    // a chunky pixel grid.
    scene_tex.setScaleMode(1)

    // Tiny target used to "blur" the scene. We draw scene_tex
    // shrunk into bloom_tex (each downsample averages a 4×4 block
    // thanks to linear filtering), and later draw bloom_tex back
    // out at full window size additively — the upscale spreads
    // each averaged pixel across many output pixels, giving a
    // soft glow around the bright stuff in the scene.
    let BLOOM_W: i32 = 225              // WIN_W / 4
    let BLOOM_H: i32 = 175              // WIN_H / 4
    let bloom_tex = new sdl.Texture(ren, BLOOM_W, BLOOM_H, 2)
    if bloom_tex.handle == 0 {
        console.log("SDL_CreateTexture(bloom) failed: " + sdl.getError())
        return
    }
    bloom_tex.setScaleMode(1)

    // Background star field — scrolls top→down with a twinkle.
    let bg_stars: Star[] = []
    let s_i: i32 = 0
    while s_i < 80 {
        bg_stars.push(new Star(
            randf() * (WIN_W as f64),
            randf() * (WIN_H as f64),
            0.4 + randf() * 1.6,
            randf() * 6.2831853,
            0.04 + randf() * 0.06
        ))
        s_i += 1
    }

    // Build the brick grid.
    let bricks: Brick[] = []
    let row: i32 = 0
    while row < ROWS {
        let col: i32 = 0
        while col < COLS {
            let x = col * (BRICK_W + BRICK_GAP) +
                    (WIN_W - COLS * (BRICK_W + BRICK_GAP) + BRICK_GAP) / 2
            let y = BRICK_TOP + row * (BRICK_H + BRICK_GAP)
            bricks.push(new Brick(x, y, paletteR(row), paletteG(row), paletteB(row)))
            col += 1
        }
        row += 1
    }

    // State.
    let paddle_x: f64 = ((WIN_W - PADDLE_W) / 2) as f64
    let paddle_y: i32 = WIN_H - 60
    let balls: Ball[] = []
    balls.push(new Ball((WIN_W / 2) as f64, (WIN_H / 2) as f64, 5.0, -5.0))
    let score: i32 = 0
    let bricks_left: i32 = COLS * ROWS
    let lives: i32 = 5
    let started: bool = false
    let game_over: bool = false
    let won: bool = false
    let combo: i32 = 0              // bricks hit since last paddle bounce
    let shake: f64 = 0.0            // current screen-shake amplitude (px)
    let popups: Popup[] = []
    // Frames the ball has been in flight (this game). Drives the
    // gradual speed-up. Doesn't reset on lose-life so tension keeps
    // building across attempts.
    let play_time: i32 = 0
    // Frames since the last (re)start. Drives the brick drop-in
    // intro and lets us trigger a single big firework burst the
    // moment the level is cleared.
    let intro_phase: i32 = 0
    let won_celebrated: bool = false
    // Power-ups falling from broken bricks. Timers count down per
    // frame while > 0; while their flag is active the matching
    // effect is applied. Multi-ball is one-shot so it doesn't need
    // a timer.
    let powerups: PowerUp[] = []
    let wider_timer: i32 = 0
    let slow_timer: i32 = 0
    // Tracks the paddle width from the previous frame so we can
    // re-anchor `paddle_x` (which is the LEFT edge) when WIDER
    // turns on or off — the visible paddle centre stays put.
    let prev_paddle_w: i32 = PADDLE_W

    // Visual effect state.
    let wave_phase: f64 = 0.0       // brick highlight wave (rad)

    // F toggles fullscreen — track edge so a held key doesn't flap.
    let fullscreen: bool = false
    let f_was_down: bool = false

    // FPS counter.
    let fps: i32 = 0
    let fps_frames: i32 = 0
    let fps_last_ms: u32 = sdl.getTicks()

    // Frame pacing: 1000/60 ≈ 16.667ms target. Track the next-frame
    // deadline as f64 so the rounding error doesn't drift the
    // average down to 58fps.
    let next_frame_ms: f64 = sdl.getTicks() as f64
    let frame_period_ms: f64 = 1000.0 / 60.0
    let particles: Particle[] = []

    // Ball trail — a small ring buffer of recent positions, oldest
    // overwritten first. Drawn each frame as fading dots behind the
    // ball.
    let TRAIL_LEN: i32 = 12
    let trail: TrailPoint[] = []
    let trail_init: i32 = 0
    while trail_init < TRAIL_LEN {
        trail.push(new TrailPoint(balls[0].x, balls[0].y))
        trail_init = trail_init + 1
    }
    let trail_head: i32 = 0

    console.log("playing — ESC to quit")

    let running = true
    while running {
        sdl.pumpEvents()
        sdl.Keyboard.pump()
        if sdl.quitRequested() {
            running = false
        }

        // FPS: count frames over a 1-second window.
        fps_frames += 1
        let now_ms = sdl.getTicks()
        if now_ms - fps_last_ms >= 1000 {
            fps = fps_frames
            fps_frames = 0
            fps_last_ms = now_ms
        }

        // Brick highlight wave advances each frame.
        wave_phase = wave_phase + 0.08
        intro_phase = intro_phase + 1

        // Top up the BGM queue so the loop never drains. Polling
        // each frame is plenty cheap.
        if audio_music.id != 0 && audio_music.queuedBytes() < bgm_bytes {
            audio_music.queue(bgm)
        }

        // Clear-time celebration: one big multi-burst the instant
        // the level is won, then a steady stream while CLEAR! is
        // shown so the screen keeps lighting up.
        if game_over && won {
            if !won_celebrated {
                let bi: i32 = 0
                while bi < 12 {
                    let fx = randf() * (WIN_W as f64)
                    let fy = (0.15 + randf() * 0.55) * (WIN_H as f64)
                    let pick = (randf() * 5.0) as i32
                    spawnParticles(particles, fx, fy,
                                   paletteR(pick), paletteG(pick), paletteB(pick))
                    bi += 1
                }
                won_celebrated = true
            }
            // ~10 fireworks per second while celebrating.
            if randf() < 0.18 {
                let fx = randf() * (WIN_W as f64)
                let fy = (0.10 + randf() * 0.65) * (WIN_H as f64)
                let pick = (randf() * 5.0) as i32
                spawnParticles(particles, fx, fy,
                               paletteR(pick), paletteG(pick), paletteB(pick))
            }
        }

        // Gamepad: left stick → paddle (analogue), DPAD ←/→ →
        // paddle (digital), A → launch, START → restart, BACK →
        // quit. The keyboard mappings below stack with these so
        // either device works.
        let pad_lx = pad.getAxis(sdl.AXIS_LEFTX) as i32  // [-32768, 32767]
        let pad_dpad_l = pad.isPressed(sdl.BUTTON_DPAD_LEFT)
        let pad_dpad_r = pad.isPressed(sdl.BUTTON_DPAD_RIGHT)
        let pad_a = pad.isPressed(sdl.BUTTON_A)
        let pad_start = pad.isPressed(sdl.BUTTON_START)
        let pad_back = pad.isPressed(sdl.BUTTON_BACK)

        if sdl.Keyboard.isPressed(sdl.SCANCODE_ESCAPE) || pad_back {
            running = false
        }

        // F: toggle fullscreen on key-down (rising edge).
        let f_now = sdl.Keyboard.isPressed(sdl.SCANCODE_F)
        if f_now && !f_was_down {
            fullscreen = !fullscreen
            win.fullscreen = fullscreen
        }
        f_was_down = f_now

        // Restart after game over.
        if game_over && (sdl.Keyboard.isPressed(sdl.SCANCODE_R) || pad_start) {
            let r2: i32 = 0
            while r2 < ROWS {
                let c2: i32 = 0
                while c2 < COLS {
                    let br = bricks[r2 * COLS + c2]
                    br.alive = true
                    br.dying = 0.0
                    c2 += 1
                }
                r2 += 1
            }
            let fresh_balls: Ball[] = []
            fresh_balls.push(new Ball((WIN_W / 2) as f64, (WIN_H / 2) as f64, 5.0, -5.0))
            balls = fresh_balls
            score = 0
            bricks_left = COLS * ROWS
            lives = 5
            started = false
            game_over = false
            won = false
            let empty_pu: PowerUp[] = []
            powerups = empty_pu
            wider_timer = 0
            slow_timer = 0
            prev_paddle_w = PADDLE_W
            let empty: Particle[] = []
            particles = empty
            let empty_pops: Popup[] = []
            popups = empty_pops
            combo = 0
            shake = 0.0
            play_time = 0
            intro_phase = 0
            won_celebrated = false
        }

        // ─── input ────────────────────────────────────────────────

        // Effective paddle width — temporarily widened by the
        // WIDER power-up. Used by both physics and rendering.
        let paddle_w_wide: i32 = PADDLE_W + (PADDLE_W as i32) / (2 as i32)
        let paddle_w: i32 = if wider_timer > 0 { paddle_w_wide } else { PADDLE_W }
        // Re-anchor on width change so the paddle grows/shrinks
        // around its centre rather than its left edge.
        if paddle_w != prev_paddle_w {
            let delta = paddle_w - prev_paddle_w
            paddle_x = paddle_x - (delta as f64) / 2.0
            prev_paddle_w = paddle_w
        }

        if sdl.Keyboard.isPressed(sdl.SCANCODE_LEFT) || sdl.Keyboard.isPressed(sdl.SCANCODE_A) || pad_dpad_l {
            paddle_x = paddle_x - PADDLE_SPEED
        }
        if sdl.Keyboard.isPressed(sdl.SCANCODE_RIGHT) || sdl.Keyboard.isPressed(sdl.SCANCODE_D) || pad_dpad_r {
            paddle_x = paddle_x + PADDLE_SPEED
        }
        // Analogue stick: deadzone ~24% then linear scale to paddle
        // speed. Ignored when the stick is centred.
        let abs_lx: i32 = if pad_lx < 0 { -pad_lx } else { pad_lx }
        if abs_lx > 8000 {
            let norm = (pad_lx as f64) / 32767.0
            paddle_x = paddle_x + norm * PADDLE_SPEED
        }
        if paddle_x < 0.0 {
            paddle_x = 0.0
        }
        let paddle_max: f64 = (WIN_W - paddle_w) as f64
        if paddle_x > paddle_max {
            paddle_x = paddle_max
        }

        // Space to launch the ball (idle on paddle until pressed).
        // Wait for the brick drop-in animation to finish before
        // accepting the launch input — otherwise the player can
        // shoot through unfilled rows.
        let intro_total = (COLS - 1) * INTRO_STAGGER + INTRO_DURATION
        if !started && !game_over && intro_phase >= intro_total &&
                (sdl.Keyboard.isPressed(sdl.SCANCODE_SPACE) || pad_a) {
            started = true
        }

        // While idle, keep the (first) ball glued to the paddle's
        // centre.
        if !started && balls.length as i32 > 0 {
            let b0 = balls[0]
            b0.x = paddle_x + (paddle_w as f64) / 2.0 - (BALL_SIZE as f64) / 2.0
            b0.y = (paddle_y as f64) - (BALL_SIZE as f64)
        }

        // ─── ball physics ─────────────────────────────────────────

        if started && !game_over {
            // Slow-mo divides ball velocity step. Renormalize uses
            // the same factor so target magnitude shrinks.
            let speed_factor: f64 = if slow_timer > 0 { 0.55 } else { 1.0 }

            let surviving: Ball[] = []
            let bi: i32 = 0
            let bn: i32 = balls.length as i32
            while bi < bn {
                let b = balls[bi]
                b.x = b.x + b.vx * speed_factor
                b.y = b.y + b.vy * speed_factor

                // Walls.
                if b.x < 0.0 {
                    b.x = 0.0
                    b.vx = -b.vx
                    audio.queue(beep_wall)
                }
                if b.x + (BALL_SIZE as f64) > WIN_W as f64 {
                    b.x = (WIN_W - BALL_SIZE) as f64
                    b.vx = -b.vx
                    audio.queue(beep_wall)
                }
                if b.y < 0.0 {
                    b.y = 0.0
                    b.vy = -b.vy
                    audio.queue(beep_wall)
                }

                // Paddle (only on the way down).
                let bx_lo = b.x
                let bx_hi = b.x + (BALL_SIZE as f64)
                let by_lo = b.y
                let by_hi = b.y + (BALL_SIZE as f64)
                let px_lo = paddle_x
                let px_hi = paddle_x + (paddle_w as f64)
                let py_lo = paddle_y as f64
                let py_hi = (paddle_y + PADDLE_H) as f64
                if b.vy > 0.0 && bx_hi > px_lo && bx_lo < px_hi
                        && by_hi > py_lo && by_lo < py_hi {
                    b.y = py_lo - (BALL_SIZE as f64)
                    let hit_centre = px_lo + (paddle_w as f64) / 2.0
                    let ball_centre = b.x + (BALL_SIZE as f64) / 2.0
                    let rel = (ball_centre - hit_centre) / ((paddle_w as f64) / 2.0)
                    let rel_clamped: f64 = if rel < -1.0 { -1.0 } elif rel > 1.0 { 1.0 } else { rel }
                    let max_angle: f64 = 1.04   // ≈60°
                    let angle = rel_clamped * max_angle
                    let speed = math.sqrt(b.vx * b.vx + b.vy * b.vy)
                    b.vx = speed * math.sin(angle)
                    b.vy = -speed * math.cos(angle)
                    combo = 0
                    audio.queue(beep_paddle)
                }

                // Bricks. First-hit-wins-this-frame for this ball.
                let i: i32 = 0
                let n: i32 = bricks.length as i32
                while i < n {
                    let br = bricks[i]
                    if br.alive {
                        let rx_lo = br.x as f64
                        let rx_hi = (br.x + BRICK_W) as f64
                        let ry_lo = br.y as f64
                        let ry_hi = (br.y + BRICK_H) as f64
                        if bx_hi > rx_lo && bx_lo < rx_hi
                                && by_hi > ry_lo && by_lo < ry_hi {
                            br.alive = false
                            br.dying = 1.0
                            bricks_left = bricks_left - 1
                            combo = combo + 1
                            let earned = 100 * combo
                            score = score + earned
                            let overlap_x = if b.vx > 0.0 { bx_hi - rx_lo } else { rx_hi - bx_lo }
                            let overlap_y = if b.vy > 0.0 { by_hi - ry_lo } else { ry_hi - by_lo }
                            if overlap_x < overlap_y {
                                b.vx = -b.vx
                            } else {
                                b.vy = -b.vy
                            }
                            let cx = (br.x as f64) + (BRICK_W as f64) / 2.0
                            let cy = (br.y as f64) + (BRICK_H as f64) / 2.0
                            spawnParticles(particles, cx, cy, br.r, br.g, br.b)
                            popups.push(new Popup(
                                cx, cy - 4.0, "+" + intToStr(earned),
                                br.r, br.g, br.b
                            ))
                            let bump = 4.0 + (combo as f64) * 1.5
                            shake = shake + bump
                            if shake > 14.0 { shake = 14.0 }
                            audio.queue(beep_brick)
                            // 14% drop chance per brick break.
                            if randf() < 0.14 {
                                let kind = (randf() * 3.0) as i32
                                powerups.push(new PowerUp(cx, cy, kind))
                            }
                            i = n   // stop after first hit this frame
                        }
                    }
                    i = i + 1
                }

                // Win: all bricks cleared. Don't break the ball
                // loop — remaining balls finish their frame.
                if bricks_left <= 0 {
                    game_over = true
                    won = true
                }

                // Speed-up renormalize per ball.
                let base_speed: f64 = 7.07106781   // sqrt(5^2 + 5^2)
                let time_bonus_raw = (play_time as f64) * 0.002
                let time_bonus: f64 = if time_bonus_raw > 6.0 { 6.0 } else { time_bonus_raw }
                let combo_bonus = (combo as f64) * 0.35
                let target = (base_speed + time_bonus + combo_bonus) * speed_factor
                let cur = math.sqrt(b.vx * b.vx + b.vy * b.vy)
                if cur > 0.001 {
                    let scale = target / cur
                    b.vx = b.vx * scale
                    b.vy = b.vy * scale
                }

                // Survives if it didn't fall below the bottom.
                if b.y <= WIN_H as f64 {
                    surviving.push(b)
                }
                bi = bi + 1
            }
            balls = surviving
            play_time = play_time + 1

            // Out of balls → lose a life.
            if balls.length as i32 == 0 {
                lives = lives - 1
                if lives <= 0 {
                    game_over = true
                    won = false
                } else {
                    started = false
                    combo = 0
                    let respawn: Ball[] = []
                    respawn.push(new Ball((WIN_W / 2) as f64, (WIN_H / 2) as f64, 5.0, -5.0))
                    balls = respawn
                }
            }
        }

        // Record the (first) ball's position into the trail ring
        // buffer. Multi-ball still draws a single trail tied to the
        // primary; secondary balls don't smear, which keeps the
        // image readable when several are in flight.
        if balls.length as i32 > 0 {
            let tp = trail[trail_head]
            tp.x = balls[0].x
            tp.y = balls[0].y
            trail_head = (trail_head + 1) % TRAIL_LEN
        }

        // ─── power-up step: fall, catch, expire ──────────────────
        if wider_timer > 0 { wider_timer = wider_timer - 1 }
        if slow_timer > 0 { slow_timer = slow_timer - 1 }
        let alive_pu: PowerUp[] = []
        let pui: i32 = 0
        let pun: i32 = powerups.length as i32
        while pui < pun {
            let pu = powerups[pui]
            pu.y = pu.y + pu.vy
            // Catch test: 36×16 chip centred on (pu.x, pu.y).
            let chip_w: f64 = 36.0
            let chip_h: f64 = 16.0
            let cx_lo = pu.x - chip_w / 2.0
            let cx_hi = pu.x + chip_w / 2.0
            let cy_lo = pu.y - chip_h / 2.0
            let cy_hi = pu.y + chip_h / 2.0
            let pdl = paddle_x
            let pdr = paddle_x + (paddle_w as f64)
            let pdt = paddle_y as f64
            let pdb = (paddle_y + PADDLE_H) as f64
            let caught = cx_hi > pdl && cx_lo < pdr && cy_hi > pdt && cy_lo < pdb
            let removed: bool = caught
            if caught {
                if pu.kind == 0 {
                    wider_timer = 600           // 10s
                } elif pu.kind == 1 {
                    // Multi-ball: clone first ball into two new balls
                    // with slightly diverged angles.
                    if balls.length as i32 > 0 {
                        let src = balls[0]
                        let extra: Ball[] = []
                        let ki: i32 = 0
                        while ki < 2 {
                            let dvx = if ki == 0 { src.vy * 0.5 } else { 0.0 - src.vy * 0.5 }
                            let dvy = if ki == 0 { 0.0 - src.vx * 0.5 } else { src.vx * 0.5 }
                            extra.push(new Ball(src.x, src.y,
                                                src.vx + dvx, src.vy + dvy))
                            ki = ki + 1
                        }
                        let kj: i32 = 0
                        let kn: i32 = extra.length as i32
                        while kj < kn {
                            balls.push(extra[kj])
                            kj = kj + 1
                        }
                    }
                } else {
                    slow_timer = 480            // 8s
                }
                audio.queue(beep_paddle)
            }
            if !removed && pu.y < (WIN_H + 40) as f64 {
                alive_pu.push(pu)
            }
            pui = pui + 1
        }
        powerups = alive_pu

        // ─── particle step ───────────────────────────────────────

        let alive_particles: Particle[] = []
        let i: i32 = 0
        let n: i32 = particles.length as i32
        while i < n {
            let p = particles[i]
            p.x = p.x + p.vx
            p.y = p.y + p.vy
            p.vy = p.vy + 0.18    // gentle gravity
            p.angle = p.angle + p.spin
            p.life = p.life - 1
            if p.life > 0 {
                alive_particles.push(p)
            }
            i = i + 1
        }
        particles = alive_particles

        // Score-popup step: drift up, fade, drop dead ones.
        let alive_popups: Popup[] = []
        let pi: i32 = 0
        let pn: i32 = popups.length as i32
        while pi < pn {
            let pp = popups[pi]
            pp.y = pp.y + pp.vy
            pp.vy = pp.vy * 0.96       // ease toward stop
            pp.life = pp.life - 1
            if pp.life > 0 {
                alive_popups.push(pp)
            }
            pi = pi + 1
        }
        popups = alive_popups

        // Decay screen-shake exponentially. Compute per-frame
        // jitter offsets here so all draw calls share them.
        let shake_off_x: i32 = 0
        let shake_off_y: i32 = 0
        if shake > 0.5 {
            shake_off_x = ((randf() - 0.5) * 2.0 * shake) as i32
            shake_off_y = ((randf() - 0.5) * 2.0 * shake) as i32
        }
        shake = shake * 0.82
        if shake < 0.05 { shake = 0.0 }

        // Background star scroll + twinkle phase.
        let bs_i: i32 = 0
        let bs_n: i32 = bg_stars.length as i32
        while bs_i < bs_n {
            let s = bg_stars[bs_i]
            s.y = s.y + s.speed
            s.phase = s.phase + s.twinkle
            if s.y > WIN_H as f64 {
                s.y = -2.0
                s.x = randf() * (WIN_W as f64)
            }
            bs_i = bs_i + 1
        }

        // ─── render ──────────────────────────────────────────────
        // The world is drawn into `scene_tex` so we can post-process
        // it (chromatic aberration). HUD lands directly on the
        // window afterwards so the readouts stay sharp.
        ren.setTarget(scene_tex)
        ren.setColor(8, 4, 24, 255)        // very dark navy
        ren.clear()

        // Background stars (drawn first, beneath everything).
        let bg_i: i32 = 0
        let bg_n: i32 = bg_stars.length as i32
        while bg_i < bg_n {
            let s = bg_stars[bg_i]
            // Brightness pulses with phase.
            let bright = 0.4 + 0.6 * (0.5 + 0.5 * math.sin(s.phase))
            let a8 = (bright * 220.0) as u8
            ren.setColor(220, 220, 255, a8)
            // Bigger stars (random radius 1 or 2).
            let half: i32 = if s.speed > 1.2 { 2 } else { 1 }
            ren.fillRect(s.x as i32 - half + shake_off_x,
                         s.y as i32 - half + shake_off_y,
                         half * 2 + 1, half * 2 + 1)
            bg_i = bg_i + 1
        }

        // Bricks with a left-to-right brightness wave. Glow padding
        // is proportional to the brick's aspect ratio, so the
        // bloom around a wide brick is wider than tall (matching
        // the brick's shape).
        let glow_pad_y: i32 = 32
        let glow_pad_x: i32 = glow_pad_y * BRICK_W / BRICK_H
        let i2: i32 = 0
        let n2: i32 = bricks.length as i32
        while i2 < n2 {
            let br = bricks[i2]
            // Drop-in intro: each column is delayed by INTRO_STAGGER
            // frames so the grid cascades in left→right. The brick
            // falls from above its target with an ease-out curve.
            let col = i2 % COLS
            let own_phase = intro_phase - col * INTRO_STAGGER
            let intro_active = own_phase < INTRO_DURATION
            let intro_offset_y: i32 = if own_phase < 0 {
                // Pre-arrival: keep the brick well above the playfield.
                0 - ((INTRO_DROP_PX as i32) + 100)
            } elif own_phase < INTRO_DURATION {
                let p = (own_phase as f64) / (INTRO_DURATION as f64)
                let inv = 1.0 - p
                0 - ((inv * inv * INTRO_DROP_PX) as i32)
            } else {
                0
            }
            if br.alive {
                fillNeonRoundedRect(br.x + shake_off_x,
                                    br.y + intro_offset_y + shake_off_y,
                                    BRICK_W, BRICK_H, 6, br.r, br.g, br.b)
                // Wave glow only after the brick has landed — looks
                // odd to highlight a brick that's mid-fall.
                let k: f64 = 0.012
                let s = math.sin(wave_phase - (br.x as f64) * k)
                let pulse: f64 = if s > 0.0 { s * s } else { 0.0 }
                if pulse > 0.0 && !intro_active {
                    let alpha = (pulse * 220.0) as u8
                    if alpha > 0 as u8 {
                        glow_tex.setColorMod(br.r, br.g, br.b)
                        glow_tex.setAlphaMod(alpha)
                        ren.drawTexture(glow_tex,
                                        br.x - glow_pad_x + shake_off_x,
                                        br.y - glow_pad_y + shake_off_y,
                                        BRICK_W + 2 * glow_pad_x,
                                        BRICK_H + 2 * glow_pad_y)
                    }
                }
            } elif br.dying > 0.0 {
                // Brick destroy puff: the rectangle scales up while
                // its alpha fades to zero. Drawn here (instead of
                // the brick row) so the wave glow doesn't keep
                // moving over a vanishing brick.
                let t = br.dying     // 1.0 → 0.0
                let progress = 1.0 - t
                let scale = 1.0 + progress * 0.6
                let cx = br.x + BRICK_W / 2
                let cy = br.y + BRICK_H / 2
                let dw = ((BRICK_W as f64) * scale) as i32
                let dh = ((BRICK_H as f64) * scale) as i32
                let alpha = (t * 220.0) as u8
                // Soft bloom (radial gradient) — biggest, dimmest.
                glow_tex.setColorMod(br.r, br.g, br.b)
                glow_tex.setAlphaMod((t * 180.0) as u8)
                let bw = ((BRICK_W + 2 * glow_pad_x) as f64) * scale
                let bh = ((BRICK_H + 2 * glow_pad_y) as f64) * scale
                ren.drawTexture(glow_tex,
                                cx - (bw as i32) / 2 + shake_off_x,
                                cy - (bh as i32) / 2 + shake_off_y,
                                bw as i32, bh as i32)
                // Sharper core.
                if alpha > 0 as u8 {
                    ren.setColor(br.r, br.g, br.b, alpha)
                    ren.fillRect(cx - dw / 2 + shake_off_x,
                                 cy - dh / 2 + shake_off_y, dw, dh)
                }
                br.dying = br.dying - 0.06
                if br.dying < 0.0 {
                    br.dying = 0.0
                }
            }
            i2 = i2 + 1
        }

        // Particles: rotating star sprites tinted with each
        // particle's colour. Drawn behind paddle/ball so big hits
        // don't hide them.
        let i3: i32 = 0
        let n3: i32 = particles.length as i32
        while i3 < n3 {
            let p = particles[i3]
            let alpha = (p.life * 8) as i32
            let a: u8 = if alpha > 255 { 255 } else { alpha as u8 }
            star_tex.setColorMod(p.r, p.g, p.b)
            // Outer halo: bigger, lower alpha. Drawn first so the
            // crisp star sits on top of its glow.
            let halo_a = ((a as i32) / 3) as u8
            star_tex.setAlphaMod(halo_a)
            let halo_size: i32 = 56
            ren.drawTextureRot(star_tex,
                               (p.x as i32) - halo_size / 2 + shake_off_x,
                               (p.y as i32) - halo_size / 2 + shake_off_y,
                               halo_size, halo_size, p.angle)
            // Sharp star body (2× the previous 16px).
            star_tex.setAlphaMod(a)
            let size: i32 = 32
            ren.drawTextureRot(star_tex,
                               (p.x as i32) - size / 2 + shake_off_x,
                               (p.y as i32) - size / 2 + shake_off_y,
                               size, size, p.angle)
            i3 = i3 + 1
        }

        // Power-up chips drifting down — coloured neon stadium with
        // a single-letter label on top.
        let pri: i32 = 0
        let prn: i32 = powerups.length as i32
        while pri < prn {
            let pu = powerups[pri]
            let pr2: u8 = if pu.kind == 0 { 120 as u8 } elif pu.kind == 1 { 255 as u8 } else { 200 as u8 }
            let pg2: u8 = if pu.kind == 0 { 220 as u8 } elif pu.kind == 1 { 220 as u8 } else { 120 as u8 }
            let pb2: u8 = if pu.kind == 0 { 255 as u8 } elif pu.kind == 1 { 120 as u8 } else { 255 as u8 }
            let chip_x = (pu.x as i32) - 18 + shake_off_x
            let chip_y = (pu.y as i32) - 8 + shake_off_y
            fillNeonStadium(chip_x, chip_y, 36, 16, pr2, pg2, pb2)
            let label: string = if pu.kind == 0 { "W" } elif pu.kind == 1 { "M" } else { "S" }
            font.drawText(ren, label, chip_x + 14, chip_y + 2,
                          2, 20 as u8, 10 as u8, 30 as u8, 255 as u8)
            pri = pri + 1
        }

        // Paddle (white/cyan glow, rounded ends). The colour shifts
        // toward magenta while the WIDER power-up is active so the
        // boost is obvious.
        let pr: u8 = if wider_timer > 0 { 255 as u8 } else { 200 as u8 }
        let pg: u8 = if wider_timer > 0 { 180 as u8 } else { 240 as u8 }
        let pb: u8 = if wider_timer > 0 { 255 as u8 } else { 255 as u8 }
        fillNeonStadium(paddle_x as i32 + shake_off_x, paddle_y + shake_off_y,
                        paddle_w, PADDLE_H, pr, pg, pb)

        // Ball trail — fade from oldest (just-overwritten slot) to
        // newest (slot before head). Draw as soft circles on top of
        // the background but beneath the ball itself.
        let tr_i: i32 = 0
        while tr_i < TRAIL_LEN {
            // age 0 = oldest (trail_head), age TRAIL_LEN-1 = newest
            let slot = (trail_head + tr_i) % TRAIL_LEN
            let pt = trail[slot]
            let age_norm = (tr_i as f64) / (TRAIL_LEN as f64)
            let a = (age_norm * age_norm * 140.0) as u8
            if a > 0 as u8 {
                let cx = (pt.x + (BALL_SIZE as f64) / 2.0) as i32 + shake_off_x
                let cy = (pt.y + (BALL_SIZE as f64) / 2.0) as i32 + shake_off_y
                let radius = (BALL_SIZE / 2) + (age_norm * 4.0) as i32
                ren.setColor(255, 255, 255, a)
                let dy: i32 = -radius
                while dy <= radius {
                    let dyf = dy as f64
                    let radf = radius as f64
                    let w = math.sqrt(radf * radf - dyf * dyf)
                    let half = w as i32
                    ren.fillRect(cx - half, cy + dy, half * 2 + 1, 1)
                    dy += 1
                }
            }
            tr_i = tr_i + 1
        }

        // Ball (round).
        let bri: i32 = 0
        let brn: i32 = balls.length as i32
        while bri < brn {
            let bb = balls[bri]
            let ball_cx = (bb.x + (BALL_SIZE as f64) / 2.0) as i32 + shake_off_x
            let ball_cy = (bb.y + (BALL_SIZE as f64) / 2.0) as i32 + shake_off_y
            fillNeonCircle(ball_cx, ball_cy, BALL_SIZE / 2, 255, 255, 255)
            bri = bri + 1
        }

        // 6px advance per glyph at scale=1 (5px cell + 1px space),
        // last glyph contributes only 5px → final width formula
        // below. Used for centering and right-alignment.
        let textPxW = fn(s: string, scale: i32): i32 {
            let nch = s.length as i32
            if nch == 0 { return 0 }
            (nch * 6 - 1) * scale
        }

        // Score popups — part of the world so they pick up shake
        // and chromatic aberration.
        let pop_i: i32 = 0
        let pop_n: i32 = popups.length as i32
        while pop_i < pop_n {
            let pp = popups[pop_i]
            let alpha_i = pp.life * 6
            let pa: u8 = if alpha_i > 255 { 255 as u8 } else { alpha_i as u8 }
            let scale_p: i32 = 2
            let tw = textPxW(pp.text, scale_p)
            font.drawText(ren, pp.text,
                          (pp.x as i32) - tw / 2 + shake_off_x,
                          (pp.y as i32) + shake_off_y,
                          scale_p, pp.r, pp.g, pp.b, pa)
            pop_i = pop_i + 1
        }

        // ─── HUD (drawn into scene_tex so it gets the same CRT
        // ─── post-effects — chromatic aberration, scanlines, and
        // ─── vignette — as the world).

        font.drawText(ren, "SCORE: " + intToStr(score),
                      20, 20, 2, 200, 220, 255, 255)
        let fps_txt = "FPS: " + intToStr(fps)
        font.drawText(ren, fps_txt,
                      20, 50, 2, 180, 200, 220, 255)

        // Active power-up countdowns (in whole seconds, 60fps).
        let pu_y: i32 = 80
        if wider_timer > 0 {
            let s = "WIDER " + intToStr((wider_timer + 59) / 60) + "S"
            font.drawText(ren, s, 20, pu_y, 2, 120, 220, 255, 255)
            pu_y = pu_y + 24
        }
        if slow_timer > 0 {
            let s = "SLOW " + intToStr((slow_timer + 59) / 60) + "S"
            font.drawText(ren, s, 20, pu_y, 2, 200, 120, 255, 255)
            pu_y = pu_y + 24
        }
        let lives_txt = "LIVES: " + intToStr(lives)
        font.drawText(ren, lives_txt,
                      WIN_W - textPxW(lives_txt, 2) - 20, 20,
                      2, 255, 200, 220, 255)

        // Combo readout — only shown while a streak is active so
        // it doesn't take up HUD real-estate at idle.
        if combo > 1 {
            let combo_txt = "COMBO x" + intToStr(combo)
            font.drawText(ren, combo_txt,
                          (WIN_W - textPxW(combo_txt, 3)) / 2, 80,
                          3, 255, 220, 120, 255)
        }

        if !started && !game_over {
            let s = "PRESS SPACE TO START"
            font.drawText(ren, s,
                          (WIN_W - textPxW(s, 3)) / 2, 400,
                          3, 200, 220, 255, 255)
        }

        if game_over {
            if won {
                let t = "CLEAR!"
                font.drawText(ren, t,
                              (WIN_W - textPxW(t, 6)) / 2, 300,
                              6, 120, 255, 200, 255)
            } else {
                let t = "GAME OVER"
                font.drawText(ren, t,
                              (WIN_W - textPxW(t, 6)) / 2, 300,
                              6, 255, 100, 200, 255)
            }
            let s = "PRESS R TO RESTART"
            font.drawText(ren, s,
                          (WIN_W - textPxW(s, 3)) / 2, 400,
                          3, 200, 220, 255, 255)
        }

        // ─── bloom prep: downsample the scene into bloom_tex ─────
        // Linear filtering averages 4×4 source blocks into each
        // bloom pixel; later we'll upscale this back to full
        // resolution which spreads the average into a soft halo.
        ren.setTarget(bloom_tex)
        ren.setColor(0, 0, 0, 0)
        ren.clear()
        ren.drawTexture(scene_tex, 0, 0, BLOOM_W, BLOOM_H)

        // ─── composite scene_tex (world + HUD) to window with
        // ─── chromatic aberration. ────────────────────────────────
        ren.resetTarget()
        ren.setColor(0, 0, 0, 255)
        ren.clear()
        // Three additive passes, each keeping only one colour
        // channel and offset slightly. Pixel overlap reproduces the
        // original colour; high-contrast edges (including text)
        // show RGB fringing.
        scene_tex.setBlendMode(2)   // SDL_BLENDMODE_ADD
        let chroma: i32 = 3
        scene_tex.setColorMod(255 as u8, 0 as u8, 0 as u8)
        ren.drawTexture(scene_tex, -chroma, 0, WIN_W, WIN_H)
        scene_tex.setColorMod(0 as u8, 255 as u8, 0 as u8)
        ren.drawTexture(scene_tex, 0, 0, WIN_W, WIN_H)
        scene_tex.setColorMod(0 as u8, 0 as u8, 255 as u8)
        ren.drawTexture(scene_tex, chroma, 0, WIN_W, WIN_H)
        // Restore for next frame's setTarget pass.
        scene_tex.setBlendMode(1)   // SDL_BLENDMODE_BLEND
        scene_tex.setColorMod(255 as u8, 255 as u8, 255 as u8)

        // ─── bloom: additively overlay the upscaled blurred
        // ─── version, brightening hot spots into a soft glow.
        // Without a shader-side bright-pass we instead darken the
        // whole bloom contribution via colour-mod so mid-brightness
        // pixels don't push the composite to white. The result is
        // a soft glow only on the hottest parts of the scene.
        bloom_tex.setBlendMode(2)
        bloom_tex.setColorMod(60 as u8, 60 as u8, 60 as u8)
        bloom_tex.setAlphaMod(255 as u8)
        ren.drawTexture(bloom_tex, 0, 0, WIN_W, WIN_H)
        bloom_tex.setBlendMode(1)
        bloom_tex.setColorMod(255 as u8, 255 as u8, 255 as u8)

        // CRT overlay (scanlines + vignette).
        ren.drawTexture(crt_tex, 0, 0, WIN_W, WIN_H)

        ren.present()

        // Sleep just long enough to keep the loop pinned to 60fps.
        // If the previous frame ran long, skip the wait but advance
        // the deadline so we don't accumulate a huge backlog.
        next_frame_ms = next_frame_ms + frame_period_ms
        let now_pace = sdl.getTicks() as f64
        let wait_ms = next_frame_ms - now_pace
        if wait_ms > 0.0 {
            sdl.delay(wait_ms as u32)
        } else {
            if wait_ms < -100.0 {
                next_frame_ms = now_pace
            }
        }
    }
}

let rc = sdl.init(sdl.INIT_VIDEO | sdl.INIT_AUDIO | sdl.INIT_GAMECONTROLLER)
if rc < 0 {
    console.log("SDL_Init failed: " + sdl.getError())
    return
}
play()
sdl.quit()
