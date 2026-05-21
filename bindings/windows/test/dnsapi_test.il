/// dnsapi — DnsFlushResolverCache + DnsValidateName_A smoke test.

use test
use windows {
    DnsFlushResolverCache, DnsValidateName_A,
    DnsQuery_A, DnsRecordListFree, DnsType, DnsQueryOptions,
    DNS_RECORD,
}

@extern(C) {
    pub fn run() {
        // Cache flush — admin-only on some hosts; tolerate non-zero.
        let _ = DnsFlushResolverCache()

        // Valid host name → 0.
        let good = cstrFromString("example.com")
        test.expect(DnsValidateName_A(good, 0 as i32) as i64, 0 as i64)
        freeCstr(good)

        // Query A record for localhost. The wire-only path may
        // fail in CI; bypass cache + tolerate both success and
        // common DNS error codes.
        let host = cstrFromString("localhost")
        let p: *DNS_RECORD = 0 as *DNS_RECORD
        let rc = DnsQuery_A(
            host,
            DnsType.a as u16,
            DnsQueryOptions.bypassCache as u32,
            0 as *void,
            &p,
            0 as *void
        )
        freeCstr(host)
        // rc 0 = success; non-zero = expected on locked-down hosts.
        if rc == 0 as i32 {
            test.expectTrue((p as i64) != 0)
            DnsRecordListFree(p, 1 as i32)
        }
    }
}

run()
