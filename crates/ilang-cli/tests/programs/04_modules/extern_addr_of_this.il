// `&this.field` inside a method body. The MIR lowerer resolves
// `this` like a regular Var, then walks the field chain the same
// way it does for plain `&local.f` — emitting the receiver
// (typically as an SSA function param) followed by `LoadField`*
// and a final `AddrOfField`. Methods that need `&` must live
// inside an `@extern(C) { ... }` block, matching the SDL-binding
// idiom of wrapping the whole class in `@extern(C)`.
//
// jit: skip
use std.test

@extern(C) {
    fn set_via_ptr(out: *i32, value: i32)

    class Counter {
        pub x: i32
        pub y: i32

        pub init() { this.x = 0; this.y = 0 }

        pub fill(vx: i32, vy: i32) {
            set_via_ptr(&this.x, vx)
            set_via_ptr(&this.y, vy)
        }
    }
}

let c = new Counter()
c.fill(7 as i32, 9 as i32)
test.expect(c.x as i64, 7)
test.expect(c.y as i64, 9)
