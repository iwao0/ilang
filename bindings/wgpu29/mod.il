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
/// File layout per feature:
///   <feature>.il         — raw C-ABI declarations only
///                          (`@handle`, structs, `@lib pub fn`,
///                          enums). No friendly wrapper classes.
///   <feature>_class.il   — the friendly wrapper class(es) that
///                          wrap a handle from `<feature>.il`.
///                          Hosts the synchronous callback pump
///                          helpers as `pub class` methods so
///                          they can take the friendly `Instance`
///                          / `Adapter` / `Buffer` / `Queue`
///                          types instead of raw handles.
///
/// Class-level DAG (importer → importee), acyclic:
///   instance_class       (Instance) — base
///   shader_class         (Shader)
///   buffer_class         (Buffer, BufferOptions) → instance_class
///   texture_class        (Texture, TextureView, TextureOptions)
///   query_class          (QuerySet, QuerySetOptions)
///   bind_group_class     → buffer_class, texture_class
///   pipeline_class       → shader_class
///   command_class        → buffer_class, texture_class,
///                          pipeline_class, bind_group_class
///   render_bundle_class  → buffer_class, bind_group_class,
///                          pipeline_class
///   queue_class          → command_class, instance_class
///   device_class         → shader_class, pipeline_class,
///                          buffer_class, texture_class,
///                          bind_group_class, command_class,
///                          query_class
///   adapter_class        → device_class, instance_class
///   surface_class        → device_class, adapter_class,
///                          texture_class
///
/// `Instance.requestAdapter` lives in `adapter_class.il` as a
/// top-level free function `requestAdapter(inst, ...)` — NOT a
/// method on `Instance` — because hosting it on `Instance` would
/// close the DAG into the cycle
///   instance_class → adapter_class → device_class
///                  → buffer_class → instance_class.
///
/// `requestAdapter` keeps a raw `surfaceHandle: i64` (not
/// `Surface`) so that `adapter_class.il` doesn't need to import
/// `surface_class.il`; `surface_class.il` imports `adapter_class`
/// for its `Surface.getCapabilities(adapter: Adapter)` method.

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

pub use instance_class.*
pub use adapter_class.*
pub use device_class.*
pub use buffer_class.*
pub use texture_class.*
pub use surface_class.*
pub use shader_class.*
pub use bind_group_class.*
pub use pipeline_class.*
pub use command_class.*
pub use queue_class.*
pub use render_bundle_class.*
pub use query_class.*
