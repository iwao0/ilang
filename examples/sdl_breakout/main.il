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
//   ilang run --jit examples/sdl_breakout/main.il
//
// The SDL2 bindings live under bindings/sdl2/; the `ilang.toml` in
// this directory points at them so `use sdl` resolves automatically.

use sdl
use math
use sprites
use audio
use palette
use ball
use brick
use particle
use powerup
use star
use neon

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

// Brick drop-in intro tuning. With STAGGER=4 and COLS=9 the last
// column starts at frame 32 and finishes 24 frames later → ~0.93s
// total, ≈0.5s "perceived" since most bricks are already settled
// by then.
const INTRO_STAGGER: i32 = 4
const INTRO_DURATION: i32 = 24
const INTRO_DROP_PX: f64 = 280.0

// ─── tiny utilities ───────────────────────────────────────────────

// Tiny LCG so particles / stars can fly off in pseudo-random
// directions without pulling in a real RNG dependency. Lives in
// main.il (not its own module) because ilang's loader currently
// double-registers fns on diamond `use`s; only one file owns the
// rng so we never trigger that.
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

fn spawnParticles(particles: particle.Particle[], cx: f64, cy: f64,
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
        let startAngle = randf() * 360.0
        particles.push(
            new particle.Particle(cx, cy, vx, vy, startAngle, spin, life, r, g, b)
        )
        i += 1
    }
}

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

    // VSync ties `present()` to the display refresh, smoothing
    // frame delivery. Without it the manual `sdl.delay` pacing
    // below visibly stutters even at a steady 60fps.
    let ren = new sdl.Renderer(win, -1, sdl.RendererFlags.presentVsync as u32)
    if ren.handle == 0 {
        console.log("SDL_CreateRenderer failed: " + sdl.getError())
        return
    }

    // Pin the renderer to our virtual WIN_W×WIN_H so toggling
    // fullscreen-desktop just changes the window backing — the
    // game's coordinates stay the same and SDL letterboxes to fit.
    ren.setLogicalSize(WIN_W, WIN_H)

    // Wraps `ren` and exposes neon-style fill helpers (rect /
    // rounded rect / stadium / circle), all with a 3-pass halo to
    // fake the glow of a saturated primary on a dark background.
    let neonR = new neon.NeonRenderer(ren)

    let font = new sdl.Font(ren)

    let sfxDevice = new sdl.AudioDevice(audio.SAMPLE_RATE, 1)
    if sfxDevice.id == 0 {
        console.log("SDL_OpenAudioDevice failed: " + sdl.getError())
        return
    }
    sfxDevice.play()

    // Second audio device dedicated to BGM. SDL allows multiple
    // simultaneous open devices, so the music keeps playing even
    // while SFX queue/dequeue rapidly on the first device.
    let audioMusic = new sdl.AudioDevice(audio.SAMPLE_RATE, 1)
    let bgm = audio.makeBgmLoop()
    let bgmBytes = (bgm.length * 2) as u32   // one loop in queued-bytes
    if audioMusic.id != 0 {
        // Pre-buffer two loops so the first refill check has slack.
        audioMusic.queue(bgm)
        audioMusic.queue(bgm)
        audioMusic.play()
    }

    // Open the first attached gamepad if any. `pad.handle == 0`
    // when nothing is plugged in — every poll degrades to
    // returning 0/false, so the keyboard path keeps working.
    let pad = new sdl.Gamepad()

    let beepWall = audio.makeBeep(440.0, 40)
    let beepPaddle = audio.makeBeep(660.0, 50)
    let beepBrick = audio.makeBeep(880.0, 70)

    // White star sprite for particle bursts. Tinted at draw time
    // via `setColorMod`, rotated via `drawTextureRot`.
    let starTex = new sdl.Texture(ren, sprites.STAR_TEX_SIZE, sprites.STAR_TEX_SIZE, 1)
    if starTex.handle == 0 {
        console.log("SDL_CreateTexture failed: " + sdl.getError())
        return
    }
    starTex.update(sprites.makeStarSprite())

    // Soft radial-gradient sprite, tinted per-brick at draw time so
    // the wave highlight blooms as a blurry circle rather than a
    // rectangle.
    let glowTex = new sdl.Texture(ren, sprites.GLOW_TEX_SIZE, sprites.GLOW_TEX_SIZE, 1)
    if glowTex.handle == 0 {
        console.log("SDL_CreateTexture failed: " + sdl.getError())
        return
    }
    glowTex.update(sprites.makeGlowSprite())

    // Full-window CRT-effect overlay — built once. The alpha
    // channel encodes scanlines + vignette, the colour is black,
    // so blending it on top darkens each pixel by the right
    // amount.
    let crtTex = new sdl.Texture(ren, WIN_W, WIN_H, 1)
    if crtTex.handle == 0 {
        console.log("SDL_CreateTexture failed: " + sdl.getError())
        return
    }
    crtTex.update(sprites.makeCrtOverlay(WIN_W, WIN_H))

    // Off-screen accumulator for the world. Drawn into each frame
    // (with a small per-frame fade applied first) and then composited
    // back to the window with chromatic-aberration channel offsets.
    // SDL_TEXTUREACCESS_TARGET = 2.
    let sceneTex = new sdl.Texture(ren, WIN_W, WIN_H, 2)
    if sceneTex.handle == 0 {
        console.log("SDL_CreateTexture(TARGET) failed: " + sdl.getError())
        return
    }
    // Linear filtering on the scene texture so that the bloom
    // pass's downscale + upscale produces a smooth blur instead of
    // a chunky pixel grid.
    sceneTex.setScaleMode(1)

    // Tiny target used to "blur" the scene. We draw sceneTex
    // shrunk into bloomTex (each downsample averages a 4×4 block
    // thanks to linear filtering), and later draw bloomTex back
    // out at full window size additively — the upscale spreads
    // each averaged pixel across many output pixels, giving a
    // soft glow around the bright stuff in the scene.
    let BLOOM_W: i32 = 225              // WIN_W / 4
    let BLOOM_H: i32 = 175              // WIN_H / 4
    let bloomTex = new sdl.Texture(ren, BLOOM_W, BLOOM_H, 2)
    if bloomTex.handle == 0 {
        console.log("SDL_CreateTexture(bloom) failed: " + sdl.getError())
        return
    }
    bloomTex.setScaleMode(1)

    // Background star field — scrolls top→down with a twinkle.
    let bgStars: star.Star[] = []
    let sI: i32 = 0
    while sI < 80 {
        bgStars.push(new star.Star(
            randf() * (WIN_W as f64),
            randf() * (WIN_H as f64),
            0.4 + randf() * 1.6,
            randf() * 6.2831853,
            0.04 + randf() * 0.06
        ))
        sI += 1
    }

    // Build the brick grid.
    let bricks: brick.Brick[] = []
    let row: i32 = 0
    while row < ROWS {
        let col: i32 = 0
        while col < COLS {
            let x = col * (BRICK_W + BRICK_GAP) +
                    (WIN_W - COLS * (BRICK_W + BRICK_GAP) + BRICK_GAP) / 2
            let y = BRICK_TOP + row * (BRICK_H + BRICK_GAP)
            bricks.push(new brick.Brick(x, y, palette.paletteR(row), palette.paletteG(row), palette.paletteB(row)))
            col += 1
        }
        row += 1
    }

    // State.
    let paddleX: f64 = ((WIN_W - PADDLE_W) / 2) as f64
    let paddleY: i32 = WIN_H - 60
    let balls: ball.Ball[] = []
    balls.push(new ball.Ball((WIN_W / 2) as f64, (WIN_H / 2) as f64, 5.0, -5.0))
    let score: i32 = 0
    let bricksLeft: i32 = COLS * ROWS
    let lives: i32 = 5
    let started: bool = false
    let gameOver: bool = false
    let won: bool = false
    let combo: i32 = 0              // bricks hit since last paddle bounce
    let shake: f64 = 0.0            // current screen-shake amplitude (px)
    let popups: particle.Popup[] = []
    // Frames the ball has been in flight (this game). Drives the
    // gradual speed-up. Doesn't reset on lose-life so tension keeps
    // building across attempts.
    let playTime: i32 = 0
    // Frames since the last (re)start. Drives the brick drop-in
    // intro and lets us trigger a single big firework burst the
    // moment the level is cleared.
    let introPhase: i32 = 0
    let wonCelebrated: bool = false
    // Power-ups falling from broken bricks. Timers count down per
    // frame while > 0; while their flag is active the matching
    // effect is applied. Multi-ball is one-shot so it doesn't need
    // a timer.
    let powerups: powerup.PowerUp[] = []
    let widerTimer: i32 = 0
    let slowTimer: i32 = 0
    // Tracks the paddle width from the previous frame so we can
    // re-anchor `paddleX` (which is the LEFT edge) when WIDER
    // turns on or off — the visible paddle centre stays put.
    let prevPaddleW: i32 = PADDLE_W

    // Visual effect state.
    let wavePhase: f64 = 0.0       // brick highlight wave (rad)

    // F toggles fullscreen — track edge so a held key doesn't flap.
    let fullscreen: bool = false
    let fWasDown: bool = false

    // FPS counter.
    let fps: i32 = 0
    let fpsFrames: i32 = 0
    let fpsLastMs: u32 = sdl.getTicks()

    // Frame pacing: 1000/60 ≈ 16.667ms target. Track the next-frame
    // deadline as f64 so the rounding error doesn't drift the
    // average down to 58fps.
    let nextFrameMs: f64 = sdl.getTicks() as f64
    let framePeriodMs: f64 = 1000.0 / 60.0
    let particles: particle.Particle[] = []

    // Trails live on each `Ball` (see the `trail` / `trailHead`
    // fields), so multi-ball renders an independent fading streak
    // for every active ball.

    console.log("playing — ESC to quit")

    let running = true
    while running {
        sdl.pumpEvents()
        sdl.Keyboard.pump()
        if sdl.quitRequested() {
            running = false
        }

        // FPS: count frames over a 1-second window.
        fpsFrames += 1
        let nowMs = sdl.getTicks()
        if nowMs - fpsLastMs >= 1000 {
            fps = fpsFrames
            fpsFrames = 0
            fpsLastMs = nowMs
        }

        // Brick highlight wave advances each frame.
        wavePhase = wavePhase + 0.08
        introPhase = introPhase + 1

        // Top up the BGM queue so the loop never drains. Polling
        // each frame is plenty cheap.
        if audioMusic.id != 0 && audioMusic.queuedBytes() < bgmBytes {
            audioMusic.queue(bgm)
        }

        // Clear-time celebration: one big multi-burst the instant
        // the level is won, then a steady stream while CLEAR! is
        // shown so the screen keeps lighting up.
        if gameOver && won {
            if !wonCelebrated {
                let bi: i32 = 0
                while bi < 12 {
                    let fx = randf() * (WIN_W as f64)
                    let fy = (0.15 + randf() * 0.55) * (WIN_H as f64)
                    let pick = (randf() * 5.0) as i32
                    spawnParticles(particles, fx, fy,
                                   palette.paletteR(pick), palette.paletteG(pick), palette.paletteB(pick))
                    bi += 1
                }
                wonCelebrated = true
            }
            // ~10 fireworks per second while celebrating.
            if randf() < 0.18 {
                let fx = randf() * (WIN_W as f64)
                let fy = (0.10 + randf() * 0.65) * (WIN_H as f64)
                let pick = (randf() * 5.0) as i32
                spawnParticles(particles, fx, fy,
                               palette.paletteR(pick), palette.paletteG(pick), palette.paletteB(pick))
            }
        }

        // Gamepad: left stick → paddle (analogue), DPAD ←/→ →
        // paddle (digital), A → launch, START → restart, BACK →
        // quit. The keyboard mappings below stack with these so
        // either device works.
        let padLx = pad.getAxis(sdl.Axis.leftX) as i32  // [-32768, 32767]
        let padDpadL = pad.isPressed(sdl.Button.dpadLeft)
        let padDpadR = pad.isPressed(sdl.Button.dpadRight)
        let padA = pad.isPressed(sdl.Button.a)
        let padStart = pad.isPressed(sdl.Button.start)
        let padBack = pad.isPressed(sdl.Button.back)

        if sdl.Keyboard.isPressed(sdl.Scancode.escape) || padBack {
            running = false
        }

        // F: toggle fullscreen on key-down (rising edge).
        let fNow = sdl.Keyboard.isPressed(sdl.Scancode.f)
        if fNow && !fWasDown {
            fullscreen = !fullscreen
            win.fullscreen = fullscreen
        }
        fWasDown = fNow

        // Restart after game over.
        if gameOver && (sdl.Keyboard.isPressed(sdl.Scancode.r) || padStart) {
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
            let freshBalls: ball.Ball[] = []
            freshBalls.push(new ball.Ball((WIN_W / 2) as f64, (WIN_H / 2) as f64, 5.0, -5.0))
            balls = freshBalls
            score = 0
            bricksLeft = COLS * ROWS
            lives = 5
            started = false
            gameOver = false
            won = false
            let emptyPu: powerup.PowerUp[] = []
            powerups = emptyPu
            widerTimer = 0
            slowTimer = 0
            prevPaddleW = PADDLE_W
            let empty: particle.Particle[] = []
            particles = empty
            let emptyPops: particle.Popup[] = []
            popups = emptyPops
            combo = 0
            shake = 0.0
            playTime = 0
            introPhase = 0
            wonCelebrated = false
        }

        // ─── input ────────────────────────────────────────────────

        // Effective paddle width — temporarily widened by the
        // WIDER power-up. Used by both physics and rendering.
        let paddleWWide: i32 = PADDLE_W + (PADDLE_W as i32) / (2 as i32)
        let paddleW: i32 = if widerTimer > 0 { paddleWWide } else { PADDLE_W }
        // Re-anchor on width change so the paddle grows/shrinks
        // around its centre rather than its left edge.
        if paddleW != prevPaddleW {
            let delta = paddleW - prevPaddleW
            paddleX = paddleX - (delta as f64) / 2.0
            prevPaddleW = paddleW
        }

        if sdl.Keyboard.isPressed(sdl.Scancode.left) || sdl.Keyboard.isPressed(sdl.Scancode.a) || padDpadL {
            paddleX = paddleX - PADDLE_SPEED
        }
        if sdl.Keyboard.isPressed(sdl.Scancode.right) || sdl.Keyboard.isPressed(sdl.Scancode.d) || padDpadR {
            paddleX = paddleX + PADDLE_SPEED
        }
        // Analogue stick: deadzone ~24% then linear scale to paddle
        // speed. Ignored when the stick is centred.
        let absLx: i32 = if padLx < 0 { -padLx } else { padLx }
        if absLx > 8000 {
            let norm = (padLx as f64) / 32767.0
            paddleX = paddleX + norm * PADDLE_SPEED
        }
        if paddleX < 0.0 {
            paddleX = 0.0
        }
        let paddleMax: f64 = (WIN_W - paddleW) as f64
        if paddleX > paddleMax {
            paddleX = paddleMax
        }

        // Space to launch the ball (idle on paddle until pressed).
        // Wait for the brick drop-in animation to finish before
        // accepting the launch input — otherwise the player can
        // shoot through unfilled rows.
        let introTotal = (COLS - 1) * INTRO_STAGGER + INTRO_DURATION
        if !started && !gameOver && introPhase >= introTotal &&
                (sdl.Keyboard.isPressed(sdl.Scancode.space) || padA) {
            started = true
        }

        // While idle, keep the (first) ball glued to the paddle's
        // centre.
        if !started && balls.length as i32 > 0 {
            let b0 = balls[0]
            b0.x = paddleX + (paddleW as f64) / 2.0 - (BALL_SIZE as f64) / 2.0
            b0.y = (paddleY as f64) - (BALL_SIZE as f64)
        }

        // ─── ball physics ─────────────────────────────────────────

        if started && !gameOver {
            // Slow-mo divides ball velocity step. Renormalize uses
            // the same factor so target magnitude shrinks.
            let speedFactor: f64 = if slowTimer > 0 { 0.55 } else { 1.0 }

            let surviving: ball.Ball[] = []
            let bi: i32 = 0
            let bn: i32 = balls.length as i32
            while bi < bn {
                let b = balls[bi]
                b.x = b.x + b.vx * speedFactor
                b.y = b.y + b.vy * speedFactor

                // Walls.
                if b.x < 0.0 {
                    b.x = 0.0
                    b.vx = -b.vx
                    sfxDevice.queue(beepWall)
                }
                if b.x + (BALL_SIZE as f64) > WIN_W as f64 {
                    b.x = (WIN_W - BALL_SIZE) as f64
                    b.vx = -b.vx
                    sfxDevice.queue(beepWall)
                }
                if b.y < 0.0 {
                    b.y = 0.0
                    b.vy = -b.vy
                    sfxDevice.queue(beepWall)
                }

                // Paddle (only on the way down).
                let bxLo = b.x
                let bxHi = b.x + (BALL_SIZE as f64)
                let byLo = b.y
                let byHi = b.y + (BALL_SIZE as f64)
                let pxLo = paddleX
                let pxHi = paddleX + (paddleW as f64)
                let pyLo = paddleY as f64
                let pyHi = (paddleY + PADDLE_H) as f64
                if b.vy > 0.0 && bxHi > pxLo && bxLo < pxHi
                        && byHi > pyLo && byLo < pyHi {
                    b.y = pyLo - (BALL_SIZE as f64)
                    let hitCentre = pxLo + (paddleW as f64) / 2.0
                    let ballCentre = b.x + (BALL_SIZE as f64) / 2.0
                    let rel = (ballCentre - hitCentre) / ((paddleW as f64) / 2.0)
                    let relClamped: f64 = if rel < -1.0 { -1.0 } elif rel > 1.0 { 1.0 } else { rel }
                    let maxAngle: f64 = 1.04   // ≈60°
                    let angle = relClamped * maxAngle
                    let speed = math.sqrt(b.vx * b.vx + b.vy * b.vy)
                    b.vx = speed * math.sin(angle)
                    b.vy = -speed * math.cos(angle)
                    combo = 0
                    sfxDevice.queue(beepPaddle)
                }

                // Bricks. First-hit-wins-this-frame for this ball.
                let i: i32 = 0
                let n: i32 = bricks.length as i32
                while i < n {
                    let br = bricks[i]
                    if br.alive {
                        let rxLo = br.x as f64
                        let rxHi = (br.x + BRICK_W) as f64
                        let ryLo = br.y as f64
                        let ryHi = (br.y + BRICK_H) as f64
                        if bxHi > rxLo && bxLo < rxHi
                                && byHi > ryLo && byLo < ryHi {
                            br.alive = false
                            br.dying = 1.0
                            bricksLeft = bricksLeft - 1
                            combo = combo + 1
                            let earned = 100 * combo
                            score = score + earned
                            let overlapX = if b.vx > 0.0 { bxHi - rxLo } else { rxHi - bxLo }
                            let overlapY = if b.vy > 0.0 { byHi - ryLo } else { ryHi - byLo }
                            if overlapX < overlapY {
                                b.vx = -b.vx
                            } else {
                                b.vy = -b.vy
                            }
                            let cx = (br.x as f64) + (BRICK_W as f64) / 2.0
                            let cy = (br.y as f64) + (BRICK_H as f64) / 2.0
                            spawnParticles(particles, cx, cy, br.r, br.g, br.b)
                            popups.push(new particle.Popup(
                                cx, cy - 4.0, "+" + intToStr(earned),
                                br.r, br.g, br.b
                            ))
                            let bump = 4.0 + (combo as f64) * 1.5
                            shake = shake + bump
                            if shake > 14.0 { shake = 14.0 }
                            sfxDevice.queue(beepBrick)
                            // 14% drop chance per brick break.
                            if randf() < 0.14 {
                                let kind = (randf() * 3.0) as i32
                                powerups.push(new powerup.PowerUp(cx, cy, kind))
                            }
                            i = n   // stop after first hit this frame
                        }
                    }
                    i = i + 1
                }

                // Win: all bricks cleared. Don't break the ball
                // loop — remaining balls finish their frame.
                if bricksLeft <= 0 {
                    gameOver = true
                    won = true
                }

                // Speed-up renormalize per ball.
                let baseSpeed: f64 = 7.07106781   // sqrt(5^2 + 5^2)
                let timeBonusRaw = (playTime as f64) * 0.002
                let timeBonus: f64 = if timeBonusRaw > 6.0 { 6.0 } else { timeBonusRaw }
                let comboBonus = (combo as f64) * 0.35
                let target = (baseSpeed + timeBonus + comboBonus) * speedFactor
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
            playTime = playTime + 1

            // Out of balls → lose a life.
            if balls.length as i32 == 0 {
                lives = lives - 1
                if lives <= 0 {
                    gameOver = true
                    won = false
                } else {
                    started = false
                    combo = 0
                    let respawn: ball.Ball[] = []
                    respawn.push(new ball.Ball((WIN_W / 2) as f64, (WIN_H / 2) as f64, 5.0, -5.0))
                    balls = respawn
                }
            }
        }

        // Record every ball's current position into its own ring
        // buffer. Each Ball owns the buffer so multi-ball produces
        // independent trails.
        let trBi: i32 = 0
        let trBn: i32 = balls.length as i32
        while trBi < trBn {
            let bb = balls[trBi]
            let tp = bb.trail[bb.trailHead]
            tp.x = bb.x
            tp.y = bb.y
            bb.trailHead = (bb.trailHead + 1) % ball.TRAIL_LEN
            trBi = trBi + 1
        }

        // ─── power-up step: fall, catch, expire ──────────────────
        if widerTimer > 0 { widerTimer = widerTimer - 1 }
        if slowTimer > 0 { slowTimer = slowTimer - 1 }
        let alivePu: powerup.PowerUp[] = []
        let pui: i32 = 0
        let pun: i32 = powerups.length as i32
        while pui < pun {
            let pu = powerups[pui]
            pu.y = pu.y + pu.vy
            // Catch test: 36×16 chip centred on (pu.x, pu.y).
            let chipW: f64 = 36.0
            let chipH: f64 = 16.0
            let cxLo = pu.x - chipW / 2.0
            let cxHi = pu.x + chipW / 2.0
            let cyLo = pu.y - chipH / 2.0
            let cyHi = pu.y + chipH / 2.0
            let pdl = paddleX
            let pdr = paddleX + (paddleW as f64)
            let pdt = paddleY as f64
            let pdb = (paddleY + PADDLE_H) as f64
            let caught = cxHi > pdl && cxLo < pdr && cyHi > pdt && cyLo < pdb
            let removed: bool = caught
            if caught {
                if pu.kind == 0 {
                    widerTimer = 600           // 10s
                } elif pu.kind == 1 {
                    // Multi-ball: clone first ball into two new balls
                    // with slightly diverged angles.
                    if balls.length as i32 > 0 {
                        let src = balls[0]
                        let extra: ball.Ball[] = []
                        let ki: i32 = 0
                        while ki < 2 {
                            let dvx = if ki == 0 { src.vy * 0.5 } else { 0.0 - src.vy * 0.5 }
                            let dvy = if ki == 0 { 0.0 - src.vx * 0.5 } else { src.vx * 0.5 }
                            extra.push(new ball.Ball(src.x, src.y,
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
                    slowTimer = 480            // 8s
                }
                sfxDevice.queue(beepPaddle)
            }
            if !removed && pu.y < (WIN_H + 40) as f64 {
                alivePu.push(pu)
            }
            pui = pui + 1
        }
        powerups = alivePu

        // ─── particle step ───────────────────────────────────────

        let aliveParticles: particle.Particle[] = []
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
                aliveParticles.push(p)
            }
            i = i + 1
        }
        particles = aliveParticles

        // Score-popup step: drift up, fade, drop dead ones.
        let alivePopups: particle.Popup[] = []
        let pi: i32 = 0
        let pn: i32 = popups.length as i32
        while pi < pn {
            let pp = popups[pi]
            pp.y = pp.y + pp.vy
            pp.vy = pp.vy * 0.96       // ease toward stop
            pp.life = pp.life - 1
            if pp.life > 0 {
                alivePopups.push(pp)
            }
            pi = pi + 1
        }
        popups = alivePopups

        // Decay screen-shake exponentially. Compute per-frame
        // jitter offsets here so all draw calls share them.
        let shakeOffX: i32 = 0
        let shakeOffY: i32 = 0
        if shake > 0.5 {
            shakeOffX = ((randf() - 0.5) * 2.0 * shake) as i32
            shakeOffY = ((randf() - 0.5) * 2.0 * shake) as i32
        }
        shake = shake * 0.82
        if shake < 0.05 { shake = 0.0 }

        // Background star scroll + twinkle phase.
        let bsI: i32 = 0
        let bsN: i32 = bgStars.length as i32
        while bsI < bsN {
            let s = bgStars[bsI]
            s.y = s.y + s.speed
            s.phase = s.phase + s.twinkle
            if s.y > WIN_H as f64 {
                s.y = -2.0
                s.x = randf() * (WIN_W as f64)
            }
            bsI = bsI + 1
        }

        // ─── render ──────────────────────────────────────────────
        // The world is drawn into `sceneTex` so we can post-process
        // it (chromatic aberration). HUD lands directly on the
        // window afterwards so the readouts stay sharp.
        ren.setTarget(sceneTex)
        ren.setColor(8, 4, 24, 255)        // very dark navy
        ren.clear()

        // Background stars (drawn first, beneath everything).
        let bgI: i32 = 0
        let bgN: i32 = bgStars.length as i32
        while bgI < bgN {
            let s = bgStars[bgI]
            // Brightness pulses with phase.
            let bright = 0.4 + 0.6 * (0.5 + 0.5 * math.sin(s.phase))
            let a8 = (bright * 220.0) as u8
            ren.setColor(220, 220, 255, a8)
            // Bigger stars (random radius 1 or 2).
            let half: i32 = if s.speed > 1.2 { 2 } else { 1 }
            ren.fillRect(s.x as i32 - half + shakeOffX,
                         s.y as i32 - half + shakeOffY,
                         half * 2 + 1, half * 2 + 1)
            bgI = bgI + 1
        }

        // Bricks with a left-to-right brightness wave. Glow padding
        // is proportional to the brick's aspect ratio, so the
        // bloom around a wide brick is wider than tall (matching
        // the brick's shape).
        let glowPadY: i32 = 32
        let glowPadX: i32 = glowPadY * BRICK_W / BRICK_H
        let i2: i32 = 0
        let n2: i32 = bricks.length as i32
        while i2 < n2 {
            let br = bricks[i2]
            // Drop-in intro: each column is delayed by INTRO_STAGGER
            // frames so the grid cascades in left→right. The brick
            // falls from above its target with an ease-out curve.
            let col = i2 % COLS
            let ownPhase = introPhase - col * INTRO_STAGGER
            let introActive = ownPhase < INTRO_DURATION
            let introOffsetY: i32 = if ownPhase < 0 {
                // Pre-arrival: keep the brick well above the playfield.
                0 - ((INTRO_DROP_PX as i32) + 100)
            } elif ownPhase < INTRO_DURATION {
                let p = (ownPhase as f64) / (INTRO_DURATION as f64)
                let inv = 1.0 - p
                0 - ((inv * inv * INTRO_DROP_PX) as i32)
            } else {
                0
            }
            if br.alive {
                neonR.fillRoundedRect(br.x + shakeOffX,
                                    br.y + introOffsetY + shakeOffY,
                                    BRICK_W, BRICK_H, 6, br.r, br.g, br.b)
                // Wave glow only after the brick has landed — looks
                // odd to highlight a brick that's mid-fall.
                let k: f64 = 0.012
                let s = math.sin(wavePhase - (br.x as f64) * k)
                let pulse: f64 = if s > 0.0 { s * s } else { 0.0 }
                if pulse > 0.0 && !introActive {
                    let alpha = (pulse * 220.0) as u8
                    if alpha > 0 as u8 {
                        glowTex.setColorMod(br.r, br.g, br.b)
                        glowTex.setAlphaMod(alpha)
                        ren.drawTexture(glowTex,
                                        br.x - glowPadX + shakeOffX,
                                        br.y - glowPadY + shakeOffY,
                                        BRICK_W + 2 * glowPadX,
                                        BRICK_H + 2 * glowPadY)
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
                glowTex.setColorMod(br.r, br.g, br.b)
                glowTex.setAlphaMod((t * 180.0) as u8)
                let bw = ((BRICK_W + 2 * glowPadX) as f64) * scale
                let bh = ((BRICK_H + 2 * glowPadY) as f64) * scale
                ren.drawTexture(glowTex,
                                cx - (bw as i32) / 2 + shakeOffX,
                                cy - (bh as i32) / 2 + shakeOffY,
                                bw as i32, bh as i32)
                // Sharper core.
                if alpha > 0 as u8 {
                    ren.setColor(br.r, br.g, br.b, alpha)
                    ren.fillRect(cx - dw / 2 + shakeOffX,
                                 cy - dh / 2 + shakeOffY, dw, dh)
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
            starTex.setColorMod(p.r, p.g, p.b)
            // Outer halo: bigger, lower alpha. Drawn first so the
            // crisp star sits on top of its glow.
            let haloA = ((a as i32) / 3) as u8
            starTex.setAlphaMod(haloA)
            let haloSize: i32 = 56
            ren.drawTextureRot(starTex,
                               (p.x as i32) - haloSize / 2 + shakeOffX,
                               (p.y as i32) - haloSize / 2 + shakeOffY,
                               haloSize, haloSize, p.angle)
            // Sharp star body (2× the previous 16px).
            starTex.setAlphaMod(a)
            let size: i32 = 32
            ren.drawTextureRot(starTex,
                               (p.x as i32) - size / 2 + shakeOffX,
                               (p.y as i32) - size / 2 + shakeOffY,
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
            let chipX = (pu.x as i32) - 18 + shakeOffX
            let chipY = (pu.y as i32) - 8 + shakeOffY
            neonR.fillStadium(chipX, chipY, 36, 16, pr2, pg2, pb2)
            let label: string = if pu.kind == 0 { "W" } elif pu.kind == 1 { "M" } else { "S" }
            font.drawText(ren, label, chipX + 14, chipY + 2,
                          2, 20 as u8, 10 as u8, 30 as u8, 255 as u8)
            pri = pri + 1
        }

        // Paddle (white/cyan glow, rounded ends). The colour shifts
        // toward magenta while the WIDER power-up is active so the
        // boost is obvious.
        let pr: u8 = if widerTimer > 0 { 255 as u8 } else { 200 as u8 }
        let pg: u8 = if widerTimer > 0 { 180 as u8 } else { 240 as u8 }
        let pb: u8 = if widerTimer > 0 { 255 as u8 } else { 255 as u8 }
        neonR.fillStadium(paddleX as i32 + shakeOffX, paddleY + shakeOffY,
                        paddleW, PADDLE_H, pr, pg, pb)

        // Ball trails — one per ball, fading from oldest
        // (just-overwritten slot) to newest (slot before head).
        // Drawn beneath the ball itself.
        let tbI: i32 = 0
        let tbN: i32 = balls.length as i32
        while tbI < tbN {
            let bb = balls[tbI]
            let trI: i32 = 0
            while trI < ball.TRAIL_LEN {
                // age 0 = oldest (trailHead), age ball.TRAIL_LEN-1 = newest
                let slot = (bb.trailHead + trI) % ball.TRAIL_LEN
                let pt = bb.trail[slot]
                let ageNorm = (trI as f64) / (ball.TRAIL_LEN as f64)
                let a = (ageNorm * ageNorm * 140.0) as u8
                if a > 0 as u8 {
                    let cx = (pt.x + (BALL_SIZE as f64) / 2.0) as i32 + shakeOffX
                    let cy = (pt.y + (BALL_SIZE as f64) / 2.0) as i32 + shakeOffY
                    let radius = (BALL_SIZE / 2) + (ageNorm * 4.0) as i32
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
                trI = trI + 1
            }
            tbI = tbI + 1
        }

        // Ball (round).
        let bri: i32 = 0
        let brn: i32 = balls.length as i32
        while bri < brn {
            let bb = balls[bri]
            let ballCx = (bb.x + (BALL_SIZE as f64) / 2.0) as i32 + shakeOffX
            let ballCy = (bb.y + (BALL_SIZE as f64) / 2.0) as i32 + shakeOffY
            neonR.fillCircle(ballCx, ballCy, BALL_SIZE / 2, 255, 255, 255)
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
        let popI: i32 = 0
        let popN: i32 = popups.length as i32
        while popI < popN {
            let pp = popups[popI]
            let alphaI = pp.life * 6
            let pa: u8 = if alphaI > 255 { 255 as u8 } else { alphaI as u8 }
            let scaleP: i32 = 2
            let tw = textPxW(pp.text, scaleP)
            font.drawText(ren, pp.text,
                          (pp.x as i32) - tw / 2 + shakeOffX,
                          (pp.y as i32) + shakeOffY,
                          scaleP, pp.r, pp.g, pp.b, pa)
            popI = popI + 1
        }

        // ─── HUD (drawn into sceneTex so it gets the same CRT
        // ─── post-effects — chromatic aberration, scanlines, and
        // ─── vignette — as the world).

        font.drawText(ren, "SCORE: " + intToStr(score),
                      20, 20, 2, 200, 220, 255, 255)
        let fpsTxt = "FPS: " + intToStr(fps)
        font.drawText(ren, fpsTxt,
                      20, 50, 2, 180, 200, 220, 255)

        // Active power-up countdowns (in whole seconds, 60fps).
        let puY: i32 = 80
        if widerTimer > 0 {
            let s = "WIDER " + intToStr((widerTimer + 59) / 60) + "S"
            font.drawText(ren, s, 20, puY, 2, 120, 220, 255, 255)
            puY = puY + 24
        }
        if slowTimer > 0 {
            let s = "SLOW " + intToStr((slowTimer + 59) / 60) + "S"
            font.drawText(ren, s, 20, puY, 2, 200, 120, 255, 255)
            puY = puY + 24
        }
        let livesTxt = "LIVES: " + intToStr(lives)
        font.drawText(ren, livesTxt,
                      WIN_W - textPxW(livesTxt, 2) - 20, 20,
                      2, 255, 200, 220, 255)

        // Combo readout — only shown while a streak is active so
        // it doesn't take up HUD real-estate at idle.
        if combo > 1 {
            let comboTxt = "COMBO x" + intToStr(combo)
            font.drawText(ren, comboTxt,
                          (WIN_W - textPxW(comboTxt, 3)) / 2, 80,
                          3, 255, 220, 120, 255)
        }

        if !started && !gameOver {
            let s = "PRESS SPACE TO START"
            font.drawText(ren, s,
                          (WIN_W - textPxW(s, 3)) / 2, 400,
                          3, 200, 220, 255, 255)
        }

        if gameOver {
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

        // ─── bloom prep: downsample the scene into bloomTex ─────
        // Linear filtering averages 4×4 source blocks into each
        // bloom pixel; later we'll upscale this back to full
        // resolution which spreads the average into a soft halo.
        ren.setTarget(bloomTex)
        ren.setColor(0, 0, 0, 0)
        ren.clear()
        ren.drawTexture(sceneTex, 0, 0, BLOOM_W, BLOOM_H)

        // ─── composite sceneTex (world + HUD) to window with
        // ─── chromatic aberration. ────────────────────────────────
        ren.resetTarget()
        ren.setColor(0, 0, 0, 255)
        ren.clear()
        // Three additive passes, each keeping only one colour
        // channel and offset slightly. Pixel overlap reproduces the
        // original colour; high-contrast edges (including text)
        // show RGB fringing.
        sceneTex.setBlendMode(2)   // SDL_BLENDMODE_ADD
        let chroma: i32 = 3
        sceneTex.setColorMod(255 as u8, 0 as u8, 0 as u8)
        ren.drawTexture(sceneTex, -chroma, 0, WIN_W, WIN_H)
        sceneTex.setColorMod(0 as u8, 255 as u8, 0 as u8)
        ren.drawTexture(sceneTex, 0, 0, WIN_W, WIN_H)
        sceneTex.setColorMod(0 as u8, 0 as u8, 255 as u8)
        ren.drawTexture(sceneTex, chroma, 0, WIN_W, WIN_H)
        // Restore for next frame's setTarget pass.
        sceneTex.setBlendMode(1)   // SDL_BLENDMODE_BLEND
        sceneTex.setColorMod(255 as u8, 255 as u8, 255 as u8)

        // ─── bloom: additively overlay the upscaled blurred
        // ─── version, brightening hot spots into a soft glow.
        // Without a shader-side bright-pass we instead darken the
        // whole bloom contribution via colour-mod so mid-brightness
        // pixels don't push the composite to white. The result is
        // a soft glow only on the hottest parts of the scene.
        bloomTex.setBlendMode(2)
        bloomTex.setColorMod(60 as u8, 60 as u8, 60 as u8)
        bloomTex.setAlphaMod(255 as u8)
        ren.drawTexture(bloomTex, 0, 0, WIN_W, WIN_H)
        bloomTex.setBlendMode(1)
        bloomTex.setColorMod(255 as u8, 255 as u8, 255 as u8)

        // CRT overlay (scanlines + vignette).
        ren.drawTexture(crtTex, 0, 0, WIN_W, WIN_H)

        ren.present()

        // Sleep just long enough to keep the loop pinned to 60fps.
        // If the previous frame ran long, skip the wait but advance
        // the deadline so we don't accumulate a huge backlog.
        nextFrameMs = nextFrameMs + framePeriodMs
        let nowPace = sdl.getTicks() as f64
        let waitMs = nextFrameMs - nowPace
        if waitMs > 0.0 {
            sdl.delay(waitMs as u32)
        } else {
            if waitMs < -100.0 {
                nextFrameMs = nowPace
            }
        }
    }
}

let rc = sdl.init(sdl.InitFlag.video | sdl.InitFlag.audio | sdl.InitFlag.gameController)
if rc < 0 {
    console.log("SDL_Init failed: " + sdl.getError())
    return
}
play()
sdl.quit()
