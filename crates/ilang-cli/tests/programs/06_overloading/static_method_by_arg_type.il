// Static method overloading: same name, different signatures,
// dispatched at the call site like instance / top-level overloads.
use std.test as test
class Greeter {
    static say(n: i64): string { "int" }
    static say(s: string): string { "str" }
    static say(b: bool): string { "bool" }
    static say(): string { "none" }
}

test.expectStr(Greeter.say(42), "int")
test.expectStr(Greeter.say("hi"), "str")
test.expectStr(Greeter.say(true), "bool")
test.expectStr(Greeter.say(), "none")
