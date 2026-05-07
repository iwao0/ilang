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
use sdl { InitFlag }
use game { Game }

let rc = sdl.init(InitFlag.video | InitFlag.audio | InitFlag.gameController)
if rc < 0 {
    console.log("SDL_Init failed: " + sdl.getError())
    return
}
// All ARC-managed resources (Window / Renderer / Audio / etc.) are
// owned by the `Game` instance, so they drop the moment this `let`
// goes out of scope at the end of the if-block — strictly before
// `sdl.quit()`. Without that order SDL would tear down its
// subsystems first and the destructor calls would crash.
{
    let g = new Game()
    g.run()
}
sdl.quit()
