use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TextMateGrammar {
    pub(super) name: String,
    pub(super) scope_name: String,
    pub(super) file_types: Vec<String>,
    pub(super) patterns: Vec<GrammarPattern>,
}

#[derive(Serialize)]
#[serde(untagged)]
pub(super) enum GrammarPattern {
    String(StringPattern),
    Match(MatchPattern),
}

#[derive(Serialize)]
pub(super) struct StringPattern {
    pub(super) name: String,
    pub(super) begin: String,
    pub(super) end: String,
    pub(super) patterns: Vec<MatchPattern>,
}

#[derive(Serialize)]
pub(super) struct MatchPattern {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<String>,
    #[serde(rename = "match")]
    pub(super) regex: String,
}

impl From<StringPattern> for GrammarPattern {
    fn from(pattern: StringPattern) -> Self {
        Self::String(pattern)
    }
}

impl From<MatchPattern> for GrammarPattern {
    fn from(pattern: MatchPattern) -> Self {
        Self::Match(pattern)
    }
}
