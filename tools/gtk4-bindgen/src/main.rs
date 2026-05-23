//! `gtk4-bindgen` — produces `bindings/gtk4/generated/*.il` from
//! GObject Introspection metadata. See the crate README for usage.

mod emit;
mod parse;
mod types;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use crate::parse::parse_gir;
use crate::types::Repo;

#[derive(Parser, Debug)]
#[command(name = "gtk4-bindgen", about = "Generate ilang bindings from GIR")]
struct Cli {
    /// GIR XML inputs. Pass each namespace's GIR separately
    /// (`--gir Gtk-4.0.gir --gir Gdk-4.0.gir ...`). Order matters:
    /// later files can reference types declared in earlier ones.
    #[arg(long = "gir", required = true)]
    gir: Vec<PathBuf>,

    /// Directory to write `ns_<name>.il` files into.
    #[arg(long = "out", required = true)]
    out: PathBuf,

    /// Optional umbrella `gtk.il` path to overwrite with a fresh
    /// `pub use generated.ns_*.*` index. Omit to leave the umbrella
    /// alone.
    #[arg(long = "umbrella")]
    umbrella: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let mut repo = Repo::default();
    for path in &cli.gir {
        eprintln!("parsing {}", path.display());
        if let Err(e) = parse_gir(path, &mut repo) {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    }
    eprintln!(
        "loaded {} namespace(s): {}",
        repo.namespaces.len(),
        repo.namespaces
            .iter()
            .map(|n| n.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    if let Err(e) = emit::emit(&repo, &cli.out) {
        eprintln!("error: {e}");
        return ExitCode::FAILURE;
    }
    if let Some(path) = &cli.umbrella {
        let stems = emit::discover_emitted_stems(&cli.out);
        if let Err(e) = emit::write_umbrella(path, &stems) {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
        eprintln!("wrote umbrella → {}", path.display());
    }
    ExitCode::SUCCESS
}
