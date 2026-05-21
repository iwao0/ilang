// Pass a fixed-length local array of an @extern(C) struct as the
// source pointer to libc memcpy. The decay must yield a base
// pointer over the inline element data (no ARC header offset), so
// the destination ends up with the exact f32 byte sequence the
// elements were initialised with. This is the shape used by the
// dx12_triangle vertex-buffer upload after the writeF32 loop is
// replaced with a single memcpy.
use test
use libc { memcpy }

@extern(C) {
    struct Vertex {
        pos:   f32[3]
        color: f32[4]
    }

    pub fn run() {
        let vertices: Vertex[3] = [
            Vertex { pos: [ 0.0_f32,   0.25_f32, 0.0_f32], color: [1.0_f32, 0.0_f32, 0.0_f32, 1.0_f32] },
            Vertex { pos: [ 0.25_f32, -0.25_f32, 0.0_f32], color: [0.0_f32, 1.0_f32, 0.0_f32, 1.0_f32] },
            Vertex { pos: [-0.25_f32, -0.25_f32, 0.0_f32], color: [0.0_f32, 0.0_f32, 1.0_f32, 1.0_f32] },
        ]

        // 3 vertices * (3 + 4) f32 = 21 f32 = 84 bytes.
        let dst: f32[] = [
            0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32,
            0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32,
            0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32,
        ]
        let _ = memcpy(dst, vertices, 84 as u64)

        // v0 — top, red
        test.expectF64(dst[0]  as f64,  0.0)
        test.expectF64(dst[1]  as f64,  0.25)
        test.expectF64(dst[2]  as f64,  0.0)
        test.expectF64(dst[3]  as f64,  1.0)
        test.expectF64(dst[4]  as f64,  0.0)
        test.expectF64(dst[5]  as f64,  0.0)
        test.expectF64(dst[6]  as f64,  1.0)
        // v1 — bottom right, green
        test.expectF64(dst[7]  as f64,  0.25)
        test.expectF64(dst[8]  as f64, -0.25)
        test.expectF64(dst[9]  as f64,  0.0)
        test.expectF64(dst[10] as f64,  0.0)
        test.expectF64(dst[11] as f64,  1.0)
        test.expectF64(dst[12] as f64,  0.0)
        test.expectF64(dst[13] as f64,  1.0)
        // v2 — bottom left, blue
        test.expectF64(dst[14] as f64, -0.25)
        test.expectF64(dst[15] as f64, -0.25)
        test.expectF64(dst[16] as f64,  0.0)
        test.expectF64(dst[17] as f64,  0.0)
        test.expectF64(dst[18] as f64,  0.0)
        test.expectF64(dst[19] as f64,  1.0)
        test.expectF64(dst[20] as f64,  1.0)
    }
}

run()
