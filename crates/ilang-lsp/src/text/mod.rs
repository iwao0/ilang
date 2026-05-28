//! Pure text / span helpers shared by the LSP. None of these reach
//! into project-specific data structures — they operate on the raw
//! source string + a `Span` (1-based line/col) and return either an
//! offset into the byte slice or another `Span`.
//!
//! Submodules:
//! * [`span`] — offset ↔ line/col conversions and `Span` / `Range` /
//!   `Position` builders.
//! * [`locate`] — `locate_*` name-finders that walk source from a
//!   declaration keyword to the identifier token.
//! * [`doc`] — `///` / `//!` doc-comment extraction.
//! * [`words`] — cursor word / prefix / identifier-classification.
//! * [`literals`] — literal-receiver detection for completion.
//! * [`signature`] — signature / call-context parsing for signature
//!   help and inlay hints.

mod doc;
mod literals;
mod locate;
mod signature;
mod span;
mod words;

pub(crate) use doc::*;
pub(crate) use literals::*;
pub(crate) use locate::*;
pub(crate) use signature::*;
pub(crate) use span::*;
pub(crate) use words::*;
