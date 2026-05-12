// Trojan-source defence: the RLO (U+202E) bidi override would
// silently flip the visual order of the assignment, making the
// reader see one expression while the lexer reads another. The
// lexer rejects every bidi-control / zero-width / line-separator
// character outside of string-literal `\u{...}` escapes.
// expect-error: trojan-source
let x = 1‮ + 2
