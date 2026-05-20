/// SDK-published GUIDs for D3D12 COM interfaces.
///
/// Wraps the standard IIDs so user code never has to retype the
/// {189819f1-…} octets. Mirrors the C SDK convention of shipping
/// pre-defined `IID_*` symbols in `dxguid.lib`.
///
/// IID layout helper:
///   `xxxxxxxx-aaaa-bbbb-cccc-dddddddddddd`
///   Data1   = 0xxxxxxxxx              (u32)
///   Data2   = 0xaaaa                  (u16)
///   Data3   = 0xbbbb                  (u16)
///   Data4lo = little-endian u32 of the first 4 bytes of `cccc || dddd`
///   Data4hi = little-endian u32 of the last 4 bytes

use d3d12 { GUID }

/// IID_ID3D12Device = 189819f1-1db6-4b57-be54-1821339b85f7
pub const IID_ID3D12Device: GUID = GUID {
    Data1: 0x189819f1, Data2: 0x1db6, Data3: 0x4b57,
    Data4lo: 0x211854be, Data4hi: 0xf7859b33,
}

/// IID_ID3D12CommandQueue = 0ec870a6-5d7e-4c22-8cfc-5baae07616ed
pub const IID_ID3D12CommandQueue: GUID = GUID {
    Data1: 0x0ec870a6, Data2: 0x5d7e, Data3: 0x4c22,
    Data4lo: 0xaa5bfc8c, Data4hi: 0xed1676e0,
}

/// IID_ID3D12CommandAllocator = 6102dee4-af59-4b09-b999-b44d73f09b24
pub const IID_ID3D12CommandAllocator: GUID = GUID {
    Data1: 0x6102dee4, Data2: 0xaf59, Data3: 0x4b09,
    Data4lo: 0x4db499b9, Data4hi: 0x249bf073,
}

/// IID_ID3D12GraphicsCommandList = 5b160d0f-ac1b-4185-8ba8-b3ae42a5a455
pub const IID_ID3D12GraphicsCommandList: GUID = GUID {
    Data1: 0x5b160d0f, Data2: 0xac1b, Data3: 0x4185,
    Data4lo: 0xaeb3a88b, Data4hi: 0x55a4a542,
}

/// IID_ID3D12Debug = 344488b7-6846-474b-b989-f027448245e0
pub const IID_ID3D12Debug: GUID = GUID {
    Data1: 0x344488b7, Data2: 0x6846, Data3: 0x474b,
    Data4lo: 0x27f089b9, Data4hi: 0xe0458244,
}

/// IID_ID3D12Resource = 696442be-a72e-4059-bc79-5b5c98040fad
pub const IID_ID3D12Resource: GUID = GUID {
    Data1: 0x696442be, Data2: 0xa72e, Data3: 0x4059,
    Data4lo: 0x5c5b79bc, Data4hi: 0xad0f0498,
}

/// IID_ID3D12DescriptorHeap = 8efb471d-616c-4f49-90f7-127bb763fa51
pub const IID_ID3D12DescriptorHeap: GUID = GUID {
    Data1: 0x8efb471d, Data2: 0x616c, Data3: 0x4f49,
    Data4lo: 0x7b12f790, Data4hi: 0x51fa63b7,
}

/// IID_ID3D12RootSignature = c54a6b66-72df-4ee8-8be5-a946a1429214
pub const IID_ID3D12RootSignature: GUID = GUID {
    Data1: 0xc54a6b66, Data2: 0x72df, Data3: 0x4ee8,
    Data4lo: 0x46a9e58b, Data4hi: 0x149242a1,
}

/// IID_ID3D12PipelineState = 765a30f3-f624-4c6f-a828-ace948622445
pub const IID_ID3D12PipelineState: GUID = GUID {
    Data1: 0x765a30f3, Data2: 0xf624, Data3: 0x4c6f,
    Data4lo: 0xe9ac28a8, Data4hi: 0x45246248,
}

/// IID_ID3D12Fence = 0a753dcf-c4d8-4b91-adf6-be5a60d95a76
pub const IID_ID3D12Fence: GUID = GUID {
    Data1: 0x0a753dcf, Data2: 0xc4d8, Data3: 0x4b91,
    Data4lo: 0x5abef6ad, Data4hi: 0x765ad960,
}

/// IID_IDXGIFactory4 = 1bc6ea02-ef36-464f-bf0c-21ca39e5168a
pub const IID_IDXGIFactory4: GUID = GUID {
    Data1: 0x1bc6ea02, Data2: 0xef36, Data3: 0x464f,
    Data4lo: 0xca210cbf, Data4hi: 0x8a16e539,
}

/// IID_IDXGISwapChain3 = 94d99bdb-f1f8-4ab0-b236-7da0170edab1
pub const IID_IDXGISwapChain3: GUID = GUID {
    Data1: 0x94d99bdb, Data2: 0xf1f8, Data3: 0x4ab0,
    Data4lo: 0xa07d36b2, Data4hi: 0xb1da0e17,
}

/// IID_ID3D12RootSignatureDeserializer = 34AB647B-3CC8-46AC-841B-C0965645C046
pub const IID_ID3D12RootSignatureDeserializer: GUID = GUID {
    Data1: 0x34AB647B, Data2: 0x3CC8, Data3: 0x46AC,
    Data4lo: 0x96C01B84, Data4hi: 0x46C04556,
}

/// IID_ID3D12InfoQueue = 0742a90b-c387-483f-b946-30a7e4e61458
pub const IID_ID3D12InfoQueue: GUID = GUID {
    Data1: 0x0742a90b, Data2: 0xc387, Data3: 0x483f,
    Data4lo: 0xa73046b9, Data4hi: 0x5814e6e4,
}

/// IID_ID3D12Heap = 6b3b2502-6e51-45b3-90ee-9884265e8df3
pub const IID_ID3D12Heap: GUID = GUID {
    Data1: 0x6b3b2502, Data2: 0x6e51, Data3: 0x45b3,
    Data4lo: 0x8498ee90, Data4hi: 0xf38d5e26,
}

/// IID_ID3D12QueryHeap = 0d9658ae-ed45-469e-a61d-970ec583cab4
pub const IID_ID3D12QueryHeap: GUID = GUID {
    Data1: 0x0d9658ae, Data2: 0xed45, Data3: 0x469e,
    Data4lo: 0x0e971da6, Data4hi: 0xb4ca83c5,
}

/// IID_ID3D12Fence1 = 433685fe-e22b-4ca0-a8db-b5b4f4dd0e4a
pub const IID_ID3D12Fence1: GUID = GUID {
    Data1: 0x433685fe, Data2: 0xe22b, Data3: 0x4ca0,
    Data4lo: 0xb4b5dba8, Data4hi: 0x4a0eddf4,
}

/// IID_ID3D12Resource1 = 9d5e227a-4430-4161-88b3-3eca6bb16e19
pub const IID_ID3D12Resource1: GUID = GUID {
    Data1: 0x9d5e227a, Data2: 0x4430, Data3: 0x4161,
    Data4lo: 0xca3eb388, Data4hi: 0x196eb16b,
}

/// IID_ID3D12Resource2 = be36ec3b-ea85-4aeb-a45a-e9d76404a495
pub const IID_ID3D12Resource2: GUID = GUID {
    Data1: 0xbe36ec3b, Data2: 0xea85, Data3: 0x4aeb,
    Data4lo: 0xd7e95aa4, Data4hi: 0x95a40464,
}

/// IID_ID3D12Device1 = 77acce80-638e-4e65-8895-c1f23386863e
pub const IID_ID3D12Device1: GUID = GUID {
    Data1: 0x77acce80, Data2: 0x638e, Data3: 0x4e65,
    Data4lo: 0xf2c19588, Data4hi: 0x3e868633,
}

/// IID_ID3D12Device2 = 30baa41e-b15b-475c-a0bb-1af5c5b64328
pub const IID_ID3D12Device2: GUID = GUID {
    Data1: 0x30baa41e, Data2: 0xb15b, Data3: 0x475c,
    Data4lo: 0xf51abba0, Data4hi: 0x2843b6c5,
}

/// IID_ID3D12Device3 = 81dadc15-2bad-4392-93c5-101345c4aa98
pub const IID_ID3D12Device3: GUID = GUID {
    Data1: 0x81dadc15, Data2: 0x2bad, Data3: 0x4392,
    Data4lo: 0x1310c593, Data4hi: 0x98aac445,
}

/// IID_ID3D12Device4 = e865df17-a9ee-46f9-a463-3098315aa2e5
pub const IID_ID3D12Device4: GUID = GUID {
    Data1: 0xe865df17, Data2: 0xa9ee, Data3: 0x46f9,
    Data4lo: 0x983063a4, Data4hi: 0xe5a25a31,
}

/// IID_ID3D12Device5 = 8b4f173b-2fea-4b80-8f58-4307191ab95d
pub const IID_ID3D12Device5: GUID = GUID {
    Data1: 0x8b4f173b, Data2: 0x2fea, Data3: 0x4b80,
    Data4lo: 0x0743588f, Data4hi: 0x5db91a19,
}

/// IID_ID3D12GraphicsCommandList1 = 553103fb-1fe7-4557-bb38-946d7d0e7ca7
pub const IID_ID3D12GraphicsCommandList1: GUID = GUID {
    Data1: 0x553103fb, Data2: 0x1fe7, Data3: 0x4557,
    Data4lo: 0x6d9438bb, Data4hi: 0xa77c0e7d,
}

/// IID_ID3D12GraphicsCommandList2 = 38c3e585-ff17-412c-9150-4fc6f9d72a28
pub const IID_ID3D12GraphicsCommandList2: GUID = GUID {
    Data1: 0x38c3e585, Data2: 0xff17, Data3: 0x412c,
    Data4lo: 0xc64f5091, Data4hi: 0x282ad7f9,
}

/// IID_ID3D12GraphicsCommandList3 = 6fda83a7-b84c-4e38-9ac8-c7bd22016b3d
pub const IID_ID3D12GraphicsCommandList3: GUID = GUID {
    Data1: 0x6fda83a7, Data2: 0xb84c, Data3: 0x4e38,
    Data4lo: 0xbdc7c89a, Data4hi: 0x3d6b0122,
}

/// IID_ID3D12GraphicsCommandList4 = 8754318e-d3a9-4541-98cf-645b50dc4874
pub const IID_ID3D12GraphicsCommandList4: GUID = GUID {
    Data1: 0x8754318e, Data2: 0xd3a9, Data3: 0x4541,
    Data4lo: 0x5b64cf98, Data4hi: 0x7448dc50,
}

/// IID_ID3D12GraphicsCommandList5 = 55050859-4024-474c-87f5-6472eaee44ea
pub const IID_ID3D12GraphicsCommandList5: GUID = GUID {
    Data1: 0x55050859, Data2: 0x4024, Data3: 0x474c,
    Data4lo: 0x7264f587, Data4hi: 0xea44eeea,
}

/// IID_ID3D12GraphicsCommandList6 = c3827890-e548-4cfa-96cf-5689a9370f80
pub const IID_ID3D12GraphicsCommandList6: GUID = GUID {
    Data1: 0xc3827890, Data2: 0xe548, Data3: 0x4cfa,
    Data4lo: 0x8956cf96, Data4hi: 0x800f37a9,
}

/// IID_ID3D12GraphicsCommandList7 = dd171223-8b61-4769-90e3-160ccde4e2c1
pub const IID_ID3D12GraphicsCommandList7: GUID = GUID {
    Data1: 0xdd171223, Data2: 0x8b61, Data3: 0x4769,
    Data4lo: 0x0c16e390, Data4hi: 0xc1e2e4cd,
}

/// IID_ID3D12Debug1 = affaa4ca-63fe-4d8e-b8ad-159000af4304
pub const IID_ID3D12Debug1: GUID = GUID {
    Data1: 0xaffaa4ca, Data2: 0x63fe, Data3: 0x4d8e,
    Data4lo: 0x9015adb8, Data4hi: 0x0443af00,
}

/// IID_ID3D12Debug3 = 5cf4e58f-f671-4ff1-a542-3686e3d153d1
pub const IID_ID3D12Debug3: GUID = GUID {
    Data1: 0x5cf4e58f, Data2: 0xf671, Data3: 0x4ff1,
    Data4lo: 0x863642a5, Data4hi: 0xd153d1e3,
}

/// IID_ID3D12Debug5 = 548d6b12-09fa-40e0-9069-5dcd589a52c9
pub const IID_ID3D12Debug5: GUID = GUID {
    Data1: 0x548d6b12, Data2: 0x09fa, Data3: 0x40e0,
    Data4lo: 0xcd5d6990, Data4hi: 0xc9529a58,
}

/// IID_ID3D12DebugDevice = 3febd6dd-4973-4787-8194-e45f9e28923e
pub const IID_ID3D12DebugDevice: GUID = GUID {
    Data1: 0x3febd6dd, Data2: 0x4973, Data3: 0x4787,
    Data4lo: 0x5fe49481, Data4hi: 0x3e92289e,
}

/// IID_ID3D12StateObject = 47016943-fca8-4594-93ea-af258b55346d
pub const IID_ID3D12StateObject: GUID = GUID {
    Data1: 0x47016943, Data2: 0xfca8, Data3: 0x4594,
    Data4lo: 0x25afea93, Data4hi: 0x6d34558b,
}

/// IID_ID3D12StateObjectProperties = de5fa827-9bf9-4f26-89ff-d7f56fde3860
pub const IID_ID3D12StateObjectProperties: GUID = GUID {
    Data1: 0xde5fa827, Data2: 0x9bf9, Data3: 0x4f26,
    Data4lo: 0xf5d7ff89, Data4hi: 0x6038de6f,
}

/// IID_ID3D12PipelineLibrary = c64226a8-9201-46af-b4cc-53fb9ff7414f
pub const IID_ID3D12PipelineLibrary: GUID = GUID {
    Data1: 0xc64226a8, Data2: 0x9201, Data3: 0x46af,
    Data4lo: 0xfb53ccb4, Data4hi: 0x4f41f79f,
}

/// IID_IDXGIFactory = 7b7166ec-21c7-44ae-b21a-c9ae321ae369
pub const IID_IDXGIFactory: GUID = GUID {
    Data1: 0x7b7166ec, Data2: 0x21c7, Data3: 0x44ae,
    Data4lo: 0xaec91ab2, Data4hi: 0x69e31a32,
}

/// IID_IDXGIFactory1 = 770aae78-f26f-4dba-a829-253c83d1b387
pub const IID_IDXGIFactory1: GUID = GUID {
    Data1: 0x770aae78, Data2: 0xf26f, Data3: 0x4dba,
    Data4lo: 0x3c2529a8, Data4hi: 0x87b3d183,
}

/// IID_IDXGIFactory2 = 50c83a1c-e072-4c48-87b0-3630fa36a6d0
pub const IID_IDXGIFactory2: GUID = GUID {
    Data1: 0x50c83a1c, Data2: 0xe072, Data3: 0x4c48,
    Data4lo: 0x3036b087, Data4hi: 0xd0a636fa,
}

/// IID_IDXGIFactory3 = 25483823-cd46-4c7d-86ca-47aa95b837bd
pub const IID_IDXGIFactory3: GUID = GUID {
    Data1: 0x25483823, Data2: 0xcd46, Data3: 0x4c7d,
    Data4lo: 0xaa47ca86, Data4hi: 0xbd37b895,
}

/// IID_IDXGIFactory5 = 7632e1f5-ee65-4dca-87fd-84cd75f8838d
pub const IID_IDXGIFactory5: GUID = GUID {
    Data1: 0x7632e1f5, Data2: 0xee65, Data3: 0x4dca,
    Data4lo: 0xcd84fd87, Data4hi: 0x8d83f875,
}

/// IID_IDXGIFactory6 = c1b6694f-ff09-44a9-b03c-77900a0a1d17
pub const IID_IDXGIFactory6: GUID = GUID {
    Data1: 0xc1b6694f, Data2: 0xff09, Data3: 0x44a9,
    Data4lo: 0x90773cb0, Data4hi: 0x171d0a0a,
}

/// IID_IDXGIFactory7 = a4966eed-76db-44da-84c1-ee9a7afb20a8
pub const IID_IDXGIFactory7: GUID = GUID {
    Data1: 0xa4966eed, Data2: 0x76db, Data3: 0x44da,
    Data4lo: 0x9aeec184, Data4hi: 0xa820fb7a,
}

/// IID_IDXGIAdapter = 2411e7e1-12ac-4ccf-bd14-9798e8534dc0
pub const IID_IDXGIAdapter: GUID = GUID {
    Data1: 0x2411e7e1, Data2: 0x12ac, Data3: 0x4ccf,
    Data4lo: 0x989714bd, Data4hi: 0xc04d53e8,
}

/// IID_IDXGIAdapter1 = 29038f61-3839-4626-91fd-086879011a05
pub const IID_IDXGIAdapter1: GUID = GUID {
    Data1: 0x29038f61, Data2: 0x3839, Data3: 0x4626,
    Data4lo: 0x6808fd91, Data4hi: 0x051a0179,
}

/// IID_IDXGIAdapter2 = 0aa1ae0a-fa0e-4b84-8644-e05ff8e5acb5
pub const IID_IDXGIAdapter2: GUID = GUID {
    Data1: 0x0aa1ae0a, Data2: 0xfa0e, Data3: 0x4b84,
    Data4lo: 0x5fe04486, Data4hi: 0xb5ace5f8,
}

/// IID_IDXGIAdapter3 = 645967a4-1392-4310-a798-8053ce3e93fd
pub const IID_IDXGIAdapter3: GUID = GUID {
    Data1: 0x645967a4, Data2: 0x1392, Data3: 0x4310,
    Data4lo: 0x538098a7, Data4hi: 0xfd933ece,
}

/// IID_IDXGIAdapter4 = 3c8d99d1-4fbf-4181-a82c-af66bf7bd24e
pub const IID_IDXGIAdapter4: GUID = GUID {
    Data1: 0x3c8d99d1, Data2: 0x4fbf, Data3: 0x4181,
    Data4lo: 0x66af2ca8, Data4hi: 0x4ed27bbf,
}

/// IID_IDXGIOutput = ae02eedb-c735-4690-8d52-5a8dc20213aa
pub const IID_IDXGIOutput: GUID = GUID {
    Data1: 0xae02eedb, Data2: 0xc735, Data3: 0x4690,
    Data4lo: 0x8d5a528d, Data4hi: 0xaa1302c2,
}

/// IID_IDXGIOutput1 = 00cddea8-939b-4b83-a340-a685226666cc
pub const IID_IDXGIOutput1: GUID = GUID {
    Data1: 0x00cddea8, Data2: 0x939b, Data3: 0x4b83,
    Data4lo: 0x85a640a3, Data4hi: 0xcc666622,
}

/// IID_IDXGIOutput4 = dc7dca35-2196-414d-9f53-617884032a60
pub const IID_IDXGIOutput4: GUID = GUID {
    Data1: 0xdc7dca35, Data2: 0x2196, Data3: 0x414d,
    Data4lo: 0x7861539f, Data4hi: 0x602a0384,
}

/// IID_IDXGIOutput5 = 80a07424-ab52-42eb-833c-0c42fd282d98
pub const IID_IDXGIOutput5: GUID = GUID {
    Data1: 0x80a07424, Data2: 0xab52, Data3: 0x42eb,
    Data4lo: 0x420c3c83, Data4hi: 0x982d28fd,
}

/// IID_IDXGIOutput6 = 068346e8-aaec-4b84-add7-137f513f77a1
pub const IID_IDXGIOutput6: GUID = GUID {
    Data1: 0x068346e8, Data2: 0xaaec, Data3: 0x4b84,
    Data4lo: 0x7f13d7ad, Data4hi: 0xa1773f51,
}

/// IID_IDXGISwapChain = 310d36a0-d2e7-4c0a-aa04-6a9d23b8886a
pub const IID_IDXGISwapChain: GUID = GUID {
    Data1: 0x310d36a0, Data2: 0xd2e7, Data3: 0x4c0a,
    Data4lo: 0x9d6a04aa, Data4hi: 0x6a88b823,
}

/// IID_IDXGISwapChain1 = 790a45f7-0d42-4876-983a-0a55cfe6f4aa
pub const IID_IDXGISwapChain1: GUID = GUID {
    Data1: 0x790a45f7, Data2: 0x0d42, Data3: 0x4876,
    Data4lo: 0x550a3a98, Data4hi: 0xaaf4e6cf,
}

/// IID_IDXGISwapChain2 = a8be2ac4-199f-4946-b331-79599fb98de7
pub const IID_IDXGISwapChain2: GUID = GUID {
    Data1: 0xa8be2ac4, Data2: 0x199f, Data3: 0x4946,
    Data4lo: 0x597931b3, Data4hi: 0xe78db99f,
}

/// IID_IDXGISwapChain4 = 3d585d5a-bd4a-489e-b1f4-3dbcb6452ffb
pub const IID_IDXGISwapChain4: GUID = GUID {
    Data1: 0x3d585d5a, Data2: 0xbd4a, Data3: 0x489e,
    Data4lo: 0xbc3df4b1, Data4hi: 0xfb2f45b6,
}

/// IID_IDXGIDevice = 54ec77fa-1377-44e6-8c32-88fd5f44c84c
pub const IID_IDXGIDevice: GUID = GUID {
    Data1: 0x54ec77fa, Data2: 0x1377, Data3: 0x44e6,
    Data4lo: 0xfd88328c, Data4hi: 0x4cc8445f,
}

/// IID_IDXGIDevice1 = 77db970f-6276-48ba-ba28-070143b4392c
pub const IID_IDXGIDevice1: GUID = GUID {
    Data1: 0x77db970f, Data2: 0x6276, Data3: 0x48ba,
    Data4lo: 0x010728ba, Data4hi: 0x2c39b443,
}


/// IID_ID3D12Device6 = c70b221b-40e4-4a17-89af-025a0727a6dc
pub const IID_ID3D12Device6: GUID = GUID {
    Data1: 0xc70b221b, Data2: 0x40e4, Data3: 0x4a17,
    Data4lo: 0x5a02af89, Data4hi: 0xdca62707,
}

/// IID_ID3D12Device7 = 5c014b53-68a1-4b9b-8bd1-dd6046b9358b
pub const IID_ID3D12Device7: GUID = GUID {
    Data1: 0x5c014b53, Data2: 0x68a1, Data3: 0x4b9b,
    Data4lo: 0x60ddd18b, Data4hi: 0x8b35b946,
}

/// IID_ID3D12Device8 = 9218e6bb-f944-4f7e-a75c-b1b2c7b701f3
pub const IID_ID3D12Device8: GUID = GUID {
    Data1: 0x9218e6bb, Data2: 0xf944, Data3: 0x4f7e,
    Data4lo: 0xb2b15ca7, Data4hi: 0xf301b7c7,
}

/// IID_ID3D12Device9 = 4c80e962-f032-4f60-bc9e-ebc2cfa1d83c
pub const IID_ID3D12Device9: GUID = GUID {
    Data1: 0x4c80e962, Data2: 0xf032, Data3: 0x4f60,
    Data4lo: 0xc2eb9ebc, Data4hi: 0x3cd8a1cf,
}

/// IID_ID3D12Device10 = 517f8718-aa66-49f9-b02b-a7ab89c06031
pub const IID_ID3D12Device10: GUID = GUID {
    Data1: 0x517f8718, Data2: 0xaa66, Data3: 0x49f9,
    Data4lo: 0xaba72bb0, Data4hi: 0x3160c089,
}

/// IID_ID3D12Device11 = 5405c344-d457-444e-b4dd-2366e45aee39
pub const IID_ID3D12Device11: GUID = GUID {
    Data1: 0x5405c344, Data2: 0xd457, Data3: 0x444e,
    Data4lo: 0x6623ddb4, Data4hi: 0x39ee5ae4,
}

/// IID_ID3D12DeviceRemovedExtendedDataSettings = 82bc481c-6b9b-4030-aedb-7ee3d1df1e63
pub const IID_ID3D12DeviceRemovedExtendedDataSettings: GUID = GUID {
    Data1: 0x82bc481c, Data2: 0x6b9b, Data3: 0x4030,
    Data4lo: 0xe37edbae, Data4hi: 0x631edfd1,
}

/// IID_ID3D12DeviceRemovedExtendedDataSettings1 = db8c63ae-2bde-4051-a757-b4afb5b0c0bf
pub const IID_ID3D12DeviceRemovedExtendedDataSettings1: GUID = GUID {
    Data1: 0xdb8c63ae, Data2: 0x2bde, Data3: 0x4051,
    Data4lo: 0xafb457a7, Data4hi: 0xbfc0b0b5,
}

/// IID_ID3D12DeviceRemovedExtendedDataSettings2 = 61552388-01ab-4008-a436-83db189566ea
pub const IID_ID3D12DeviceRemovedExtendedDataSettings2: GUID = GUID {
    Data1: 0x61552388, Data2: 0x01ab, Data3: 0x4008,
    Data4lo: 0xdb8336a4, Data4hi: 0xea669518,
}

/// IID_ID3D12DeviceRemovedExtendedData = 98931d33-5ae8-4791-aa3c-1a73a2934e71
pub const IID_ID3D12DeviceRemovedExtendedData: GUID = GUID {
    Data1: 0x98931d33, Data2: 0x5ae8, Data3: 0x4791,
    Data4lo: 0x731a3caa, Data4hi: 0x714e93a2,
}

/// IID_ID3D12DeviceRemovedExtendedData1 = 9727a022-cf1d-4dda-9eba-effa653fc506
pub const IID_ID3D12DeviceRemovedExtendedData1: GUID = GUID {
    Data1: 0x9727a022, Data2: 0xcf1d, Data3: 0x4dda,
    Data4lo: 0xfaefba9e, Data4hi: 0x06c53f65,
}

/// IID_ID3D12DeviceRemovedExtendedData2 = 67fc5816-e4ca-4915-bf18-42541272da54
pub const IID_ID3D12DeviceRemovedExtendedData2: GUID = GUID {
    Data1: 0x67fc5816, Data2: 0xe4ca, Data3: 0x4915,
    Data4lo: 0x544218bf, Data4hi: 0x54da7212,
}

/// IID_ID3D12CommandSignature = c36a797c-ec80-4f0a-8985-a7b2475082d1
pub const IID_ID3D12CommandSignature: GUID = GUID {
    Data1: 0xc36a797c, Data2: 0xec80, Data3: 0x4f0a,
    Data4lo: 0xb2a78589, Data4hi: 0xd1825047,
}

/// IID_ID3D12ProtectedResourceSession = 6cd696f4-f289-40cc-8091-5a6c0a099c3d
pub const IID_ID3D12ProtectedResourceSession: GUID = GUID {
    Data1: 0x6cd696f4, Data2: 0xf289, Data3: 0x40cc,
    Data4lo: 0x6c5a9180, Data4hi: 0x3d9c090a,
}

/// IID_ID3D12LifetimeTracker = 3fd03d36-4eb1-424a-a582-494ecb8ba813
pub const IID_ID3D12LifetimeTracker: GUID = GUID {
    Data1: 0x3fd03d36, Data2: 0x4eb1, Data3: 0x424a,
    Data4lo: 0x4e4982a5, Data4hi: 0x13a88bcb,
}

/// IID_IDXGIResource = 035f3ab4-482e-4e50-b41f-8a7f8bd8960b
pub const IID_IDXGIResource: GUID = GUID {
    Data1: 0x035f3ab4, Data2: 0x482e, Data3: 0x4e50,
    Data4lo: 0x7f8a1fb4, Data4hi: 0x0b96d88b,
}

/// IID_IDXGIResource1 = 30961379-4609-4a41-998e-54fe567ee0c1
pub const IID_IDXGIResource1: GUID = GUID {
    Data1: 0x30961379, Data2: 0x4609, Data3: 0x4a41,
    Data4lo: 0xfe548e99, Data4hi: 0xc1e07e56,
}

/// IID_IDXGIKeyedMutex = 9d8e1289-d7b3-465f-8126-250e349af85d
pub const IID_IDXGIKeyedMutex: GUID = GUID {
    Data1: 0x9d8e1289, Data2: 0xd7b3, Data3: 0x465f,
    Data4lo: 0x0e252681, Data4hi: 0x5df89a34,
}

/// IID_IDXGISurface = cafcb56c-6ac3-4889-bf47-9e23bbd260ec
pub const IID_IDXGISurface: GUID = GUID {
    Data1: 0xcafcb56c, Data2: 0x6ac3, Data3: 0x4889,
    Data4lo: 0x239e47bf, Data4hi: 0xec60d2bb,
}

/// IID_IDXGISurface1 = 4ae63092-6327-4c1b-80ae-bfe12ea32b86
pub const IID_IDXGISurface1: GUID = GUID {
    Data1: 0x4ae63092, Data2: 0x6327, Data3: 0x4c1b,
    Data4lo: 0xe1bfae80, Data4hi: 0x862ba32e,
}

/// IID_IDXGISurface2 = aba496dd-b617-4cb8-a866-bc44d7eb1fa2
pub const IID_IDXGISurface2: GUID = GUID {
    Data1: 0xaba496dd, Data2: 0xb617, Data3: 0x4cb8,
    Data4lo: 0x44bc66a8, Data4hi: 0xa21febd7,
}

/// IID_IDXGIOutputDuplication = 191cfac3-a341-470d-b26e-a864f428319c
pub const IID_IDXGIOutputDuplication: GUID = GUID {
    Data1: 0x191cfac3, Data2: 0xa341, Data3: 0x470d,
    Data4lo: 0x64a86eb2, Data4hi: 0x9c3128f4,
}

/// IID_IDXGIDebug = 119e7452-de9e-40fe-8806-88f90c12b441
pub const IID_IDXGIDebug: GUID = GUID {
    Data1: 0x119e7452, Data2: 0xde9e, Data3: 0x40fe,
    Data4lo: 0xf9880688, Data4hi: 0x41b4120c,
}

/// IID_IDXGIDebug1 = c5a05f0c-16f2-4adf-9f4d-a8c4d58ac550
pub const IID_IDXGIDebug1: GUID = GUID {
    Data1: 0xc5a05f0c, Data2: 0x16f2, Data3: 0x4adf,
    Data4lo: 0xc4a84d9f, Data4hi: 0x50c58ad5,
}

/// IID_IDXGIInfoQueue = d67441c7-672a-476f-9e82-cd55b44949ce
pub const IID_IDXGIInfoQueue: GUID = GUID {
    Data1: 0xd67441c7, Data2: 0x672a, Data3: 0x476f,
    Data4lo: 0x55cd829e, Data4hi: 0xce4949b4,
}

/// IID_DXGI_DEBUG_ALL = e48ae283-da80-490b-87e6-43e9a9cfda08
pub const IID_DXGI_DEBUG_ALL: GUID = GUID {
    Data1: 0xe48ae283, Data2: 0xda80, Data3: 0x490b,
    Data4lo: 0xe943e687, Data4hi: 0x08dacfa9,
}

/// IID_DXGI_DEBUG_DX = 35cdd7fc-13b2-421d-a5d7-7e4451287d64
pub const IID_DXGI_DEBUG_DX: GUID = GUID {
    Data1: 0x35cdd7fc, Data2: 0x13b2, Data3: 0x421d,
    Data4lo: 0x447ed7a5, Data4hi: 0x647d2851,
}

/// IID_DXGI_DEBUG_DXGI = 25cddaa4-b1c6-47e1-ac3e-98875b5a2e2a
pub const IID_DXGI_DEBUG_DXGI: GUID = GUID {
    Data1: 0x25cddaa4, Data2: 0xb1c6, Data3: 0x47e1,
    Data4lo: 0x87983eac, Data4hi: 0x2a2e5a5b,
}

/// IID_DXGI_DEBUG_APP = 06cd6e01-4219-4ebd-8709-27ed23360c62
pub const IID_DXGI_DEBUG_APP: GUID = GUID {
    Data1: 0x06cd6e01, Data2: 0x4219, Data3: 0x4ebd,
    Data4lo: 0xed270987, Data4hi: 0x620c3623,
}

/// IID_D3D12_DEBUG = cf59a98c-a950-4326-91ef-9bbaa17bfd78
pub const IID_D3D12_DEBUG: GUID = GUID {
    Data1: 0xcf59a98c, Data2: 0xa950, Data3: 0x4326,
    Data4lo: 0xba9bef91, Data4hi: 0x78fd7ba1,
}

