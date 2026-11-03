/// Thread-safe host projection of a parsed Rust function signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustFunctionSignature {
    /// Whether the declaration is asynchronous.
    pub is_async: bool,
    /// Whether the declaration is unsafe.
    pub is_unsafe: bool,
    /// Generic parameter declarations, preserved as Rust source text.
    pub generics: Vec<String>,
    /// Function parameters in source order.
    pub parameters: Vec<RustFunctionParameter>,
    /// Return type source text, or `None` when Rust's unit return is implicit.
    pub return_type: Option<String>,
}

/// One typed parameter projection retained by the Rust source host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustFunctionParameter {
    /// Parameter pattern, such as `value` or `mut state`.
    pub pattern: String,
    /// Type expression as written in the source.
    pub type_text: String,
}
