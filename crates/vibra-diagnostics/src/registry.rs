//! The closed diagnostic registry.
//!
//! `docs/spec/07-diagnostics-and-conformance.md`, "Diagnostics are a language
//! surface", carries the table of every diagnostic code and its fixed level.
//! The registry maps each code to that level plus its domain, summary, and fix
//! capability. It is compiler data: queryable and covered by tests, never
//! loaded from Vibra source, and never configurable by a project.
//!
//! The registry is a closed Rust enum rather than a parsed string, so "every
//! diagnostic code is unique" holds by construction rather than by test. The
//! rows below are in the order of the specification table, which is also the
//! registry order that breaks ties when diagnostics are sorted.

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
    /// The construct is accepted, and the diagnostic reports it.
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
/// The specification fixes this vocabulary at exactly two values; there is no
/// "review needed" level between them. Unlike a level, a code's capability
/// describes what the implementation can currently do and may widen from
/// [`Self::None`] to [`Self::Safe`] as fixes are added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FixCapability {
    /// A fix may be offered and applied automatically.
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
    /// Tokens, the source grammar, and recovery.
    Syntax,
    /// Resolution, visibility, and reserved spellings.
    Name,
    /// Module layout, paths, and imports.
    Module,
    /// The type system and interfaces.
    Type,
    /// Pattern irrefutability and coverage.
    Pattern,
    /// Effect rows, ceilings, and `await`.
    Effect,
    /// External declarations and their registries.
    External,
    /// VIBON data documents.
    Data,
    /// Projects, targets, dependencies, and locks.
    Project,
    /// Execution defects and host events.
    Runtime,
    /// Naming conventions.
    Style,
    /// Guards, predicates, consequences, and written ceilings.
    Contract,
    /// Grant bindings and policies.
    Grant,
    /// Tool commands, limits, and builds.
    Tool,
}

impl Domain {
    /// Every domain, in the order of its first code in the registry.
    pub const ALL: &'static [Self] = &[
        Self::Syntax,
        Self::Name,
        Self::Module,
        Self::Type,
        Self::Pattern,
        Self::Effect,
        Self::External,
        Self::Data,
        Self::Project,
        Self::Runtime,
        Self::Style,
        Self::Contract,
        Self::Grant,
        Self::Tool,
    ];

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
            Self::Grant => "grant",
            Self::Tool => "tool",
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
        formatter.write_str("not a diagnostic code of the registry")
    }
}

impl std::error::Error for UnknownDiagnosticCode {}

/// Declares the closed registry from one table.
///
/// One row per code keeps the Rust registry laid out the same way the
/// specification's table is, so the two can be read side by side.
macro_rules! diagnostic_registry {
    ($($variant:ident => $atom:literal, $domain:ident, $level:ident, $fix:ident,
       $summary:literal;)*) => {
        /// Every diagnostic code of the registry.
        ///
        /// The set is closed. A retired code is never reused for another
        /// meaning.
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
            ///
            /// The wording is compiler data, not fixed text.
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
    SyntaxUnmatchedDelimiter => "@syntax.unmatchedDelimiter", Syntax, Error, None,
        "a closer does not match the innermost open construct, a construct is never closed, or a quote is unterminated";
    SyntaxMissingSeparator => "@syntax.missingSeparator", Syntax, Error, None,
        "two adjacent tokens that need trivia between them have none";
    SyntaxInvalidCharacterLiteral => "@syntax.invalidCharacterLiteral", Syntax, Error, None,
        "a character literal is empty, holds several scalars, or holds a surrogate";
    SyntaxInvalidStringLiteral => "@syntax.invalidStringLiteral", Syntax, Error, None,
        "a terminated string has a malformed or unsupported escape";
    SyntaxInvalidNumericLiteral => "@syntax.invalidNumericLiteral", Syntax, Error, None,
        "a numeric literal has an unknown suffix, an integer suffix on a float body, or trailing text";
    SyntaxInvalidName => "@syntax.invalidName", Syntax, Error, None,
        "an identifier is malformed";
    SyntaxInvalidForm => "@syntax.invalidForm", Syntax, Error, None,
        "the grammar does not admit a form in this position";
    SyntaxMisplacedBinding => "@syntax.misplacedBinding", Syntax, Error, None,
        "a `let` is written where a statement is not allowed";
    SyntaxDuplicateSection => "@syntax.duplicateSection", Syntax, Error, None,
        "a declaration repeats a section";
    SyntaxInvalidEncoding => "@syntax.invalidEncoding", Syntax, Error, None,
        "a document has a byte-order mark, invalid UTF-8, or a NUL";
    SyntaxInvalidSection => "@syntax.invalidSection", Syntax, Error, None,
        "a declaration has a section it does not admit";
    NameUnknownSymbol => "@name.unknownSymbol", Name, Error, None,
        "a name resolves to nothing";
    NameWrongEntityKind => "@name.wrongEntityKind", Name, Error, None,
        "a name resolves to an entity of the wrong kind for its position";
    NamePrivateAccess => "@name.privateAccess", Name, Error, None,
        "a reference reaches a private declaration";
    NameInaccessibleRepresentation => "@name.inaccessibleRepresentation", Name, Error, None,
        "a hidden constructor, pattern, or field read is used";
    NameRedeclaration => "@name.redeclaration", Name, Error, None,
        "a name is introduced twice in one scope, or shadows another";
    NameMemberCollision => "@name.memberCollision", Name, Error, None,
        "two members of one owner share a name";
    NameGenericRedeclaration => "@name.genericRedeclaration", Name, Error, None,
        "a type parameter is repeated, or an associated type takes the name of a visible type parameter";
    NameReservedLabel => "@name.reservedLabel", Name, Error, None,
        "a reserved word is written in a label position";
    NameReservedDeclaration => "@name.reservedDeclaration", Name, Error, None,
        "a reserved word or prelude name is declared or bound";
    NameReservedValueSpelling => "@name.reservedValueSpelling", Name, Error, None,
        "a prelude spelling is used for a module value, function, or import";
    ModuleFileDirectoryCollision => "@module.fileDirectoryCollision", Module, Error, None,
        "a `.vib` file lies beside a directory of the same name";
    ModuleSourceIdCollision => "@module.sourceIdCollision", Module, Error, None,
        "one source identity belongs to two modules";
    ModuleCaseCollision => "@module.caseCollision", Module, Error, None,
        "two source paths differ only by case";
    ModuleTestOutsideTests => "@module.testOutsideTests", Module, Error, None,
        "a test is declared outside the tests unit";
    ModuleAssertionOutsideTests => "@module.assertionOutsideTests", Module, Error, None,
        "an assertion operation is used outside the tests unit";
    ModuleUnknownPath => "@module.unknownPath", Module, Error, None,
        "a path reaches no module visible to the importing module";
    ModuleMissingRequiredImport => "@module.missingRequiredImport", Module, Error, None,
        "a test module has no import that resolves to `@std.assert`";
    ModuleImportCycle => "@module.importCycle", Module, Error, None,
        "module imports form a cycle";
    ModuleInvalidSegment => "@module.invalidSegment", Module, Error, None,
        "a source path segment is invalid";
    ModulePathEscape => "@module.pathEscape", Module, Error, None,
        "a link leaves the source root";
    ModuleIoError => "@module.ioError", Module, Error, None,
        "a module cannot be enumerated or read, or an entry is unreadable or dangling";
    TypeArgumentMismatch => "@type.argumentMismatch", Type, Error, None,
        "an argument does not fit its slot, or an application or pattern has the wrong shape";
    TypeTypeArgumentMismatch => "@type.typeArgumentMismatch", Type, Error, None,
        "a type application has the wrong arguments or labels";
    TypeRedundantImplementation => "@type.redundantImplementation", Type, Error, None,
        "an `impl` inside an interface targets a type of the same module";
    TypeDefaultOverride => "@type.defaultOverride", Type, Error, None,
        "an implementation supplies a default member again";
    TypeMissingAbstractMember => "@type.missingAbstractMember", Type, Error, None,
        "an implementation does not supply an abstract member or an associated type";
    TypeFunctionNotEquatable => "@type.functionNotEquatable", Type, Error, None,
        "a callable is used where equality is required";
    TypeNotApplicable => "@type.notApplicable", Type, Error, None,
        "a value that is not applicable is applied";
    TypeUnknownRecordField => "@type.unknownRecordField", Type, Error, None,
        "a field read names no field of the value";
    TypeNumericOutOfRange => "@type.numericOutOfRange", Type, Error, None,
        "a literal does not fit its type";
    TypeNotConstant => "@type.notConstant", Type, Error, None,
        "a constant expression is required and the expression is not one";
    TypeInitializerCycle => "@type.initializerCycle", Type, Error, None,
        "a module value's initializer reaches itself";
    TypeUndispatchableContractMember => "@type.undispatchableContractMember", Type, Error, None,
        "a contract member has no receiver slot and its result type does not contain `Self`";
    TypeUnsupportedSelfPosition => "@type.unsupportedSelfPosition", Type, Error, None,
        "a contract member's result type places `Self` outside the `Self` map";
    TypeOverlappingImplementation => "@type.overlappingImplementation", Type, Error, None,
        "two implementations of one receiver have targets that unify";
    TypeAmbiguousImplementation => "@type.ambiguousImplementation", Type, Error, None,
        "two implementations are still candidates at a call";
    TypeUnsatisfiedBound => "@type.unsatisfiedBound", Type, Error, None,
        "a type has no implementation the call or bound needs";
    TypeAmbiguousDestination => "@type.ambiguousDestination", Type, Error, None,
        "a destination-dispatched call has no written expected type";
    TypeRedundantConversion => "@type.redundantConversion", Type, Error, None,
        "`From` and `TryFrom` sources of one receiver unify";
    TypeMismatch => "@type.mismatch", Type, Error, None,
        "a type or expression differs from the one its position requires";
    TypeAmbiguousInference => "@type.ambiguousInference", Type, Error, None,
        "the call does not determine a type argument";
    TypeInfiniteSize => "@type.infiniteSize", Type, Error, None,
        "a declared type's expansion repeats";
    TypeInvalidDictKey => "@type.invalidDictKey", Type, Error, None,
        "a dict key type is not admissible";
    TypeInvalidTry => "@type.invalidTry", Type, Error, None,
        "`try` is on a wrong carrier, in a callable with another carrier, or directly in a test";
    TypeUnhandledFallible => "@type.unhandledFallible", Type, Error, None,
        "a fallible value is left unused as a statement";
    TypeInvalidReturn => "@type.invalidReturn", Type, Error, None,
        "`return` is outside a callable, in a module value, or directly in a test";
    TypeMissingReturn => "@type.missingReturn", Type, Error, None,
        "a callable body can run off its end";
    TypeInvalidInvariant => "@type.invalidInvariant", Type, Error, None,
        "a type invariant is not a predicate over the representation";
    TypeSuspiciousRow => "@type.suspiciousRow", Type, Warning, None,
        "a result type is a tuple of operation interfaces";
    TypeUnreachableCode => "@type.unreachableCode", Type, Error, None,
        "a statement follows one that cannot complete";
    PatternRefutableBinding => "@pattern.refutableBinding", Pattern, Error, None,
        "a `let` or parameter pattern can fail and has no `else`";
    PatternIrrefutableLetElse => "@pattern.irrefutableLetElse", Pattern, Error, None,
        "a `let … else` pattern cannot fail";
    PatternNonExhaustive => "@pattern.nonExhaustive", Pattern, Error, None,
        "a `match` does not cover its type";
    PatternUnreachableArm => "@pattern.unreachableArm", Pattern, Error, None,
        "an arm is repeated, already covered, or uninhabited";
    PatternUnsupportedCoverage => "@pattern.unsupportedCoverage", Pattern, Error, None,
        "the checker cannot decide coverage for this type";
    EffectOutsideCeiling => "@effect.outsideCeiling", Effect, Error, None,
        "a body performs an interface outside its ceiling";
    EffectInvalidReference => "@effect.invalidReference", Effect, Error, None,
        "a row entry is not an operation interface";
    EffectDuplicateRoot => "@effect.duplicateRoot", Effect, Error, None,
        "a row names one interface twice";
    EffectInvalidAwait => "@effect.invalidAwait", Effect, Error, None,
        "`await` is outside a `proc` or test, or of a value that is not a plan";
    ExternalUnknownSymbol => "@external.unknownSymbol", External, Error, None,
        "an external symbol is not in the registry";
    ExternalOwnerMismatch => "@external.ownerMismatch", External, Error, None,
        "an external member is declared on the wrong interface";
    ExternalSignatureMismatch => "@external.signatureMismatch", External, Error, None,
        "an external member's signature or row differs from the registry";
    DataInvalidExtension => "@data.invalidExtension", Data, Error, None,
        "a file has the wrong extension";
    DataInvalidShape => "@data.invalidShape", Data, Error, None,
        "a VIBON field is missing or unknown, or a value has the wrong form";
    DataInvalidValue => "@data.invalidValue", Data, Error, None,
        "a VIBON value is not admitted by its slot, or fields relate wrongly";
    DataDuplicateField => "@data.duplicateField", Data, Error, None,
        "a VIBON record repeats a field";
    DataDuplicateKey => "@data.duplicateKey", Data, Error, None,
        "a VIBON dict repeats a key after decoding";
    ProjectStaleLock => "@project.staleLock", Project, Error, None,
        "the lock does not match the project or the vendored content";
    ProjectDependencyCycle => "@project.dependencyCycle", Project, Error, None,
        "package dependencies form a cycle";
    ProjectEntryOutsideTarget => "@project.entryOutsideTarget", Project, Error, None,
        "a target `entry` does not start with the target's name";
    ProjectEntryOnLibrary => "@project.entryOnLibrary", Project, Error, None,
        "a library target has an `entry`";
    ProjectInvalidEntrySignature => "@project.invalidEntrySignature", Project, Error, None,
        "an `entry` is not a module-level `proc` with an entry signature";
    ProjectAmbiguousDependencyTarget => "@project.ambiguousDependencyTarget", Project, Error, None,
        "a dependency without `target` names a package with several library targets";
    ProjectReservedUnitName => "@project.reservedUnitName", Project, Error, None,
        "a target or dependency is named `tests`";
    ProjectOverlappingTargetRoots => "@project.overlappingTargetRoots", Project, Error, None,
        "target roots overlap each other or `tests`";
    ProjectNotFound => "@project.notFound", Project, Error, None,
        "no `project.vibon` lies at or above the workspace";
    ProjectInvalidTargetRoot => "@project.invalidTargetRoot", Project, Error, None,
        "a target or test root is missing, not a directory, absolute, or escapes";
    ProjectIoError => "@project.ioError", Project, Error, None,
        "a project input or output operation failed";
    RuntimeInvalidHostValue => "@runtime.invalidHostValue", Runtime, Error, None,
        "a boundary value is invalid, stale, foreign, or of the wrong kind";
    RuntimeInvalidCheckedProgram => "@runtime.invalidCheckedProgram", Runtime, Error, None,
        "checked IR, a descriptor, or a continuation is impossible";
    RuntimeMemoryExhausted => "@runtime.memoryExhausted", Runtime, Error, None,
        "memory is exhausted, a host event rather than a trap";
    StyleCasing => "@style.casing", Style, Warning, None,
        "a name departs from the casing convention";
    ContractUnusedEffect => "@contract.unusedEffect", Contract, Warning, None,
        "a ceiling or `effects` entry names an interface nothing performs";
    ContractUnproved => "@contract.unproved", Contract, Error, None,
        "a bare guard is not established by the rules";
    ContractRefuted => "@contract.refuted", Contract, Error, None,
        "a bare guard is disproved";
    ContractUnsupportedPredicate => "@contract.unsupportedPredicate", Contract, Error, None,
        "a predicate is outside the certified fragment";
    ContractObligationLost => "@contract.obligationLost", Contract, Error, None,
        "a conversion to a plain callable type would hide a bare `requires`";
    ContractInvalidPredicate => "@contract.invalidPredicate", Contract, Error, None,
        "a predicate has the wrong type or shape";
    ContractInvalidConsequence => "@contract.invalidConsequence", Contract, Error, None,
        "a consequence has the wrong type, is not constant where required, or is not pure";
    GrantInvalidBinding => "@grant.invalidBinding", Grant, Error, None,
        "a grant binding is missing, extra, ill-typed, unapproved, or names a wrong `after` or `source`";
    GrantBindingCycle => "@grant.bindingCycle", Grant, Error, None,
        "grant `after` edges form a cycle";
    GrantRequiredUnavailable => "@grant.requiredUnavailable", Grant, Error, None,
        "a required grant is refused at startup";
    GrantAcquisitionFailed => "@grant.acquisitionFailed", Grant, Error, None,
        "a grant acquisition fails at startup";
    GrantInvalidPolicy => "@grant.invalidPolicy", Grant, Error, None,
        "a policy is invalid or names another root";
    GrantPolicyTooComplex => "@grant.policyTooComplex", Grant, Error, None,
        "a policy is over the size or depth limit";
    ToolUnavailable => "@tool.unavailable", Tool, Error, None,
        "a valid command or construct is outside the tool's profile";
    ToolStalePlan => "@tool.stalePlan", Tool, Error, None,
        "a plan's workspace or document revision no longer matches";
    ToolActionDenied => "@tool.actionDenied", Tool, Error, None,
        "the server's permissions deny a tool action";
    ToolLimitExceeded => "@tool.limitExceeded", Tool, Error, None,
        "a tool limit is reached and the result is incomplete";
    ToolUnsupportedValue => "@tool.unsupportedValue", Tool, Error, None,
        "a function, plan, or grant cannot be shown";
    ToolUnsupportedTarget => "@tool.unsupportedTarget", Tool, Error, None,
        "the toolchain does not emit for the requested platform";
    ToolMissingLinkTool => "@tool.missingLinkTool", Tool, Error, None,
        "the platform linker is missing";
    ToolLinkFailed => "@tool.linkFailed", Tool, Error, None,
        "a link step failed";
    ToolIncompatibleArtifact => "@tool.incompatibleArtifact", Tool, Error, None,
        "the runtime cannot admit an artifact's ABI, registry, or identities";
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

    /// The count of the specification table.
    const REGISTERED_CODES: usize = 115;

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
        assert_eq!(spellings.len(), DiagnosticCode::ALL.len());
    }

    #[test]
    fn every_code_is_spelled_as_an_atom_of_two_camel_case_components() {
        for code in DiagnosticCode::ALL {
            let atom = code.as_atom();
            let path = atom
                .strip_prefix('@')
                .unwrap_or_else(|| panic!("{atom} does not start with @"));

            let components: Vec<&str> = path.split('.').collect();
            assert_eq!(components.len(), 2, "{atom} is not `@<domain>.<condition>`");

            for component in components {
                assert!(
                    component.starts_with(|first: char| first.is_ascii_lowercase()),
                    "{atom} has a component not starting with a lowercase letter"
                );
                assert!(
                    component
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric()),
                    "{atom} has a component that is not camelCase"
                );
            }
            assert!(!atom.contains('-'), "{atom} is spelled with a hyphen");
        }
    }

    #[test]
    fn every_code_domain_matches_the_first_component_of_its_spelling() {
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
    fn every_domain_has_a_code() {
        for domain in Domain::ALL {
            assert!(
                DiagnosticCode::ALL
                    .iter()
                    .any(|code| code.domain() == *domain),
                "{domain} has no code"
            );
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
    fn exactly_three_codes_are_warnings() {
        let warnings: Vec<DiagnosticCode> = DiagnosticCode::ALL
            .iter()
            .copied()
            .filter(|code| code.level() == Level::Warning)
            .collect();
        assert_eq!(
            warnings,
            [
                DiagnosticCode::TypeSuspiciousRow,
                DiagnosticCode::StyleCasing,
                DiagnosticCode::ContractUnusedEffect,
            ]
        );
    }

    #[test]
    fn every_code_has_fix_capability_none() {
        for code in DiagnosticCode::ALL {
            assert_eq!(code.fix_capability(), FixCapability::None, "for {code}");
        }
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
        // Built at run time so that the registry source holds no atom
        // literal other than the registered ones.
        let unknown = format!("@{}.{}", "type", "noSuchCondition");
        assert_eq!(
            unknown.parse::<DiagnosticCode>(),
            Err(UnknownDiagnosticCode)
        );
        assert_eq!("".parse::<DiagnosticCode>(), Err(UnknownDiagnosticCode));
        let without_at = DiagnosticCode::TypeMismatch
            .as_atom()
            .trim_start_matches('@');
        assert_eq!(
            without_at.parse::<DiagnosticCode>(),
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
            DiagnosticCode::TypeArgumentMismatch.as_atom()
        );
        assert_eq!(Domain::Type.to_string(), "type");
        assert_eq!(Level::Error.to_string(), "@error");
    }
}
