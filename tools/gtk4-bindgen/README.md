# gtk4-bindgen

Generates `bindings/gtk4/generated/*.il` from the GObject Introspection
metadata (`/usr/share/gir-1.0/Gtk-4.0.gir` on Linux,
`/opt/homebrew/share/gir-1.0/Gtk-4.0.gir` on Homebrew macOS).

## Usage

```sh
~/.cargo/bin/cargo run -p gtk4-bindgen -- \
    --gir /opt/homebrew/share/gir-1.0/Gtk-4.0.gir \
    --gir /opt/homebrew/share/gir-1.0/Gdk-4.0.gir \
    --gir /opt/homebrew/share/gir-1.0/GObject-2.0.gir \
    --gir /opt/homebrew/share/gir-1.0/GLib-2.0.gir \
    --gir /opt/homebrew/share/gir-1.0/Gio-2.0.gir \
    --out bindings/gtk4/generated
```

The generated output is meant to be **committed** so the bindings
remain usable on machines that don't have the GIR files installed.

## Scope

Phase A: every enum / bitfield, every class as a `@handle struct`,
plus the subset of constructors / methods / functions whose signature
parses cleanly (no out parameters, no callbacks, no arrays — those
get skipped for the first cut). Deprecated items are skipped.

Hand-written bindings (`bindings/gtk4/{core,input,menu,widgets}.il`)
are kept on disk but the umbrella `gtk.il` only re-exports from
`generated/`. As coverage grows, the manual files become smaller
and eventually go away.
