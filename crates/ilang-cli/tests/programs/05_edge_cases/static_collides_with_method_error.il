// A static method can't share a name with an instance method.
// expect-error: collides
class C {
    init() {}
    foo(): i64 { 1 }
    static foo(): i64 { 2 }
}
