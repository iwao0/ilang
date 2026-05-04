// SDL2 minimal demo: open a 640×480 window for 2 seconds, then
// tear everything down. Each `use` pulls in one feature module
// from the same directory.
//
// Run (JIT only — `@lib(...)` extern fns need dlsym):
//   ilang run --jit examples/sdl2/main.il

use sdl_core
use sdl_window
use sdl_time

let rc = sdl_core.init(sdl_core.INIT_VIDEO as u32)
if rc < 0 as i32 {
    console.log("SDL_Init failed: " + sdl_core.getError())
} else {
    let win = sdl_window.create("ilang × SDL2", 640 as i32, 480 as i32)
    if win == 0 {
        console.log("SDL_CreateWindow failed: " + sdl_core.getError())
    } else {
        console.log("window opened — closing in 2 seconds")
        // ~60fps event-pump loop so macOS Cocoa actually paints.
        let elapsed: i64 = 0
        let frame_ms: i64 = 16
        while elapsed < 2000 {
            sdl_time.pumpEvents()
            sdl_time.delay(frame_ms as u32)
            elapsed = elapsed + frame_ms
        }
        sdl_window.destroy(win)
    }
    sdl_core.quit()
}
