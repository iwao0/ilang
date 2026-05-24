// Fixture for the LSP `@lib` / C-only struct completion-filter
// tests. The body intentionally references nothing from the
// `windows` namespace beyond the `use`, so completion at top
// level reflects only the harvest output, not buffer-local
// bindings.

use windows
