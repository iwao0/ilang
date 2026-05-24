// Map literal `{k: v, ...}`. K is inferred from the first entry's key
// type; V from its value type.
use std.test

let scores = {"alice": 30, "bob": 25, "carol": 40}
test.expect(scores.size(), 3)
test.expect(scores["alice"], 30)
test.expect(scores["bob"], 25)
test.expectBool(scores.has("carol"), true)

// Same with int keys.
let by_id = {1: "one", 2: "two", 3: "three"}
test.expect(by_id.size(), 3)
test.expectStr(by_id[2], "two")

// Bool keys are valid too.
let by_flag = {true: "on", false: "off"}
test.expectStr(by_flag[true], "on")
test.expectStr(by_flag[false], "off")
