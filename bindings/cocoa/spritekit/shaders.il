/// Custom-shader pipeline — SKShader (GLSL fragment shader source),
/// SKUniform (per-shader uniforms), SKAttribute (per-node attributes
/// reachable by name from inside the shader), SKAttributeValue.

use foundation { NSObject, NSString, NSArray, NSVector }
use textures { SKTexture }

/// SKAttributeType — value type for a custom `SKAttribute` slot
/// exposed to a fragment shader.
pub enum SKAttributeType: i64 {
    none             = 0
    float            = 1
    vectorFloat2     = 2
    vectorFloat3     = 3
    vectorFloat4     = 4
    halfFloat        = 5
    vectorHalfFloat2 = 6
    vectorHalfFloat3 = 7
    vectorHalfFloat4 = 8
}

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
    /// `SKUniform` — named scalar / texture value handed to a
    /// fragment shader. Construct with the typed factory that
    /// matches the GLSL uniform's declared type.
    @objc pub class SKUniform : NSObject {
        @objc("uniformWithName:")
        pub static uniformWithName(name: NSString): SKUniform
        @objc("uniformWithName:float:")
        pub static uniformWithNameFloat(name: NSString, v: f32): SKUniform
        @objc("uniformWithName:texture:")
        pub static uniformWithNameTexture(name: NSString, texture: SKTexture): SKUniform

        @objc("setFloatValue:")
        pub setFloatValue(v: f32)
        @objc("floatValue")
        pub floatValue(): f32
        @objc("setTextureValue:")
        pub setTextureValue(t: SKTexture)
    }

    /// `SKAttribute` — per-node shader input declaration. The
    /// fragment shader reads it as `a_<name>`; runtime values are
    /// supplied per-node via `SKNode.setValueForAttributeNamed(...)`.
    @objc pub class SKAttribute : NSObject {
        @objc("attributeWithName:type:")
        pub static attributeWithName(name: NSString, type: SKAttributeType): SKAttribute

        @objc("name")
        pub name(): NSString
        @objc("type")
        pub attrType(): SKAttributeType
    }

    /// `SKShader` — custom fragment shader (GLSL ES). Attach via
    /// `SKSpriteNode.setShader:` / `SKEffectNode.setShader:`;
    /// per-shader globals are passed as `SKUniform`s,
    /// per-node values as `SKAttribute`s.
    @objc pub class SKShader : NSObject {
        @objc("shaderWithSource:")
        pub static shaderWithSource(source: NSString): SKShader
        @objc("shaderWithFileNamed:")
        pub static shaderWithFileNamed(name: NSString): SKShader

        @objc("setSource:")
        pub setSource(source: NSString)
        @objc("setUniforms:")
        pub setUniforms(arr: NSArray)
        @objc("uniforms")
        pub uniforms(): NSArray
        @objc("addUniform:")
        pub addUniform(u: SKUniform)
        @objc("uniformNamed:")
        pub uniformNamed(name: NSString): SKUniform
        @objc("setAttributes:")
        pub setAttributes(arr: NSArray)
        @objc("attributes")
        pub attributes(): NSArray
    }

    /// `SKAttributeValue` — per-node value handed to a custom
    /// shader, paired with the matching `SKAttribute` declared
    /// on the shader. Scalar `valueWithFloat:` covers the common
    /// 1-uniform case; the vector setters take SIMD literals
    /// (`[x, y]` etc.) at the call site.
    @objc pub class SKAttributeValue : NSObject {
        @objc("valueWithFloat:")
        pub static valueWithFloat(v: f32): SKAttributeValue
        @objc("valueWithVectorFloat2:")
        pub static valueWithVectorFloat2(v: simd.f32x2): SKAttributeValue
        /// `vector_float3` shares ABI with `vector_float4` —
        /// the 4th element is padding; supply 0.0 there.
        @objc("valueWithVectorFloat3:")
        pub static valueWithVectorFloat3(v: simd.f32x4): SKAttributeValue
        @objc("valueWithVectorFloat4:")
        pub static valueWithVectorFloat4(v: simd.f32x4): SKAttributeValue

        @objc("setFloatValue:")
        pub setFloatValue(v: f32)
        @objc("floatValue")
        pub floatValue(): f32
    }

}
