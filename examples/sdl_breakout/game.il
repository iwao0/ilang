// The whole game session: window / renderer / audio / SFX /
// gamepad / textures / world state / main loop. `Game.run()`
// is what `play()` in main.il used to be — moved here so the
// entry file is just the SDL bootstrap.

use sdl
use math
use sprites
use audio { AudioSession }
use palette
use ball { Ball }
use brick { BrickGrid }
use particle { ParticleSystem }
use powerup { PowerUp }
use star { BgStars }
use neon { NeonRenderer }
use paddle { Paddle }
use rng
use consts
use input { Input }
use assets { Assets }

// Width of `s` rendered through `sdl.Font.drawText`. The font is
// 5×7 with 1 px between glyphs, so `n` chars = `n * 6 - 1` px at
// scale 1. Lifted to a top-level fn so the loader doesn't try to
// rewrite a per-frame `let` binding.
pub fn textPxW(s: string, scale: i32): i32 {
    let nch = s.length as i32
    if nch == 0 { return 0 }
    (nch * 6 - 1) * scale
}

// ilang's `as` cast is numeric-only; this stringifies a counter
// for the on-screen text. Lives here because `run()` is its only
// caller.
pub fn intToStr(n: i32): string {
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

pub class Game {
    pub window: sdl.Window
    pub renderer: sdl.Renderer
    pub neonRenderer: NeonRenderer
    pub font: sdl.Font
    /// SFX + BGM device pair plus the pre-rendered beep buffers.
    pub audioSession: AudioSession
    /// Keyboard / gamepad bundle.
    pub input: Input
    pub assets: Assets
    pub bgStars: BgStars
    pub brickGrid: BrickGrid
    pub paddle: Paddle
    pub particleSystem: ParticleSystem
    pub balls: Ball[]
    pub powerups: PowerUp[]
    pub score: i32
    pub lives: i32
    pub started: bool
    pub gameOver: bool
    pub won: bool
    pub combo: i32
    pub shake: f64
    pub playTime: i32
    pub introPhase: i32
    pub wonCelebrated: bool
    pub slowTimer: i32
    pub wavePhase: f64
    pub fullscreen: bool
    pub fps: i32
    pub fpsFrames: i32
    pub fpsLastMs: u32
    pub nextFrameMs: f64
    pub framePeriodMs: f64

    /// Build the SDL resources and prime every piece of game
    /// state. The matching `run()` then drives the main loop.
    /// Failure paths log to console.log and return early; the
    /// caller checks `this.renderer.handle != 0` style on individual
    /// fields if it cares.
    pub init() {
        this.window = new sdl.Window("ilang × SDL2 — NEON BREAKOUT", consts.winW, consts.winH)
        if this.window.handle == 0 {
            console.log("SDL_CreateWindow failed: " + sdl.getError())
            return
        }

        // VSync ties `present()` to the display refresh, smoothing
        // frame delivery. Without it the manual `sdl.delay` pacing
        // below visibly stutters even at a steady 60fps.
        this.renderer = new sdl.Renderer(this.window, -1, sdl.RendererFlags.presentVsync as u32)
        if this.renderer.handle == 0 {
            console.log("SDL_CreateRenderer failed: " + sdl.getError())
            return
        }

        // Pin the renderer to our virtual consts.winW×consts.winH so toggling
        // this.fullscreen-desktop just changes the window backing — the
        // game's coordinates stay the same and SDL letterboxes to fit.
        this.renderer.setLogicalSize(consts.winW, consts.winH)

        // Wraps `this.renderer` and exposes neon-style fill helpers (rect /
        // rounded rect / stadium / circle), all with a 3-pass halo to
        // fake the glow of a saturated primary on a dark background.
        this.neonRenderer = new NeonRenderer(this.renderer)

        this.font = new sdl.Font(this.renderer)

        this.audioSession = new AudioSession()
        if !this.audioSession.sfxOk() {
            console.log("SDL_OpenAudioDevice failed: " + sdl.getError())
            return
        }
        this.input = new Input()

        this.assets = new Assets(this.renderer)
        let starTex = this.assets.starTex
        let glowTex = this.assets.glowTex
        let crtTex = this.assets.crtTex
        let sceneTex = this.assets.sceneTex
        let bloomTex = this.assets.bloomTex

        // Background star field — scrolls top→down with a twinkle.
        this.bgStars = new BgStars(80, consts.winW, consts.winH)

        // Build the brick this.brickGrid.
        this.brickGrid = new BrickGrid(
            consts.cols, consts.rows, consts.brickW, consts.brickH, consts.brickGap, consts.brickTop, consts.winW
        )

        // State.
        this.paddle = new Paddle(
            ((consts.winW - consts.paddleW) / 2) as f64,
            consts.winH - 60,
            consts.paddleW,
            consts.paddleH,
            consts.paddleSpeed
        )
        let bs: Ball[] = []
        bs.push(new Ball((consts.winW / 2) as f64, (consts.winH / 2) as f64, 5.0, -5.0))
        this.balls = bs
        this.score = 0
        this.lives = 5
        this.started = false
        this.gameOver = false
        this.won = false
        this.combo = 0              // bricks hit since last paddle bounce
        this.shake = 0.0            // current screen-this.shake amplitude (px)
        // Visual effects (Particle bursts + this.score popups). The `this.particleSystem`
        // system owns both arrays plus their per-frame step.
        this.particleSystem = new ParticleSystem()
        // Frames the ball has been in flight (this game). Drives the
        // gradual speed-up. Doesn't reset on lose-life so tension keeps
        // building across attempts.
        this.playTime = 0
        // Frames since the last (re)start. Drives the brick drop-in
        // intro and lets us trigger a single big firework burst the
        // moment the level is cleared.
        this.introPhase = 0
        this.wonCelebrated = false
        // Power-ups falling from broken bricks. Timers count down per
        // frame while > 0; while their flag is active the matching
        // effect is applied. Multi-ball is one-shot so it doesn't need
        // a timer.
        let pus: PowerUp[] = []
        this.powerups = pus
        this.slowTimer = 0

        // Visual effect state.
        this.wavePhase = 0.0       // brick highlight wave (rad)

        // F toggles this.fullscreen — `Keyboard.isPressed`
        // already gives us a one-frame rising edge.
        this.fullscreen = false

        // FPS counter.
        this.fps = 0
        this.fpsFrames = 0
        this.fpsLastMs = sdl.getTicks()

        // Frame pacing: 1000/60 ≈ 16.667ms target. Track the next-frame
        // deadline as f64 so the rounding error doesn't drift the
        // average down to 58fps.
        this.nextFrameMs = sdl.getTicks() as f64
        this.framePeriodMs = 1000.0 / 60.0

        // Trails live on each `Ball` (see the `trail` / `trailHead`
        // fields), so multi-ball renders an independent fading streak
        // for every active ball.

        console.log("playing — ESC to quit")
    }

    /// Main loop. Polls input, advances the world, renders one
    /// frame, and waits out the rest of the 16.6 ms budget. Exits
    /// when the user quits via ESC / window close / gamepad BACK.
    pub run() {
        let running = true
        while running {
            // `Input.poll()` (called below) drains the event queue
            // and pumps the keyboard. We just need the quit-request
            // peek here, which itself runs `pumpEvents` internally.
            if sdl.quitRequested() {
                running = false
            }

            // FPS: count frames over a 1-second window.
            this.fpsFrames += 1
            let nowMs = sdl.getTicks()
            if nowMs - this.fpsLastMs >= 1000 {
                this.fps = this.fpsFrames
                this.fpsFrames = 0
                this.fpsLastMs = nowMs
            }

            // Brick highlight wave advances each frame.
            this.wavePhase = this.wavePhase + 0.08
            this.introPhase = this.introPhase + 1

            // Top up the BGM queue so the loop never drains. Polling
            // each frame is plenty cheap.
            this.audioSession.refillBgm()

            // Clear-time celebration: one big multi-burst the instant
            // the level is this.won, then a steady stream while CLEAR! is
            // shown so the screen keeps lighting up.
            if this.gameOver && this.won {
                if !this.wonCelebrated {
                    let bi: i32 = 0
                    while bi < 12 {
                        let fwX = rng.randf() * (consts.winW as f64)
                        let fwY = (0.15 + rng.randf() * 0.55) * (consts.winH as f64)
                        let pick = (rng.randf() * 5.0) as i32
                        this.particleSystem.spawn(fwX, fwY,
                                 palette.paletteR(pick), palette.paletteG(pick), palette.paletteB(pick))
                        bi += 1
                    }
                    this.wonCelebrated = true
                }
                // ~10 fireworks per second while celebrating.
                if rng.randf() < 0.18 {
                    let fwX = rng.randf() * (consts.winW as f64)
                    let fwY = (0.10 + rng.randf() * 0.65) * (consts.winH as f64)
                    let pick = (rng.randf() * 5.0) as i32
                    this.particleSystem.spawn(fwX, fwY,
                             palette.paletteR(pick), palette.paletteG(pick), palette.paletteB(pick))
                }
            }

            // Gamepad + keyboard: one snapshot per frame. The
            // keyboard / gamepad mappings stack — either device
            // works.
            let in_ = this.input.poll()

            if in_.keyEscDown || in_.padBackDown {
                running = false
            }

            // F: toggle this.fullscreen on key-down (rising edge).
            if in_.keyFDown {
                this.fullscreen = !this.fullscreen
                this.window.fullscreen = this.fullscreen
            }

            // Restart after game over.
            if this.gameOver && (in_.keyRDown || in_.padStartDown) {
                this.brickGrid.reset()
                let freshBalls: Ball[] = []
                freshBalls.push(new Ball((consts.winW / 2) as f64, (consts.winH / 2) as f64, 5.0, -5.0))
                this.balls = freshBalls
                this.score = 0
                this.lives = 5
                this.started = false
                this.gameOver = false
                this.won = false
                let emptyPu: PowerUp[] = []
                this.powerups = emptyPu
                this.paddle.widerTimer = 0
                this.paddle.prevW = this.paddle.baseW
                this.slowTimer = 0
                this.particleSystem.reset()
                this.combo = 0
                this.shake = 0.0
                this.playTime = 0
                this.introPhase = 0
                this.wonCelebrated = false
            }

            // ─── input ────────────────────────────────────────────────

            // Tick the paddle's WIDER timer + re-anchor `this.paddle.x` if the
            // width changed since last frame. The current width is then
            // pinned to a local for the physics formulas below.
            this.paddle.tick()
            let paddleW: i32 = this.paddle.width()

            if in_.keyLeft || in_.keyA || in_.padDpadL {
                this.paddle.moveLeft()
            }
            if in_.keyRight || in_.keyD || in_.padDpadR {
                this.paddle.moveRight()
            }
            this.paddle.moveAnalog(in_.padLx)
            this.paddle.clamp(consts.winW)

            // Space to launch the ball (idle on paddle until pressed).
            // Wait for the brick drop-in animation to finish before
            // accepting the launch input — otherwise the player can
            // shoot through unfilled rows.
            let introTotal = (consts.cols - 1) * consts.introStagger + consts.introDuration
            if !this.started && !this.gameOver && this.introPhase >= introTotal &&
                    (in_.keySpaceDown || in_.padADown) {
                this.started = true
            }

            // While idle, keep the (first) ball glued to the paddle's
            // centre.
            if !this.started && this.balls.length as i32 > 0 {
                let b0 = this.balls[0]
                b0.x = this.paddle.x + (paddleW as f64) / 2.0 - (consts.ballSize as f64) / 2.0
                b0.y = (this.paddle.y as f64) - (consts.ballSize as f64)
            }

            // ─── ball physics ─────────────────────────────────────────

            if this.started && !this.gameOver {
                // Slow-mo divides ball velocity step. Renormalize uses
                // the same factor so target magnitude shrinks.
                let speedFactor: f64 = if this.slowTimer > 0 { 0.55 } else { 1.0 }

                let surviving: Ball[] = []
                let bi: i32 = 0
                let bn: i32 = this.balls.length as i32
                while bi < bn {
                    let b = this.balls[bi]
                    b.x = b.x + b.vx * speedFactor
                    b.y = b.y + b.vy * speedFactor

                    // Walls.
                    if b.x < 0.0 {
                        b.x = 0.0
                        b.vx = -b.vx
                        this.audioSession.playWall()
                    }
                    if b.x + (consts.ballSize as f64) > consts.winW as f64 {
                        b.x = (consts.winW - consts.ballSize) as f64
                        b.vx = -b.vx
                        this.audioSession.playWall()
                    }
                    if b.y < 0.0 {
                        b.y = 0.0
                        b.vy = -b.vy
                        this.audioSession.playWall()
                    }

                    // Paddle (only on the way down).
                    let bxLo = b.x
                    let bxHi = b.x + (consts.ballSize as f64)
                    let byLo = b.y
                    let byHi = b.y + (consts.ballSize as f64)
                    let pxLo = this.paddle.x
                    let pxHi = this.paddle.x + (paddleW as f64)
                    let pyLo = this.paddle.y as f64
                    let pyHi = (this.paddle.y + consts.paddleH) as f64
                    if b.vy > 0.0 && bxHi > pxLo && bxLo < pxHi
                            && byHi > pyLo && byLo < pyHi {
                        b.y = pyLo - (consts.ballSize as f64)
                        let hitCentre = pxLo + (paddleW as f64) / 2.0
                        let ballCentre = b.x + (consts.ballSize as f64) / 2.0
                        let rel = (ballCentre - hitCentre) / ((paddleW as f64) / 2.0)
                        let relClamped: f64 = if rel < -1.0 { -1.0 } elif rel > 1.0 { 1.0 } else { rel }
                        let maxAngle: f64 = 1.04   // ≈60°
                        let angle = relClamped * maxAngle
                        let speed = math.sqrt(b.vx * b.vx + b.vy * b.vy)
                        b.vx = speed * math.sin(angle)
                        b.vy = -speed * math.cos(angle)
                        this.combo = 0
                        this.audioSession.playPaddle()
                    }

                    // Bricks. First-hit-wins-this-frame for this ball.
                    let i: i32 = 0
                    let n: i32 = this.brickGrid.bricks.length as i32
                    while i < n {
                        let br = this.brickGrid.bricks[i]
                        if br.alive {
                            let rxLo = br.x as f64
                            let rxHi = (br.x + consts.brickW) as f64
                            let ryLo = br.y as f64
                            let ryHi = (br.y + consts.brickH) as f64
                            if bxHi > rxLo && bxLo < rxHi
                                    && byHi > ryLo && byLo < ryHi {
                                br.alive = false
                                br.dying = 1.0
                                this.brickGrid.aliveCount = this.brickGrid.aliveCount - 1
                                this.combo = this.combo + 1
                                let earned = 100 * this.combo
                                this.score = this.score + earned
                                let overlapX = if b.vx > 0.0 { bxHi - rxLo } else { rxHi - bxLo }
                                let overlapY = if b.vy > 0.0 { byHi - ryLo } else { ryHi - byLo }
                                if overlapX < overlapY {
                                    b.vx = -b.vx
                                } else {
                                    b.vy = -b.vy
                                }
                                let cx = (br.x as f64) + (consts.brickW as f64) / 2.0
                                let cy = (br.y as f64) + (consts.brickH as f64) / 2.0
                                this.particleSystem.spawn(cx, cy, br.r, br.g, br.b)
                                this.particleSystem.spawnPopup(
                                    cx, cy - 4.0, "+" + intToStr(earned),
                                    br.r, br.g, br.b
                                )
                                let bump = 4.0 + (this.combo as f64) * 1.5
                                this.shake = this.shake + bump
                                if this.shake > 14.0 { this.shake = 14.0 }
                                this.audioSession.playBrick()
                                // 14% drop chance per brick break.
                                if rng.randf() < 0.14 {
                                    let kind = (rng.randf() * 3.0) as i32
                                    this.powerups.push(new PowerUp(cx, cy, kind))
                                }
                                i = n   // stop after first hit this frame
                            }
                        }
                        i = i + 1
                    }

                    // Win: all bricks cleared. Don't break the ball
                    // loop — remaining this.balls finish their frame.
                    if this.brickGrid.aliveCount <= 0 {
                        this.gameOver = true
                        this.won = true
                    }

                    // Speed-up renormalize per ball.
                    let baseSpeed: f64 = 7.07106781   // sqrt(5^2 + 5^2)
                    let timeBonusRaw = (this.playTime as f64) * 0.002
                    let timeBonus: f64 = if timeBonusRaw > 6.0 { 6.0 } else { timeBonusRaw }
                    let comboBonus = (this.combo as f64) * 0.35
                    let target = (baseSpeed + timeBonus + comboBonus) * speedFactor
                    let cur = math.sqrt(b.vx * b.vx + b.vy * b.vy)
                    if cur > 0.001 {
                        let scale = target / cur
                        b.vx = b.vx * scale
                        b.vy = b.vy * scale
                    }

                    // Survives if it didn't fall below the bottom.
                    if b.y <= consts.winH as f64 {
                        surviving.push(b)
                    }
                    bi = bi + 1
                }
                this.balls = surviving
                this.playTime = this.playTime + 1

                // Out of this.balls → lose a life.
                if this.balls.length as i32 == 0 {
                    this.lives = this.lives - 1
                    if this.lives <= 0 {
                        this.gameOver = true
                        this.won = false
                    } else {
                        this.started = false
                        this.combo = 0
                        let respawn: Ball[] = []
                        respawn.push(new Ball((consts.winW / 2) as f64, (consts.winH / 2) as f64, 5.0, -5.0))
                        this.balls = respawn
                    }
                }
            }

            // Record every ball's current position into its own ring
            // buffer. Each Ball owns the buffer so multi-ball produces
            // independent trails.
            let trBi: i32 = 0
            let trBn: i32 = this.balls.length as i32
            while trBi < trBn {
                let bb = this.balls[trBi]
                let tp = bb.trail[bb.trailHead]
                tp.x = bb.x
                tp.y = bb.y
                bb.trailHead = (bb.trailHead + 1) % consts.trailLen
                trBi = trBi + 1
            }

            // ─── power-up step: fall, catch, expire ──────────────────
            // (paddle's WIDER timer ticks down inside `this.paddle.tick()` at
            // the top of the frame.)
            if this.slowTimer > 0 { this.slowTimer = this.slowTimer - 1 }
            let alivePu: PowerUp[] = []
            let pui: i32 = 0
            let pun: i32 = this.powerups.length as i32
            while pui < pun {
                let pu = this.powerups[pui]
                pu.y = pu.y + pu.vy
                // Catch test: 36×16 chip centred on (pu.x, pu.y).
                let chipW: f64 = 36.0
                let chipH: f64 = 16.0
                let cxLo = pu.x - chipW / 2.0
                let cxHi = pu.x + chipW / 2.0
                let cyLo = pu.y - chipH / 2.0
                let cyHi = pu.y + chipH / 2.0
                let pdl = this.paddle.x
                let pdr = this.paddle.x + (paddleW as f64)
                let pdt = this.paddle.y as f64
                let pdb = (this.paddle.y + consts.paddleH) as f64
                let caught = cxHi > pdl && cxLo < pdr && cyHi > pdt && cyLo < pdb
                let removed: bool = caught
                if caught {
                    match pu.kind {
                        0 {
                            this.paddle.widerTimer = 600       // 10s
                        }
                        1 {
                            // Multi-ball: clone first ball into two new this.balls
                            // with slightly diverged angles.
                            if this.balls.length as i32 > 0 {
                                let src = this.balls[0]
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
                                    this.balls.push(extra[kj])
                                    kj = kj + 1
                                }
                            }
                        }
                        _ {
                            this.slowTimer = 480            // 8s
                        }
                    }
                    this.audioSession.playPaddle()
                }
                if !removed && pu.y < (consts.winH + 40) as f64 {
                    alivePu.push(pu)
                }
                pui = pui + 1
            }
            this.powerups = alivePu

            // ─── particle / popup step ───────────────────────────────
            this.particleSystem.step()

            // Decay screen-this.shake exponentially. Compute per-frame
            // jitter offsets here so all draw calls share them.
            let shakeOffX: i32 = 0
            let shakeOffY: i32 = 0
            if this.shake > 0.5 {
                shakeOffX = ((rng.randf() - 0.5) * 2.0 * this.shake) as i32
                shakeOffY = ((rng.randf() - 0.5) * 2.0 * this.shake) as i32
            }
            this.shake = this.shake * 0.82
            if this.shake < 0.05 { this.shake = 0.0 }

            // Background star scroll + twinkle phase.
            this.bgStars.step(consts.winW, consts.winH)

            // ─── render ──────────────────────────────────────────────
            // The world is drawn into `this.assets.sceneTex` so we can post-process
            // it (chromatic aberration). HUD lands directly on the
            // window afterwards so the readouts stay sharp.
            this.renderer.setTarget(this.assets.sceneTex)
            this.renderer.setColor(8, 4, 24, 255)        // very dark navy
            this.renderer.clear()

            // Background stars (drawn first, beneath everything).
            let bgI: i32 = 0
            let bgN: i32 = this.bgStars.stars.length as i32
            while bgI < bgN {
                let s = this.bgStars.stars[bgI]
                // Brightness pulses with phase.
                let bright = 0.4 + 0.6 * (0.5 + 0.5 * math.sin(s.phase))
                let a8 = (bright * 220.0) as u8
                this.renderer.setColor(220, 220, 255, a8)
                // Bigger stars (random radius 1 or 2).
                let half: i32 = if s.speed > 1.2 { 2 } else { 1 }
                this.renderer.fillRect(s.x as i32 - half + shakeOffX,
                             s.y as i32 - half + shakeOffY,
                             half * 2 + 1, half * 2 + 1)
                bgI = bgI + 1
            }

            // Bricks with a left-to-right brightness wave. Glow padding
            // is proportional to the brick's aspect ratio, so the
            // bloom around a wide brick is wider than tall (matching
            // the brick's shape).
            let glowPadY: i32 = 32
            let glowPadX: i32 = glowPadY * consts.brickW / consts.brickH
            let i2: i32 = 0
            let n2: i32 = this.brickGrid.bricks.length as i32
            while i2 < n2 {
                let br = this.brickGrid.bricks[i2]
                // Drop-in intro: each column is delayed by consts.introStagger
                // frames so the this.brickGrid cascades in left→right. The brick
                // falls from above its target with an ease-out curve.
                let col = i2 % consts.cols
                let ownPhase = this.introPhase - col * consts.introStagger
                let introActive = ownPhase < consts.introDuration
                let introOffsetY: i32 = if ownPhase < 0 {
                    // Pre-arrival: keep the brick well above the playfield.
                    0 - ((consts.introDropPx as i32) + 100)
                } elif ownPhase < consts.introDuration {
                    let p = (ownPhase as f64) / (consts.introDuration as f64)
                    let inv = 1.0 - p
                    0 - ((inv * inv * consts.introDropPx) as i32)
                } else {
                    0
                }
                if br.alive {
                    this.neonRenderer.fillRoundedRect(br.x + shakeOffX,
                                        br.y + introOffsetY + shakeOffY,
                                        consts.brickW, consts.brickH, 6, br.r, br.g, br.b)
                    // Wave glow only after the brick has landed — looks
                    // odd to highlight a brick that's mid-fall.
                    let k: f64 = 0.012
                    let s = math.sin(this.wavePhase - (br.x as f64) * k)
                    let pulse: f64 = if s > 0.0 { s * s } else { 0.0 }
                    if pulse > 0.0 && !introActive {
                        let alpha = (pulse * 220.0) as u8
                        if alpha > 0 as u8 {
                            this.assets.glowTex.setColorMod(br.r, br.g, br.b)
                            this.assets.glowTex.setAlphaMod(alpha)
                            this.renderer.drawTexture(this.assets.glowTex,
                                            br.x - glowPadX + shakeOffX,
                                            br.y - glowPadY + shakeOffY,
                                            consts.brickW + 2 * glowPadX,
                                            consts.brickH + 2 * glowPadY)
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
                    let cx = br.x + consts.brickW / 2
                    let cy = br.y + consts.brickH / 2
                    let dw = ((consts.brickW as f64) * scale) as i32
                    let dh = ((consts.brickH as f64) * scale) as i32
                    let alpha = (t * 220.0) as u8
                    // Soft bloom (radial gradient) — biggest, dimmest.
                    this.assets.glowTex.setColorMod(br.r, br.g, br.b)
                    this.assets.glowTex.setAlphaMod((t * 180.0) as u8)
                    let bw = ((consts.brickW + 2 * glowPadX) as f64) * scale
                    let bh = ((consts.brickH + 2 * glowPadY) as f64) * scale
                    this.renderer.drawTexture(this.assets.glowTex,
                                    cx - (bw as i32) / 2 + shakeOffX,
                                    cy - (bh as i32) / 2 + shakeOffY,
                                    bw as i32, bh as i32)
                    // Sharper core.
                    if alpha > 0 as u8 {
                        this.renderer.setColor(br.r, br.g, br.b, alpha)
                        this.renderer.fillRect(cx - dw / 2 + shakeOffX,
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
            let n3: i32 = this.particleSystem.particles.length as i32
            while i3 < n3 {
                let p = this.particleSystem.particles[i3]
                let alpha = (p.life * 8) as i32
                let a: u8 = if alpha > 255 { 255 } else { alpha as u8 }
                this.assets.starTex.setColorMod(p.r, p.g, p.b)
                // Outer halo: bigger, lower alpha. Drawn first so the
                // crisp star sits on top of its glow.
                let haloA = ((a as i32) / 3) as u8
                this.assets.starTex.setAlphaMod(haloA)
                let haloSize: i32 = 56
                this.renderer.drawTextureRot(this.assets.starTex,
                                   (p.x as i32) - haloSize / 2 + shakeOffX,
                                   (p.y as i32) - haloSize / 2 + shakeOffY,
                                   haloSize, haloSize, p.angle)
                // Sharp star body (2× the previous 16px).
                this.assets.starTex.setAlphaMod(a)
                let size: i32 = 32
                this.renderer.drawTextureRot(this.assets.starTex,
                                   (p.x as i32) - size / 2 + shakeOffX,
                                   (p.y as i32) - size / 2 + shakeOffY,
                                   size, size, p.angle)
                i3 = i3 + 1
            }

            // Power-up chips drifting down — coloured neon stadium with
            // a single-letter label on top.
            let pri: i32 = 0
            let prn: i32 = this.powerups.length as i32
            while pri < prn {
                let pu = this.powerups[pri]
                let pr2: u8 = match pu.kind {
                    0 { 120 as u8 }
                    1 { 255 as u8 }
                    _ { 200 as u8 }
                }
                let pg2: u8 = match pu.kind {
                    0 { 220 as u8 }
                    1 { 220 as u8 }
                    _ { 120 as u8 }
                }
                let pb2: u8 = match pu.kind {
                    0 { 255 as u8 }
                    1 { 120 as u8 }
                    _ { 255 as u8 }
                }
                let chipX = (pu.x as i32) - 18 + shakeOffX
                let chipY = (pu.y as i32) - 8 + shakeOffY
                this.neonRenderer.fillStadium(chipX, chipY, 36, 16, pr2, pg2, pb2)
                let label: string = match pu.kind {
                    0 { "W" }
                    1 { "M" }
                    _ { "S" }
                }
                this.font.drawText(this.renderer, label, chipX + 14, chipY + 2,
                              2, 20 as u8, 10 as u8, 30 as u8, 255 as u8)
                pri = pri + 1
            }

            // Paddle (white/cyan glow, rounded ends). The colour shifts
            // toward magenta while the WIDER power-up is active so the
            // boost is obvious.
            let pr: u8 = if this.paddle.widerTimer > 0 { 255 as u8 } else { 200 as u8 }
            let pg: u8 = if this.paddle.widerTimer > 0 { 180 as u8 } else { 240 as u8 }
            let pb: u8 = if this.paddle.widerTimer > 0 { 255 as u8 } else { 255 as u8 }
            this.neonRenderer.fillStadium(this.paddle.x as i32 + shakeOffX, this.paddle.y + shakeOffY,
                            paddleW, consts.paddleH, pr, pg, pb)

            // Ball trails — one per ball, fading from oldest
            // (just-overwritten slot) to newest (slot before head).
            // Drawn beneath the ball itself.
            let tbI: i32 = 0
            let tbN: i32 = this.balls.length as i32
            while tbI < tbN {
                let bb = this.balls[tbI]
                let trI: i32 = 0
                while trI < consts.trailLen {
                    // age 0 = oldest (trailHead), age consts.trailLen-1 = newest
                    let slot = (bb.trailHead + trI) % consts.trailLen
                    let pt = bb.trail[slot]
                    let ageNorm = (trI as f64) / (consts.trailLen as f64)
                    let a = (ageNorm * ageNorm * 140.0) as u8
                    if a > 0 as u8 {
                        let cx = (pt.x + (consts.ballSize as f64) / 2.0) as i32 + shakeOffX
                        let cy = (pt.y + (consts.ballSize as f64) / 2.0) as i32 + shakeOffY
                        let radius = (consts.ballSize / 2) + (ageNorm * 4.0) as i32
                        this.renderer.setColor(255, 255, 255, a)
                        let dy: i32 = -radius
                        while dy <= radius {
                            let dyf = dy as f64
                            let radf = radius as f64
                            let w = math.sqrt(radf * radf - dyf * dyf)
                            let half = w as i32
                            this.renderer.fillRect(cx - half, cy + dy, half * 2 + 1, 1)
                            dy += 1
                        }
                    }
                    trI = trI + 1
                }
                tbI = tbI + 1
            }

            // Ball (round).
            let bri: i32 = 0
            let brn: i32 = this.balls.length as i32
            while bri < brn {
                let bb = this.balls[bri]
                let ballCx = (bb.x + (consts.ballSize as f64) / 2.0) as i32 + shakeOffX
                let ballCy = (bb.y + (consts.ballSize as f64) / 2.0) as i32 + shakeOffY
                this.neonRenderer.fillCircle(ballCx, ballCy, consts.ballSize / 2, 255, 255, 255)
                bri = bri + 1
            }

            // 6px advance per glyph at scale=1 (5px cell + 1px space),
            // last glyph contributes only 5px → final width formula
            // is in `textPxW` below. Used for centering and right-
            // alignment.

            // Score popups — part of the world so they pick up this.shake
            // and chromatic aberration.
            let popI: i32 = 0
            let popN: i32 = this.particleSystem.popups.length as i32
            while popI < popN {
                let pp = this.particleSystem.popups[popI]
                let alphaI = pp.life * 6
                let pa: u8 = if alphaI > 255 { 255 as u8 } else { alphaI as u8 }
                let scaleP: i32 = 2
                let tw = textPxW(pp.text, scaleP)
                this.font.drawText(this.renderer, pp.text,
                              (pp.x as i32) - tw / 2 + shakeOffX,
                              (pp.y as i32) + shakeOffY,
                              scaleP, pp.r, pp.g, pp.b, pa)
                popI = popI + 1
            }

            // ─── HUD (drawn into this.assets.sceneTex so it gets the same CRT
            // ─── post-effects — chromatic aberration, scanlines, and
            // ─── vignette — as the world).

            this.font.drawText(this.renderer, "SCORE: " + intToStr(this.score),
                          20, 20, 2, 200, 220, 255, 255)
            let fpsTxt = "FPS: " + intToStr(this.fps)
            this.font.drawText(this.renderer, fpsTxt,
                          20, 50, 2, 180, 200, 220, 255)

            // Active power-up countdowns (in whole seconds, 60fps).
            let puY: i32 = 80
            if this.paddle.widerTimer > 0 {
                let s = "WIDER " + intToStr((this.paddle.widerTimer + 59) / 60) + "S"
                this.font.drawText(this.renderer, s, 20, puY, 2, 120, 220, 255, 255)
                puY = puY + 24
            }
            if this.slowTimer > 0 {
                let s = "SLOW " + intToStr((this.slowTimer + 59) / 60) + "S"
                this.font.drawText(this.renderer, s, 20, puY, 2, 200, 120, 255, 255)
                puY = puY + 24
            }
            let livesTxt = "LIVES: " + intToStr(this.lives)
            this.font.drawText(this.renderer, livesTxt,
                          consts.winW - textPxW(livesTxt, 2) - 20, 20,
                          2, 255, 200, 220, 255)

            // Combo readout — only shown while a streak is active so
            // it doesn't take up HUD real-estate at idle.
            if this.combo > 1 {
                let comboTxt = "COMBO x" + intToStr(this.combo)
                this.font.drawText(this.renderer, comboTxt,
                              (consts.winW - textPxW(comboTxt, 3)) / 2, 80,
                              3, 255, 220, 120, 255)
            }

            if !this.started && !this.gameOver {
                let s = "PRESS SPACE TO START"
                this.font.drawText(this.renderer, s,
                              (consts.winW - textPxW(s, 3)) / 2, 400,
                              3, 200, 220, 255, 255)
            }

            if this.gameOver {
                if this.won {
                    let t = "CLEAR!"
                    this.font.drawText(this.renderer, t,
                                  (consts.winW - textPxW(t, 6)) / 2, 300,
                                  6, 120, 255, 200, 255)
                } else {
                    let t = "GAME OVER"
                    this.font.drawText(this.renderer, t,
                                  (consts.winW - textPxW(t, 6)) / 2, 300,
                                  6, 255, 100, 200, 255)
                }
                let s = "PRESS R TO RESTART"
                this.font.drawText(this.renderer, s,
                              (consts.winW - textPxW(s, 3)) / 2, 400,
                              3, 200, 220, 255, 255)
            }

            // ─── bloom prep: downsample the scene into this.assets.bloomTex ─────
            // Linear filtering averages 4×4 source blocks into each
            // bloom pixel; later we'll upscale this back to full
            // resolution which spreads the average into a soft halo.
            this.renderer.setTarget(this.assets.bloomTex)
            this.renderer.setColor(0, 0, 0, 0)
            this.renderer.clear()
            this.renderer.drawTexture(this.assets.sceneTex, 0, 0, consts.bloomW, consts.bloomH)

            // ─── composite this.assets.sceneTex (world + HUD) to window with
            // ─── chromatic aberration. ────────────────────────────────
            this.renderer.resetTarget()
            this.renderer.setColor(0, 0, 0, 255)
            this.renderer.clear()
            // Three additive passes, each keeping only one colour
            // channel and offset slightly. Pixel overlap reproduces the
            // original colour; high-contrast edges (including text)
            // show RGB fringing.
            this.assets.sceneTex.setBlendMode(2)   // SDL_BLENDMODE_ADD
            let chroma: i32 = 3
            this.assets.sceneTex.setColorMod(255 as u8, 0 as u8, 0 as u8)
            this.renderer.drawTexture(this.assets.sceneTex, -chroma, 0, consts.winW, consts.winH)
            this.assets.sceneTex.setColorMod(0 as u8, 255 as u8, 0 as u8)
            this.renderer.drawTexture(this.assets.sceneTex, 0, 0, consts.winW, consts.winH)
            this.assets.sceneTex.setColorMod(0 as u8, 0 as u8, 255 as u8)
            this.renderer.drawTexture(this.assets.sceneTex, chroma, 0, consts.winW, consts.winH)
            // Restore for next frame's setTarget pass.
            this.assets.sceneTex.setBlendMode(1)   // SDL_BLENDMODE_BLEND
            this.assets.sceneTex.setColorMod(255 as u8, 255 as u8, 255 as u8)

            // ─── bloom: additively overlay the upscaled blurred
            // ─── version, brightening hot spots into a soft glow.
            // Without a shader-side bright-pass we instead darken the
            // whole bloom contribution via colour-mod so mid-brightness
            // pixels don't push the composite to white. The result is
            // a soft glow only on the hottest parts of the scene.
            this.assets.bloomTex.setBlendMode(2)
            this.assets.bloomTex.setColorMod(60 as u8, 60 as u8, 60 as u8)
            this.assets.bloomTex.setAlphaMod(255 as u8)
            this.renderer.drawTexture(this.assets.bloomTex, 0, 0, consts.winW, consts.winH)
            this.assets.bloomTex.setBlendMode(1)
            this.assets.bloomTex.setColorMod(255 as u8, 255 as u8, 255 as u8)

            // CRT overlay (scanlines + vignette).
            this.renderer.drawTexture(this.assets.crtTex, 0, 0, consts.winW, consts.winH)

            this.renderer.present()

            // Sleep just long enough to keep the loop pinned to 60fps.
            // If the previous frame ran long, skip the wait but advance
            // the deadline so we don't accumulate a huge backlog.
            this.nextFrameMs = this.nextFrameMs + this.framePeriodMs
            let nowPace = sdl.getTicks() as f64
            let waitMs = this.nextFrameMs - nowPace
            if waitMs > 0.0 {
                sdl.delay(waitMs as u32)
            } else {
                if waitMs < -100.0 {
                    this.nextFrameMs = nowPace
                }
            }
        }
    }
}
