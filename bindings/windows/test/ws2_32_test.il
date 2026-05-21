/// ws2_32 — Winsock startup and byte-order conversions.

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
            szDescription: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ],
            szSystemStatus: [
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0, 0, 0, 0, 0
            ],
            _pad: [0, 0, 0, 0, 0, 0],
        }
        // 0x0202 = Winsock 2.2
        let rc = WSAStartup(0x0202 as u16, &wsa)
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
