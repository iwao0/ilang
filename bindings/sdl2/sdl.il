// Umbrella module — re-exports every per-feature SDL2 binding under
// a single `sdl.*` namespace. After `use sdl` the caller can write
// `sdl.init`, `new sdl.Window(...)`, `ren.fillRect(...)` etc.
// without naming the individual feature modules.

@export use sdl_core
@export use sdl_window
@export use sdl_renderer
@export use sdl_time
@export use sdl_keyboard
@export use sdl_audio
@export use sdl_gamepad
