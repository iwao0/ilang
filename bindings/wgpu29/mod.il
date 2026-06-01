/// wgpu-native (WebGPU) bindings — umbrella module.
///
/// Pinned to wgpu-native v29.0.0.0 (webgpu.h + wgpu.h extension).
/// One feature area per file; this umbrella flattens them all
/// into the `wgpu` namespace so user code can write
/// `use wgpu { Device, RenderPass, TextureFormat, ... }`
/// without naming the individual files.
///
/// Library: bare name "wgpu_native" → `libwgpu_native.dylib` /
/// `.so` / `.dll`. Point the loader at it via
/// `DYLD_LIBRARY_PATH` (macOS), `LD_LIBRARY_PATH` (Linux), or
/// `PATH` (Windows) when running the JIT.
///
/// Dependency order between siblings (importer → imported):
///   core             (holds WGPUInstance / WGPUAdapter /
///                     WGPUDevice / WGPUQueue handles + the
///                     adapter-info types so per-feature files
///                     don't form cycles via friendly methods)
///   external_texture → core
///   device           → core, shader, pipeline, buffer, texture,
///                       bind_group, command, query
///   adapter          → core, device         (requestDevice)
///   instance         → core, adapter        (requestAdapter)
///   buffer           → core
///   texture          → core, buffer
///   surface          → core, adapter, texture, device
///   shader           → core
///   bind_group       → core, buffer, texture
///   pipeline         → core, shader, bind_group
///   command          → core, buffer, texture, pipeline, bind_group
///   queue            → core, buffer, texture, command
///   render_bundle    → core, buffer, bind_group, pipeline, command
///   query            → core, buffer, command
///
/// The `adapter → surface` reverse edge (RequestAdapterOptions'
/// `compatibleSurface`) is broken by typing that field as `*void`
/// in `adapter.il`; cast `surface.handle as *void` at the call
/// site. The `buffer / queue → instance` edge for
/// `wgpuInstanceProcessEvents` is broken by hoisting that decl
/// (and `WGPUInstance`) into `core.il`.

pub use core.*
pub use instance.*
pub use adapter.*
pub use device.*
pub use buffer.*
pub use texture.*
pub use surface.*
pub use shader.*
pub use bind_group.*
pub use pipeline.*
pub use command.*
pub use queue.*
pub use render_bundle.*
pub use query.*
pub use external_texture.*
