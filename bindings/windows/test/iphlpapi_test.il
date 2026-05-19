// iphlpapi — buffer-size query for GetNetworkParams.

use test
use windows {
    GetNetworkParams,
}

@extern(C) {
    pub fn run() {
        // First call with pFixedInfo = NULL and *pOutBufLen = 0 returns
        // ERROR_BUFFER_OVERFLOW (111) and fills *pOutBufLen with the
        // required size.
        let bufLen: u32 = 0
        let rc = GetNetworkParams(0 as *void, &bufLen)
        test.expect(rc as i64, 111)
        test.expectTrue(bufLen > 0)
    }
}

run()
