/// d3d11 — WARP device creation smoke test.

use test
use d3d11 { D3D11CreateDevice, D3dDriverType, D3dFeatureLevel, D3d11CreateDeviceFlags }

@extern(C) {
    pub fn run() {
        // Target Feature Level 11.0.
        let levels: u32[] = [D3dFeatureLevel.level_11_0 as u32]
        let device: *void = 0 as *void
        let context: *void = 0 as *void
        let chosen: u32 = 0

        // WARP (software) driver always succeeds even without a GPU.
        let hr = D3D11CreateDevice(
            0 as *void,
            D3dDriverType.warp as i32,
            0 as i64,
            D3d11CreateDeviceFlags.none as u32,
            levels,
            1 as u32,
            7 as u32,                // D3D11_SDK_VERSION
            &device,
            &chosen,
            &context
        )
        test.expect(hr as i64, 0 as i64)
        test.expectTrue((device as i64) != 0)
        test.expectTrue((context as i64) != 0)
        test.expect(chosen as i64, D3dFeatureLevel.level_11_0 as i64)
    }
}

run()
