// d3d12 — Create an ID3D12CommandQueue through the COM vtable.
//
// Multi-arg COM method that yields another COM object. Vtable
// offsets come from `ID3D12DeviceVtbl` / `IUnknownVtbl` enums.

use test
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    GUID,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12CommandListType, D3D12CommandQueueFlags, D3D12CommandQueuePriority,
    IID_ID3D12Device, IID_ID3D12CommandQueue,
    ID3D12DeviceVtbl, IUnknownVtbl,
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
        if rc != 0 {
            return
        }

        let vtbl = readU64(device as *const void, 0 as i64)

        // ID3D12Device::CreateCommandQueue.
        let createQueuePtr = readU64(
            vtbl as i64 as *const void,
            ID3D12DeviceVtbl.CreateCommandQueue as i64
        )
        let createCommandQueue =
            createQueuePtr as i64 as *void
            as fn(i64, *const D3D12_COMMAND_QUEUE_DESC, *const GUID, **void): i32

        let desc = D3D12_COMMAND_QUEUE_DESC {
            Type:     D3D12CommandListType.direct as i32,
            Priority: D3D12CommandQueuePriority.normal as i32,
            Flags:    D3D12CommandQueueFlags.none as u32,
            NodeMask: 0,
        }

        let queue: *void = 0 as *void
        let hr = createCommandQueue(
            device as i64,
            desc,
            IID_ID3D12CommandQueue,
            &queue
        )
        test.expect(hr as i64, 0)
        test.expectTrue((queue as i64) != 0)

        // Release the queue then the device — IUnknown::Release on each.
        let queueVtbl = readU64(queue as *const void, 0 as i64)
        let queueReleasePtr = readU64(
            queueVtbl as i64 as *const void,
            IUnknownVtbl.Release as i64
        )
        let queueRelease = queueReleasePtr as i64 as *void as fn(i64): u32
        test.expect(queueRelease(queue as i64) as i64, 0)

        let releasePtr = readU64(
            vtbl as i64 as *const void,
            IUnknownVtbl.Release as i64
        )
        let release = releasePtr as i64 as *void as fn(i64): u32
        let _ = release(device as i64)
    }
}

run()
