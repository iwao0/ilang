# vscode-ilang

VSCode extension for the ilang language. Includes syntax
highlighting and a language server (`ilang-lsp`) for diagnostics,
hover, and go-to-definition.

日本語版: [README_ja.md](README_ja.md)

## Local install

```sh
# 1. Build the language server
cargo build -p ilang-lsp

# 2. Build the extension client (TypeScript -> JS)
cd vscode-extension
npm install
npm run compile

# 3. Symlink into VSCode's extensions directory
ln -s "$(pwd)" ~/.vscode/extensions/ilang
```

Restart VSCode. `.il` files now get highlighting and the language
server starts on demand.

The extension looks for the `ilang-lsp` binary in this order:

1. The `ilang.serverPath` setting (absolute path)
2. The `ILANG_LSP_PATH` environment variable
3. `<workspace>/target/debug/ilang-lsp` (default during dev)

## Features

- `.il` file association
- Default language icon for `.il` files when the active VSCode file icon
  theme allows language-provided icons
- Highlighting for keywords, types, numeric literals, strings,
  comments, and attributes (`@flags`, `@extern`, ...)
- Bracket auto-closing and comment toggling
- **Diagnostics** — parser and type-checker errors as red squiggles
- **Hover** — signature for the identifier under the cursor
- **Go-to-definition (F12)** — jumps to the declaring identifier

## Scope

The LSP indexes the open file as follows:

- **Top-level decls** — fn / class / enum / const
- **Locals + parameters** — `let`, fn params, fn-expr params, `for x in ...`
- **`this`** — resolves to the enclosing class
- **`this.field` / `this.method(...)`** — resolves to the class member
- **`obj.field` / `obj.method(...)`** when `obj` is a known-class local
  (declared with an annotation or as `new ClassName(...)`)

Diagnostics use the same loader pipeline as `ilang run`: `use
module` items and `ilang.toml` `[deps]` paths are resolved, and
top-level `const` values are inlined before type-checking. The
on-disk version of the file is canonical for diagnostics —
unsaved buffer edits aren't reflected until you save.

Cross-file F12 / hover (jumping into another `use module` file)
is not yet supported; the index covers only the current file.
