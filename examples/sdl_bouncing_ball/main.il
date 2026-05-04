// 🌃 ilang × SDL2 — neon-style breakout (single stage).
//
// Controls:
//   ←/→  or  A/D  — move the paddle
//   ESC          — quit
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

// ─── audio: short sine-wave beeps ─────────────────────────────────

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

// ─── game data ────────────────────────────────────────────────────

class Brick {
    x: i32
    y: i32
    r: u8
    g: u8
    b: u8
    alive: bool
    init(x: i32, y: i32, r: u8, g: u8, b: u8) {
        this.x = x
        this.y = y
        this.r = r
        this.g = g
        this.b = b
        this.alive = true
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

    let ren = new sdl.Renderer(win.handle)
    if ren.handle == 0 {
        console.log("SDL_CreateRenderer failed: " + sdl.getError())
        return
    }

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

    let beep_wall = makeBeep(440.0, 40)
    let beep_paddle = makeBeep(660.0, 50)
    let beep_brick = makeBeep(880.0, 70)

    // White star sprite for particle bursts. Tinted at draw time
    // via `setColorMod`, rotated via `drawTextureRot`.
    let star_tex = new sdl.Texture(ren, STAR_TEX_SIZE, STAR_TEX_SIZE)
    if star_tex.handle == 0 {
        console.log("SDL_CreateTexture failed: " + sdl.getError())
        return
    }
    star_tex.update(make_star_sprite())

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
    let ball_x: f64 = (WIN_W / 2) as f64
    let ball_y: f64 = (WIN_H / 2) as f64
    let ball_vx: f64 = 5.0
    let ball_vy: f64 = -5.0
    let score: i32 = 0
    let bricks_left: i32 = COLS * ROWS
    let lives: i32 = 5
    let started: bool = false
    let game_over: bool = false
    let won: bool = false

    // Visual effect state.
    let wave_phase: f64 = 0.0       // brick highlight wave (rad)

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
        trail.push(new TrailPoint(ball_x, ball_y))
        trail_init = trail_init + 1
    }
    let trail_head: i32 = 0

    console.log("playing — ESC to quit")

    let running = true
    while running {
        sdl.pumpEvents()

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

        if sdl.Keyboard.isPressed(sdl.SCANCODE_ESCAPE) {
            running = false
        }

        // Restart after game over.
        if game_over && sdl.Keyboard.isPressed(sdl.SCANCODE_R) {
            let r2: i32 = 0
            while r2 < ROWS {
                let c2: i32 = 0
                while c2 < COLS {
                    bricks[r2 * COLS + c2].alive = true
                    c2 += 1
                }
                r2 += 1
            }
            ball_vx = 5.0
            ball_vy = -5.0
            score = 0
            bricks_left = COLS * ROWS
            lives = 5
            started = false
            game_over = false
            won = false
            let empty: Particle[] = []
            particles = empty
        }

        // ─── input ────────────────────────────────────────────────

        if sdl.Keyboard.isPressed(sdl.SCANCODE_LEFT) || sdl.Keyboard.isPressed(sdl.SCANCODE_A) {
            paddle_x = paddle_x - PADDLE_SPEED
        }
        if sdl.Keyboard.isPressed(sdl.SCANCODE_RIGHT) || sdl.Keyboard.isPressed(sdl.SCANCODE_D) {
            paddle_x = paddle_x + PADDLE_SPEED
        }
        if paddle_x < 0.0 {
            paddle_x = 0.0
        }
        let paddle_max: f64 = (WIN_W - PADDLE_W) as f64
        if paddle_x > paddle_max {
            paddle_x = paddle_max
        }

        // Space to launch the ball (idle on paddle until pressed).
        if !started && !game_over && sdl.Keyboard.isPressed(sdl.SCANCODE_SPACE) {
            started = true
        }
        // While idle, keep the ball glued to the paddle's centre.
        if !started {
            ball_x = paddle_x + (PADDLE_W as f64) / 2.0 - (BALL_SIZE as f64) / 2.0
            ball_y = (paddle_y as f64) - (BALL_SIZE as f64)
        }

        // ─── ball physics ─────────────────────────────────────────

        if started && !game_over {
            ball_x = ball_x + ball_vx
            ball_y = ball_y + ball_vy

            // Walls.
            if ball_x < 0.0 {
                ball_x = 0.0
                ball_vx = -ball_vx
                audio.queue(beep_wall)
            }
            if ball_x + (BALL_SIZE as f64) > WIN_W as f64 {
                ball_x = (WIN_W - BALL_SIZE) as f64
                ball_vx = -ball_vx
                audio.queue(beep_wall)
            }
            if ball_y < 0.0 {
                ball_y = 0.0
                ball_vy = -ball_vy
                audio.queue(beep_wall)
            }

            // Paddle (only on the way down).
            let bx_lo = ball_x
            let bx_hi = ball_x + (BALL_SIZE as f64)
            let by_lo = ball_y
            let by_hi = ball_y + (BALL_SIZE as f64)
            let px_lo = paddle_x
            let px_hi = paddle_x + (PADDLE_W as f64)
            let py_lo = paddle_y as f64
            let py_hi = (paddle_y + PADDLE_H) as f64
            if ball_vy > 0.0 && bx_hi > px_lo && bx_lo < px_hi
                    && by_hi > py_lo && by_lo < py_hi {
                ball_y = py_lo - (BALL_SIZE as f64)
                // Classic Breakout deflection: hit position on the
                // paddle (-1 left edge … +1 right edge) maps to an
                // exit angle, while keeping the ball's overall speed
                // constant. The further from centre, the steeper.
                let hit_centre = px_lo + (PADDLE_W as f64) / 2.0
                let ball_centre = ball_x + (BALL_SIZE as f64) / 2.0
                let rel = (ball_centre - hit_centre) / ((PADDLE_W as f64) / 2.0)
                let rel_clamped: f64 = if rel < -1.0 { -1.0 } elif rel > 1.0 { 1.0 } else { rel }
                let max_angle: f64 = 1.04   // ≈60°
                let angle = rel_clamped * max_angle
                let speed = math.sqrt(ball_vx * ball_vx + ball_vy * ball_vy)
                ball_vx = speed * math.sin(angle)
                ball_vy = -speed * math.cos(angle)
                audio.queue(beep_paddle)
            }

            // Bricks. First-hit-wins-this-frame so we don't bounce
            // through two bricks in a single tick.
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
                        bricks_left = bricks_left - 1
                        score = score + 100
                        // Decide which axis to flip based on overlap depth.
                        let overlap_x = if ball_vx > 0.0 { bx_hi - rx_lo } else { rx_hi - bx_lo }
                        let overlap_y = if ball_vy > 0.0 { by_hi - ry_lo } else { ry_hi - by_lo }
                        if overlap_x < overlap_y {
                            ball_vx = -ball_vx
                        } else {
                            ball_vy = -ball_vy
                        }
                        let cx = (br.x as f64) + (BRICK_W as f64) / 2.0
                        let cy = (br.y as f64) + (BRICK_H as f64) / 2.0
                        spawnParticles(particles, cx, cy, br.r, br.g, br.b)
                        audio.queue(beep_brick)
                        i = n   // stop after first hit this frame
                    }
                }
                i = i + 1
            }

            // Ball below the bottom: lose a life and respawn at
            // mid-screen. When lives run out the round ends.
            if ball_y > WIN_H as f64 {
                lives = lives - 1
                if lives <= 0 {
                    game_over = true
                    won = false
                } else {
                    started = false
                    ball_vx = 5.0
                    ball_vy = -5.0
                }
            }
            // Win: all bricks cleared.
            if bricks_left <= 0 {
                game_over = true
                won = true
            }
        }

        // Record ball position into the trail ring buffer.
        let tp = trail[trail_head]
        tp.x = ball_x
        tp.y = ball_y
        trail_head = (trail_head + 1) % TRAIL_LEN

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
            ren.fillRect(s.x as i32 - half, s.y as i32 - half,
                         half * 2 + 1, half * 2 + 1)
            bg_i = bg_i + 1
        }

        // Bricks with a left-to-right brightness wave.
        let i2: i32 = 0
        let n2: i32 = bricks.length as i32
        while i2 < n2 {
            let br = bricks[i2]
            if br.alive {
                fillNeon(br.x, br.y, BRICK_W, BRICK_H, br.r, br.g, br.b)
                // Wave highlight: a soft white pass whose alpha
                // peaks where `wave_phase - x*k` crosses zero.
                let k: f64 = 0.012
                let s = math.sin(wave_phase - (br.x as f64) * k)
                let pulse: f64 = if s > 0.0 { s * s } else { 0.0 }
                let glow_a = (pulse * 160.0) as u8
                if glow_a > 0 as u8 {
                    ren.setColor(255 as u8, 255 as u8, 255 as u8, glow_a)
                    ren.fillRect(br.x - 2, br.y - 2, BRICK_W + 4, BRICK_H + 4)
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
            ren.drawTextureRot(star_tex.handle,
                               (p.x as i32) - halo_size / 2,
                               (p.y as i32) - halo_size / 2,
                               halo_size, halo_size, p.angle)
            // Sharp star body (2× the previous 16px).
            star_tex.setAlphaMod(a)
            let size: i32 = 32
            ren.drawTextureRot(star_tex.handle,
                               (p.x as i32) - size / 2,
                               (p.y as i32) - size / 2,
                               size, size, p.angle)
            i3 = i3 + 1
        }

        // Paddle (white/cyan glow, rounded ends).
        fillNeonStadium(paddle_x as i32, paddle_y, PADDLE_W, PADDLE_H, 200, 240, 255)

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
                let cx = (pt.x + (BALL_SIZE as f64) / 2.0) as i32
                let cy = (pt.y + (BALL_SIZE as f64) / 2.0) as i32
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
        let ball_cx = (ball_x + (BALL_SIZE as f64) / 2.0) as i32
        let ball_cy = (ball_y + (BALL_SIZE as f64) / 2.0) as i32
        fillNeonCircle(ball_cx, ball_cy, BALL_SIZE / 2, 255, 255, 255)

        // ─── HUD ─────────────────────────────────────────────────
        // 6px advance per glyph at scale=1 (5px cell + 1px space),
        // last glyph contributes only 5px → final width formula
        // below. Used for centering and right-alignment.
        let textPxW = fn(s: string, scale: i32): i32 {
            let nch = s.length as i32
            if nch == 0 { return 0 }
            (nch * 6 - 1) * scale
        }

        font.drawText(ren, "SCORE: " + intToStr(score),
                      20, 20, 2, 200, 220, 255, 255)
        let fps_txt = "FPS: " + intToStr(fps)
        font.drawText(ren, fps_txt,
                      20, 50, 2, 180, 200, 220, 255)
        let lives_txt = "LIVES: " + intToStr(lives)
        font.drawText(ren, lives_txt,
                      WIN_W - textPxW(lives_txt, 2) - 20, 20,
                      2, 255, 200, 220, 255)

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

let rc = sdl.init(sdl.INIT_VIDEO | sdl.INIT_AUDIO)
if rc < 0 {
    console.log("SDL_Init failed: " + sdl.getError())
    return
}
play()
sdl.quit()
