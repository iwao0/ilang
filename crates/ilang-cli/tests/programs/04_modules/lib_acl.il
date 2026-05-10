// Sibling module — visibility-test fixture support. The class
// has an explicit pub init and pub `outer` (the cross-module
// caller can reach those), and a non-pub `secret` field plus
// `inner` method (cross-module call must fail).

pub class Foo {
    secret: i32
    pub visible: i32

    pub init() {
        this.secret = 1
        this.visible = 2
    }

    inner(): i32 { this.secret }
    pub outer(): i32 { this.visible }
}
