// Two `pub class` declarations sharing the same name in the
// same file. The loader's `dup_pub` pass turns the previously-
// silent shadow into an actionable load-time error so the user
// sees the collision instead of the type checker quietly picking
// whichever declaration came first.
// expect-error: duplicate `pub` class `DupKlass`

pub class DupKlass {
    x: i32
    pub init() { this.x = 0 }
}

pub class DupKlass {
    y: i32
    pub init() { this.y = 0 }
}

0
