// `async fn` and `await` are recognized at the parser/AST level
// but the state-machine lowering that turns an async fn body
// into a `Promise<T>`-returning poll fn is not yet implemented.
// The type checker rejects both with an actionable hint.
//
// expect-error: not yet implemented

async fn foo(): i64 {
    42
}
