# SDL2 bindings for ilang

ARC-managed wrappers around a useful subset of SDL2. JIT-only — the
ilang interpreter has no dlsym path.

## Modules

| File | Provides |
| --- | --- |
| `sdl.il` | umbrella — `@export use`s every other file under the `sdl.*` namespace |
| `sdl_core.il` | `init` / `quit` / `getError`, `INIT_*` flags |
| `sdl_window.il` | `class Window` (ARC-managed handle, opens / destroys) |
| `sdl_renderer.il` | `class Renderer` — `setColor` / `clear` / `fillRect` / `present` |
| `sdl_time.il` | `delay` / `pumpEvents` |
| `sdl_keyboard.il` | `class Keyboard` (`static isPressed`), `SCANCODE_*` constants |
| `sdl_audio.il` | `class AudioDevice` — queue-based 16-bit signed LE PCM playback |

## Using these bindings from your project

Create an `ilang.toml` next to your entry file:

```toml
[package]
name = "my_game"

[deps]
sdl2 = "/absolute/or/relative/path/to/bindings/sdl2"
```

Then:

```ilang
use sdl

fn main() {
    let rc = sdl.init(sdl.INIT_VIDEO)
    if rc < 0 { return }
    let win = new sdl.Window("my game", 640, 480)
    let ren = new sdl.Renderer(win.handle)
    // ...
    sdl.quit()
}

main()
```

Run with `ilang run --jit path/to/your/main.il`. The CLI walks
upward from the entry file looking for `ilang.toml`; the `[deps]`
table contributes search directories for `use module` resolution.

## Requirements

- **macOS**: `brew install sdl2` (the loader checks `/opt/homebrew/lib`
  and `/usr/local/lib` automatically)
- **Linux**: `apt install libsdl2-2.0-0` (or your distro's equivalent)
- **Windows**: untested; the `dylib` candidate list would need a
  `.dll` form

## What's covered today

Enough for a 2D game with keyboard input + sound effects: window /
renderer / fillRect drawing / 16-bit PCM audio queue / keyboard
state polling. No image loading (SDL_image), no fonts (SDL_ttf),
no networking, no multithreaded audio callback.

## See also

`examples/sdl_bouncing_ball/` — minimal demo (paddle + bouncing
ball + bounce-on-edge beep) that uses every module here.
