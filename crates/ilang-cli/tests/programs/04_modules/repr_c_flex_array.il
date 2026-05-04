// C99 flexible array member: `T[]` as the last field of a
// `@repr(C) struct`. `new StructName(n)` widens the allocation by
// `n * sizeof(T)` bytes for the trailing variable area; the FAM
// field returns a base pointer at `instance + sizeof(fixed area)`.
// Bounds checks are skipped — like in C, the user maintains the
// element count themselves (typically in a sibling field).
use test

@extern(C) {
    @repr(C) struct packet {
        kind: u8
        len: u32
        data: u8[]
    }

    @repr(C) struct i32_buf {
        n: u32
        items: i32[]
    }
}

let p = new packet(8)
p.kind = 7 as u8
p.len = 8 as u32

// Write each byte; read back to confirm independent slots.
p.data[0] = 100 as u8
p.data[1] = 101 as u8
p.data[2] = 102 as u8
p.data[3] = 103 as u8
p.data[4] = 104 as u8
p.data[5] = 105 as u8
p.data[6] = 106 as u8
p.data[7] = 107 as u8

test.expect(p.kind as i64, 7)
test.expect(p.len as i64, 8)
test.expect(p.data[0] as i64, 100)
test.expect(p.data[3] as i64, 103)
test.expect(p.data[7] as i64, 107)

// Wider element type — i32 FAM.
let buf = new i32_buf(4)
buf.n = 4 as u32
buf.items[0] = -1 as i32
buf.items[1] = 100 as i32
buf.items[2] = 200 as i32
buf.items[3] = 300 as i32
test.expect(buf.items[0] as i64, -1)
test.expect(buf.items[1] as i64, 100)
test.expect(buf.items[2] as i64, 200)
test.expect(buf.items[3] as i64, 300)

// Multiple instances independent — different counts coexist.
let small = new packet(2)
small.kind = 1 as u8
small.len = 2 as u32
small.data[0] = 0xAA as u8
small.data[1] = 0xBB as u8

test.expect(p.data[0] as i64, 100)        // p still has its 8 bytes
test.expect(small.data[0] as i64, 0xAA)   // small has its own 2
test.expect(small.data[1] as i64, 0xBB)
