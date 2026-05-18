// Method overloading is now allowed on subclasses too — earlier
// it was root-class only ("overloading is not supported in
// inheritance hierarchies"). The check is: overloads are fine
// among methods the child declares, as long as none of them
// `override` a parent method (override stays single-sig, since
// override resolution against an overloaded parent method is
// ambiguous).
// expect: i64=42
// expect: str=hi
// expect: parent.greet

class Parent {
    pub init() {}
    pub greet(): string { "parent.greet" }
}

class Child: Parent {
    pub init() { super() }
    // Two overloads of `handle`. The parent has no `handle`, so
    // there's no override conflict.
    pub handle(x: i64): string { "i64=" + x.toString() }
    pub handle(x: string): string { "str=" + x }
}

let c = new Child()
console.log(c.handle(42))
console.log(c.handle("hi"))
console.log(c.greet())
