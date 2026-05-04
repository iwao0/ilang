// SDL2 minimal game-screen demo: open a window, create a renderer,
// then animate a "ball" (filled rectangle) bouncing off the edges
// and a stationary "paddle" along the bottom. Runs for 5 seconds
// at ~60fps then tears everything down.
//
// Window / Renderer are ARC-managed classes — `deinit` automatically
// runs SDL_DestroyWindow / SDL_DestroyRenderer when the bindings
// leave scope, so the only explicit cleanup the caller writes is
// `sdl.quit()` for the SDL subsystems themselves.
//
// Run (JIT only — `@lib(...)` extern fns need dlsym):
//   ilang run --jit examples/sdl2/main.il

use sdl

// Coordinates / sizes are i32 to match SDL's int-based API. Time
// values are u32 (SDL_Delay / SDL_GetTicks contract).
const WIN_W: i32 = 800
const WIN_H: i32 = 600

const BALL_SIZE: i32 = 32
const PADDLE_W: i32 = 120
const PADDLE_H: i32 = 16

const PLAY_MS: u32 = 5000
const FRAME_MS: u32 = 16

fn main() {
    let rc = sdl.init(sdl.INIT_VIDEO)
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

    console.log("playing for 5 seconds")

    // Game state.
    let ball_x: i32 = 100
    let ball_y: i32 = 100
    let ball_vx: i32 = 5
    let ball_vy: i32 = 4
    let paddle_x: i32 = (WIN_W - PADDLE_W) / 2
    let paddle_y: i32 = WIN_H - PADDLE_H - 20

    let elapsed: u32 = 0
    while elapsed < PLAY_MS {
        // Update — bounce the ball off all four edges.
        ball_x = ball_x + ball_vx
        ball_y = ball_y + ball_vy
        if ball_x < 0 {
            ball_x = 0
            ball_vx = 0 - ball_vx
        }
        if ball_x + BALL_SIZE > WIN_W {
            ball_x = WIN_W - BALL_SIZE
            ball_vx = 0 - ball_vx
        }
        if ball_y < 0 {
            ball_y = 0
            ball_vy = 0 - ball_vy
        }
        if ball_y + BALL_SIZE > WIN_H {
            ball_y = WIN_H - BALL_SIZE
            ball_vy = 0 - ball_vy
        }

        // Render — clear to dark blue, draw paddle (white)
        // and ball (red), then present.
        let _ = ren.setColor(20, 24, 48, 255)
        let _ = ren.clear()

        let _ = ren.setColor(240, 240, 240, 255)
        let _ = ren.fillRect(paddle_x, paddle_y, PADDLE_W, PADDLE_H)

        let _ = ren.setColor(220, 60, 60, 255)
        let _ = ren.fillRect(ball_x, ball_y, BALL_SIZE, BALL_SIZE)

        ren.present()
        sdl.pumpEvents()
        sdl.delay(FRAME_MS)
        elapsed += FRAME_MS
    }

    // ren / win deinit fires automatically as they leave main()'s
    // scope (renderer first, then window — reverse declaration order).
    sdl.quit()
}

main()
