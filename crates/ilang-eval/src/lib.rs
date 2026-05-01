pub mod error;
pub mod interpreter;
mod ops;
pub mod value;

use ilang_ast::Program;

pub use error::RuntimeError;
pub use interpreter::Interpreter;
pub use value::Value;

/// Convenience for one-shot evaluation (file mode).
pub fn run_program(prog: &Program) -> Result<Value, RuntimeError> {
    Interpreter::new().run(prog)
}
