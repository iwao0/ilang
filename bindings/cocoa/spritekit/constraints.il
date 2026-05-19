// Range / Constraint / Region / inverse-kinematics / warp
// transforms — value-types and helpers that operate on
// SKNodes but aren't themselves nodes.

use foundation { NSObject, NSArray, NSPoint, NSSize, NSRect }
use node { SKNode }

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
    /// `SKRange` — open-ended numeric interval. Used by
    /// `SKConstraint` to express the allowed band for a
    /// position / rotation / distance.
    @objc pub class SKRange : NSObject {
        @objc("rangeWithLowerLimit:upperLimit:")
        pub static rangeWithLowerUpper(lower: f64, upperLimit: f64): SKRange
        @objc("rangeWithConstantValue:")
        pub static rangeWithConstantValue(v: f64): SKRange
        @objc("rangeWithLowerLimit:")
        pub static rangeWithLowerLimit(v: f64): SKRange
        @objc("rangeWithUpperLimit:")
        pub static rangeWithUpperLimit(v: f64): SKRange
        @objc("rangeWithNoLimits")
        pub static rangeWithNoLimits(): SKRange

        @objc("setLowerLimit:")
        pub setLowerLimit(v: f64)
        @objc("setUpperLimit:")
        pub setUpperLimit(v: f64)
    }

    /// `SKConstraint` — applied to an `SKNode` to clamp its
    /// position / rotation / orientation / distance to a target.
    /// Multiple constraints run in declaration order.
    @objc pub class SKConstraint : NSObject {
        @objc("positionX:")
        pub static positionX(range: SKRange): SKConstraint
        @objc("positionY:")
        pub static positionY(range: SKRange): SKConstraint
        @objc("positionX:y:")
        pub static positionXY(rx: SKRange, y: SKRange): SKConstraint
        @objc("distance:toNode:")
        pub static distanceToNode(range: SKRange, toNode: SKNode): SKConstraint
        @objc("distance:toPoint:")
        pub static distanceToPoint(range: SKRange, toPoint: NSPoint): SKConstraint
        @objc("orientToNode:offset:")
        pub static orientToNode(node: SKNode, offset: SKRange): SKConstraint
        @objc("orientToPoint:offset:")
        pub static orientToPoint(point: NSPoint, offset: SKRange): SKConstraint
        @objc("zRotation:")
        pub static zRotation(range: SKRange): SKConstraint

        @objc("setEnabled:")
        pub setEnabled(flag: bool)
        @objc("enabled")
        pub enabled(): bool
    }

    /// `SKRegion` — 2-D area, attached to `SKFieldNode` /
    /// `SKEmitterNode` (and a few hit-test APIs) to clip the
    /// effect's influence to a shape. Build with the size /
    /// radius factories, then compose via the boolean helpers
    /// (`union` / `intersection` / `difference`).
    @objc pub class SKRegion : NSObject {
        @objc("alloc")
        pub static alloc(): SKRegion
        @objc("infiniteRegion")
        pub static infiniteRegion(): SKRegion
        @objc("initWithRadius:")
        pub initWithRadius(r: f32): SKRegion
        @objc("initWithSize:")
        pub initWithSize(s: NSSize): SKRegion

        @objc("inverseRegion")
        pub inverseRegion(): SKRegion
        @objc("regionByUnionWithRegion:")
        pub regionByUnion(other: SKRegion): SKRegion
        @objc("regionByDifferenceFromRegion:")
        pub regionByDifferenceFrom(other: SKRegion): SKRegion
        @objc("regionByIntersectionWithRegion:")
        pub regionByIntersection(other: SKRegion): SKRegion
        @objc("containsPoint:")
        pub containsPoint(p: NSPoint): bool
    }

    // ─── Inverse kinematics ──────────────────────────────────────

    /// `SKReachConstraints` — per-joint angle limits used by
    /// SpriteKit's IK solver
    /// (`SKAction.reachTo:rootNode:duration:`). Both angles are
    /// in radians; pass `±π` for "no limit".
    @objc pub class SKReachConstraints : NSObject {
        @objc("alloc")
        pub static alloc(): SKReachConstraints
        @objc("initWithLowerAngleLimit:upperAngleLimit:")
        pub init(lower: f64, upperAngleLimit: f64): SKReachConstraints

        @objc("setLowerAngleLimit:")
        pub setLowerAngleLimit(v: f64)
        @objc("setUpperAngleLimit:")
        pub setUpperAngleLimit(v: f64)
        @objc("lowerAngleLimit")
        pub lowerAngleLimit(): f64
        @objc("upperAngleLimit")
        pub upperAngleLimit(): f64
    }

    // ─── Warp geometry ───────────────────────────────────────────

    /// `SKWarpGeometry` — abstract base for the warp meshes
    /// applied to `SKSpriteNode` / `SKEffectNode` via
    /// `setWarpGeometry:`. Concrete shapes are subclasses
    /// (today: `SKWarpGeometryGrid`).
    ///
    /// Apple exposes the warp-enabled node set via the
    /// `SKWarpable` protocol — `SKSpriteNode` and `SKEffectNode`
    /// both adopt it. ilang code doesn't need a separate type
    /// for the protocol; just call `setWarpGeometry:` /
    /// `warpGeometry` directly on those nodes, and pair with
    /// `SKAction.warpTo` for animated transitions.
    @objc pub class SKWarpGeometry : NSObject {}

    /// `SKWarpGeometryGrid` — `columns × rows` quad mesh that
    /// remaps source positions (in normalised 0..1 texture
    /// coords) to destination positions (in normalised 0..1
    /// node coords). The position-array factories take
    /// `const vector_float2 *` (= `simd.f32x2[]`); pass row-
    /// major source / dest arrays of length `(cols+1)×(rows+1)`.
    /// `sourcePositionAtIndex:` / `destPositionAtIndex:` return
    /// `vector_float2`, not `CGPoint` — match Apple's header.
    @objc pub class SKWarpGeometryGrid : SKWarpGeometry {
        @objc("gridWithColumns:rows:")
        pub static gridWithColumns(columns: i64, rows: i64): SKWarpGeometryGrid

        @objc("gridWithColumns:rows:sourcePositions:destPositions:")
        pub static gridWithColumnsRowsSourceDest(
            columns: i64,
            rows: i64,
            source: simd.f32x2[],
            dest: simd.f32x2[]
        ): SKWarpGeometryGrid

        @objc("gridByReplacingSourcePositions:")
        pub gridByReplacingSourcePositions(source: simd.f32x2[]): SKWarpGeometryGrid
        @objc("gridByReplacingDestPositions:")
        pub gridByReplacingDestPositions(dest: simd.f32x2[]): SKWarpGeometryGrid

        @objc("numberOfColumns")
        pub numberOfColumns(): i64
        @objc("numberOfRows")
        pub numberOfRows(): i64
        @objc("vertexCount")
        pub vertexCount(): i64
        @objc("sourcePositionAtIndex:")
        pub sourcePositionAtIndex(i: i64): simd.f32x2
        @objc("destPositionAtIndex:")
        pub destPositionAtIndex(i: i64): simd.f32x2
    }

}
