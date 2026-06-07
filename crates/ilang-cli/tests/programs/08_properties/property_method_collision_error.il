// A `get`/`set` property can't share a name with a method.
// Field collision is in `property_field_collision_error.il`; this
// fixture pins the method case from syntax.md §7 "Property names
// can't collide with field or method names".
// expect-error: collides
class Bad {
    n: i64
    init() { this.n = 0 }
    x(): i64 { this.n }
    get x(): i64 { this.n }
}
let _ = new Bad()
