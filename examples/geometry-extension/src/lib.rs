/// Project-relative `.langmeld` source files exported by this extension.
///
/// The consumer resolves these keys relative to its own language root.
pub const DEFINITION_SOURCES: &[(&str, &str)] = &[(
    "language/extensions/geometry/drawing.langmeld",
    include_str!("../language/drawing.langmeld"),
)];
