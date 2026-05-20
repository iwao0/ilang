// Physics — SKPhysicsBody / SKPhysicsWorld / SKPhysicsContact /
// SKPhysicsJoint* and the SKFieldNode region-of-influence node.

use foundation { NSObject, NSArray, NSPoint, NSSize, NSRect, NSVector }
use node { SKNode }
use textures { SKTexture }

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
    /// `SKPhysicsBody` — rigid body attached to an `SKNode` via
    /// `setPhysicsBody:`. The factory class methods build the
    /// shape (rect / circle / edge); the setters configure mass
    /// / damping / categoryBitMask / etc. Set
    /// `setDynamic(false)` for static colliders (walls / floor).
    @objc pub class SKPhysicsBody : NSObject {
        @objc("bodyWithRectangleOfSize:")
        pub static bodyWithRectangleOfSize(s: NSSize): SKPhysicsBody
        @objc("bodyWithCircleOfRadius:")
        pub static bodyWithCircleOfRadius(r: f64): SKPhysicsBody
        @objc("bodyWithEdgeFromPoint:toPoint:")
        pub static bodyWithEdgeFromTo(a: NSPoint, toPoint: NSPoint): SKPhysicsBody
        @objc("bodyWithEdgeLoopFromRect:")
        pub static bodyWithEdgeLoopFromRect(r: NSRect): SKPhysicsBody

        @objc("isDynamic")
        pub get isDynamic(): bool
        @objc("setDynamic:")
        pub set isDynamic(flag: bool)
        @objc("setAffectedByGravity:")
        pub setAffectedByGravity(flag: bool)
        @objc("setAllowsRotation:")
        pub setAllowsRotation(flag: bool)
        @objc("setUsesPreciseCollisionDetection:")
        pub setUsesPreciseCollisionDetection(flag: bool)

        @objc("setMass:")
        pub setMass(m: f64)
        @objc("mass")
        pub mass(): f64
        @objc("setDensity:")
        pub setDensity(d: f64)
        @objc("setFriction:")
        pub setFriction(f: f64)
        @objc("setRestitution:")
        pub setRestitution(r: f64)
        @objc("setLinearDamping:")
        pub setLinearDamping(v: f64)
        @objc("setAngularDamping:")
        pub setAngularDamping(v: f64)

        @objc("setVelocity:")
        pub setVelocity(v: NSVector)
        @objc("velocity")
        pub velocity(): NSVector
        @objc("setAngularVelocity:")
        pub setAngularVelocity(v: f64)

        @objc("setCategoryBitMask:")
        pub setCategoryBitMask(m: u32)
        @objc("setCollisionBitMask:")
        pub setCollisionBitMask(m: u32)
        @objc("setContactTestBitMask:")
        pub setContactTestBitMask(m: u32)
        @objc("setFieldBitMask:")
        pub setFieldBitMask(m: u32)

        @objc("applyForce:")
        pub applyForce(f: NSVector)
        @objc("applyForce:atPoint:")
        pub applyForceAtPoint(f: NSVector, atPoint: NSPoint)
        @objc("applyImpulse:")
        pub applyImpulse(i: NSVector)
        @objc("applyImpulse:atPoint:")
        pub applyImpulseAtPoint(i: NSVector, atPoint: NSPoint)
        @objc("applyTorque:")
        pub applyTorque(t: f64)
        @objc("applyAngularImpulse:")
        pub applyAngularImpulse(i: f64)
    }

    /// `SKPhysicsWorld` — owned by each `SKScene`. Sets the
    /// simulation's gravity / speed; routes contact events to a
    /// delegate.
    ///
    /// The delegate is any ilang `@objc` class implementing the
    /// `SKPhysicsContactDelegate` selectors:
    ///
    /// * `@objc("didBeginContact:")` — `(contact: SKPhysicsContact)`
    /// * `@objc("didEndContact:")` — `(contact: SKPhysicsContact)`
    /// Install via `world.setContactDelegate(delegate)`. Read
    /// the contact off `contact.bodyA()` / `bodyB()` /
    /// `contactPoint()` / `contactNormal()` / `collisionImpulse()`.
    @objc pub class SKPhysicsWorld : NSObject {
        @objc("setGravity:")
        pub setGravity(v: NSVector)
        @objc("gravity")
        pub gravity(): NSVector
        @objc("setSpeed:")
        pub setSpeed(s: f64)
        @objc("setContactDelegate:")
        pub setContactDelegate(d: NSObject)

        @objc("addJoint:")
        pub addJoint(j: SKPhysicsJoint)
        @objc("removeJoint:")
        pub removeJoint(j: SKPhysicsJoint)
        @objc("removeAllJoints")
        pub removeAllJoints()
    }

    /// `SKFieldNode` — physics field that perturbs nearby
    /// dynamic bodies. The class methods cover the most common
    /// kinds; `strength` scales their effect.
    @objc pub class SKFieldNode : SKNode {
        @objc("radialGravityField")
        pub static radialGravityField(): SKFieldNode
        @objc("linearGravityFieldWithVector:")
        pub static linearGravityFieldWithVector(v: NSVector): SKFieldNode
        @objc("vortexField")
        pub static vortexField(): SKFieldNode
        @objc("dragField")
        pub static dragField(): SKFieldNode
        @objc("springField")
        pub static springField(): SKFieldNode
        @objc("electricField")
        pub static electricField(): SKFieldNode
        @objc("magneticField")
        pub static magneticField(): SKFieldNode

        @objc("setStrength:")
        pub setStrength(s: f64)
        @objc("setFalloff:")
        pub setFalloff(f: f64)
        @objc("setEnabled:")
        pub setFieldEnabled(flag: bool)
        @objc("setCategoryBitMask:")
        pub setCategoryBitMask(m: u32)
        @objc("setRegion:")
        pub setRegion(r: SKRegion)
        @objc("region")
        pub region(): SKRegion
    }

    /// `SKPhysicsJoint` — abstract base for the joint classes
    /// below. Holds the two bodies it connects.
    @objc pub class SKPhysicsJoint : NSObject {
        @objc("bodyA")
        pub bodyA(): SKPhysicsBody
        @objc("bodyB")
        pub bodyB(): SKPhysicsBody
    }

    /// `SKPhysicsJointPin` — hinges two bodies around a shared
    /// anchor; bodies rotate around the anchor but can't
    /// translate apart from it.
    @objc pub class SKPhysicsJointPin : SKPhysicsJoint {
        @objc("jointWithBodyA:bodyB:anchor:")
        pub static jointWithBodies(
            a: SKPhysicsBody, bodyB: SKPhysicsBody, anchor: NSPoint
        ): SKPhysicsJointPin

        @objc("setShouldEnableLimits:")
        pub setShouldEnableLimits(flag: bool)
        @objc("setLowerAngleLimit:")
        pub setLowerAngleLimit(v: f64)
        @objc("setUpperAngleLimit:")
        pub setUpperAngleLimit(v: f64)
        @objc("setFrictionTorque:")
        pub setFrictionTorque(v: f64)
        @objc("setRotationSpeed:")
        pub setRotationSpeed(v: f64)
    }

    /// `SKPhysicsJointSpring` — soft constraint between two
    /// anchor points; configurable frequency and damping.
    @objc pub class SKPhysicsJointSpring : SKPhysicsJoint {
        @objc("jointWithBodyA:bodyB:anchorA:anchorB:")
        pub static jointWithBodies(
            a: SKPhysicsBody, bodyB: SKPhysicsBody, anchorA: NSPoint, anchorB: NSPoint
        ): SKPhysicsJointSpring

        @objc("setFrequency:")
        pub setFrequency(v: f64)
        @objc("setDamping:")
        pub setDamping(v: f64)
    }

    /// `SKPhysicsJointFixed` — rigidly bonds two bodies at a
    /// common anchor — they move together as one rigid body.
    @objc pub class SKPhysicsJointFixed : SKPhysicsJoint {
        @objc("jointWithBodyA:bodyB:anchor:")
        pub static jointWithBodies(
            a: SKPhysicsBody, bodyB: SKPhysicsBody, anchor: NSPoint
        ): SKPhysicsJointFixed
    }

    /// `SKPhysicsJointLimit` — caps the distance between two
    /// anchor points to `maxLength`. Bodies can swing around
    /// the anchor but never get further apart.
    @objc pub class SKPhysicsJointLimit : SKPhysicsJoint {
        @objc("jointWithBodyA:bodyB:anchorA:anchorB:")
        pub static jointWithBodies(
            a: SKPhysicsBody, bodyB: SKPhysicsBody, anchorA: NSPoint, anchorB: NSPoint
        ): SKPhysicsJointLimit

        @objc("setMaxLength:")
        pub setMaxLength(v: f64)
        @objc("maxLength")
        pub maxLength(): f64
    }

    /// `SKPhysicsJointSliding` — slider along an axis;
    /// optionally clamps the distance with lower / upper limits.
    @objc pub class SKPhysicsJointSliding : SKPhysicsJoint {
        @objc("jointWithBodyA:bodyB:anchor:axis:")
        pub static jointWithBodies(
            a: SKPhysicsBody, bodyB: SKPhysicsBody, anchor: NSPoint, axis: NSVector
        ): SKPhysicsJointSliding

        @objc("setShouldEnableLimits:")
        pub setShouldEnableLimits(flag: bool)
        @objc("setLowerDistanceLimit:")
        pub setLowerDistanceLimit(v: f64)
        @objc("setUpperDistanceLimit:")
        pub setUpperDistanceLimit(v: f64)
    }

    /// `SKPhysicsContact` — read-only contact record passed to
    /// `SKPhysicsContactDelegate.didBeginContact:` /
    /// `…didEndContact:`. The fields are derived from the
    /// physics step; don't mutate.
    @objc pub class SKPhysicsContact : NSObject {
        @objc("bodyA")
        pub bodyA(): SKPhysicsBody
        @objc("bodyB")
        pub bodyB(): SKPhysicsBody
        @objc("contactPoint")
        pub contactPoint(): NSPoint
        @objc("contactNormal")
        pub contactNormal(): NSVector
        @objc("collisionImpulse")
        pub collisionImpulse(): f64
    }

}
