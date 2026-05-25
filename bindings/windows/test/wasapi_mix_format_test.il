/// wasapi — bring up the default render endpoint and read the
/// shared-mode mix format. Exercises:
///   * IMMDeviceEnumerator.GetDefaultAudioEndpoint
///   * IMMDevice.Activate(IID_IAudioClient)
///   * IAudioClient.GetMixFormat
///   * IAudioClient.GetDevicePeriod
///   * IAudioClient.IsFormatSupported
///   * IAudioClient.GetBufferSize (after Initialize)
///   * IAudioClient.GetStreamLatency
///
/// Bails cleanly on hosts without an active render endpoint or
/// where the audio service isn't running.

use std.test as test
use ole32 {
    CoInitializeEx, CoUninitialize, CoCreateInstance, CoTaskMemFree,
    CoInit, ClsCtx,
}
use windows {
    NULL,
}
use wasapi {
    EDataFlow,
    ERole,
    AudioClientShareMode,
    CLSID_MMDeviceEnumerator,
    IID_IMMDeviceEnumerator,
    IID_IAudioClient,
    IMMDeviceEnumerator,
    IMMDevice,
    IAudioClient,
    WAVEFORMATEX_WASAPI,
}

@extern(C) {
    pub fn run() {
        if CoInitializeEx(NULL, CoInit.apartmentThreaded as u32) < 0 {
            return
        }

        let rawEnum: *void = NULL
        let chr = CoCreateInstance(
            &CLSID_MMDeviceEnumerator, NULL,
            ClsCtx.inprocServer as u32,
            &IID_IMMDeviceEnumerator, &rawEnum
        )
        if chr != 0 {
            CoUninitialize()
            return
        }
        let enumr = rawEnum as IMMDeviceEnumerator

        let rawDev: *void = NULL
        let dhr = enumr.GetDefaultAudioEndpoint(
            EDataFlow.render as u32,
            ERole.console as u32,
            &rawDev
        )
        if dhr != 0 {
            test.expect(enumr.Release() as i64, 0)
            CoUninitialize()
            return
        }
        let dev = rawDev as IMMDevice

        // Activate IAudioClient against the endpoint.
        let rawClient: *void = NULL
        let ahr = dev.Activate(
            &IID_IAudioClient,
            ClsCtx.inprocServer as u32,
            NULL,
            &rawClient
        )
        test.expect(ahr as i64, 0)
        let client = rawClient as IAudioClient

        // ── GetMixFormat ────────────────────────────────────────────
        let pFmt: i64 = 0
        let mhr = client.GetMixFormat(&pFmt as **WAVEFORMATEX_WASAPI)
        test.expect(mhr as i64, 0)
        test.expectTrue(pFmt != 0)
        let fmt = pFmt as *WAVEFORMATEX_WASAPI
        // Sanity bounds on the shared-mode format.
        test.expectTrue(fmt.nChannels        >= 1 as u16)
        test.expectTrue(fmt.nSamplesPerSec   >= 8000 as u32)
        test.expectTrue(fmt.wBitsPerSample   >= 8 as u16)
        test.expect(fmt.nBlockAlign as i64,
                    (fmt.nChannels as i64) * ((fmt.wBitsPerSample as i64) / 8))

        // ── IsFormatSupported (shared mode, same format) ────────────
        let pClosest: i64 = 0
        let ihr = client.IsFormatSupported(
            AudioClientShareMode.shared as u32,
            fmt as *const WAVEFORMATEX_WASAPI,
            &pClosest as **void
        )
        // Shared mode against the mix format itself must succeed.
        test.expect(ihr as i64, 0)
        if pClosest != 0 {
            // S_OK with a closest-match shouldn't happen for an exact
            // shared-mode match, but the runtime sometimes returns a
            // duplicate of the input. Free it if it appears.
            CoTaskMemFree(pClosest as *void)
        }

        // ── GetDevicePeriod ─────────────────────────────────────────
        let defPeriod: i64 = 0
        let minPeriod: i64 = 0
        test.expect(client.GetDevicePeriod(&defPeriod, &minPeriod) as i64, 0)
        // Both reported in 100-ns ticks; default ≥ minimum and both
        // are positive on a real audio device.
        test.expectTrue(minPeriod > 0)
        test.expectTrue(defPeriod >= minPeriod)

        // ── Initialize + GetBufferSize + GetStreamLatency ───────────
        // 100 ms buffer, no event-driven mode.
        let bufDuration: i64 = 1000000          // 100 ms in 100-ns ticks
        let initHr = client.Initialize(
            AudioClientShareMode.shared as u32,
            0 as u32,                            // no stream flags
            bufDuration,
            0 as i64,                            // periodicity (shared)
            fmt as *const WAVEFORMATEX_WASAPI,
            NULL
        )
        if initHr == 0 {
            let bufFrames: u32 = 0
            test.expect(client.GetBufferSize(&bufFrames) as i64, 0)
            test.expectTrue(bufFrames > 0 as u32)

            let latency: i64 = 0
            test.expect(client.GetStreamLatency(&latency) as i64, 0)
            test.expectTrue(latency >= 0)
        }

        CoTaskMemFree(pFmt as *void)
        test.expect(client.Release() as i64, 0)
        test.expect(dev.Release()    as i64, 0)
        test.expect(enumr.Release()  as i64, 0)
        CoUninitialize()
    }
}

run()
