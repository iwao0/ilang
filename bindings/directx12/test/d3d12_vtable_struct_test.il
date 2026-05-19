// d3d12 — COM vtable dispatch through `pub struct` (no offset enums).
//
// Verifies that fn-typed fields inside an `@extern(C) pub struct`
// can be loaded from a `*T` raw pointer and called directly,
// replacing the older `readU64(vtbl, OFFSET) as ... as fn(...)`
// dance. Vtable structs live in `bindings/directx12/d3d12_vtable_structs.il`.

use test
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12CommandListType, D3D12CommandQueueFlags, D3D12CommandQueuePriority,
    IID_ID3D12Device,
    IID_ID3D12CommandQueue,
    VtIUnknown, VtID3D12Device,
}

@extern(C) {
    pub fn run() {
        let device: *void = 0 as *void
        let rc = D3D12CreateDevice(
            0 as *void,
            D3DFeatureLevel.level_11_0 as u32,
            IID_ID3D12Device,
            &device
        )
        // No D3D12 GPU on this host → exit clean.
        if rc != 0 {
            return
        }
        test.expectTrue((device as i64) != 0)

        let devVtbl = readU64(device as *const void, 0 as i64)
            as i64 as *VtID3D12Device

        // GetNodeCount returns the active node count for the device.
        // On a single-GPU box it's 1.
        let nodes = devVtbl.GetNodeCount(device as i64)
        test.expectTrue(nodes >= 1 as u32)

        // CreateCommandQueue — full COM call through the struct.
        let queueDesc = D3D12_COMMAND_QUEUE_DESC {
            Type:     D3D12CommandListType.direct as i32,
            Priority: D3D12CommandQueuePriority.normal as i32,
            Flags:    D3D12CommandQueueFlags.none as u32,
            NodeMask: 0,
        }
        let queue: *void = 0 as *void
        let qhr = devVtbl.CreateCommandQueue(
            device as i64,
            queueDesc,
            IID_ID3D12CommandQueue,
            &queue
        )
        test.expect(qhr as i64, 0)
        test.expectTrue((queue as i64) != 0)

        // Release the queue via its own IUnknown vtable.
        let qVtbl = readU64(queue as *const void, 0 as i64)
            as i64 as *VtIUnknown
        let qrc = qVtbl.Release(queue as i64)
        test.expect(qrc as i64, 0)

        // Release the device.
        let drc = devVtbl.Release(device as i64)
        test.expect(drc as i64, 0)
    }
}

run()
