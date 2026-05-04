// ARC-managed wrappers around SDL2's window / renderer handles.
// Each class owns the underlying `i64` C handle and frees it in
// `deinit`, so callers don't have to remember explicit destroy
// calls — when the binding leaves scope the resource is released.
//
// Local variables in ilang drop in reverse declaration order, so
// declaring `let win = ...` followed by `let ren = ...` releases
// the renderer first and the window second — which is the order
// SDL2 itself requires (renderer references the window).

use sdl_window
use sdl_renderer

class Window {
    handle: i64

    init(title: string, w: i32, h: i32) {
        this.handle = sdl_window.createWindow(title, w, h)
    }

    deinit() {
        if this.handle != 0 {
            sdl_window.destroyWindow(this.handle)
        }
    }
}

class Renderer {
    handle: i64

    init(window: Window) {
        this.handle = sdl_renderer.createRenderer(window.handle)
    }

    deinit() {
        if this.handle != 0 {
            sdl_renderer.destroyRenderer(this.handle)
        }
    }

    setColor(r: u8, g: u8, b: u8, a: u8): i32 {
        sdl_renderer.setColor(this.handle, r, g, b, a)
    }

    clear(): i32 {
        sdl_renderer.clear(this.handle)
    }

    fillRect(x: i32, y: i32, w: i32, h: i32): i32 {
        sdl_renderer.fillRect(this.handle, x, y, w, h)
    }

    present() {
        sdl_renderer.present(this.handle)
    }
}
