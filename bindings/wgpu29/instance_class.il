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
use instance {
    wgpuInstanceRelease, wgpuCreateInstance,
    wgpuSetLogLevel, wgpuSetLogCallback, LogLevel,
}

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

    /// Construct a default `Instance`. Equivalent to
    /// `new Instance(wgpuCreateInstance(0 as *void))` but lets the
    /// caller stay out of `@extern(C)`. ilang doesn't yet allow
    /// static methods on classes, so this is a free function.
    pub fn createInstance(): Instance {
        new Instance(wgpuCreateInstance(0 as *void))
    }

    /// Set the global wgpu log level. Friendly wrapper over
    /// `wgpuSetLogLevel` that accepts the `LogLevel` enum and
    /// performs the `u32` cast itself.
    pub fn setLogLevel(level: LogLevel) {
        wgpuSetLogLevel(level as u32)
    }

    /// Register a global log callback. Friendly wrapper over
    /// `wgpuSetLogCallback`: takes the same `fn(level, msgData,
    /// msgLen, ud)` signature wgpu expects and forwards the C
    /// function pointer + userdata as-is.
    pub fn setLogCallback(callback: fn(i32, i64, i64, i64), userdata: i64) {
        wgpuSetLogCallback(callback, userdata as *void)
    }
}
