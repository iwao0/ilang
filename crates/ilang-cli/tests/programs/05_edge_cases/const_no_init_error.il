// `const` requires an initializer expression — the parser
// rejects bare `const NAME: T` (no `= …`) at decl time.
//
// expect-error: `const` requires an initializer expression

const X: i32
