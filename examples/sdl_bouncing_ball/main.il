// SDL2 game-screen demo: a procedurally-generated sprite bounces
// around the window, rotating as it goes, with a paddle the user
// controls along the bottom and a beep on every wall hit. Runs for
// 5 seconds at ~60fps then tears everything down.
//
// Window / Renderer / Texture / AudioDevice are ARC-managed
// classes — `deinit` automatically runs the matching SDL_*Destroy
// / SDL_CloseAudioDevice when the bindings leave scope, so the
// only explicit cleanup the caller writes is `sdl.quit()` for the
// SDL subsystems themselves.
//
// Controls:
//   ←/→  or  A/D  — move the paddle
//   ESC          — exit early
//
// Run (JIT only — `@lib(...)` extern fns need dlsym):
//   ilang run --jit examples/sdl_bouncing_ball/main.il
//
// The SDL2 bindings live under bindings/sdl2/; the `ilang.toml` in
// this directory points at them so `use sdl` resolves automatically.

use sdl
use math

// Coordinates / sizes are i32 to match SDL's int-based API. Time
// values are u32 (SDL_Delay / SDL_GetTicks contract).
const WIN_W: i32 = 800
const WIN_H: i32 = 600

const SPRITE_TEX_SIZE: i32 = 64       // sprite texture is 64×64 pixels
const SPRITE_DRAW_SIZE: i32 = 64      // and drawn at the same size
const PADDLE_W: i32 = 120
const PADDLE_H: i32 = 16

const PLAY_MS: u32 = 5000
const FRAME_MS: u32 = 16
const PADDLE_SPEED: i32 = 8

const SAMPLE_RATE: i32 = 44100
const BEEP_HZ: f64 = 660.0
const BEEP_MS: i32 = 80
const BEEP_AMP: f64 = 8000.0

// Tiny int → string helper. ilang's `as` cast is numeric-only, so
// stringifying counters needs a few lines of code.
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

// Procedurally render a 64×64 RGBA8 sprite into a flat byte buffer
// suitable for `Texture.update`. Pattern: a soft-edged disc with a
// radial gradient (warm orange → bright yellow centre) and a
// transparent corner. Looks like a tiny sun.
fn make_sprite(): u8[] {
    let size: i32 = SPRITE_TEX_SIZE
    let buf: u8[] = []
    let cx = (size as f64) / 2.0
    let cy = (size as f64) / 2.0
    let radius = cx - 1.0
    let y: i32 = 0
    while y < size {
        let x: i32 = 0
        while x < size {
            let dx = (x as f64) - cx + 0.5
            let dy = (y as f64) - cy + 0.5
            let d = math.sqrt(dx * dx + dy * dy)
            if d <= radius {
                // t = 1 at centre, 0 at edge.
                let t = 1.0 - d / radius
                // Bright yellow centre fading to deep orange.
                let r = 255.0
                let g = 200.0 * t + 90.0 * (1.0 - t)
                let b = 80.0 * t
                let a = 255.0
                buf.push(r as u8)
                buf.push(g as u8)
                buf.push(b as u8)
                buf.push(a as u8)
            } else {
                // Outside the disc → fully transparent so the
                // background shows through.
                buf.push(0)
                buf.push(0)
                buf.push(0)
                buf.push(0)
            }
            x += 1
        }
        y += 1
    }
    buf
}

// Pre-render a short sine-wave beep. Returns an `i16[]` ready to be
// queued straight into an `AudioDevice` (mono, 44.1 kHz, S16LE).
fn make_beep(freq_hz: f64, ms: i32): i16[] {
    let n = (SAMPLE_RATE * ms) / 1000
    let buf: i16[] = []
    let i: i32 = 0
    let two_pi: f64 = 2.0 * math.pi
    while i < n {
        // Soft envelope so the beep doesn't click on start/stop.
        let ramp = SAMPLE_RATE / 200
        let env: f64 = if i < ramp {
            (i as f64) / (ramp as f64)
        } elif i > n - ramp {
            ((n - i) as f64) / (ramp as f64)
        } else {
            1.0
        }
        let t = (i as f64) / (SAMPLE_RATE as f64)
        let sample = math.sin(two_pi * freq_hz * t) * BEEP_AMP * env
        buf.push(sample as i16)
        i += 1
    }
    buf
}

let rc = sdl.init(sdl.INIT_VIDEO | sdl.INIT_AUDIO)
if rc < 0 {
    console.log("SDL_Init failed: " + sdl.getError())
    return
}

let win = new sdl.Window("ilang × SDL2", WIN_W, WIN_H)
if win.handle == 0 {
    console.log("SDL_CreateWindow failed: " + sdl.getError())
    sdl.quit()
    return
}

let ren = new sdl.Renderer(win.handle)
if ren.handle == 0 {
    console.log("SDL_CreateRenderer failed: " + sdl.getError())
    sdl.quit()
    return
}

let sprite = new sdl.Texture(ren, SPRITE_TEX_SIZE, SPRITE_TEX_SIZE)
if sprite.handle == 0 {
    console.log("SDL_CreateTexture failed: " + sdl.getError())
    sdl.quit()
    return
}
sprite.update(make_sprite())

let font = new sdl.Font(ren)

let audio = new sdl.AudioDevice(SAMPLE_RATE, 1)
if audio.id == 0 {
    console.log("SDL_OpenAudioDevice failed: " + sdl.getError())
    sdl.quit()
    return
}
audio.play()
let beep = make_beep(BEEP_HZ, BEEP_MS)

console.log("playing for 5 seconds")

// Game state.
let ball_x: i32 = 100
let ball_y: i32 = 100
let ball_vx: i32 = 5
let ball_vy: i32 = 4
let bounces: i32 = 0
let angle: f64 = 0.0
let paddle_x: i32 = (WIN_W - PADDLE_W) / 2
let paddle_y: i32 = WIN_H - PADDLE_H - 20

let elapsed: u32 = 0
while elapsed < PLAY_MS {
    // Drain SDL events so the OS run loop advances and the
    // keyboard-state snapshot reflects the current frame.
    sdl.pumpEvents()

    // ESC quits early.
    if sdl.Keyboard.isPressed(sdl.SCANCODE_ESCAPE) {
        elapsed = PLAY_MS
    }

    // Paddle: arrow keys or A/D move horizontally, clamped to the edges.
    if sdl.Keyboard.isPressed(sdl.SCANCODE_LEFT) || sdl.Keyboard.isPressed(sdl.SCANCODE_A) {
        paddle_x = paddle_x - PADDLE_SPEED
    }
    if sdl.Keyboard.isPressed(sdl.SCANCODE_RIGHT) || sdl.Keyboard.isPressed(sdl.SCANCODE_D) {
        paddle_x = paddle_x + PADDLE_SPEED
    }
    if paddle_x < 0 {
        paddle_x = 0
    }
    if paddle_x + PADDLE_W > WIN_W {
        paddle_x = WIN_W - PADDLE_W
    }

    // Bounce the sprite off all four edges, beeping on each hit.
    ball_x = ball_x + ball_vx
    ball_y = ball_y + ball_vy
    let bounced = false
    if ball_x < 0 {
        ball_x = 0
        ball_vx = 0 - ball_vx
        bounced = true
    }
    if ball_x + SPRITE_DRAW_SIZE > WIN_W {
        ball_x = WIN_W - SPRITE_DRAW_SIZE
        ball_vx = 0 - ball_vx
        bounced = true
    }
    if ball_y < 0 {
        ball_y = 0
        ball_vy = 0 - ball_vy
        bounced = true
    }
    if ball_y + SPRITE_DRAW_SIZE > WIN_H {
        ball_y = WIN_H - SPRITE_DRAW_SIZE
        ball_vy = 0 - ball_vy
        bounced = true
    }
    if bounced {
        audio.queue(beep)
        bounces += 1
    }

    angle += 3.0    // degrees per frame

    // Render — dark-blue background, white paddle, rotating sprite.
    ren.setColor(20, 24, 48, 255)
    ren.clear()

    ren.setColor(240, 240, 240, 255)
    ren.fillRect(paddle_x, paddle_y, PADDLE_W, PADDLE_H)

    ren.drawTextureRot(
        sprite.handle,
        ball_x, ball_y, SPRITE_DRAW_SIZE, SPRITE_DRAW_SIZE,
        angle
    )

    // Title (top-centre) and bounce counter (top-left).
    font.drawText(ren, "ILANG X SDL2", 250, 16, 4, 240, 240, 255, 255)
    font.drawText(ren, "BOUNCES: " + intToStr(bounces),
                  16, 16, 2, 200, 200, 220, 255)

    ren.present()
    sdl.delay(FRAME_MS)
    elapsed += FRAME_MS
}

// sprite / audio / ren / win deinit fires automatically as the
// bindings leave scope (sprite first, then audio, then renderer,
// then window).
sdl.quit()
