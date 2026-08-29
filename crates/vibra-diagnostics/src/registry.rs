//! The closed diagnostic registry.
//!
//! `docs/spec/07-diagnostics-and-conformance.md` carries a canonical table of
//! every diagnostic code and its fixed level. The registry maps each code to
//! that level plus its domain, summary, and fix capability. It is compiler
//! data: queryable and covered by tests, never loaded from Vibra source, and
//! never configurable by a project.
//!
//! The registry is a closed Rust enum rather than a parsed string, so "every
//! diagnostic code is unique" — a release-gate requirement — holds by
//! construction rather than by test.

use std::fmt;
use std::str::FromStr;

/// The level a diagnostic carries.
///
/// Fixed per code by the specification and not configurable by a project. A
/// command may fail on warnings by policy without changing this.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// The construct is rejected.
    Error,
    /// The construct is accepted, and normalized where a fix applies.
    Warning,
}

impl Level {
    /// The exact atom spelling, as serialized to JSON.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Error => "@error",
            Self::Warning => "@warning",
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_atom())
    }
}

/// Whether the compiler can offer a fix for a code.
///
/// The specification fixes this vocabulary at exactly two values, because v1
/// has no command that would apply a fix requiring human review. Unlike a
/// level, a code's capability describes what the implementation can currently
/// do and may widen from [`Self::None`] to [`Self::Safe`] as fixes are added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FixCapability {
    /// A fix may be offered, and `vibra edit fix` may apply it.
    Safe,
    /// No fix is ever offered.
    None,
}

impl FixCapability {
    /// The exact atom spelling, as serialized to JSON.
    #[must_use]
    pub const fn as_atom(self) -> &'static str {
        match self {
            Self::Safe => "@safe",
            Self::None => "@none",
        }
    }
}

impl fmt::Display for FixCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_atom())
    }
}

/// The first component of a diagnostic code's spelling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Domain {
    /// Reader, tokens, and document grammar.
    Syntax,
    /// Resolution, namespaces, and reserved spellings.
    Name,
    /// Module layout and paths.
    Module,
    /// The type system.
    Type,
    /// Pattern irrefutability.
    Pattern,
    /// Effect rows and ceilings.
    Effect,
    /// External declarations and their provider registries.
    External,
    /// VIBON data documents.
    Data,
    /// Projects, targets, dependencies, and locks.
    Project,
    /// Execution and the host ABI.
    Runtime,
    /// Canonical presentation.
    Style,
    /// Declared contracts that hold but say more than they need to.
    Contract,
}

impl Domain {
    /// The domain's spelling, without the leading `@` of a code.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Name => "name",
            Self::Module => "module",
            Self::Type => "type",
            Self::Pattern => "pattern",
            Self::Effect => "effect",
            Self::External => "external",
            Self::Data => "data",
            Self::Project => "project",
            Self::Runtime => "runtime",
            Self::Style => "style",
            Self::Contract => "contract",
        }
    }
}

impl fmt::Display for Domain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A code that is not in the closed registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnknownDiagnosticCode;

impl fmt::Display for UnknownDiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("not a diagnostic code in the v1 registry")
    }
}

impl std::error::Error for UnknownDiagnosticCode {}

/// Declares the closed registry from one table.
///
/// One row per code keeps the Rust registry laid out the same way the
/// specification's canonical table is, so the two can be read side by side.
macro_rules! diagnostic_registry {
    ($($variant:ident => $atom:literal, $domain:ident, $level:ident, $fix:ident,
       $summary:literal;)*) => {
        /// Every diagnostic code in the v1 line.
        ///
        /// The set is closed and its spellings are stable within v1.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum DiagnosticCode {
            $(
                #[doc = $summary]
                $variant,
            )*
        }

        impl DiagnosticCode {
            /// Every registered code, in specification table order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),*];

            /// The exact atom spelling, as serialized to JSON.
            #[must_use]
            pub const fn as_atom(self) -> &'static str {
                match self { $(Self::$variant => $atom,)* }
            }

            /// The code's domain.
            #[must_use]
            pub const fn domain(self) -> Domain {
                match self { $(Self::$variant => Domain::$domain,)* }
            }

            /// The code's fixed level.
            #[must_use]
            pub const fn level(self) -> Level {
                match self { $(Self::$variant => Level::$level,)* }
            }

            /// Whether the compiler can offer a fix for this code.
            #[must_use]
            pub const fn fix_capability(self) -> FixCapability {
                match self { $(Self::$variant => FixCapability::$fix,)* }
            }

            /// A one-line description of the condition.
            #[must_use]
            pub const fn summary(self) -> &'static str {
                match self { $(Self::$variant => $summary,)* }
            }
        }

        impl FromStr for DiagnosticCode {
            type Err = UnknownDiagnosticCode;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                match text {
                    $($atom => Ok(Self::$variant),)*
                    _ => Err(UnknownDiagnosticCode),
                }
            }
        }
    };
}

diagnostic_registry! {
    SyntaxUnmatchedDelimiter => "@syntax.unmatched-delimiter", Syntax, Error, None,
        "a list is not closed before the end of the document";
    SyntaxInvalidCharacterLiteral => "@syntax.invalid-character-literal", Syntax, Error, None,
        "a character literal is not one valid EDN character spelling";
    SyntaxInvalidNumericLiteral => "@syntax.invalid-numeric-literal", Syntax, Error, None,
        "a numeric literal has a malformed or unknown suffix, or trailing text";
    SyntaxRetiredForm => "@syntax.retired-form", Syntax, Error, None,
        "a form retired from v1, such as `while` or `return`, was written";
    NameUnknownSymbol => "@name.unknown-symbol", Name, Error, None,
        "a symbol does not resolve to a visible entity";
    NameWrongEntityKind => "@name.wrong-entity-kind", Name, Error, None,
        "a resolved entity is not the kind this position requires";
    NameMemberCollision => "@name.member-collision", Name, Error, None,
        "two members of one owner share a name in its flat member namespace";
    NameGenericRedeclaration => "@name.generic-redeclaration", Name, Error, None,
        "a nested declaration redeclares a generic name its owner already binds";
    NameReservedLabel => "@name.reserved-label", Name, Error, None,
        "a label uses a spelling the grammar reserves";
    NameReservedDeclaration => "@name.reserved-declaration", Name, Error, None,
        "a declaration or generic name uses a reserved type head";
    NameReservedValueSpelling => "@name.reserved-value-spelling", Name, Error, None,
        "a module-level value is spelled `map`, `array`, or `tuple`";
    ModuleFileDirectoryCollision => "@module.file-directory-collision", Module, Error, None,
        "a module path is claimed by both a file and a directory";
    ModuleUnknownPath => "@module.unknown-path", Module, Error, None,
        "a module path does not resolve to a source unit";
    TypeArgumentMismatch => "@type.argument-mismatch", Type, Error, None,
        "an operand does not match the parameter it binds to";
    TypeTypeArgumentMismatch => "@type.type-argument-mismatch", Type, Error, None,
        "a `types:` list does not match the addressed entity's parameter list";
    TypeRedundantImplementation => "@type.redundant-implementation", Type, Error, None,
        "an implementation duplicates one already supplied";
    TypeDefaultOverride => "@type.default-override", Type, Error, None,
        "an implementation supplies a member the contract defines as a default";
    TypeMissingAbstractMember => "@type.missing-abstract-member", Type, Error, None,
        "an implementation omits an abstract contract member";
    TypeFunctionNotEquatable => "@type.function-not-equatable", Type, Error, None,
        "a function value is used where equality is required";
    TypeNotApplicable => "@type.not-applicable", Type, Error, None,
        "a value of this type cannot head an application";
    TypeInvalidTupleIndex => "@type.invalid-tuple-index", Type, Error, None,
        "a tuple index is out of bounds or is not a literal";
    TypeUnknownRecordField => "@type.unknown-record-field", Type, Error, None,
        "an atom selector names no field of this record";
    TypeNumericOutOfRange => "@type.numeric-out-of-range", Type, Error, None,
        "a literal lies outside the range of its suffixed type";
    TypeAnonymousTypeBody => "@type.anonymous-type-body", Type, Error, None,
        "`record`, `enum`, `union`, or `newtype` appears outside a declaration body";
    TypeUndispatchableContractMember => "@type.undispatchable-contract-member", Type, Error, None,
        "a contract member does not name `self` in a dispatchable position";
    TypeUnionTooFewMembers => "@type.union-too-few-members", Type, Error, None,
        "a union body declares fewer than two members";
    TypeUnionMemberOverlap => "@type.union-member-overlap", Type, Error, None,
        "two union members are unifiable";
    TypeUnionMemberNotConcrete => "@type.union-member-not-concrete", Type, Error, None,
        "a union member is a union, an interface, or a bare generic";
    TypeOverlappingImplementation => "@type.overlapping-implementation", Type, Error, None,
        "two implementations on one receiver overlap under instantiation";
    TypeAmbiguousImplementation => "@type.ambiguous-implementation", Type, Error, None,
        "a call matches more than one implementation";
    TypeAmbiguousDestination => "@type.ambiguous-destination", Type, Error, None,
        "a destination-dispatched member has no written expected type";
    TypeInvalidAscription => "@type.invalid-ascription", Type, Error, None,
        "`as` names a type the operand does not admit";
    TypeNarrowingNonUnion => "@type.narrowing-non-union", Type, Error, None,
        "an `as` pattern is written against a non-union scrutinee";
    TypeNotAUnionMember => "@type.not-a-union-member", Type, Error, None,
        "an `as` pattern names a type outside the union's member set";
    TypeRedundantConversion => "@type.redundant-conversion", Type, Error, None,
        "`from` and `try-from` are both implemented for one source";
    PatternRefutableBinding => "@pattern.refutable-binding", Pattern, Error, None,
        "a binding pattern is refutable for its expected type";
    EffectOutsideCeiling => "@effect.outside-ceiling", Effect, Error, None,
        "a performed effect root lies outside the written or default ceiling";
    EffectInvalidReference => "@effect.invalid-reference", Effect, Error, None,
        "an effect row entry is not a lexical symbol resolved through imports";
    ExternalUnknownSymbol => "@external.unknown-symbol", External, Error, None,
        "an external declaration names no symbol in its provider registry";
    DataInvalidExtension => "@data.invalid-extension", Data, Error, None,
        "a document was presented to the loader for the other grammar";
    ProjectStaleLock => "@project.stale-lock", Project, Error, None,
        "the lock does not match the declared dependencies";
    ProjectEntryOutsideTarget => "@project.entry-outside-target", Project, Error, None,
        "an `entry` declaration resolves outside its target root";
    ProjectEntryOnLibrary => "@project.entry-on-library", Project, Error, None,
        "a library target declares an `entry`";
    ProjectInvalidEntrySignature => "@project.invalid-entry-signature", Project, Error, None,
        "an `entry` declaration does not have the required signature";
    ProjectAmbiguousDependencyTarget => "@project.ambiguous-dependency-target", Project, Error, None,
        "a dependency alias does not bind exactly one `@lib` target";
    ProjectOverlappingTargetRoots => "@project.overlapping-target-roots", Project, Error, None,
        "two target roots nest or coincide";
    RuntimeInvalidHostValue => "@runtime.invalid-host-value", Runtime, Error, None,
        "a host operation received or returned a value its ABI does not admit";
    StyleArgumentOrder => "@style.argument-order", Style, Warning, Safe,
        "operands are in a noncanonical but unambiguous order";
    ContractUnusedEffect => "@contract.unused-effect", Contract, Warning, None,
        "a declared effect root is never performed";
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_atom())
    }
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticCode, Domain, FixCapability, Level, UnknownDiagnosticCode};
    use std::collections::BTreeSet;

    /// The count in the specification's canonical table.
    const REGISTERED_CODES: usize = 49;

    #[test]
    fn the_registry_has_every_code_in_the_specification_table() {
        assert_eq!(DiagnosticCode::ALL.len(), REGISTERED_CODES);
    }

    #[test]
    fn every_code_spelling_is_unique() {
        let spellings: BTreeSet<&str> = DiagnosticCode::ALL
            .iter()
            .map(|code| code.as_atom())
            .collect();
        assert_eq!(
            spellings.len(),
            DiagnosticCode::ALL.len(),
            "the release gate requires every diagnostic code to be unique"
        );
    }

    #[test]
    fn every_code_is_spelled_as_an_atom_of_two_kebab_components() {
        for code in DiagnosticCode::ALL {
            let atom = code.as_atom();
            let path = atom
                .strip_prefix('@')
                .unwrap_or_else(|| panic!("{atom} does not start with @"));

            let components: Vec<&str> = path.split('.').collect();
            assert_eq!(components.len(), 2, "{atom} is not `@<domain>.<thing>`");

            for component in components {
                assert!(!component.is_empty(), "{atom} has an empty component");
                assert!(
                    component.starts_with(|first: char| first.is_ascii_lowercase()),
                    "{atom} has a component not starting with a lowercase letter"
                );
                assert!(
                    component
                        .chars()
                        .all(|character| character.is_ascii_lowercase()
                            || character == '-'),
                    "{atom} has a component that is not kebab-case"
                );
                assert!(!component.ends_with('-'), "{atom} has a trailing hyphen");
            }
        }
    }

    #[test]
    fn every_code_domain_matches_the_first_component_of_its_spelling() {
        // The specification states this directly, and the conformance corpus
        // asserts it too. Getting it wrong would make `vibra query` disagree
        // with the code it was asked about.
        for code in DiagnosticCode::ALL {
            let atom = code.as_atom();
            let spelled = atom
                .strip_prefix('@')
                .and_then(|path| path.split('.').next())
                .unwrap_or_else(|| panic!("{atom} has no domain component"));
            assert_eq!(code.domain().as_str(), spelled, "for {atom}");
        }
    }

    #[test]
    fn every_code_has_a_nonempty_summary() {
        for code in DiagnosticCode::ALL {
            assert!(
                !code.summary().trim().is_empty(),
                "{} has no summary",
                code.as_atom()
            );
        }
    }

    #[test]
    fn exactly_two_codes_are_warnings() {
        // The specification's table fixes 47 errors and 2 warnings. Pinning
        // the split catches a level silently flipping in either direction.
        let warnings: Vec<&str> = DiagnosticCode::ALL
            .iter()
            .filter(|code| code.level() == Level::Warning)
            .map(|code| code.as_atom())
            .collect();
        assert_eq!(
            warnings,
            ["@style.argument-order", "@contract.unused-effect"]
        );
    }

    #[test]
    fn levels_the_specification_states_in_prose_match_the_registry() {
        // These are the codes whose level the chapter restates outside the
        // canonical table. The table governs, so the two must agree.
        for atom in [
            "@syntax.invalid-character-literal",
            "@syntax.invalid-numeric-literal",
            "@type.numeric-out-of-range",
            "@effect.invalid-reference",
            "@data.invalid-extension",
            "@type.not-applicable",
            "@type.invalid-tuple-index",
            "@type.unknown-record-field",
            "@pattern.refutable-binding",
            "@name.reserved-value-spelling",
            "@type.function-not-equatable",
            "@type.anonymous-type-body",
            "@type.undispatchable-contract-member",
            "@type.union-too-few-members",
            "@type.union-member-overlap",
            "@type.union-member-not-concrete",
            "@type.overlapping-implementation",
            "@type.ambiguous-implementation",
            "@type.ambiguous-destination",
            "@type.invalid-ascription",
            "@type.narrowing-non-union",
            "@type.not-a-union-member",
            "@type.redundant-conversion",
        ] {
            let code: DiagnosticCode = atom
                .parse()
                .unwrap_or_else(|_| panic!("{atom} is not registered"));
            assert_eq!(code.level(), Level::Error, "for {atom}");
        }
    }

    #[test]
    fn only_a_code_with_a_safe_fix_advertises_one() {
        // Today the formatter's operand reordering is the one fix the
        // compiler can offer. This will widen as fixes are added; it should
        // never widen by accident.
        let fixable: Vec<&str> = DiagnosticCode::ALL
            .iter()
            .filter(|code| code.fix_capability() == FixCapability::Safe)
            .map(|code| code.as_atom())
            .collect();
        assert_eq!(fixable, ["@style.argument-order"]);
    }

    #[test]
    fn a_code_round_trips_through_its_atom_spelling() {
        for code in DiagnosticCode::ALL {
            let parsed: DiagnosticCode = code
                .as_atom()
                .parse()
                .unwrap_or_else(|_| panic!("{} does not round trip", code.as_atom()));
            assert_eq!(parsed, *code);
        }
    }

    #[test]
    fn an_unregistered_code_does_not_parse() {
        assert_eq!(
            "@type.no-such-condition".parse::<DiagnosticCode>(),
            Err(UnknownDiagnosticCode)
        );
        assert_eq!("".parse::<DiagnosticCode>(), Err(UnknownDiagnosticCode));
        assert_eq!(
            "type.argument-mismatch".parse::<DiagnosticCode>(),
            Err(UnknownDiagnosticCode),
            "the leading @ is part of the spelling"
        );
    }

    #[test]
    fn levels_and_capabilities_serialize_as_their_atoms() {
        assert_eq!(Level::Error.as_atom(), "@error");
        assert_eq!(Level::Warning.as_atom(), "@warning");
        assert_eq!(FixCapability::Safe.as_atom(), "@safe");
        assert_eq!(FixCapability::None.as_atom(), "@none");
    }

    #[test]
    fn a_code_displays_as_its_atom() {
        assert_eq!(
            DiagnosticCode::TypeArgumentMismatch.to_string(),
            "@type.argument-mismatch"
        );
        assert_eq!(Domain::Type.to_string(), "type");
        assert_eq!(Level::Error.to_string(), "@error");
    }
}
