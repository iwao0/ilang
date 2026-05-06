// Neon-style draw helpers: every shape gets three accumulating
// halo passes (large + faint, medium, sharp + opaque) to fake the
// glow of a saturated primary on a dark background. The renderer's
// blend mode must already be BLEND for the haloes to compose
// correctly — `sdl.Renderer.init` enables it by default.

use sdl
use math

class NeonRenderer {
    ren: sdl.Renderer
    init(ren: sdl.Renderer) {
        this.ren = ren
    }

    /// Filled rectangle with two semi-transparent halo passes
    /// around it.
    fillRect(x: i32, y: i32, w: i32, h: i32,
             r: u8, g: u8, b: u8) {
        let ren = this.ren
        ren.setColor(r, g, b, 50)
        ren.fillRect(x - 6, y - 6, w + 12, h + 12)
        ren.setColor(r, g, b, 110)
        ren.fillRect(x - 3, y - 3, w + 6, h + 6)
        ren.setColor(r, g, b, 255)
        ren.fillRect(x, y, w, h)
        return
    }

    /// Filled rectangle with small rounded corners (pill / chiclet
    /// shape). Three accumulating halo passes give the same neon
    /// glow as `fillRect`.
    fillRoundedRect(x: i32, y: i32, w: i32, h: i32, radius: i32,
                    r: u8, g: u8, b: u8) {
        let ren = this.ren
        let pass: i32 = 0
        while pass < 3 {
            let extra: i32 = match pass {
                0 { 6 }
                1 { 3 }
                _ { 0 }
            }
            let alpha: u8 = match pass {
                0 { 50 as u8 }
                1 { 110 as u8 }
                _ { 255 as u8 }
            }
            let xx = x - extra
            let yy = y - extra
            let ww = w + 2 * extra
            let hh = h + 2 * extra
            let rad = radius + extra
            ren.setColor(r, g, b, alpha)
            // Middle band: full width, between the rounded caps.
            ren.fillRect(xx, yy + rad, ww, hh - 2 * rad)
            // Top + bottom with corner inset computed from the arc.
            let dy: i32 = 0
            while dy < rad {
                let cy = (rad as f64) - 0.5 - (dy as f64)
                let radf = rad as f64
                let chord = math.sqrt(radf * radf - cy * cy)
                let inset = ((rad as f64) - chord) as i32
                ren.fillRect(xx + inset, yy + dy, ww - 2 * inset, 1)
                ren.fillRect(xx + inset, yy + hh - 1 - dy, ww - 2 * inset, 1)
                dy += 1
            }
            pass += 1
        }
    }

    /// Filled stadium (rectangle with semicircular ends) — used for
    /// the paddle. Three accumulating halo passes again.
    fillStadium(x: i32, y: i32, w: i32, h: i32,
                r: u8, g: u8, b: u8) {
        let ren = this.ren
        let pass: i32 = 0
        while pass < 3 {
            let extra: i32 = match pass {
                0 { 6 }
                1 { 3 }
                _ { 0 }
            }
            let alpha: u8 = match pass {
                0 { 50 as u8 }
                1 { 110 as u8 }
                _ { 255 as u8 }
            }
            let xx = x - extra
            let yy = y - extra
            let ww = w + 2 * extra
            let hh = h + 2 * extra
            let rad = hh / 2
            ren.setColor(r, g, b, alpha)
            // Centre rectangle (between the two end caps).
            ren.fillRect(xx + rad, yy, ww - 2 * rad, hh)
            // Two semicircle caps via scanlines.
            let dy: i32 = -rad
            while dy <= rad {
                let dyf = dy as f64
                let radf = rad as f64
                let half = (math.sqrt(radf * radf - dyf * dyf)) as i32
                // Left cap.
                ren.fillRect(xx + rad - half, yy + rad + dy, half + 1, 1)
                // Right cap.
                ren.fillRect(xx + ww - rad, yy + rad + dy, half + 1, 1)
                dy += 1
            }
            pass += 1
        }
    }

    /// Filled circle as a stack of 1-pixel-tall horizontal strips
    /// (scanline rasteriser). Three passes at different radii +
    /// alphas give the same neon halo as the rectangles.
    fillCircle(cx: i32, cy: i32, radius: i32,
               r: u8, g: u8, b: u8) {
        let ren = this.ren
        let pass: i32 = 0
        while pass < 3 {
            let extra: i32 = match pass {
                0 { 6 }
                1 { 3 }
                _ { 0 }
            }
            let alpha: u8 = match pass {
                0 { 50 as u8 }
                1 { 110 as u8 }
                _ { 255 as u8 }
            }
            let rad = radius + extra
            ren.setColor(r, g, b, alpha)
            let dy: i32 = -rad
            while dy <= rad {
                let dyf = dy as f64
                let radf = rad as f64
                let w = math.sqrt(radf * radf - dyf * dyf)
                let half = w as i32
                ren.fillRect(cx - half, cy + dy, half * 2 + 1, 1)
                dy += 1
            }
            pass += 1
        }
    }
}
