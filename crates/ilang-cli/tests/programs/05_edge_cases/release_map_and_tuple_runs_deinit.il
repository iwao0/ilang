// When a Map / Tuple holding class instances is released, the
// interpreter must walk its values and call `deinit` on each —
// matching the JIT's per-Map / per-Tuple drop wrapper.
//
// Pre-fix the eval `release` helper handled Object / Array /
// Enum / Some but skipped Map and Tuple, so map values and tuple
// elements silently leaked their `deinit` calls. The harness
// caught the divergence once a fixture compared `deinit` traces
// across backends.
//
// Single-key map keeps the iteration-order trap out of the
// expectation (both runtimes use unordered HashMap internally
// for storage, so a multi-entry map's drop order would only
// agree by hash-seed luck).
//
// expect: --- end
// expect: deinit lone
// expect: deinit tup-1
// expect: deinit tup-2
// expect: deinit map-only

class T {
    name: string
    init(n: string) { this.name = n }
    deinit() { console.log("deinit " + this.name) }
}

let m = new Map<string, T>()
m.set("k", new T("map-only"))

let pair: (T, T) = (new T("tup-1"), new T("tup-2"))

let lone = new T("lone")

console.log("--- end")
