use langmeld_core::SourceFile;
use langmeld_language::{DefinitionSource, DefinitionSourceParts};

#[test]
fn into_parts_returns_named_key_and_source() {
    let source = SourceFile::new("language/main.langmeld", "language \"Example\" {}");
    let definition = DefinitionSource::new("language/main.langmeld", source.clone());

    let parts: DefinitionSourceParts = definition.into_parts();

    assert_eq!(parts.key, "language/main.langmeld");
    assert_eq!(parts.source, source);
}
