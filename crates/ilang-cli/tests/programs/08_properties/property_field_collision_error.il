// A property and a field can't share a name.
// expect-error: collides
class Bad {
    x: i64
    init() { this.x = 0 }
    get x(): i64 { this.x }
}
let b = new Bad()
let _ = b.x
