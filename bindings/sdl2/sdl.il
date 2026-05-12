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
pub use sdl_core as _ { * }
pub use sdl_surface as _ { * }
pub use sdl_window as _ { * }
pub use sdl_renderer as _ { * }
pub use sdl_time as _ { * }
pub use sdl_keyboard as _ { * }
pub use sdl_audio as _ { * }
pub use sdl_joystick as _ { * }
pub use sdl_gamecontroller as _ { * }
pub use sdl_mouse as _ { * }
pub use sdl_filesystem as _ { * }
pub use sdl_rwops as _ { * }
pub use sdl_messagebox as _ { * }
// `sdl_ttf` is intentionally NOT re-exported here — SDL2_ttf is a
// separate library (`libSDL2_ttf`) that may not be installed on
// every system. Including it in the umbrella would force the JIT
// to dlopen SDL2_ttf at startup for every program that touches
// `sdl`, even ones that never render text. Apps that use TTF
// `use sdl_ttf` directly.
