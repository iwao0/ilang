// `lib_struct_lit_in_method.Builder.make()` builds a `Pair`
// via struct literal. Without the loader fix that qualifies
// `StructLit.class`, this fails to load with
//   "undefined class \"Pair\""
// because the type-checker looks up the bare name instead of
// `lib_struct_lit_in_method.Pair`.

// expect: 42

use lib_struct_lit_in_method { Builder }

let b = new Builder()
console.log(b.make())
