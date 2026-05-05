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
- Highlighting for keywords, types, numeric literals, strings,
  comments, and attributes (`@flags`, `@extern`, ...)
- Bracket auto-closing and comment toggling
- **Diagnostics** — parser and type-checker errors as red squiggles
- **Hover** — type and signature for top-level fn / class / enum / const
- **Go-to-definition (F12)** — jumps to the declaring identifier

## Limitations

The LSP currently indexes only **top-level declarations** in the
single open file. Local variables, class members, and cross-file
references are not yet resolved.
