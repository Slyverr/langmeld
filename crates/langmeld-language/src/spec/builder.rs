use super::{ConstructSpec, LanguageSpec, LexicalRules, SpecError};

/// Builds one validated grammar from declarative and Rust-authored constructs.
///
/// Use [`LanguageSpec::into_builder`] to add Rust constructs to a specification
/// loaded from `.langmeld`. The builder validates the complete construct set
/// at [`Self::build`], so constructs may refer to other constructs added later.
pub struct LanguageSpecBuilder {
    name: String,
    constructs: Vec<ConstructSpec>,
    lexical_rules: LexicalRules,
    root_constructs: Option<Vec<String>>,
}

impl LanguageSpecBuilder {
    pub(super) fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            constructs: Vec::new(),
            lexical_rules: LexicalRules::default(),
            root_constructs: None,
        }
    }

    pub(super) fn from_spec(spec: LanguageSpec) -> Self {
        let root_constructs = spec.root_constructs_explicit.then(|| {
            spec.root_constructs()
                .map(|construct| construct.name().to_owned())
                .collect()
        });

        Self {
            name: spec.name,
            constructs: spec.constructs,
            lexical_rules: spec.lexical_rules,
            root_constructs,
        }
    }

    /// Adds one construct to the grammar being built.
    #[must_use]
    pub fn construct(mut self, construct: ConstructSpec) -> Self {
        self.constructs.push(construct);
        self
    }

    /// Adds constructs collected by a Rust module or extension.
    #[must_use]
    pub fn extend(mut self, constructs: impl IntoIterator<Item = ConstructSpec>) -> Self {
        self.constructs.extend(constructs);
        self
    }

    /// Replaces the comment delimiters used by the built-in lexer.
    #[must_use]
    pub fn with_lexical_rules(mut self, rules: LexicalRules) -> Self {
        self.lexical_rules = rules;
        self
    }

    /// Replaces the set of constructs allowed at the source root.
    ///
    /// When a builder comes from a specification with explicit roots, that
    /// root allowlist is preserved unless this method is called. Rust-added
    /// constructs are not allowed at the root by default in that case.
    #[must_use]
    pub fn with_root_constructs(mut self, names: &[&str]) -> Self {
        self.root_constructs = Some(names.iter().map(|name| (*name).to_owned()).collect());
        self
    }

    /// Validates and creates the combined language grammar.
    pub fn build(self) -> Result<LanguageSpec, SpecError> {
        let mut language = LanguageSpec::new(self.name, self.constructs)?
            .with_lexical_rules(self.lexical_rules)?;

        if let Some(names) = self.root_constructs {
            let names = names.iter().map(String::as_str).collect::<Vec<_>>();
            language = language.with_root_constructs(&names)?;
        }

        Ok(language)
    }
}
