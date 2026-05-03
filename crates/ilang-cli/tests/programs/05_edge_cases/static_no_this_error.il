// `this` is invalid in a static method body.
// expect-error: this
class C {
    n: i64
    init() { this.n = 0 }
    static bad(): i64 { this.n }
}
