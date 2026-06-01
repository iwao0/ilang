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
///   core
///   instance      → core
///   adapter       → instance
///   device        → adapter
///   buffer        → core, device
///   texture       → core, device, buffer
///   surface       → core, instance, adapter, device, texture
///   shader        → core, device
///   bind_group    → core, device, buffer, texture
///   pipeline      → core, device, shader, bind_group
///   command       → core, device, buffer, texture, pipeline, bind_group
///   queue         → core, device, buffer, texture, command
///   render_bundle → core, device, buffer, bind_group, pipeline, command
///
/// The `adapter → surface` reverse edge (RequestAdapterOptions'
/// `compatibleSurface`) is broken by typing that field as `*void`
/// in `adapter.il`; cast `surface.handle as *void` at the call
/// site.

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
