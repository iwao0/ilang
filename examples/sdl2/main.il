// SDL2 minimal game-screen demo: open a window, create a renderer,
// then animate a "ball" (filled rectangle) bouncing off the edges
// and a stationary "paddle" along the bottom. Runs for 5 seconds
// at ~60fps then tears everything down.
//
// Run (JIT only — `@lib(...)` extern fns need dlsym):
//   ilang run --jit examples/sdl2/main.il

use sdl

const WIN_W: i64 = 800
const WIN_H: i64 = 600

const BALL_SIZE: i64 = 32
const PADDLE_W: i64 = 120
const PADDLE_H: i64 = 16

fn main() {
    let rc = sdl.init(sdl.INIT_VIDEO as u32)
    if rc < 0 as i32 {
        console.log("SDL_Init failed: " + sdl.getError())
        return
    }

    let win = sdl.createWindow("ilang × SDL2", WIN_W as i32, WIN_H as i32)
    if win == 0 {
        console.log("SDL_CreateWindow failed: " + sdl.getError())
        sdl.quit()
        return
    }

    let ren = sdl.createRenderer(win)
    if ren == 0 {
        console.log("SDL_CreateRenderer failed: " + sdl.getError())
        sdl.destroyWindow(win)
        sdl.quit()
        return
    }

    console.log("playing for 5 seconds")

    // Game state.
    let ball_x: i64 = 100
    let ball_y: i64 = 100
    let ball_vx: i64 = 5
    let ball_vy: i64 = 4
    let paddle_x: i64 = (WIN_W - PADDLE_W) / 2
    let paddle_y: i64 = WIN_H - PADDLE_H - 20

    let elapsed: i64 = 0
    let frame_ms: i64 = 16
    while elapsed < 5000 {
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
        let _ = sdl.setColor(ren, 20 as u8, 24 as u8, 48 as u8, 255 as u8)
        let _ = sdl.clear(ren)

        let _ = sdl.setColor(ren, 240 as u8, 240 as u8, 240 as u8, 255 as u8)
        let _ = sdl.fillRect(
            ren,
            paddle_x as i32, paddle_y as i32,
            PADDLE_W as i32, PADDLE_H as i32
        )

        let _ = sdl.setColor(ren, 220 as u8, 60 as u8, 60 as u8, 255 as u8)
        let _ = sdl.fillRect(
            ren,
            ball_x as i32, ball_y as i32,
            BALL_SIZE as i32, BALL_SIZE as i32
        )

        sdl.present(ren)
        sdl.pumpEvents()
        sdl.delay(frame_ms as u32)
        elapsed = elapsed + frame_ms
    }

    sdl.destroyRenderer(ren)
    sdl.destroyWindow(win)
    sdl.quit()
}

main()
