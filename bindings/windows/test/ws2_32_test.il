// ws2_32 — Winsock startup and byte-order conversions.

use test
use windows {
    WSAStartup, WSACleanup, WSAGetLastError,
    WSADATA,
    htons, htonl, ntohs, ntohl,
    inet_addr,
    socket, closesocket,
    AddressFamily, SocketType, Protocol,
}

@extern(C) {
    pub fn run() {
        // Build an empty WSADATA — we don't read the trailing
        // description / status buffers.
        let wsa = WSADATA {
            wVersion: 0,
            wHighVersion: 0,
            iMaxSockets: 0,
            iMaxUdpDg: 0,
            lpVendorInfo: 0 as *char,
            _d00: 0 as u64, _d01: 0 as u64, _d02: 0 as u64, _d03: 0 as u64,
            _d04: 0 as u64, _d05: 0 as u64, _d06: 0 as u64, _d07: 0 as u64,
            _d08: 0 as u64, _d09: 0 as u64, _d10: 0 as u64, _d11: 0 as u64,
            _d12: 0 as u64, _d13: 0 as u64, _d14: 0 as u64, _d15: 0 as u64,
            _d16: 0 as u64, _d17: 0 as u64, _d18: 0 as u64, _d19: 0 as u64,
            _d20: 0 as u64, _d21: 0 as u64, _d22: 0 as u64, _d23: 0 as u64,
            _d24: 0 as u64, _d25: 0 as u64, _d26: 0 as u64, _d27: 0 as u64,
            _d28: 0 as u64, _d29: 0 as u64, _d30: 0 as u64, _d31: 0 as u64,
            _d32: 0 as u64, _d33: 0 as u64, _d34: 0 as u64, _d35: 0 as u64,
            _d36: 0 as u64, _d37: 0 as u64, _d38: 0 as u64, _d39: 0 as u64,
            _d40: 0 as u64, _d41: 0 as u64, _d42: 0 as u64, _d43: 0 as u64,
            _d44: 0 as u64, _d45: 0 as u64, _d46: 0 as u64, _d47: 0 as u64,
            _d48: 0 as u64,
        }
        // 0x0202 = Winsock 2.2
        let rc = WSAStartup(0x0202 as u16, wsa)
        test.expect(rc as i64, 0)
        test.expectTrue(wsa.wVersion != 0)

        // htons / ntohs round-trip: host → network → host.
        let h: u16 = 0x1234
        let n = htons(h)
        test.expect(ntohs(n) as i64, h as i64)
        // Network order swaps the bytes on little-endian hosts.
        test.expectTrue(n != h)

        // htonl / ntohl round-trip.
        let hl: u32 = 0x12345678
        let nl = htonl(hl)
        test.expect(ntohl(nl) as i64, hl as i64)

        // inet_addr: dotted-decimal → network-order u32.
        // "127.0.0.1" → 0x0100007F in network byte order (little-endian
        // host returns 0x0100007F since result is already net order).
        let lpStr = cstrFromString("127.0.0.1")
        let addr = inet_addr(lpStr)
        test.expectTrue(addr != 0xFFFFFFFF as u32)
        freeCstr(lpStr)

        let bad = cstrFromString("not-an-ip")
        test.expect(inet_addr(bad) as i64, 0xFFFFFFFF)
        freeCstr(bad)

        // Create / close a UDP socket — minimal sanity check.
        let s = socket(
            AddressFamily.inet as i32,
            SocketType.dgram as i32,
            Protocol.udp as i32
        )
        test.expectTrue(s != 0xFFFFFFFFFFFFFFFF as u64)
        test.expect(closesocket(s) as i64, 0)

        WSACleanup()
    }
}

run()
