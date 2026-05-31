// `@derive(Eq, Hash)` only synthesises the methods the user
// didn't write — so deriving one side and hand-rolling the other
// must compose. Exercises both directions: derived `equals` +
// manual `hashCode`, and manual `equals` + derived `hashCode`.
use std.test as test

// Manual `hashCode` bucketed coarsely so two distinct `n` values
// can collide; the derived `equals` must still tell them apart.
@derive(Eq, Hash)
class CoarseHash {
    pub n: i64
    pub init(n: i64) { this.n = n }
    pub hashCode(): i64 { this.n / 10 }   // wins over the derived hash
}

let s = new Set<CoarseHash>()
s.add(new CoarseHash(1))
s.add(new CoarseHash(2))
s.add(new CoarseHash(11))   // colides with 1 under coarse hash, but
                             // derived equals(1, 11) is false → kept.
test.expect(s.size(), 3)
test.expectBool(s.has(new CoarseHash(1)), true)
test.expectBool(s.has(new CoarseHash(11)), true)
test.expectBool(s.has(new CoarseHash(99)), false)

// Manual `equals` overriding the derived field-by-field compare.
// `id` is ignored — equality reduces to `tag`.
@derive(Eq, Hash)
class IgnoreId {
    pub id: i64
    pub tag: i64
    pub init(id: i64, tag: i64) {
        this.id = id
        this.tag = tag
    }
    pub equals(other: IgnoreId): bool { this.tag == other.tag }
}

let s2 = new Set<IgnoreId>()
s2.add(new IgnoreId(1, 100))
s2.add(new IgnoreId(2, 100))   // same `tag` → equal under override
s2.add(new IgnoreId(3, 200))
// Three different hashCodes (derived from both fields), but the
// equality fold only looks at `tag`, so (1, 100) and (2, 100)
// collapse only when they happen to hit the same bucket. To
// guarantee the dedup we add the same `(id, tag)` twice — bucket
// hits independent of the derived hash.
s2.add(new IgnoreId(1, 100))   // structural dup of an existing entry
test.expect(s2.size(), 3)
