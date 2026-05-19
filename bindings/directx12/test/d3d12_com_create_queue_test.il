// d3d12 — Create an ID3D12CommandQueue through the COM vtable.
//
// Goes one step further than `d3d12_com_method_test`: instead of a
// no-arg method, calls a multi-arg COM method that produces another
// COM object. End-to-end this is the same pattern any real D3D12 app
// would use (queue / heap / fence / list creation).
//
// ID3D12Device::CreateCommandQueue at vtable offset +64:
//   HRESULT (this, const D3D12_COMMAND_QUEUE_DESC*, REFIID, void**)

use test
use directx12 {
    D3D12CreateDevice,
    D3DFeatureLevel,
    GUID,
    D3D12_COMMAND_QUEUE_DESC,
    D3D12CommandListType, D3D12CommandQueueFlags, D3D12CommandQueuePriority,
}

@extern(C) {
    pub fn run() {
        let deviceIid = GUID {
            Data1:   0x189819f1,
            Data2:   0x1db6,
            Data3:   0x4b57,
            Data4lo: 0x211854be,
            Data4hi: 0xf7859b33,
        }
        let device: *void = 0 as *void
        let rc = D3D12CreateDevice(
            0 as *void,
            D3DFeatureLevel.level_11_0 as u32,
            deviceIid,
            &device
        )
        if rc != 0 {
            return
        }

        let vtbl = readU64(device as *const void, 0 as i64)

        // CreateCommandQueue at vtable +64.
        let createQueuePtr = readU64(vtbl as i64 as *const void, 64 as i64)
        let createCommandQueue =
            createQueuePtr as i64 as *void
            as fn(i64, *const D3D12_COMMAND_QUEUE_DESC, *const GUID, **void): i32

        // IID_ID3D12CommandQueue = {0ec870a6-5d7e-4c22-8cfc-5baae07616ed}
        // Data4 bytes: 8c fc 5b aa e0 76 16 ed
        //   lo = 0x8c | 0xfc<<8 | 0x5b<<16 | 0xaa<<24 = 0xaa5bfc8c
        //   hi = 0xe0 | 0x76<<8 | 0x16<<16 | 0xed<<24 = 0xed1676e0
        let queueIid = GUID {
            Data1:   0x0ec870a6,
            Data2:   0x5d7e,
            Data3:   0x4c22,
            Data4lo: 0xaa5bfc8c,
            Data4hi: 0xed1676e0,
        }

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
            queueIid,
            &queue
        )
        test.expect(hr as i64, 0)
        test.expectTrue((queue as i64) != 0)

        // Release the queue and the device via vtable.
        let queueVtbl = readU64(queue as *const void, 0 as i64)
        let queueReleasePtr = readU64(queueVtbl as i64 as *const void, 16 as i64)
        let queueRelease = queueReleasePtr as i64 as *void as fn(i64): u32
        test.expect(queueRelease(queue as i64) as i64, 0)

        let releasePtr = readU64(vtbl as i64 as *const void, 16 as i64)
        let release = releasePtr as i64 as *void as fn(i64): u32
        let _ = release(device as i64)
    }
}

run()
