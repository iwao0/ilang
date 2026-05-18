// Predicate / expression / linguistic-tagger bindings —
// `NSArray.filteredArrayUsingPredicate:` is the canonical user
// of the predicate types; expressions and the tagger are
// standalone tools.
//
// The variadic `+predicateWithFormat:` factory is intentionally
// not bound (ilang doesn't carry the ObjC `va_list` plumbing).
// Use `+predicateWithFormat:argumentArray:` and pass an NSArray
// of substitutions instead.

use core { NSObject, NSString, NSArray, NSDictionary }

// NSCompoundPredicateType — AND / OR / NOT combinator for
// NSCompoundPredicate. The numeric values come straight from
// `<Foundation/NSCompoundPredicate.h>`.
pub enum CompoundPredicateType: u64 {
    not = 0
    and = 1
    or  = 2
}

// NSExpressionType — what kind of expression an NSExpression
// holds. Common ones are `constantValue` (literal) and `keyPath`
// (KVC traversal).
pub enum ExpressionType: u64 {
    constantValue   = 0
    evaluatedObject = 1
    variable        = 2
    keyPath         = 3
    function        = 4
    unionSet        = 5
    intersectSet    = 6
    minusSet        = 7
    subquery        = 13
    aggregate       = 14
    anyKey          = 15
    block           = 19
    conditional     = 20
}

// NSLinguisticTaggerOptions — bit flags for an NSLinguisticTagger
// run controlling which tokens it emits.
@flags
pub enum LinguisticTaggerOptions: u64 {
    none                 = 0x00
    omitWords            = 0x01
    omitPunctuation      = 0x02
    omitWhitespace       = 0x04
    omitOther            = 0x08
    joinNames            = 0x10
}

@extern(ObjC, "/System/Library/Frameworks/Foundation.framework/Foundation") {
    /// `NSPredicate` — abstract base for `NSCompoundPredicate` /
    /// `NSComparisonPredicate`. Use the argument-array form
    /// (`predicateWithFormat(...)` below) to dodge the variadic
    /// C API; the substitutions in `args` fill in for `%@` / `%K`
    /// placeholders in `format`.
    @objc pub class NSPredicate : NSObject {
        @objc("predicateWithFormat:argumentArray:")
        pub static predicateWithFormat(
            format: NSString, args: NSArray
        ): NSPredicate
        @objc("predicateWithValue:")
        pub static predicateWithValue(value: bool): NSPredicate

        @objc("predicateFormat")
        pub get predicateFormat(): NSString

        @objc("evaluateWithObject:")
        pub evaluateWithObject(obj: NSObject): bool
        @objc("evaluateWithObject:substitutionVariables:")
        pub evaluateWithObjectAndSubs(
            obj: NSObject, substitutionVariables: NSDictionary
        ): bool

        @objc("predicateWithSubstitutionVariables:")
        pub predicateWithSubstitutionVariables(
            vars: NSDictionary
        ): NSPredicate
    }

    /// `NSCompoundPredicate` — AND / OR / NOT of subpredicates.
    /// Builds the boolean combinator tree most query-style
    /// filtering uses.
    @objc pub class NSCompoundPredicate : NSPredicate {
        @objc("andPredicateWithSubpredicates:")
        pub static andPredicate(subs: NSArray): NSCompoundPredicate
        @objc("orPredicateWithSubpredicates:")
        pub static orPredicate(subs: NSArray): NSCompoundPredicate
        @objc("notPredicateWithSubpredicate:")
        pub static notPredicate(sub: NSPredicate): NSCompoundPredicate

        @objc("compoundPredicateType")
        pub get compoundPredicateType(): CompoundPredicateType
        @objc("subpredicates")
        pub get subpredicates(): NSArray
    }

    /// `NSComparisonPredicate` — the leaf of the predicate tree.
    /// Holds a left-side `NSExpression`, a right-side
    /// `NSExpression`, the comparison operator
    /// (`<` / `<=` / `>=` / `>` / `==` / `!=` / `contains` /
    /// `between` / `beginsWith` / `endsWith` / `like` / `matches`
    /// / `in`), and modifiers (case-insensitive, etc.). Build
    /// directly with the static factory when format-string
    /// parsing isn't expressive enough.
    @objc pub class NSComparisonPredicate : NSPredicate {
        @objc("predicateWithLeftExpression:rightExpression:modifier:type:options:")
        pub static predicate(
            left: NSExpression, rightExpression: NSExpression,
            modifier: u64, type: u64, options: u64
        ): NSComparisonPredicate

        @objc("leftExpression")
        pub get leftExpression(): NSExpression
        @objc("rightExpression")
        pub get rightExpression(): NSExpression
        @objc("predicateOperatorType")
        pub get predicateOperatorType(): u64
        @objc("comparisonPredicateModifier")
        pub get comparisonPredicateModifier(): u64
        @objc("options")
        pub get options(): u64
    }

    /// `NSExpression` — a piece of a predicate that produces a
    /// value (constant, key-path traversal, function call, …).
    /// `expressionValueWithObject:context:` evaluates it against
    /// a subject; `expressionForKeyPath:` is the bread-and-butter
    /// constructor for KVC-style references.
    @objc pub class NSExpression : NSObject {
        @objc("expressionForConstantValue:")
        pub static expressionForConstantValue(v: NSObject): NSExpression
        @objc("expressionForKeyPath:")
        pub static expressionForKeyPath(p: NSString): NSExpression
        @objc("expressionForVariable:")
        pub static expressionForVariable(v: NSString): NSExpression
        @objc("expressionForEvaluatedObject")
        pub static expressionForEvaluatedObject(): NSExpression
        @objc("expressionForAggregate:")
        pub static expressionForAggregate(collection: NSArray): NSExpression
        @objc("expressionForFunction:arguments:")
        pub static expressionForFunction(
            name: NSString, arguments: NSArray
        ): NSExpression

        @objc("expressionType")
        pub get expressionType(): ExpressionType
        @objc("constantValue")
        pub get constantValue(): NSObject
        @objc("keyPath")
        pub get keyPath(): NSString
        @objc("function")
        pub get function(): NSString
        @objc("variable")
        pub get variable(): NSString
        @objc("arguments")
        pub get arguments(): NSArray

        @objc("expressionValueWithObject:context:")
        pub expressionValueWithObject(
            obj: NSObject, context: NSDictionary
        ): NSObject
    }

    /// `NSLinguisticTagger` — pre-Natural-Language-framework
    /// linguistic analysis: tokenisation, language ID, POS /
    /// lemma tagging. Construct with the schemes you care about
    /// (`NSLinguisticTagSchemeTokenType`,
    /// `NSLinguisticTagSchemeLanguage`, etc.); set `.string` to
    /// the text to tag, then iterate
    /// `enumerateTagsInRange:scheme:options:usingBlock:` (which
    /// takes a block we haven't bound — for now reach for
    /// `tagAtIndex:scheme:tokenRange:sentenceRange:` for
    /// per-position queries instead).
    @objc pub class NSLinguisticTagger : NSObject {
        @objc("alloc")
        pub static alloc(): NSLinguisticTagger
        @objc("initWithTagSchemes:options:")
        pub initWithTagSchemes(
            schemes: NSArray, options: LinguisticTaggerOptions
        ): NSLinguisticTagger

        @objc("string")
        pub get string(): NSString
        @objc("setString:")
        pub set string(s: NSString)

        @objc("tagSchemes")
        pub get tagSchemes(): NSArray

        @objc("dominantLanguage")
        pub get dominantLanguage(): NSString
        @objc("availableTagSchemesForLanguage:")
        pub static availableTagSchemesForLanguage(lang: NSString): NSArray
    }
}
