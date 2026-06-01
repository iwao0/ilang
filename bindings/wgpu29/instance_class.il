/// Friendly `Instance` wrapper for `WGPUInstance`. Split off
/// from `instance.il` so downstream class files
/// (`buffer_class.il`, `queue_class.il`, `adapter_class.il`)
/// can take `Instance` as a parameter without re-introducing
/// the class-level module cycle that forced the original sync
/// wrappers to accept raw `WGPUInstance` handles.
///
/// `Instance.requestAdapter` does NOT live here — it would
/// force `instance_class.il → adapter_class.il → device_class.il
/// → buffer_class.il → instance_class.il` (the same cycle the
/// split is supposed to eliminate, just one level up).
/// Use the top-level `requestAdapter(inst, ...)` in
/// `adapter_class.il` instead.

use core { WGPUInstance, wgpuInstanceProcessEvents }
use instance { wgpuInstanceRelease }

@extern(C, "wgpu_native") {
    /// Wraps `WGPUInstance` — the root object every other wgpu
    /// resource hangs off. Build via
    /// `wgpuCreateInstance(0 as *void)` and reuse one per process.
    pub class Instance {
        pub handle: i64

        pub init(handle: WGPUInstance) {
            this.handle = handle as i64
        }
        pub release() {
            if this.handle != 0 {
                wgpuInstanceRelease(this.handle as WGPUInstance)
                this.handle = 0
            }
        }
        /// Drive any pending callbacks registered via
        /// `AllowProcessEvents` mode. Call between request* and
        /// reading the callback's output cell.
        pub processEvents() {
            wgpuInstanceProcessEvents(this.handle as WGPUInstance)
        }
    }
}
