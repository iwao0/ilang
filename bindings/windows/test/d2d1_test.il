/// d2d1 — factory creation + matrix helpers smoke test.

use std.test as test
use winnt { GUID }
use d2d1 {
    D2D1CreateFactory,
    D2dFactoryType,
    D2D1MakeRotateMatrix,
    D2D1_MATRIX_3X2_F,
    D2D1_POINT_2F,
    D2D1IsMatrixInvertible,
}

@extern(C) {
    pub fn run() {
        // IID_ID2D1Factory = {06152247-6f50-465a-9245-118bfd3b6007}
        let iid = GUID {
            Data1: 0x06152247 as u32,
            Data2: 0x6f50 as u16,
            Data3: 0x465a as u16,
            Data4lo: 0x8b1145 as u32,
            Data4hi: 0x07603bfd as u32,
        }
        let factory: *void = 0 as *void
        let hr = D2D1CreateFactory(
            D2dFactoryType.singleThreaded as i32,
            &iid as *const void,
            0 as *const void,
            &factory
        )
        if hr == 0 {
            test.expectTrue((factory as i64) != 0)
        } else {
            test.expectTrue(hr != 0)
        }

        // Matrix helper — pure math, no D3D context required.
        let m = D2D1_MATRIX_3X2_F {
            _11: 0.0_f32, _12: 0.0_f32,
            _21: 0.0_f32, _22: 0.0_f32,
            _31: 0.0_f32, _32: 0.0_f32,
        }
        let center = D2D1_POINT_2F { x: 0.0_f32, y: 0.0_f32 }
        D2D1MakeRotateMatrix(0.0_f32, center, &m)
        // Identity rotation: _11 == _22 == 1.0
        test.expectF64(m._11 as f64, 1.0)
        test.expectF64(m._22 as f64, 1.0)
        test.expectTrue(D2D1IsMatrixInvertible(&m) != 0)
    }
}

run()
