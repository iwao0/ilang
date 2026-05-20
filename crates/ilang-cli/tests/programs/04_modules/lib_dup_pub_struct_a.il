// Sibling fixture for `dup_pub_struct_via_umbrella.il`. Declares
// `pub struct Shared` so the umbrella re-export can collide it
// against the other sibling's declaration.

@extern(C) {
    pub struct Shared {
        x: i32
    }
}
