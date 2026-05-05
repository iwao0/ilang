# vscode-ilang

VSCode extension for the ilang language. Stage A ships syntax
highlighting only. Language-server features (F12 / hover / etc.)
will land in Stage B.

日本語版: [README_ja.md](README_ja.md)

## Local install

Two options to enable this extension in VSCode (or Cursor).

### Option 1: development symlink (recommended)

```sh
ln -s "$(pwd)/vscode-extension" ~/.vscode/extensions/ilang
```

Restart VSCode and `.il` files will pick up the highlighting.
Restart again after editing the grammar to reload it.

### Option 2: install as a `.vsix`

```sh
npm install -g @vscode/vsce
cd vscode-extension
vsce package          # produces ilang-0.1.0.vsix
code --install-extension ilang-0.1.0.vsix
```

## Features (Stage A)

- `.il` file association
- Highlighting for keywords, types, numeric literals, strings,
  comments, and attributes (`@flags`, `@extern`, ...)
- Bracket auto-closing and comment toggling

## Coming next (Stage B)

- New crate `ilang-lsp` built on `tower-lsp`
- Features: go-to-definition (F12), hover, diagnostics
  (red squiggles), completion
