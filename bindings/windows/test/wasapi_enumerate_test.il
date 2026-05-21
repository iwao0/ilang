/// wasapi — enumerate audio render endpoints.
///
/// CoInitialize → CoCreateInstance(MMDeviceEnumerator) →
/// EnumAudioEndpoints(render, active) → walk the collection,
/// read each device's id (LPWSTR, freed via CoTaskMemFree) and
/// state. Bails cleanly on hosts where the audio service isn't
/// running.

use test
use ole32 {
    CoInitializeEx, CoUninitialize, CoCreateInstance, CoTaskMemFree,
    CoInit, ClsCtx,
}
use windows {
    NULL,
    EDataFlow, DeviceState,
    CLSID_MMDeviceEnumerator, IID_IMMDeviceEnumerator,
    IID_IMMDevice,
    IMMDeviceEnumerator, IMMDeviceCollection, IMMDevice,
}

@extern(C) {
    pub fn run() {
        let ihr = CoInitializeEx(NULL, CoInit.apartmentThreaded as u32)
        // S_OK (0) or S_FALSE (1) — both mean "we now own a COM
        // apartment we must Uninitialize on the way out". RPC_E_CHANGED_MODE
        // (-2147417850 / 0x80010106) means another thread already
        // picked a mode; everything else is a real failure.
        if ihr < 0 {
            return
        }

        let rawEnum: *void = NULL
        let chr = CoCreateInstance(
            &CLSID_MMDeviceEnumerator,
            NULL,
            ClsCtx.inprocServer as u32,
            &IID_IMMDeviceEnumerator,
            &rawEnum
        )
        if chr != 0 {
            CoUninitialize()
            return
        }
        let enumr = rawEnum as IMMDeviceEnumerator

        let rawColl: *void = NULL
        let ehr = enumr.EnumAudioEndpoints(
            EDataFlow.render as u32,
            DeviceState.active as u32,
            &rawColl
        )
        test.expect(ehr as i64, 0)
        test.expectTrue((rawColl as i64) != 0)
        let coll = rawColl as IMMDeviceCollection

        let count: u32 = 0
        test.expect(coll.GetCount(&count) as i64, 0)
        // Most machines have at least one active render endpoint;
        // VMs / headless hosts may have zero. Both are acceptable.
        test.expectTrue(count < 256 as u32)

        let i: u32 = 0
        while i < count {
            let rawDev: *void = NULL
            let dhr = coll.Item(i, &rawDev)
            test.expect(dhr as i64, 0)
            let dev = rawDev as IMMDevice

            // State must be ACTIVE (we filtered by it above).
            let state: u32 = 0
            test.expect(dev.GetState(&state) as i64, 0)
            test.expect(state as i64, DeviceState.active as i64)

            // GetId returns a CoTaskMemAlloc'd LPWSTR we must free.
            let idPtr: i64 = 0
            test.expect(dev.GetId(&idPtr as **u16) as i64, 0)
            test.expectTrue(idPtr != 0)
            CoTaskMemFree(idPtr as *void)

            test.expect(dev.Release() as i64, 0)
            i = i + 1
        }

        test.expect(coll.Release() as i64, 0)
        test.expect(enumr.Release() as i64, 0)
        CoUninitialize()
    }
}

run()
