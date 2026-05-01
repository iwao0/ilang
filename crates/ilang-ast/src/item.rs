use crate::stmt::Block;
use crate::types::Type;

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

/// Attribute on a function declaration, e.g. `#[requires(net, file::read)]`.
/// Phase 2 parses these but does not enforce them.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub args: Vec<AttrArg>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttrArg {
    /// A capability path like `net` or `file::read`.
    Path(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub attrs: Vec<Attribute>,
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Fn(FnDecl),
}
