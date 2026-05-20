/// SpriteKit bindings — umbrella module. Flattens the sibling
/// category files into the `spritekit` namespace so callers can
/// `use spritekit { SKScene, SKSpriteNode, ... }` (or reach
/// through `cocoa.X`) without picking a category by hand.

pub use node.*
pub use textures.*
pub use sprites.*
pub use actions.*
pub use effects.*
pub use physics.*
pub use shaders.*
pub use tiles.*
pub use constraints.*
pub use renderer.*
