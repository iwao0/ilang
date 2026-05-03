// Methods returning the receiver enable chaining (fluent style).
use test

class Builder {
    n: i64
    init() { this.n = 0 }
    add(k: i64): Builder { this.n = this.n + k; this }
    mul(k: i64): Builder { this.n = this.n * k; this }
    get(): i64 { this.n }
}

let b = new Builder()
let r = b.add(2).mul(3).add(4).get()      // ((0+2)*3)+4 = 10
test.expect(r, 10)

// Call methods that return non-receiver too.
class StringBox {
    s: string
    init(v: string) { this.s = v }
    upper(): string { this.s.toUpper() }
    lenPlus(k: i64): i64 { this.s.length + k }
}
let sb = new StringBox("hi")
test.expectStr(sb.upper(), "HI")
test.expect(sb.lenPlus(10), 12)
