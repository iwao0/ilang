/// Metal bindings — folder umbrella. Mirror the foundation /
/// appkit / spritekit layout so a single `use metal { … }` keeps
/// reaching every Metal class regardless of which category file
/// it actually lives in.

pub use types.*
pub use device.*
pub use resources.*
pub use shaders.*
pub use renderpass.*
pub use pipeline.*
pub use encoders.*
pub use sync.*
