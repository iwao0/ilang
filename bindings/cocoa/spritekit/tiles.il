// Tile map family — SKTileDefinition / SKTileGroup / SKTileSet /
// SKTileMapNode / SKTileGroupRule.

use foundation { NSObject, NSString, NSArray, NSSize, NSPoint }
use node { SKNode }
use textures { SKTexture }

@extern(ObjC, "/System/Library/Frameworks/SpriteKit.framework/SpriteKit") {
    /// `SKTileDefinition` — one paintable tile, backed by an
    /// `SKTexture`. Carries the tile's pixel size and optional
    /// debug name.
    @objc pub class SKTileDefinition : NSObject {
        @objc("tileDefinitionWithTexture:")
        pub static tileDefinitionWithTexture(t: SKTexture): SKTileDefinition
        @objc("tileDefinitionWithTexture:size:")
        pub static tileDefinitionWithTextureAndSize(t: SKTexture, size: NSSize): SKTileDefinition

        @objc("setSize:")
        pub setSize(s: NSSize)
        @objc("size")
        pub size(): NSSize
        @objc("setName:")
        pub setName(n: NSString)
        @objc("name")
        pub name(): NSString
    }

    /// `SKTileGroup` — paintable cluster of tile definitions
    /// (typically one definition for the simple case, multiple
    /// rules for auto-tiling). Painted into an `SKTileMapNode`
    /// at a (column, row) cell.
    @objc pub class SKTileGroup : NSObject {
        @objc("tileGroupWithTileDefinition:")
        pub static tileGroupWithTileDefinition(def: SKTileDefinition): SKTileGroup
        @objc("tileGroupWithRules:")
        pub static tileGroupWithRules(rules: NSArray): SKTileGroup
        @objc("emptyTileGroup")
        pub static emptyTileGroup(): SKTileGroup

        @objc("setName:")
        pub setGroupName(n: NSString)
        @objc("rules")
        pub rules(): NSArray
    }

    /// `SKTileSet` — collection of `SKTileGroup`s loaded from
    /// the asset catalog. Pass into `SKTileMapNode`'s factory to
    /// pick the brush palette.
    @objc pub class SKTileSet : NSObject {
        @objc("tileSetNamed:")
        pub static tileSetNamed(name: NSString): SKTileSet
        @objc("tileSetWithTileGroups:")
        pub static tileSetWithTileGroups(groups: NSArray): SKTileSet

        @objc("tileGroups")
        pub tileGroups(): NSArray
        @objc("setName:")
        pub setSetName(n: NSString)
        @objc("setDefaultTileSize:")
        pub setDefaultTileSize(s: NSSize)
    }

    /// `SKTileMapNode` — grid of tiles drawn from one or more
    /// `SKTileGroup`s. Build via
    /// `tileMapNodeWithTileSet:columns:rows:tileSize:`, then
    /// paint cells with `setTileGroup:forColumn:row:`.
    @objc pub class SKTileMapNode : SKNode {
        @objc("tileMapNodeWithTileSet:columns:rows:tileSize:")
        pub static tileMapNodeWithTileSet(
            tileSet: SKTileSet, columns: i64, rows: i64, tileSize: NSSize
        ): SKTileMapNode

        @objc("numberOfColumns")
        pub numberOfColumns(): i64
        @objc("numberOfRows")
        pub numberOfRows(): i64
        @objc("tileSize")
        pub tileSize(): NSSize
        @objc("mapSize")
        pub mapSize(): NSSize

        @objc("setTileGroup:forColumn:row:")
        pub setTileGroup(g: SKTileGroup, forColumn: i64, row: i64)
        @objc("tileGroupAtColumn:row:")
        pub tileGroupAtColumn(col: i64, row: i64): SKTileGroup
        @objc("fillWithTileGroup:")
        pub fillWithTileGroup(g: SKTileGroup)
        @objc("setEnableAutomapping:")
        pub setEnableAutomapping(flag: bool)
    }

    /// `SKTileGroupRule` — one entry in an `SKTileGroup`'s
    /// `rules` array driving auto-tiling. Pair the adjacency
    /// bitmask (combine `cocoa.TileAdjacency*` constants) with
    /// the tile definitions to use when neighbouring cells
    /// match.
    @objc pub class SKTileGroupRule : NSObject {
        @objc("tileGroupRuleWithAdjacency:tileDefinitions:")
        pub static tileGroupRuleWithAdjacency(
            adjacency: u32, tileDefinitions: NSArray
        ): SKTileGroupRule

        @objc("setAdjacency:")
        pub setAdjacency(a: u32)
        @objc("adjacency")
        pub adjacency(): u32
        @objc("setTileDefinitions:")
        pub setTileDefinitions(defs: NSArray)
        @objc("tileDefinitions")
        pub tileDefinitions(): NSArray
        @objc("setName:")
        pub setRuleName(n: NSString)
        @objc("name")
        pub ruleName(): NSString
    }

}
