// Method overloading by parameter type, dispatched at the call site.
use test

class Show {
    init() {}
    show(n: i64): string { "int" }
    show(s: string): string { "str" }
    show(b: bool): string { "bool" }
}

let s = new Show()
test.expectStr(s.show(42), "int")
test.expectStr(s.show("hi"), "str")
test.expectStr(s.show(true), "bool")
