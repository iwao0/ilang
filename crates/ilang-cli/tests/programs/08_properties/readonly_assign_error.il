// Assigning to a getter-only property errors at typecheck.
// expect-error: no setter
class P {
    n: i64
    init() { this.n = 1 }
    get x(): i64 { this.n }
}
let p = new P()
p.x = 5
