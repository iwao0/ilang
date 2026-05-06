// The whole game session: window / renderer / audio / SFX /
// gamepad / textures / world state / main loop. `Game.run()`
// is what `play()` in main.il used to be — moved here so the
// entry file is just the SDL bootstrap.

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
use paddle
use rng
use consts

// Width of `s` rendered through `sdl.Font.drawText`. The font is
// 5×7 with 1 px between glyphs, so `n` chars = `n * 6 - 1` px at
// scale 1. Lifted to a top-level fn so the loader doesn't try to
// rewrite a per-frame `let` binding.
fn textPxW(s: string, scale: i32): i32 {
    let nch = s.length as i32
    if nch == 0 { return 0 }
    (nch * 6 - 1) * scale
}

// ilang's `as` cast is numeric-only; this stringifies a counter
// for the on-screen text. Lives here because `run()` is its only
// caller.
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

class Game {
    win: sdl.Window
    ren: sdl.Renderer
    neonR: neon.NeonRenderer
    font: sdl.Font
    sfxDevice: sdl.AudioDevice
    audioMusic: sdl.AudioDevice
    bgm: i16[]
    bgmBytes: u32
    gamepad: sdl.Gamepad
    beepWall: i16[]
    beepPaddle: i16[]
    beepBrick: i16[]
    starTex: sdl.Texture
    glowTex: sdl.Texture
    crtTex: sdl.Texture
    sceneTex: sdl.Texture
    bloomTex: sdl.Texture
    sky: star.BgStars
    grid: brick.BrickGrid
    pad: paddle.Paddle
    fx: particle.ParticleSystem
    balls: ball.Ball[]
    powerups: powerup.PowerUp[]
    score: i32
    lives: i32
    started: bool
    gameOver: bool
    won: bool
    combo: i32
    shake: f64
    playTime: i32
    introPhase: i32
    wonCelebrated: bool
    slowTimer: i32
    wavePhase: f64
    fullscreen: bool
    fWasDown: bool
    fps: i32
    fpsFrames: i32
    fpsLastMs: u32
    nextFrameMs: f64
    framePeriodMs: f64

    /// Build the SDL resources and prime every piece of game
    /// state. The matching `run()` then drives the main loop.
    /// Failure paths log to console.log and return early; the
    /// caller checks `this.ren.handle != 0` style on individual
    /// fields if it cares.
    init() {
        this.win = new sdl.Window("ilang × SDL2 — NEON BREAKOUT", consts.winW, consts.winH)
        if this.win.handle == 0 {
            console.log("SDL_CreateWindow failed: " + sdl.getError())
            return
        }

        // VSync ties `present()` to the display refresh, smoothing
        // frame delivery. Without it the manual `sdl.delay` pacing
        // below visibly stutters even at a steady 60fps.
        this.ren = new sdl.Renderer(this.win, -1, sdl.RendererFlags.presentVsync as u32)
        if this.ren.handle == 0 {
            console.log("SDL_CreateRenderer failed: " + sdl.getError())
            return
        }

        // Pin the renderer to our virtual consts.winW×consts.winH so toggling
        // this.fullscreen-desktop just changes the window backing — the
        // game's coordinates stay the same and SDL letterboxes to fit.
        this.ren.setLogicalSize(consts.winW, consts.winH)

        // Wraps `this.ren` and exposes neon-style fill helpers (rect /
        // rounded rect / stadium / circle), all with a 3-pass halo to
        // fake the glow of a saturated primary on a dark background.
        this.neonR = new neon.NeonRenderer(this.ren)

        this.font = new sdl.Font(this.ren)

        this.sfxDevice = new sdl.AudioDevice(consts.sampleRate, 1)
        if this.sfxDevice.id == 0 {
            console.log("SDL_OpenAudioDevice failed: " + sdl.getError())
            return
        }
        this.sfxDevice.play()

        // Second audio device dedicated to BGM. SDL allows multiple
        // simultaneous open devices, so the music keeps playing even
        // while SFX queue/dequeue rapidly on the first device.
        this.audioMusic = new sdl.AudioDevice(consts.sampleRate, 1)
        this.bgm = audio.makeBgmLoop()
        this.bgmBytes = (this.bgm.length * 2) as u32   // one loop in queued-bytes
        if this.audioMusic.id != 0 {
            // Pre-buffer two loops so the first refill check has slack.
            this.audioMusic.queue(this.bgm)
            this.audioMusic.queue(this.bgm)
            this.audioMusic.play()
        }

        // Open the first attached this.gamepad if any. `this.pad.handle == 0`
        // when nothing is plugged in — every poll degrades to
        // returning 0/false, so the keyboard path keeps working.
        this.gamepad = new sdl.Gamepad()

        this.beepWall = audio.makeBeep(440.0, 40)
        this.beepPaddle = audio.makeBeep(660.0, 50)
        this.beepBrick = audio.makeBeep(880.0, 70)

        // White star sprite for particle bursts. Tinted at draw time
        // via `setColorMod`, rotated via `drawTextureRot`.
        this.starTex = new sdl.Texture(this.ren, consts.starTexSize, consts.starTexSize, 1)
        if this.starTex.handle == 0 {
            console.log("SDL_CreateTexture failed: " + sdl.getError())
            return
        }
        this.starTex.update(sprites.makeStarSprite())

        // Soft radial-gradient sprite, tinted per-brick at draw time so
        // the wave highlight blooms as a blurry circle rather than a
        // rectangle.
        this.glowTex = new sdl.Texture(this.ren, consts.glowTexSize, consts.glowTexSize, 1)
        if this.glowTex.handle == 0 {
            console.log("SDL_CreateTexture failed: " + sdl.getError())
            return
        }
        this.glowTex.update(sprites.makeGlowSprite())

        // Full-window CRT-effect overlay — built once. The alpha
        // channel encodes scanlines + vignette, the colour is black,
        // so blending it on top darkens each pixel by the right
        // amount.
        this.crtTex = new sdl.Texture(this.ren, consts.winW, consts.winH, 1)
        if this.crtTex.handle == 0 {
            console.log("SDL_CreateTexture failed: " + sdl.getError())
            return
        }
        this.crtTex.update(sprites.makeCrtOverlay(consts.winW, consts.winH))

        // Off-screen accumulator for the world. Drawn into each frame
        // (with a small per-frame fade applied first) and then composited
        // back to the window with chromatic-aberration channel offsets.
        // SDL_TEXTUREACCESS_TARGET = 2.
        this.sceneTex = new sdl.Texture(this.ren, consts.winW, consts.winH, 2)
        if this.sceneTex.handle == 0 {
            console.log("SDL_CreateTexture(TARGET) failed: " + sdl.getError())
            return
        }
        // Linear filtering on the scene texture so that the bloom
        // pass's downscale + upscale produces a smooth blur instead of
        // a chunky pixel this.grid.
        this.sceneTex.setScaleMode(1)

        // Tiny target used to "blur" the scene. We draw this.sceneTex
        // shrunk into this.bloomTex (each downsample averages a 4×4 block
        // thanks to linear filtering), and later draw this.bloomTex back
        // out at full window size additively — the upscale spreads
        // each averaged pixel across many output pixels, giving a
        // soft glow around the bright stuff in the scene.
        this.bloomTex = new sdl.Texture(this.ren, consts.bloomW, consts.bloomH, 2)
        if this.bloomTex.handle == 0 {
            console.log("SDL_CreateTexture(bloom) failed: " + sdl.getError())
            return
        }
        this.bloomTex.setScaleMode(1)

        // Background star field — scrolls top→down with a twinkle.
        this.sky = new star.BgStars(80, consts.winW, consts.winH)

        // Build the brick this.grid.
        this.grid = new brick.BrickGrid(
            consts.cols, consts.rows, consts.brickW, consts.brickH, consts.brickGap, consts.brickTop, consts.winW
        )

        // State.
        this.pad = new paddle.Paddle(
            ((consts.winW - consts.paddleW) / 2) as f64,
            consts.winH - 60,
            consts.paddleW,
            consts.paddleH,
            consts.paddleSpeed
        )
        let bs: ball.Ball[] = []
        bs.push(new ball.Ball((consts.winW / 2) as f64, (consts.winH / 2) as f64, 5.0, -5.0))
        this.balls = bs
        this.score = 0
        this.lives = 5
        this.started = false
        this.gameOver = false
        this.won = false
        this.combo = 0              // bricks hit since last paddle bounce
        this.shake = 0.0            // current screen-this.shake amplitude (px)
        // Visual effects (Particle bursts + this.score popups). The `this.fx`
        // system owns both arrays plus their per-frame step.
        this.fx = new particle.ParticleSystem()
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
        let pus: powerup.PowerUp[] = []
        this.powerups = pus
        this.slowTimer = 0

        // Visual effect state.
        this.wavePhase = 0.0       // brick highlight wave (rad)

        // F toggles this.fullscreen — track edge so a held key doesn't flap.
        this.fullscreen = false
        this.fWasDown = false

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
    run() {
        let running = true
        while running {
            sdl.pumpEvents()
            sdl.Keyboard.pump()
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
            if this.audioMusic.id != 0 && this.audioMusic.queuedBytes() < this.bgmBytes {
                this.audioMusic.queue(this.bgm)
            }

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
                        this.fx.spawn(fwX, fwY,
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
                    this.fx.spawn(fwX, fwY,
                             palette.paletteR(pick), palette.paletteG(pick), palette.paletteB(pick))
                }
            }

            // Gamepad: left stick → paddle (analogue), DPAD ←/→ →
            // paddle (digital), A → launch, START → restart, BACK →
            // quit. The keyboard mappings below stack with these so
            // either device works.
            let padLx = this.gamepad.getAxis(sdl.Axis.leftX) as i32  // [-32768, 32767]
            let padDpadL = this.gamepad.isPressed(sdl.Button.dpadLeft)
            let padDpadR = this.gamepad.isPressed(sdl.Button.dpadRight)
            let padA = this.gamepad.isPressed(sdl.Button.a)
            let padStart = this.gamepad.isPressed(sdl.Button.start)
            let padBack = this.gamepad.isPressed(sdl.Button.back)

            if sdl.Keyboard.isPressed(sdl.Scancode.escape) || padBack {
                running = false
            }

            // F: toggle this.fullscreen on key-down (rising edge).
            let fNow = sdl.Keyboard.isPressed(sdl.Scancode.f)
            if fNow && !this.fWasDown {
                this.fullscreen = !this.fullscreen
                this.win.fullscreen = this.fullscreen
            }
            this.fWasDown = fNow

            // Restart after game over.
            if this.gameOver && (sdl.Keyboard.isPressed(sdl.Scancode.r) || padStart) {
                this.grid.reset()
                let freshBalls: ball.Ball[] = []
                freshBalls.push(new ball.Ball((consts.winW / 2) as f64, (consts.winH / 2) as f64, 5.0, -5.0))
                this.balls = freshBalls
                this.score = 0
                this.lives = 5
                this.started = false
                this.gameOver = false
                this.won = false
                let emptyPu: powerup.PowerUp[] = []
                this.powerups = emptyPu
                this.pad.widerTimer = 0
                this.pad.prevW = this.pad.baseW
                this.slowTimer = 0
                this.fx.reset()
                this.combo = 0
                this.shake = 0.0
                this.playTime = 0
                this.introPhase = 0
                this.wonCelebrated = false
            }

            // ─── input ────────────────────────────────────────────────

            // Tick the paddle's WIDER timer + re-anchor `this.pad.x` if the
            // width changed since last frame. The current width is then
            // pinned to a local for the physics formulas below.
            this.pad.tick()
            let paddleW: i32 = this.pad.width()

            if sdl.Keyboard.isPressed(sdl.Scancode.left) || sdl.Keyboard.isPressed(sdl.Scancode.a) || padDpadL {
                this.pad.moveLeft()
            }
            if sdl.Keyboard.isPressed(sdl.Scancode.right) || sdl.Keyboard.isPressed(sdl.Scancode.d) || padDpadR {
                this.pad.moveRight()
            }
            this.pad.moveAnalog(padLx)
            this.pad.clamp(consts.winW)

            // Space to launch the ball (idle on paddle until pressed).
            // Wait for the brick drop-in animation to finish before
            // accepting the launch input — otherwise the player can
            // shoot through unfilled rows.
            let introTotal = (consts.cols - 1) * consts.introStagger + consts.introDuration
            if !this.started && !this.gameOver && this.introPhase >= introTotal &&
                    (sdl.Keyboard.isPressed(sdl.Scancode.space) || padA) {
                this.started = true
            }

            // While idle, keep the (first) ball glued to the paddle's
            // centre.
            if !this.started && this.balls.length as i32 > 0 {
                let b0 = this.balls[0]
                b0.x = this.pad.x + (paddleW as f64) / 2.0 - (consts.ballSize as f64) / 2.0
                b0.y = (this.pad.y as f64) - (consts.ballSize as f64)
            }

            // ─── ball physics ─────────────────────────────────────────

            if this.started && !this.gameOver {
                // Slow-mo divides ball velocity step. Renormalize uses
                // the same factor so target magnitude shrinks.
                let speedFactor: f64 = if this.slowTimer > 0 { 0.55 } else { 1.0 }

                let surviving: ball.Ball[] = []
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
                        this.sfxDevice.queue(this.beepWall)
                    }
                    if b.x + (consts.ballSize as f64) > consts.winW as f64 {
                        b.x = (consts.winW - consts.ballSize) as f64
                        b.vx = -b.vx
                        this.sfxDevice.queue(this.beepWall)
                    }
                    if b.y < 0.0 {
                        b.y = 0.0
                        b.vy = -b.vy
                        this.sfxDevice.queue(this.beepWall)
                    }

                    // Paddle (only on the way down).
                    let bxLo = b.x
                    let bxHi = b.x + (consts.ballSize as f64)
                    let byLo = b.y
                    let byHi = b.y + (consts.ballSize as f64)
                    let pxLo = this.pad.x
                    let pxHi = this.pad.x + (paddleW as f64)
                    let pyLo = this.pad.y as f64
                    let pyHi = (this.pad.y + consts.paddleH) as f64
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
                        this.sfxDevice.queue(this.beepPaddle)
                    }

                    // Bricks. First-hit-wins-this-frame for this ball.
                    let i: i32 = 0
                    let n: i32 = this.grid.bricks.length as i32
                    while i < n {
                        let br = this.grid.bricks[i]
                        if br.alive {
                            let rxLo = br.x as f64
                            let rxHi = (br.x + consts.brickW) as f64
                            let ryLo = br.y as f64
                            let ryHi = (br.y + consts.brickH) as f64
                            if bxHi > rxLo && bxLo < rxHi
                                    && byHi > ryLo && byLo < ryHi {
                                br.alive = false
                                br.dying = 1.0
                                this.grid.aliveCount = this.grid.aliveCount - 1
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
                                this.fx.spawn(cx, cy, br.r, br.g, br.b)
                                this.fx.spawnPopup(
                                    cx, cy - 4.0, "+" + intToStr(earned),
                                    br.r, br.g, br.b
                                )
                                let bump = 4.0 + (this.combo as f64) * 1.5
                                this.shake = this.shake + bump
                                if this.shake > 14.0 { this.shake = 14.0 }
                                this.sfxDevice.queue(this.beepBrick)
                                // 14% drop chance per brick break.
                                if rng.randf() < 0.14 {
                                    let kind = (rng.randf() * 3.0) as i32
                                    this.powerups.push(new powerup.PowerUp(cx, cy, kind))
                                }
                                i = n   // stop after first hit this frame
                            }
                        }
                        i = i + 1
                    }

                    // Win: all bricks cleared. Don't break the ball
                    // loop — remaining this.balls finish their frame.
                    if this.grid.aliveCount <= 0 {
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
                        let respawn: ball.Ball[] = []
                        respawn.push(new ball.Ball((consts.winW / 2) as f64, (consts.winH / 2) as f64, 5.0, -5.0))
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
            // (paddle's WIDER timer ticks down inside `this.pad.tick()` at
            // the top of the frame.)
            if this.slowTimer > 0 { this.slowTimer = this.slowTimer - 1 }
            let alivePu: powerup.PowerUp[] = []
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
                let pdl = this.pad.x
                let pdr = this.pad.x + (paddleW as f64)
                let pdt = this.pad.y as f64
                let pdb = (this.pad.y + consts.paddleH) as f64
                let caught = cxHi > pdl && cxLo < pdr && cyHi > pdt && cyLo < pdb
                let removed: bool = caught
                if caught {
                    if pu.kind == 0 {
                        this.pad.widerTimer = 600       // 10s
                    } elif pu.kind == 1 {
                        // Multi-ball: clone first ball into two new this.balls
                        // with slightly diverged angles.
                        if this.balls.length as i32 > 0 {
                            let src = this.balls[0]
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
                                this.balls.push(extra[kj])
                                kj = kj + 1
                            }
                        }
                    } else {
                        this.slowTimer = 480            // 8s
                    }
                    this.sfxDevice.queue(this.beepPaddle)
                }
                if !removed && pu.y < (consts.winH + 40) as f64 {
                    alivePu.push(pu)
                }
                pui = pui + 1
            }
            this.powerups = alivePu

            // ─── particle / popup step ───────────────────────────────
            this.fx.step()

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
            this.sky.step(consts.winW, consts.winH)

            // ─── render ──────────────────────────────────────────────
            // The world is drawn into `this.sceneTex` so we can post-process
            // it (chromatic aberration). HUD lands directly on the
            // window afterwards so the readouts stay sharp.
            this.ren.setTarget(this.sceneTex)
            this.ren.setColor(8, 4, 24, 255)        // very dark navy
            this.ren.clear()

            // Background stars (drawn first, beneath everything).
            let bgI: i32 = 0
            let bgN: i32 = this.sky.stars.length as i32
            while bgI < bgN {
                let s = this.sky.stars[bgI]
                // Brightness pulses with phase.
                let bright = 0.4 + 0.6 * (0.5 + 0.5 * math.sin(s.phase))
                let a8 = (bright * 220.0) as u8
                this.ren.setColor(220, 220, 255, a8)
                // Bigger stars (random radius 1 or 2).
                let half: i32 = if s.speed > 1.2 { 2 } else { 1 }
                this.ren.fillRect(s.x as i32 - half + shakeOffX,
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
            let n2: i32 = this.grid.bricks.length as i32
            while i2 < n2 {
                let br = this.grid.bricks[i2]
                // Drop-in intro: each column is delayed by consts.introStagger
                // frames so the this.grid cascades in left→right. The brick
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
                    this.neonR.fillRoundedRect(br.x + shakeOffX,
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
                            this.glowTex.setColorMod(br.r, br.g, br.b)
                            this.glowTex.setAlphaMod(alpha)
                            this.ren.drawTexture(this.glowTex,
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
                    this.glowTex.setColorMod(br.r, br.g, br.b)
                    this.glowTex.setAlphaMod((t * 180.0) as u8)
                    let bw = ((consts.brickW + 2 * glowPadX) as f64) * scale
                    let bh = ((consts.brickH + 2 * glowPadY) as f64) * scale
                    this.ren.drawTexture(this.glowTex,
                                    cx - (bw as i32) / 2 + shakeOffX,
                                    cy - (bh as i32) / 2 + shakeOffY,
                                    bw as i32, bh as i32)
                    // Sharper core.
                    if alpha > 0 as u8 {
                        this.ren.setColor(br.r, br.g, br.b, alpha)
                        this.ren.fillRect(cx - dw / 2 + shakeOffX,
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
            let n3: i32 = this.fx.particles.length as i32
            while i3 < n3 {
                let p = this.fx.particles[i3]
                let alpha = (p.life * 8) as i32
                let a: u8 = if alpha > 255 { 255 } else { alpha as u8 }
                this.starTex.setColorMod(p.r, p.g, p.b)
                // Outer halo: bigger, lower alpha. Drawn first so the
                // crisp star sits on top of its glow.
                let haloA = ((a as i32) / 3) as u8
                this.starTex.setAlphaMod(haloA)
                let haloSize: i32 = 56
                this.ren.drawTextureRot(this.starTex,
                                   (p.x as i32) - haloSize / 2 + shakeOffX,
                                   (p.y as i32) - haloSize / 2 + shakeOffY,
                                   haloSize, haloSize, p.angle)
                // Sharp star body (2× the previous 16px).
                this.starTex.setAlphaMod(a)
                let size: i32 = 32
                this.ren.drawTextureRot(this.starTex,
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
                let pr2: u8 = if pu.kind == 0 { 120 as u8 } elif pu.kind == 1 { 255 as u8 } else { 200 as u8 }
                let pg2: u8 = if pu.kind == 0 { 220 as u8 } elif pu.kind == 1 { 220 as u8 } else { 120 as u8 }
                let pb2: u8 = if pu.kind == 0 { 255 as u8 } elif pu.kind == 1 { 120 as u8 } else { 255 as u8 }
                let chipX = (pu.x as i32) - 18 + shakeOffX
                let chipY = (pu.y as i32) - 8 + shakeOffY
                this.neonR.fillStadium(chipX, chipY, 36, 16, pr2, pg2, pb2)
                let label: string = if pu.kind == 0 { "W" } elif pu.kind == 1 { "M" } else { "S" }
                this.font.drawText(this.ren, label, chipX + 14, chipY + 2,
                              2, 20 as u8, 10 as u8, 30 as u8, 255 as u8)
                pri = pri + 1
            }

            // Paddle (white/cyan glow, rounded ends). The colour shifts
            // toward magenta while the WIDER power-up is active so the
            // boost is obvious.
            let pr: u8 = if this.pad.widerTimer > 0 { 255 as u8 } else { 200 as u8 }
            let pg: u8 = if this.pad.widerTimer > 0 { 180 as u8 } else { 240 as u8 }
            let pb: u8 = if this.pad.widerTimer > 0 { 255 as u8 } else { 255 as u8 }
            this.neonR.fillStadium(this.pad.x as i32 + shakeOffX, this.pad.y + shakeOffY,
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
                        this.ren.setColor(255, 255, 255, a)
                        let dy: i32 = -radius
                        while dy <= radius {
                            let dyf = dy as f64
                            let radf = radius as f64
                            let w = math.sqrt(radf * radf - dyf * dyf)
                            let half = w as i32
                            this.ren.fillRect(cx - half, cy + dy, half * 2 + 1, 1)
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
                this.neonR.fillCircle(ballCx, ballCy, consts.ballSize / 2, 255, 255, 255)
                bri = bri + 1
            }

            // 6px advance per glyph at scale=1 (5px cell + 1px space),
            // last glyph contributes only 5px → final width formula
            // is in `textPxW` below. Used for centering and right-
            // alignment.

            // Score popups — part of the world so they pick up this.shake
            // and chromatic aberration.
            let popI: i32 = 0
            let popN: i32 = this.fx.popups.length as i32
            while popI < popN {
                let pp = this.fx.popups[popI]
                let alphaI = pp.life * 6
                let pa: u8 = if alphaI > 255 { 255 as u8 } else { alphaI as u8 }
                let scaleP: i32 = 2
                let tw = textPxW(pp.text, scaleP)
                this.font.drawText(this.ren, pp.text,
                              (pp.x as i32) - tw / 2 + shakeOffX,
                              (pp.y as i32) + shakeOffY,
                              scaleP, pp.r, pp.g, pp.b, pa)
                popI = popI + 1
            }

            // ─── HUD (drawn into this.sceneTex so it gets the same CRT
            // ─── post-effects — chromatic aberration, scanlines, and
            // ─── vignette — as the world).

            this.font.drawText(this.ren, "SCORE: " + intToStr(this.score),
                          20, 20, 2, 200, 220, 255, 255)
            let fpsTxt = "FPS: " + intToStr(this.fps)
            this.font.drawText(this.ren, fpsTxt,
                          20, 50, 2, 180, 200, 220, 255)

            // Active power-up countdowns (in whole seconds, 60fps).
            let puY: i32 = 80
            if this.pad.widerTimer > 0 {
                let s = "WIDER " + intToStr((this.pad.widerTimer + 59) / 60) + "S"
                this.font.drawText(this.ren, s, 20, puY, 2, 120, 220, 255, 255)
                puY = puY + 24
            }
            if this.slowTimer > 0 {
                let s = "SLOW " + intToStr((this.slowTimer + 59) / 60) + "S"
                this.font.drawText(this.ren, s, 20, puY, 2, 200, 120, 255, 255)
                puY = puY + 24
            }
            let livesTxt = "LIVES: " + intToStr(this.lives)
            this.font.drawText(this.ren, livesTxt,
                          consts.winW - textPxW(livesTxt, 2) - 20, 20,
                          2, 255, 200, 220, 255)

            // Combo readout — only shown while a streak is active so
            // it doesn't take up HUD real-estate at idle.
            if this.combo > 1 {
                let comboTxt = "COMBO x" + intToStr(this.combo)
                this.font.drawText(this.ren, comboTxt,
                              (consts.winW - textPxW(comboTxt, 3)) / 2, 80,
                              3, 255, 220, 120, 255)
            }

            if !this.started && !this.gameOver {
                let s = "PRESS SPACE TO START"
                this.font.drawText(this.ren, s,
                              (consts.winW - textPxW(s, 3)) / 2, 400,
                              3, 200, 220, 255, 255)
            }

            if this.gameOver {
                if this.won {
                    let t = "CLEAR!"
                    this.font.drawText(this.ren, t,
                                  (consts.winW - textPxW(t, 6)) / 2, 300,
                                  6, 120, 255, 200, 255)
                } else {
                    let t = "GAME OVER"
                    this.font.drawText(this.ren, t,
                                  (consts.winW - textPxW(t, 6)) / 2, 300,
                                  6, 255, 100, 200, 255)
                }
                let s = "PRESS R TO RESTART"
                this.font.drawText(this.ren, s,
                              (consts.winW - textPxW(s, 3)) / 2, 400,
                              3, 200, 220, 255, 255)
            }

            // ─── bloom prep: downsample the scene into this.bloomTex ─────
            // Linear filtering averages 4×4 source blocks into each
            // bloom pixel; later we'll upscale this back to full
            // resolution which spreads the average into a soft halo.
            this.ren.setTarget(this.bloomTex)
            this.ren.setColor(0, 0, 0, 0)
            this.ren.clear()
            this.ren.drawTexture(this.sceneTex, 0, 0, consts.bloomW, consts.bloomH)

            // ─── composite this.sceneTex (world + HUD) to window with
            // ─── chromatic aberration. ────────────────────────────────
            this.ren.resetTarget()
            this.ren.setColor(0, 0, 0, 255)
            this.ren.clear()
            // Three additive passes, each keeping only one colour
            // channel and offset slightly. Pixel overlap reproduces the
            // original colour; high-contrast edges (including text)
            // show RGB fringing.
            this.sceneTex.setBlendMode(2)   // SDL_BLENDMODE_ADD
            let chroma: i32 = 3
            this.sceneTex.setColorMod(255 as u8, 0 as u8, 0 as u8)
            this.ren.drawTexture(this.sceneTex, -chroma, 0, consts.winW, consts.winH)
            this.sceneTex.setColorMod(0 as u8, 255 as u8, 0 as u8)
            this.ren.drawTexture(this.sceneTex, 0, 0, consts.winW, consts.winH)
            this.sceneTex.setColorMod(0 as u8, 0 as u8, 255 as u8)
            this.ren.drawTexture(this.sceneTex, chroma, 0, consts.winW, consts.winH)
            // Restore for next frame's setTarget pass.
            this.sceneTex.setBlendMode(1)   // SDL_BLENDMODE_BLEND
            this.sceneTex.setColorMod(255 as u8, 255 as u8, 255 as u8)

            // ─── bloom: additively overlay the upscaled blurred
            // ─── version, brightening hot spots into a soft glow.
            // Without a shader-side bright-pass we instead darken the
            // whole bloom contribution via colour-mod so mid-brightness
            // pixels don't push the composite to white. The result is
            // a soft glow only on the hottest parts of the scene.
            this.bloomTex.setBlendMode(2)
            this.bloomTex.setColorMod(60 as u8, 60 as u8, 60 as u8)
            this.bloomTex.setAlphaMod(255 as u8)
            this.ren.drawTexture(this.bloomTex, 0, 0, consts.winW, consts.winH)
            this.bloomTex.setBlendMode(1)
            this.bloomTex.setColorMod(255 as u8, 255 as u8, 255 as u8)

            // CRT overlay (scanlines + vignette).
            this.ren.drawTexture(this.crtTex, 0, 0, consts.winW, consts.winH)

            this.ren.present()

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
