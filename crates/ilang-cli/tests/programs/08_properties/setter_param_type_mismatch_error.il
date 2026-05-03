// Getter return type and setter param type must match the property type.
// expect-error: expected
class C {
    n: i64
    init() { this.n = 0 }
    get x(): i64 { this.n }
    set x(s: string) { this.n = 0 }   // setter type i64 ≠ string → error
}
let _ = new C()
