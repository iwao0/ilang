/// rpcrt4 — UUID generation, comparison, string round-trip.

use std.test as test
use windows {
    UUID,
    UuidCreate, UuidCreateNil, UuidIsNil, UuidEqual, UuidCompare, UuidHash,
    UuidToStringA, UuidFromStringA, RpcStringFreeA,
}

@extern(C) {
    pub fn run() {
        // UuidCreate: succeed and produce a non-nil value.
        let u1 = UUID { Data1: 0, Data2: 0, Data3: 0, Data4lo: 0, Data4hi: 0 }
        let rc1 = UuidCreate(&u1)
        test.expect(rc1 as i64, 0)

        let stat: i32 = 0
        // Newly-minted UUID must not be nil.
        test.expect(UuidIsNil(&u1, &stat) as i64, 0)
        test.expect(stat as i64, 0)

        // UuidCreateNil — zero out a UUID, then verify.
        let nilU = UUID { Data1: 1, Data2: 2, Data3: 3, Data4lo: 4, Data4hi: 5 }
        UuidCreateNil(&nilU)
        test.expect(nilU.Data1 as i64, 0)
        test.expect(nilU.Data2 as i64, 0)
        test.expect(UuidIsNil(&nilU, &stat) as i64, 1)

        // UuidEqual: identical UUIDs compare equal.
        test.expect(UuidEqual(&u1, &u1, &stat) as i64, 1)
        test.expect(UuidEqual(&u1, &nilU, &stat) as i64, 0)

        // UuidCompare returns 0 for identical UUIDs.
        test.expect(UuidCompare(&u1, &u1, &stat) as i64, 0)

        // UuidHash returns a non-zero hash for a random UUID.
        let h = UuidHash(&u1, &stat)
        test.expect(stat as i64, 0)
        // hash can technically be 0 but should be rare; just touch it.

        // String round-trip: u1 → string → u2 → must equal u1.
        let asStr: *char = 0 as *char
        test.expect(UuidToStringA(&u1, &asStr) as i64, 0)
        test.expectTrue((asStr as i64) != 0)

        let u2 = UUID { Data1: 0, Data2: 0, Data3: 0, Data4lo: 0, Data4hi: 0 }
        test.expect(UuidFromStringA(asStr as *const char, &u2) as i64, 0)
        test.expect(UuidEqual(&u1, &u2, &stat) as i64, 1)

        RpcStringFreeA(&asStr)
    }
}

run()
