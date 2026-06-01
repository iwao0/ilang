/// `WGPUInstance` + logging + `wgpuCreateInstance` /
/// `wgpuInstanceProcessEvents`. The instance is the root object
/// every other wgpu resource hangs off; create exactly one per
/// process and reuse it across surfaces / devices.

@extern(C, "wgpu_native") {
    @handle pub struct WGPUInstance {}

    // ─── logging (wgpu.h native extension) ────────────────────
    // WGPULogLevel: Off=0 Error=1 Warn=2 Info=3 Debug=4 Trace=5.
    // callback: void(WGPULogLevel, WGPUStringView message, void*)
    // → (level i32, msg.data i64, msg.len i64, userdata i64).
    @lib pub fn wgpuSetLogLevel(level: u32)
    @lib pub fn wgpuSetLogCallback(
        callback: fn(i32, i64, i64, i64), userdata: *void
    )

    // `descriptor` is nullable — pass `0 as *void` for the
    // default `WGPUInstanceDescriptor`.
    @lib pub fn wgpuCreateInstance(descriptor: *void): WGPUInstance

    // Drive any pending callbacks registered via
    // `AllowProcessEvents` mode. Call between request* and
    // reading the callback's output cell.
    @lib pub fn wgpuInstanceProcessEvents(instance: WGPUInstance)
    @lib pub fn wgpuInstanceAddRef(instance: WGPUInstance)
    @lib pub fn wgpuInstanceRelease(instance: WGPUInstance)
}
