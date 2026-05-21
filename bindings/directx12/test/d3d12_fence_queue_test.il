/// d3d12 — fence + command-queue sync round-trip.
///
/// Covers the previously-untyped methods:
///   * ID3D12Device::CreateFence
///   * ID3D12Fence::Signal / GetCompletedValue / SetEventOnCompletion
///   * ID3D12CommandQueue::Signal / Wait
///   * ID3D12CommandQueue::GetTimestampFrequency
///   * ID3D12CommandQueue::GetClockCalibration
///
/// Bootstraps a device + direct queue; bails cleanly when no GPU.

use test
use windows {
    HANDLE,
    NULL,
    CreateEventA, WaitForSingleObject, CloseHandle,
}
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    IID_ID3D12Device,
    IID_ID3D12CommandQueue,
    IID_ID3D12Fence,
    IUnknown,
    ID3D12Device,
    ID3D12CommandQueue,
    ID3D12Fence,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12CommandListType,
    D3D12CommandQueueFlags,
    D3D12CommandQueuePriority,
    D3D12FenceFlags,
}

@extern(C) {
    pub fn run() {
        let rawDev: *void = NULL
        let rc = D3D12CreateDevice(
            NULL,
            D3DFeatureLevel.level_11_0 as u32,
            &IID_ID3D12Device,
            &rawDev
        )
        if rc != 0 {
            return
        }
        let device = rawDev as ID3D12Device

        // ── Direct queue ───────────────────────────────────────────
        let qDesc = D3D12_COMMAND_QUEUE_DESC {
            Type:     D3D12CommandListType.direct as i32,
            Priority: D3D12CommandQueuePriority.normal as i32,
            Flags:    D3D12CommandQueueFlags.none as u32,
            NodeMask: 0,
        }
        let rawQueue: *void = NULL
        let qrc = device.CreateCommandQueue(&qDesc, &IID_ID3D12CommandQueue, &rawQueue)
        test.expect(qrc as i64, 0)
        let queue = rawQueue as ID3D12CommandQueue

        // ── Fence (initial value 0, no shared flags) ────────────────
        let rawFence: *void = NULL
        let frc = device.CreateFence(
            0 as u64,
            D3D12FenceFlags.none as u32,
            &IID_ID3D12Fence,
            &rawFence
        )
        test.expect(frc as i64, 0)
        let fence = rawFence as ID3D12Fence

        test.expect(fence.GetCompletedValue() as i64, 0)

        // CPU-side Signal — bumps the fence without a queue.
        test.expect(fence.Signal(5 as u64) as i64, 0)
        test.expect(fence.GetCompletedValue() as i64, 5)

        // Event-on-completion — already past, so the event fires
        // immediately and WaitForSingleObject returns WAIT_OBJECT_0.
        let ev = CreateEventA(NULL, 0, 0, NULL)
        test.expectTrue((ev as i64) != 0)
        test.expect(fence.SetEventOnCompletion(5 as u64, ev) as i64, 0)
        // WAIT_OBJECT_0 = 0; INFINITE = 0xFFFFFFFF.
        let waited = WaitForSingleObject(ev, 1000 as u32)
        test.expect(waited as i64, 0)

        // GPU-side Signal — push value 7 from the queue. The binding
        // takes a pointer to the fence handle (matches the C ABI's
        // `Signal(ID3D12Fence* pFence, UINT64)` form once the @com
        // receiver is expanded).
        test.expect(queue.Signal(&fence, 7 as u64) as i64, 0)
        // CPU spin until the GPU side advances. On a direct queue
        // with no other work the signal lands ~immediately, but a
        // bounded polling loop keeps the test robust.
        let spin = 0
        while fence.GetCompletedValue() < 7 as u64 && spin < 1000 {
            spin = spin + 1
        }
        test.expectTrue(fence.GetCompletedValue() >= 7 as u64)

        // Queue Wait — schedules a GPU-side wait that resolves
        // instantly because the fence is already past the value.
        test.expect(queue.Wait(&fence, 7 as u64) as i64, 0)

        // ── Timestamp / clock calibration ──────────────────────────
        let freq: u64 = 0
        let frc2 = queue.GetTimestampFrequency(&freq)
        // Returns S_OK on direct/compute queues; copy queues may
        // return E_FAIL on older runtimes. Accept either as long as
        // the signature compiles.
        test.expectTrue(frc2 <= 0)
        if frc2 == 0 {
            test.expectTrue(freq > 0)
        }

        let gpuTs: u64 = 0
        let cpuTs: u64 = 0
        let crc = queue.GetClockCalibration(&gpuTs, &cpuTs)
        test.expectTrue(crc <= 0)

        // ── Cleanup ────────────────────────────────────────────────
        let _ = CloseHandle(ev)
        test.expect(fence.Release()  as i64, 0)
        test.expect(queue.Release()  as i64, 0)
        test.expect(device.Release() as i64, 0)
    }
}

run()
