// Fixed-length local array whose element type is a top-level
// @extern(C) struct. Elements are populated with struct literals
// inside a bare array literal and read back via indexed field
// access. The storage must be the embedded CRepr layout (no ARC
// header, no per-element heap box) — otherwise downstream code
// that passes the array as a base pointer into a C-side `void*`
// would see header bytes instead of the first element.
use test

@extern(C) {
    struct Vertex {
        pos:   f32[3]
        color: f32[4]
    }
}

let vertices: Vertex[3] = [
    Vertex { pos: [ 0.0_f32,   0.25_f32, 0.0_f32], color: [1.0_f32, 0.0_f32, 0.0_f32, 1.0_f32] },
    Vertex { pos: [ 0.25_f32, -0.25_f32, 0.0_f32], color: [0.0_f32, 1.0_f32, 0.0_f32, 1.0_f32] },
    Vertex { pos: [-0.25_f32, -0.25_f32, 0.0_f32], color: [0.0_f32, 0.0_f32, 1.0_f32, 1.0_f32] },
]

// v0 — top, red
test.expectF64(vertices[0].pos[0]   as f64,  0.0)
test.expectF64(vertices[0].pos[1]   as f64,  0.25)
test.expectF64(vertices[0].color[0] as f64,  1.0)
test.expectF64(vertices[0].color[3] as f64,  1.0)
// v1 — bottom right, green
test.expectF64(vertices[1].pos[0]   as f64,  0.25)
test.expectF64(vertices[1].pos[1]   as f64, -0.25)
test.expectF64(vertices[1].color[1] as f64,  1.0)
// v2 — bottom left, blue
test.expectF64(vertices[2].pos[0]   as f64, -0.25)
test.expectF64(vertices[2].color[2] as f64,  1.0)
test.expectF64(vertices[2].color[3] as f64,  1.0)
