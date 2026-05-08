// Umbrella module — re-exports every per-feature SDL2 binding under
// a single `sdl.*` namespace. After `use sdl` the caller can write
// `sdl.init`, `new sdl.Window(...)`, `ren.fillRect(...)` etc.
// without naming the individual feature modules.

// Order matters: each module that does `use otherModule { … }`
// must come after the module it imports from, so the umbrella
// pulls them in dependency order. Current chain:
//   sdl_core   ← (root)
//   sdl_renderer ← sdl_core
//   sdl_surface  ← sdl_core, sdl_renderer (Color)
//   sdl_window   ← sdl_core, sdl_surface  (SDL_Surface)
@export use sdl_core
@export use sdl_renderer
@export use sdl_surface
@export use sdl_window
@export use sdl_time
@export use sdl_keyboard
@export use sdl_audio
@export use sdl_gamepad
@export use sdl_mouse
@export use sdl_joystick
@export use sdl_filesystem
@export use sdl_rwops
@export use sdl_messagebox
@export use sdl_shape
@export use sdl_syswm
// `sdl_ttf` is intentionally NOT re-exported here — SDL2_ttf is a
// separate library (`libSDL2_ttf`) that may not be installed on
// every system. Including it in the umbrella would force the JIT
// to dlopen SDL2_ttf at startup for every program that touches
// `sdl`, even ones that never render text. Apps that use TTF
// `use sdl_ttf` directly.
