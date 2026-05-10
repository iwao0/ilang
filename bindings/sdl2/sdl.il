// Umbrella module — re-exports every per-feature SDL2 binding under
// a single `sdl.*` namespace. After `use sdl` the caller can write
// `sdl.init`, `new sdl.Window(...)`, `ren.fillRect(...)` etc.
// without naming the individual feature modules.

// Order matters: each module that does `use otherModule { … }`
// must come after the module it imports from, so the umbrella
// pulls them in dependency order. Current chain:
//   sdl_core     ← (root — Color / Rect / Point / Size live here
//                  precisely so the rest of the tree can share
//                  them without circular imports)
//   sdl_surface  ← sdl_core
//   sdl_window   ← sdl_core, sdl_surface (SDL_Surface)
//   sdl_renderer ← sdl_core, sdl_window  (Window)
pub use sdl_core
pub use sdl_surface
pub use sdl_window
pub use sdl_renderer
pub use sdl_time
pub use sdl_keyboard
pub use sdl_audio
pub use sdl_gamecontroller
pub use sdl_mouse
pub use sdl_joystick
pub use sdl_filesystem
pub use sdl_rwops
pub use sdl_messagebox
// `sdl_ttf` is intentionally NOT re-exported here — SDL2_ttf is a
// separate library (`libSDL2_ttf`) that may not be installed on
// every system. Including it in the umbrella would force the JIT
// to dlopen SDL2_ttf at startup for every program that touches
// `sdl`, even ones that never render text. Apps that use TTF
// `use sdl_ttf` directly.
