/// cabinet — XPRESS_HUFF compress / decompress round-trip.

use std.test
use windows {
    GetProcessHeap, HeapAlloc, HeapFree,
}
use cabinet {
    CreateCompressor,
    CreateDecompressor,
    Compress,
    Decompress,
    CloseCompressor,
    CloseDecompressor,
    CompressAlgo,
}

@extern(C) {
    pub fn run() {
        let h: i64 = 0
        test.expectTrue(CreateCompressor(
            CompressAlgo.xpressHuff as u32,
            0 as *void,
            &h
        ) != 0)
        test.expectTrue(h != 0 as i64)

        let d: i64 = 0
        test.expectTrue(CreateDecompressor(
            CompressAlgo.xpressHuff as u32,
            0 as *void,
            &d
        ) != 0)

        // Compress 1024 bytes of zeros — should fit comfortably in 256.
        let heap = GetProcessHeap()
        let src = HeapAlloc(heap, 8 as u32, 1024 as u64)
        let dst = HeapAlloc(heap, 0 as u32, 4096 as u64)
        let comp: u64 = 0
        test.expectTrue(Compress(
            h, src, 1024 as u64,
            dst, 4096 as u64,
            &comp
        ) != 0)
        test.expectTrue(comp > 0 as u64)
        test.expectTrue(comp < 1024 as u64)        // must actually compress

        // Round-trip — decompress should hand back exactly 1024 bytes.
        let out = HeapAlloc(heap, 0 as u32, 2048 as u64)
        let outSize: u64 = 0
        test.expectTrue(Decompress(
            d, dst, comp,
            out, 2048 as u64,
            &outSize
        ) != 0)
        test.expect(outSize as i64, 1024 as i64)

        HeapFree(heap, 0 as u32, src)
        HeapFree(heap, 0 as u32, dst)
        HeapFree(heap, 0 as u32, out)
        test.expectTrue(CloseCompressor(h) != 0)
        test.expectTrue(CloseDecompressor(d) != 0)
    }
}

run()
