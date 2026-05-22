# vscode-ilang

VSCode extension for the ilang language. Includes syntax
highlighting and a language server (`ilang-lsp`).

日本語版: [README_ja.md](README_ja.md)

## Local install

```sh
# 1. Build the language server (release recommended)
cargo build --release -p ilang-lsp

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
3. `<workspace>/target/release/ilang-lsp`
4. `<workspace>/target/debug/ilang-lsp`

## Editor features

- `.il` file association
- Default language icon for `.il` files when the active VSCode file icon
  theme allows language-provided icons
- Highlighting for keywords, types, numeric literals, strings,
  comments, and attributes (`@flags`, `@extern`, ...)
- Bracket auto-closing and comment toggling

## Language server features

- **Diagnostics** — parser and type-checker errors as red squiggles,
  refreshed from the live buffer (unsaved edits included)
- **Hover** — signature plus `///` doc comment for the identifier
  under the cursor; works for locals, params, fields, methods,
  getters/setters, enum variants, imported `use module` names,
  builtin array/string/map methods
- **Go-to-definition (F12)**
  - Same-file decls
  - Cross-file via `use module` (including stdlib and `ilang.toml`
    `[deps]` paths)
  - `use super.M` — walks the parent package in the dep DAG
  - Class bases, interface names, type annotations
- **Find All References** — workspace-wide (`.il` files reachable
  from the file's `ilang.toml`, plus all open buffers)
- **Rename** — same scope as Find References; refuses on `this`
- **Completion**
  - Top-level decls, locals, params, enum variants
  - Member completion on `obj.` (fields / methods / getters /
    setters / builtin methods on array / string / map)
  - Type-position completion after `:` / `,` / `<`
  - Attribute completion after `@`
  - `use M { … }` selective-import names
  - Interface method stubs when implementing an interface
  - Keyword completion (including `super`)
- **Signature help** — parameter hints with overload navigation,
  triggered on `(` / `,` and re-triggerable on `<`
- **Document formatting** — whole-file formatting
- **Code actions**
  - `source.organizeImports` — sort and dedupe `use` items
  - Quick fixes for common diagnostics
  - Generate `init(...)` from class fields
  - Implement missing interface methods
  - Fill match arms with all enum variants
- **Document symbol (outline)** — nested tree of top-level fns,
  classes (with fields / methods / properties / static members),
  interfaces, enums (with variants), consts, and `@extern(C)`
  items

## Scope

The LSP analyses the open file together with everything reachable
from its `ilang.toml`: `use module` items, `[deps]` paths
(including `target` filters), and transitive deps. The buffer text
is canonical — diagnostics, hover, completion, etc. reflect
unsaved edits.
