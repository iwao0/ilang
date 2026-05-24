/// iphlpapi — ICMP echo (ping) to the localhost address.

use test
use windows {
    HeapAlloc, HeapFree, GetProcessHeap,
}
use iphlpapi { IcmpCreateFile, IcmpCloseHandle, IcmpSendEcho }

@extern(C) {
    pub fn run() {
        let h = IcmpCreateFile()
        // INVALID_HANDLE_VALUE bit pattern check.
        test.expectTrue((h as i64) != -1)
        test.expectTrue((h as i64) != 0)

        // 127.0.0.1 in network byte order = 0x0100007F.
        let dest: u32 = 0x0100007F

        let heap = GetProcessHeap()
        // ReplyBuffer must hold sizeof(ICMP_ECHO_REPLY) (28) + payload + 8 IO_STATUS.
        // Allocate generously: 4 KB.
        let reply = HeapAlloc(heap, 8 as u32, 4096 as u64)
        let payload = cstrFromString("ping")

        let count = IcmpSendEcho(
            h as *void,
            dest,
            payload as *const void,
            4 as u16,
            0 as *const void,
            reply,
            4096 as u32,
            1000 as u32          // 1 s timeout
        )
        // 127.0.0.1 always replies — count should be 1.
        test.expectTrue(count >= 1)

        freeCstr(payload)
        HeapFree(heap, 0 as u32, reply)
        test.expectTrue(IcmpCloseHandle(h as *void) != 0)
    }
}

run()
