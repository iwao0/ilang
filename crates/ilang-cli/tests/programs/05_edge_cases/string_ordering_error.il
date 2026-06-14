// Strings support `+` (concatenation) and `==` / `!=` (structural
// equality) but NOT ordering — `<` / `<=` / `>` / `>=` between two
// strings is a type error, not a silent byte/length comparison. (Use
// explicit code-point or length comparisons if ordering is needed.)
//
// expect-error: cannot apply binary op between string and string

let a = "apple"
let b = "banana"
console.log((a < b).toString())
