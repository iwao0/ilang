import * as path from "path";
import { workspace, ExtensionContext, window } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext) {
  // Resolve the LSP binary. Order:
  // 1. Setting `ilang.serverPath` (absolute path)
  // 2. ILANG_LSP_PATH env var
  // 3. `target/release/ilang-lsp` relative to the workspace root
  // 4. `target/debug/ilang-lsp` as a fallback
  // The release build is roughly 20× faster on the bigger files
  // (sdl_breakout.il, etc.); the debug build keeps tripping VSCode's
  // unresponsive-process timeout under sustained typing.
  const config = workspace.getConfiguration("ilang");
  let serverPath = config.get<string>("serverPath") || process.env.ILANG_LSP_PATH;
  if (!serverPath) {
    const root = workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (root) {
      const fs = require("fs");
      const release = path.join(root, "target", "release", "ilang-lsp");
      const debug = path.join(root, "target", "debug", "ilang-lsp");
      serverPath = fs.existsSync(release) ? release : debug;
    }
  }
  if (!serverPath) {
    window.showErrorMessage(
      "ilang-lsp: cannot locate the language server. Set `ilang.serverPath`."
    );
    return;
  }

  const serverOptions: ServerOptions = {
    run: { command: serverPath },
    debug: { command: serverPath },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "ilang" }],
  };

  client = new LanguageClient(
    "ilang-lsp",
    "ilang Language Server",
    serverOptions,
    clientOptions
  );
  client.start();
  context.subscriptions.push({
    dispose: () => {
      client?.stop();
    },
  });
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
