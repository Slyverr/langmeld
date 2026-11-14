use std::collections::{HashMap, HashSet};

use super::validation;
use super::{ConstructSpec, LanguageSpecBuilder, LexicalRules, SpecError};

/// A language's currently supported declarative grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageSpec {
    /// Human-readable language name.
    pub(crate) name: String,
    /// Syntax constructs accepted by this language.
    pub(crate) constructs: Vec<ConstructSpec>,
    /// Comment delimiters used by the built-in lexer.
    pub(crate) lexical_rules: LexicalRules,
    /// Indices of constructs allowed at the source root.
    pub(crate) root_constructs: Vec<usize>,
    /// Whether the source-root allowlist was explicitly declared.
    pub(crate) root_constructs_explicit: bool,
}

impl LanguageSpec {
    /// Starts a builder for a grammar authored in Rust.
    #[must_use]
    pub fn builder(name: impl Into<String>) -> LanguageSpecBuilder {
        LanguageSpecBuilder::new(name)
    }

    /// Converts this specification into a builder for adding Rust constructs.
    ///
    /// Existing lexical rules and explicit root-construct restrictions are
    /// preserved. If root constructs were not explicitly restricted, added
    /// constructs are allowed at the root by default.
    #[must_use]
    pub fn into_builder(self) -> LanguageSpecBuilder {
        LanguageSpecBuilder::from_spec(self)
    }

    /// Validates and creates a language grammar.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError`] when names or patterns are invalid, declarations
    /// conflict or overlap, or a block refers to an unknown construct or
    /// category.
    pub fn new(
        name: impl Into<String>,
        constructs: impl IntoIterator<Item = ConstructSpec>,
    ) -> Result<Self, SpecError> {
        let name = name.into();
        let mut constructs: Vec<_> = constructs.into_iter().collect();
        if name.trim().is_empty() {
            return Err(SpecError::EmptyLanguageName);
        }
        if constructs.is_empty() {
            return Err(SpecError::NoConstructs);
        }
        let mut names = HashSet::new();
        let mut category_members: HashMap<String, Vec<String>> = HashMap::new();
        for construct in &constructs {
            if construct.name.trim().is_empty() {
                return Err(SpecError::EmptyConstructName);
            }
            if !names.insert(construct.name.clone()) {
                return Err(SpecError::DuplicateConstruct(construct.name.clone()));
            }
            let mut variant_names = HashSet::new();
            for variant in construct.variant_names.iter().flatten() {
                if variant.trim().is_empty() {
                    return Err(SpecError::EmptyVariantName(construct.name.clone()));
                }
                if !variant_names.insert(variant.as_str()) {
                    return Err(SpecError::DuplicateVariantName {
                        construct: construct.name.clone(),
                        variant: variant.clone(),
                    });
                }
            }
            let mut categories = HashSet::new();
            for category in &construct.categories {
                if category.trim().is_empty() {
                    return Err(SpecError::EmptyCategoryName(construct.name.clone()));
                }
                if !categories.insert(category.as_str()) {
                    return Err(SpecError::DuplicateImplementedCategory {
                        construct: construct.name.clone(),
                        category: category.clone(),
                    });
                }
                category_members
                    .entry(category.clone())
                    .or_default()
                    .push(construct.name.clone());
            }
            if construct.patterns.is_empty() {
                return Err(SpecError::EmptyPattern(construct.name.clone()));
            }
            for (index, pattern) in construct.patterns.iter().enumerate() {
                if pattern.is_empty() || validation::sequence_is_nullable(pattern) {
                    return Err(SpecError::EmptyPattern(construct.name.clone()));
                }
                let mut fields = HashSet::new();
                validation::validate_pattern(pattern, &construct.name, &mut fields)?;
                validation::validate_pattern_names(pattern, &construct.name)?;
                validation::validate_decision_points(pattern, &construct.name)?;
                if construct.patterns[..index].iter().any(|previous| {
                    !validation::has_group(pattern)
                        && !validation::has_group(previous)
                        && validation::patterns_overlap(previous, pattern)
                }) {
                    return Err(SpecError::AmbiguousVariants(construct.name.clone()));
                }
            }
        }
        for construct in &mut constructs {
            if let Some(block) = &mut construct.block {
                if block.open.trim().is_empty()
                    || block.close.trim().is_empty()
                    || block.open == block.close
                    || !validation::is_single_token(&block.open)
                    || !validation::is_single_token(&block.close)
                {
                    return Err(SpecError::InvalidBlockDelimiter(construct.name.clone()));
                }
                let mut children = HashSet::new();
                for child in &block.constructs {
                    if !names.contains(child.as_str()) {
                        return Err(SpecError::UnknownChildConstruct {
                            construct: construct.name.clone(),
                            child: child.clone(),
                        });
                    }
                    if !children.insert(child.clone()) {
                        return Err(SpecError::DuplicateChildConstruct {
                            construct: construct.name.clone(),
                            child: child.clone(),
                        });
                    }
                }
                let mut categories = HashSet::new();
                let mut expanded_children = block.constructs.clone();
                for category in &block.categories {
                    if category.trim().is_empty() {
                        return Err(SpecError::EmptyCategoryName(construct.name.clone()));
                    }
                    if !categories.insert(category.as_str()) {
                        return Err(SpecError::DuplicateBlockCategory {
                            construct: construct.name.clone(),
                            category: category.clone(),
                        });
                    }
                    let Some(members) = category_members.get(category) else {
                        return Err(SpecError::UnknownBlockCategory {
                            construct: construct.name.clone(),
                            category: category.clone(),
                        });
                    };
                    for member in members {
                        if children.insert(member.clone()) {
                            expanded_children.push(member.clone());
                        }
                    }
                }
                block.constructs = expanded_children;
            }
        }
        for (index, left) in constructs.iter().enumerate() {
            for right in &constructs[index + 1..] {
                for left_pattern in &left.patterns {
                    for right_pattern in &right.patterns {
                        let overlaps = !validation::has_group(left_pattern)
                            && !validation::has_group(right_pattern)
                            && validation::patterns_overlap(left_pattern, right_pattern);
                        if overlaps && !left.fallback && !right.fallback {
                            return Err(SpecError::AmbiguousConstructs(
                                left.name.clone(),
                                right.name.clone(),
                            ));
                        }
                    }
                }
            }
        }
        let root_constructs = (0..constructs.len()).collect();
        Ok(Self {
            name,
            constructs,
            lexical_rules: LexicalRules::default(),
            root_constructs,
            root_constructs_explicit: false,
        })
    }

    /// Configures comment delimiters for the built-in lexer.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError`] when a comment marker is invalid or duplicates
    /// another comment start marker.
    pub fn with_lexical_rules(mut self, rules: LexicalRules) -> Result<Self, SpecError> {
        rules.validate()?;
        self.lexical_rules = rules;
        Ok(self)
    }

    /// Returns the comment delimiters used by the built-in lexer.
    #[must_use]
    pub fn lexical_rules(&self) -> &LexicalRules {
        &self.lexical_rules
    }

    /// Restricts which constructs may appear at the source root.
    ///
    /// By default every construct is allowed at the root. Constructs allowed
    /// inside delimited blocks are configured separately with
    /// [`ConstructSpec::block`]. With an explicit root list, parsing can
    /// recover from an unclosed block when it reaches a complete root
    /// construct that is not valid in that block.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError::UnknownRootConstruct`] when a name is not declared,
    /// or [`SpecError::DuplicateRootConstruct`] when a name appears more than
    /// once.
    pub fn with_root_constructs(mut self, names: &[&str]) -> Result<Self, SpecError> {
        let mut root_constructs = vec![false; self.constructs.len()];
        for name in names {
            let Some(index) = self
                .constructs
                .iter()
                .position(|construct| construct.name == *name)
            else {
                return Err(SpecError::UnknownRootConstruct((*name).to_owned()));
            };
            if std::mem::replace(&mut root_constructs[index], true) {
                return Err(SpecError::DuplicateRootConstruct((*name).to_owned()));
            }
        }
        self.root_constructs = root_constructs
            .into_iter()
            .enumerate()
            .filter_map(|(index, allowed)| allowed.then_some(index))
            .collect();
        self.root_constructs_explicit = true;
        Ok(self)
    }

    pub(crate) fn has_explicit_root_constructs(&self) -> bool {
        self.root_constructs_explicit
    }

    /// Returns the human-readable language name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the validated syntax constructs.
    #[must_use]
    pub fn constructs(&self) -> &[ConstructSpec] {
        &self.constructs
    }

    /// Returns constructs allowed at the source root in declaration order.
    pub fn root_constructs(&self) -> impl Iterator<Item = &ConstructSpec> {
        self.root_constructs
            .iter()
            .map(|index| &self.constructs[*index])
    }
}
