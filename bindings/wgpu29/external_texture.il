/// `WGPUExternalTexture` — 2D sampleable texture imported from
/// an outside source (camera frame, video decoder, GPU surface
/// from another API). Creation is intentionally undefined by
/// the WebGPU spec; wgpu-native exposes no `Create*` entry
/// point in this header. The handle exists so user code can
/// hold one returned from a future extension API and run the
/// standard refcount / label ops.
///
/// Use sites: chains into a `WGPUBindGroupEntry` via
/// `WGPUExternalTextureBindingEntry { externalTexture }` (struct
/// not bound here — caller can construct it inline as needed).

use core { WGPUStringView }

@extern(C, "wgpu_native") {
    @handle pub struct WGPUExternalTexture {}

    @lib pub fn wgpuExternalTextureSetLabel(
        externalTexture: WGPUExternalTexture, label: WGPUStringView
    )
    @lib pub fn wgpuExternalTextureAddRef(externalTexture: WGPUExternalTexture)
    @lib pub fn wgpuExternalTextureRelease(externalTexture: WGPUExternalTexture)
}
