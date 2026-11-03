//! Compile validated language syntax for reuse by parser and editor services.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    error::Error,
    fmt,
    sync::Arc,
};

use langmeld_core::{ConstructSpec, FieldKind, SourceFile, ValidatedLanguageSpec};

use super::{
    candidates::BlockCandidateIndex,
    pattern::{self, PatternPart},
};
use crate::{CustomConstructParser, CustomLexer, LanguageTokenKind, ParseLimits, ParseResult};

#[derive(Clone, Debug)]
pub(super) struct CompiledPattern {
    pub(super) variant_id: String,
    pub(super) parts: Vec<PatternPart>,
    pub(super) capture_kinds: BTreeMap<String, FieldKind>,
}

#[derive(Clone, Debug)]
pub(super) struct CompiledConstruct {
    pub(super) construct_index: usize,
    pub(super) patterns: Vec<CompiledPattern>,
}

/// A validated pattern that the shared parser could not compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternCompileError {
    /// Construct that owns the unsupported pattern.
    pub construct: String,
    /// Pattern text that failed compilation.
    pub pattern: String,
}

impl fmt::Display for PatternCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not compile syntax pattern for construct `{}`: `{}`",
            self.construct, self.pattern
        )
    }
}

impl Error for PatternCompileError {}

/// Validated language data with syntax patterns compiled once for reuse.
#[derive(Clone, Debug)]
pub struct PreparedLanguage {
    pub(super) spec: ValidatedLanguageSpec,
    pub(super) compiled: HashMap<String, CompiledConstruct>,
    pub(super) block_candidates: HashMap<String, Arc<BlockCandidateIndex>>,
    keywords: BTreeSet<String>,
}

impl PreparedLanguage {
    /// Compiles a validated definition for repeated source parsing.
    ///
    /// A validated model should only contain patterns supported by the shared
    /// grammar. This fallible boundary catches drift between schema validation
    /// and runtime pattern compilation instead of silently dropping a construct.
    pub fn new(spec: ValidatedLanguageSpec) -> Result<Self, PatternCompileError> {
        let mut compiled = HashMap::new();
        let mut block_candidates = HashMap::new();
        let mut keywords = BTreeSet::new();

        for (construct_index, construct) in spec.spec().constructs.iter().enumerate() {
            let mut patterns = Vec::with_capacity(construct.syntax.len());
            for (index, source_pattern) in construct.syntax.iter().enumerate() {
                let Some(parts) = pattern::compile_pattern(source_pattern, construct) else {
                    return Err(PatternCompileError {
                        construct: construct.id.clone(),
                        pattern: source_pattern.clone(),
                    });
                };
                pattern::collect_keywords(&parts, &mut keywords);
                let mut capture_kinds = BTreeMap::new();
                collect_capture_kinds(&parts, &mut capture_kinds);
                patterns.push(CompiledPattern {
                    variant_id: syntax_variant_id(construct, index, source_pattern),
                    parts,
                    capture_kinds,
                });
            }
            compiled.insert(
                construct.id.clone(),
                CompiledConstruct {
                    construct_index,
                    patterns,
                },
            );
        }

        let needs_block_indexes = spec.spec().constructs.iter().any(|construct| {
            construct
                .block
                .as_ref()
                .is_some_and(|block| !block.allowed.is_empty())
        });
        let (construct_ids, category_implementors) = if needs_block_indexes {
            let mut category_implementors = HashMap::<&str, Vec<&str>>::new();
            for construct in &spec.spec().constructs {
                for category in &construct.implements {
                    category_implementors
                        .entry(category.as_str())
                        .or_default()
                        .push(construct.id.as_str());
                }
            }
            let construct_ids = spec
                .spec()
                .constructs
                .iter()
                .map(|construct| construct.id.as_str())
                .collect::<HashSet<_>>();
            (construct_ids, category_implementors)
        } else {
            (HashSet::new(), HashMap::new())
        };

        let mut indexes_by_allowed = HashMap::<Vec<String>, Arc<BlockCandidateIndex>>::new();
        for construct in &spec.spec().constructs {
            let Some(block) = &construct.block else {
                continue;
            };
            let index = indexes_by_allowed
                .entry(block.allowed.clone())
                .or_insert_with(|| {
                    let mut children = Vec::new();
                    let mut seen = HashSet::new();
                    for allowed in &block.allowed {
                        if construct_ids.contains(allowed.as_str()) {
                            if seen.insert(allowed.as_str()) {
                                children.push(allowed.clone());
                            }
                        } else if let Some(implementors) =
                            category_implementors.get(allowed.as_str())
                        {
                            for implementor in implementors {
                                if seen.insert(*implementor) {
                                    children.push((*implementor).to_owned());
                                }
                            }
                        }
                    }
                    Arc::new(BlockCandidateIndex::new(
                        children,
                        &spec.spec().constructs,
                        &compiled,
                    ))
                })
                .clone();
            block_candidates.insert(construct.id.clone(), index);
        }

        Ok(Self {
            spec,
            compiled,
            block_candidates,
            keywords,
        })
    }

    /// Binds and validates the host parser callbacks declared by this language.
    ///
    /// Declarative-only languages pass an empty iterator. Missing, duplicate,
    /// and undeclared callback IDs are rejected before source parsing starts.
    pub fn with_custom_parsers(
        self,
        parsers: impl IntoIterator<Item = CustomConstructParser>,
    ) -> Result<ConfiguredLanguageParser, ParserRegistrationError> {
        let mut registered = BTreeMap::new();
        let mut issues = Vec::new();
        for parser in parsers {
            let id = parser.id.clone();
            if registered.insert(id.clone(), parser).is_some() {
                issues.push(ParserRegistrationIssue::Duplicate(id));
            }
        }
        let declared = self
            .spec
            .spec()
            .constructs
            .iter()
            .filter_map(|construct| construct.custom_parser.as_deref())
            .collect::<BTreeSet<_>>();
        for id in &declared {
            if !registered.contains_key(*id) {
                issues.push(ParserRegistrationIssue::Missing((*id).to_owned()));
            }
        }
        for id in registered.keys() {
            if !declared.contains(id.as_str()) {
                issues.push(ParserRegistrationIssue::Unused(id.clone()));
            }
        }
        if !issues.is_empty() {
            issues.sort();
            return Err(ParserRegistrationError { issues });
        }
        Ok(ConfiguredLanguageParser {
            language: self,
            parsers: registered.into_values().collect(),
            lexer: None,
        })
    }

    /// Creates a parser configuration when the language declares no custom parsers.
    pub fn configure(self) -> Result<ConfiguredLanguageParser, ParserRegistrationError> {
        self.with_custom_parsers(std::iter::empty())
    }

    /// Returns the validated language definition used by this prepared parser.
    #[must_use]
    pub const fn spec(&self) -> &ValidatedLanguageSpec {
        &self.spec
    }

    /// Returns the declarative keywords found in compiled patterns, in sorted order.
    pub fn keywords(&self) -> impl Iterator<Item = &str> + '_ {
        self.keywords.iter().map(String::as_str)
    }

    /// Checks whether a word is a literal keyword in any compiled pattern.
    #[must_use]
    pub fn contains_keyword(&self, word: &str) -> bool {
        self.keywords.contains(word)
    }

    /// Returns the stable syntax-variant IDs for a construct in definition order.
    #[must_use]
    pub fn syntax_variant_ids(&self, construct_id: &str) -> Option<Vec<&str>> {
        self.compiled.get(construct_id).map(|construct| {
            construct
                .patterns
                .iter()
                .map(|pattern| pattern.variant_id.as_str())
                .collect()
        })
    }

    /// Returns the captured field kinds for one compiled syntax variant.
    #[must_use]
    pub fn capture_kinds(
        &self,
        construct_id: &str,
        variant_id: &str,
    ) -> Option<&BTreeMap<String, FieldKind>> {
        self.compiled
            .get(construct_id)?
            .patterns
            .iter()
            .find(|pattern| pattern.variant_id == variant_id)
            .map(|pattern| &pattern.capture_kinds)
    }

    /// Returns likely child constructs for a block and its next token.
    ///
    /// Results preserve definition order. They are a parser/editor hint only:
    /// complete syntax matching is still required to determine a construct.
    #[must_use]
    pub fn candidate_constructs_for_token(
        &self,
        construct_id: &str,
        token: &LanguageTokenKind,
    ) -> Option<Vec<&str>> {
        self.block_candidates
            .get(construct_id)
            .map(|index| index.candidates(token).collect())
    }
}

/// A problem found while binding host parser callbacks to a language definition.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ParserRegistrationIssue {
    /// The same callback ID was registered more than once.
    Duplicate(String),
    /// The language declares a callback ID that the host did not register.
    Missing(String),
    /// The host registered a callback ID that the language does not declare.
    Unused(String),
}

impl fmt::Display for ParserRegistrationIssue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Duplicate(id) => write!(
                formatter,
                "custom parser `{id}` is registered more than once"
            ),
            Self::Missing(id) => write!(
                formatter,
                "language requires custom parser `{id}` but the host did not register it"
            ),
            Self::Unused(id) => write!(
                formatter,
                "host registered custom parser `{id}` but the language does not declare it"
            ),
        }
    }
}

/// Registration issues found before parsing source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParserRegistrationError {
    /// Sorted problems for deterministic startup diagnostics.
    pub issues: Vec<ParserRegistrationIssue>,
}

impl fmt::Display for ParserRegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, issue) in self.issues.iter().enumerate() {
            if index > 0 {
                formatter.write_str("; ")?;
            }
            issue.fmt(formatter)?;
        }
        Ok(())
    }
}

impl Error for ParserRegistrationError {}

/// Prepared declarative syntax with a checked host callback registry.
#[derive(Clone, Debug)]
pub struct ConfiguredLanguageParser {
    pub(super) language: PreparedLanguage,
    pub(super) parsers: Vec<CustomConstructParser>,
    pub(super) lexer: Option<CustomLexer>,
}

impl ConfiguredLanguageParser {
    /// Returns the prepared language associated with this parser.
    #[must_use]
    pub const fn language(&self) -> &PreparedLanguage {
        &self.language
    }

    /// Returns whether parsing uses a host-provided lexer.
    #[must_use]
    pub const fn has_custom_lexer(&self) -> bool {
        self.lexer.is_some()
    }

    /// Parses source with default input and work limits.
    #[must_use]
    pub fn parse(&self, source: &SourceFile) -> ParseResult {
        self.parse_with_limits(source, ParseLimits::default())
    }

    /// Parses source with caller-selected input, nesting, node, and matcher bounds.
    #[must_use]
    pub fn parse_with_limits(&self, source: &SourceFile, limits: ParseLimits) -> ParseResult {
        super::input::parse_configured(self, source, limits)
    }

    /// Replaces built-in tokenization with a validated host lexer.
    #[must_use]
    pub fn with_custom_lexer(mut self, lexer: CustomLexer) -> Self {
        self.lexer = Some(lexer);
        self
    }
}

fn collect_capture_kinds(parts: &[PatternPart], output: &mut BTreeMap<String, FieldKind>) {
    for part in parts {
        match part {
            PatternPart::Capture { field, kind } => {
                output.insert(field.clone(), kind.clone());
            }
            PatternPart::Optional(inner)
            | PatternPart::ZeroOrMore { parts: inner, .. }
            | PatternPart::OneOrMore { parts: inner, .. } => {
                collect_capture_kinds(inner, output);
            }
            PatternPart::Choice(alternatives) => {
                if let Some(first) = alternatives.first() {
                    collect_capture_kinds(first, output);
                }
            }
            PatternPart::Literal(_) => {}
        }
    }
}

fn syntax_variant_id(construct: &ConstructSpec, index: usize, pattern: &str) -> String {
    construct
        .syntax_ids
        .get(index)
        .cloned()
        .unwrap_or_else(|| pattern_identifier(pattern))
}

fn pattern_identifier(pattern: &str) -> String {
    let hash = pattern
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    format!("pattern_{hash:016x}")
}

#[cfg(test)]
mod tests {
    use langmeld_core::{ConstructBuilder, FieldKind, FieldSpec, LanguageBuilder};

    use super::*;
    use crate::{LanguageTokenKind, LexedSource};

    fn language() -> ValidatedLanguageSpec {
        LanguageBuilder::new("Prepared")
            .version("0.1.0")
            .root("program")
            .construct(
                ConstructBuilder::new("program")
                    .syntax("program {")
                    .block(["binding"]),
            )
            .construct(
                ConstructBuilder::new("binding")
                    .syntax_variant("let-binding", "let {name} = {value}")
                    .syntax_variant("set-binding", "set {name} to {value}")
                    .field(FieldSpec::required("name", FieldKind::Identifier))
                    .field(FieldSpec::required("value", FieldKind::Number)),
            )
            .build()
            .expect("test language is structurally valid")
    }

    #[test]
    fn prepared_language_compiles_patterns_and_exposes_stable_keywords() {
        let first = PreparedLanguage::new(language()).expect("patterns compile");
        let second = PreparedLanguage::new(language()).expect("patterns compile again");

        assert_eq!(first.compiled["binding"].construct_index, 1);
        assert_eq!(
            first.compiled["binding"]
                .patterns
                .iter()
                .map(|pattern| pattern.variant_id.as_str())
                .collect::<Vec<_>>(),
            ["let-binding", "set-binding"]
        );
        assert_eq!(
            first.keywords().collect::<Vec<_>>(),
            ["let", "program", "set", "to"]
        );
        assert!(first.contains_keyword("let"));
        assert!(!first.contains_keyword("name"));
        assert_eq!(
            first.compiled["program"].patterns[0].variant_id,
            second.compiled["program"].patterns[0].variant_id
        );
        assert_eq!(first.spec().spec().name, "Prepared");
    }

    #[test]
    fn configured_parser_reports_custom_lexer_registration() {
        let parser = PreparedLanguage::new(language())
            .expect("language prepares")
            .configure()
            .expect("declarative parser configures");
        assert!(!parser.has_custom_lexer());

        let parser = parser.with_custom_lexer(CustomLexer::new(|_| LexedSource::default()));
        assert!(parser.has_custom_lexer());
    }

    #[test]
    fn block_index_keeps_exact_matches_and_custom_fallbacks_in_definition_order() {
        let spec = LanguageBuilder::new("Indexed")
            .version("0.1.0")
            .root("program")
            .construct(
                ConstructBuilder::new("program")
                    .syntax("program {")
                    .block(["word", "marker", "custom"]),
            )
            .construct(
                ConstructBuilder::new("word")
                    .syntax("node {name}")
                    .field(FieldSpec::required("name", FieldKind::Identifier)),
            )
            .construct(
                ConstructBuilder::new("marker")
                    .syntax(r"\@ {name}")
                    .field(FieldSpec::required("name", FieldKind::Identifier)),
            )
            .construct(
                ConstructBuilder::new("custom")
                    .syntax("custom {name}")
                    .field(FieldSpec::required("name", FieldKind::Identifier))
                    .custom_parser("custom-parser"),
            )
            .build()
            .expect("test language is valid");
        let prepared = PreparedLanguage::new(spec).expect("patterns compile");

        assert_eq!(
            prepared.candidate_constructs_for_token(
                "program",
                &LanguageTokenKind::Identifier("node".to_owned())
            ),
            Some(vec!["word", "custom"])
        );
        assert_eq!(
            prepared
                .candidate_constructs_for_token("program", &LanguageTokenKind::Punctuation('@')),
            Some(vec!["marker", "custom"])
        );
        assert_eq!(
            prepared.candidate_constructs_for_token(
                "program",
                &LanguageTokenKind::Number("1".to_owned())
            ),
            Some(vec!["custom"])
        );
        assert_eq!(
            prepared.candidate_constructs_for_token("unknown", &LanguageTokenKind::End),
            None
        );
    }
}
