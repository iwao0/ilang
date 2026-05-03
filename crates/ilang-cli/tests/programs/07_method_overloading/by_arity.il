// Method overloading by arity (different number of params).
use test

class Greeter {
    init() {}
    greet(): string { "hi" }
    greet(name: string): string { "hi " + name }
    greet(first: string, last: string): string { "hi " + first + " " + last }
}

let g = new Greeter()
test.expectStr(g.greet(), "hi")
test.expectStr(g.greet("alice"), "hi alice")
test.expectStr(g.greet("ada", "lovelace"), "hi ada lovelace")
