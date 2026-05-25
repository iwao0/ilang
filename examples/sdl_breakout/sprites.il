// Procedurally-generated RGBA8 sprites used by the renderer:
//   • makeStarSprite — 5-point neon star (32×32)
//   • makeGlowSprite — soft radial gradient (64×64)
//   • makeCrtOverlay — full-window scanline + vignette darken
//
// All return `u8[]` buffers ready to feed into `Texture.update`.

use std.math as math
use consts

// Render a 5-point white star into a 32×32 RGBA8 buffer suitable
// for `Texture.update`. The shape is computed as the union of the
// triangles between the centre, an outer point, and an inner point,
// alternating around 10 radial steps. Pixels are anti-aliased by
// coverage-fraction (the closer the centre, the brighter), giving
// a soft glow that drops to transparent at the edge. Color
// modulation at draw time tints it.
pub fn makeStarSprite(): u8[] {
    let size: i32 = consts.starTexSize
    let buf: u8[] = []
    let cx = (size as f64) / 2.0
    let cy = (size as f64) / 2.0
    let rOuter = cx - 1.5
    let rInner = rOuter * 0.42
    let twoPi: f64 = 2.0 * math.pi
    let y: i32 = 0
    while y < size {
        let x: i32 = 0
        while x < size {
            let dx = (x as f64) - cx + 0.5
            let dy = (y as f64) - cy + 0.5
            let d = math.sqrt(dx * dx + dy * dy)
            // Angle of this pixel from centre, normalised so 5 wedges
            // cover the circle. Within each wedge, distance from
            // centre is bounded by a linear interpolation between
            // rOuter and rInner depending on local angle.
            let theta = math.atan2(dy, dx)
            // Shift so a point sits at the top.
            let t = theta + math.pi / 2.0
            let wedge = t / (twoPi / 10.0)
            let frac = wedge - (math.floor(wedge / 2.0) * 2.0)
            // Triangular wave 0→1→0 across two wedges.
            let tri: f64 = if frac < 1.0 { frac } else { 2.0 - frac }
            let rLocal = rInner + (rOuter - rInner) * tri
            if d <= rLocal {
                let alpha = 1.0 - d / rLocal
                let a = (alpha * 255.0) as u8
                buf.push(255 as u8)
                buf.push(255 as u8)
                buf.push(255 as u8)
                buf.push(a)
            } else {
                buf.push(0 as u8)
                buf.push(0 as u8)
                buf.push(0 as u8)
                buf.push(0 as u8)
            }
            x += 1
        }
        y += 1
    }
    buf
}

// 64×64 soft white radial gradient (alpha falls smoothly from
// centre to edge). Tinted at draw time and stretched to whatever
// size is needed for a blurry bloom — used for the brick wave
// highlight so the glow doesn't look rectangular.
pub fn makeGlowSprite(): u8[] {
    let size: i32 = consts.glowTexSize
    let buf: u8[] = []
    let cx = (size as f64) / 2.0
    let cy = (size as f64) / 2.0
    let radius = cx
    let y: i32 = 0
    while y < size {
        let x: i32 = 0
        while x < size {
            let dx = (x as f64) - cx + 0.5
            let dy = (y as f64) - cy + 0.5
            let d = math.sqrt(dx * dx + dy * dy)
            // Smooth falloff: 1 at centre, 0 at and past `radius`.
            let t: f64 = if d >= radius { 0.0 } else { 1.0 - d / radius }
            // Gaussian-ish exponent for a softer bell.
            let aNorm = t * t
            let a = (aNorm * 255.0) as u8
            buf.push(255 as u8)
            buf.push(255 as u8)
            buf.push(255 as u8)
            buf.push(a)
            x += 1
        }
        y += 1
    }
    buf
}

// Pre-render a full-window RGBA black overlay where the alpha
// encodes how much the underlying scene should be darkened at
// each pixel. Combines two CRT-style effects:
//   • scanlines — odd rows have a small fixed darken
//   • vignette  — soft radial darken toward the corners
// Drawn over everything (including HUD) at draw-time. Built once
// at startup so the per-frame cost is just one drawTexture call.
pub fn makeCrtOverlay(w: i32, h: i32): u8[] {
    let buf: u8[] = []
    let cx = (w as f64) / 2.0
    let cy = (h as f64) / 2.0
    let maxD = math.sqrt(cx * cx + cy * cy)
    let y: i32 = 0
    while y < h {
        // 1.0 = pixel left untouched, lower = darken applied.
        let scan: f64 = if y % 2 == 1 { 0.82 } else { 1.0 }
        let x: i32 = 0
        while x < w {
            let dx = (x as f64) - cx
            let dy = (y as f64) - cy
            let d = math.sqrt(dx * dx + dy * dy)
            let vNorm = d / maxD
            // Vignette: 1.0 in centre, ~0.5 at corners.
            let vig = 1.0 - vNorm * vNorm * 0.55
            let combined = scan * vig
            let darken: f64 = if combined < 0.0 { 1.0 } else { 1.0 - combined }
            let alpha = (darken * 255.0) as u8
            buf.push(0 as u8)
            buf.push(0 as u8)
            buf.push(0 as u8)
            buf.push(alpha)
            x += 1
        }
        y += 1
    }
    buf
}
