use langmeld_core::Span;

/// A schema containing top-level record declarations.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Schema {
    /// Records in source order.
    pub records: Vec<Record>,
}

/// A record declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Record name.
    pub name: String,
    /// Source range of the declared name.
    pub name_span: Span,
    /// Fields in source order.
    pub fields: Vec<Field>,
    /// Complete source range of the declaration.
    pub span: Span,
}

/// A field declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Field {
    /// Field name.
    pub name: String,
    /// Source range of the declared name.
    pub name_span: Span,
    /// Field type.
    pub ty: TypeName,
    /// Complete source range of the declaration.
    pub span: Span,
}

/// A built-in or user-defined field type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeName {
    /// Type spelling.
    pub name: String,
    /// Source range of the type name.
    pub span: Span,
}
