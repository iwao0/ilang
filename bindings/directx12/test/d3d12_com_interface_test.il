/// d3d12 — COM dispatch via `@com interface`.
///
/// Builds on the `@com interface IUnknown` / `: IUnknown` chain
/// in `bindings/directx12/d3d12_com.il`. The runtime path is
/// the same `(*recv)[slot](recv, args...)` indirection as the
/// struct fixture, but the call site reads as `device.Method(args)`
/// without manual offset / cast boilerplate.

use std.test as test// HANDLE is referenced deep in the binding (ID3D12Fence::SetEventOnCompletion);
// ilang's import resolver wants it in scope here too.
use windows { HANDLE }
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12CommandListType, D3D12CommandQueueFlags, D3D12CommandQueuePriority,
    IID_ID3D12Device,
    IID_ID3D12CommandQueue,
    ID3D12Device
}
use ole32 { IUnknown }

@extern(C) {
    pub fn run() {
        let raw: *void = 0 as *void
        let rc = D3D12CreateDevice(
            0 as *void,
            D3DFeatureLevel.level_11_0 as u32,
            &IID_ID3D12Device,
            &raw
        )
        if rc != 0 {
            return
        }
        test.expectTrue((raw as i64) != 0)

        let device = raw as ID3D12Device
        // Inherited from ID3D12Object → IUnknown → ID3D12Device:
        // GetNodeCount sits 7 slots into the device vtable
        // (3 IUnknown + 4 ID3D12Object).
        let nodes = device.GetNodeCount()
        test.expectTrue(nodes >= 1 as u32)

        // CreateCommandQueue — full COM call with structured args.
        let queueDesc = D3D12_COMMAND_QUEUE_DESC {
            Type:     D3D12CommandListType.direct as i32,
            Priority: D3D12CommandQueuePriority.normal as i32,
            Flags:    D3D12CommandQueueFlags.none as u32,
            NodeMask: 0,
        }
        let queueRaw: *void = 0 as *void
        let qhr = device.CreateCommandQueue(&queueDesc, &IID_ID3D12CommandQueue, &queueRaw)
        test.expect(qhr as i64, 0)
        test.expectTrue((queueRaw as i64) != 0)

        // Release through the inherited IUnknown vtable. The queue
        // is typed as the base interface; slot 2 = Release.
        let queue = queueRaw as IUnknown
        test.expect(queue.Release() as i64, 0)
        test.expect(device.Release() as i64, 0)
    }
}

run()
