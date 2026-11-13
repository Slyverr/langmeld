use langmeld_core::{SourceDatabase, SourceFile, Span};
use langmeld_language::{
    ConstructContext, ConstructHandler, ConstructHandlerRegistry, ConstructSpec, HandlerControl,
    HandlerRegistrationError, LanguageSpec, PatternPart, parse,
};

struct RecordIdentifier;

impl ConstructHandler<Vec<String>> for RecordIdentifier {
    const CONSTRUCT: &'static str = "record";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, Vec<String>>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let value = context.field_identifier("value")?;
        context.state_mut().push(value.to_owned());

        Ok(HandlerControl::Continue)
    }
}

struct BorrowedRecordPrefix<'prefix> {
    prefix: &'prefix str,
}

impl ConstructHandler<Vec<String>> for BorrowedRecordPrefix<'_> {
    const CONSTRUCT: &'static str = "record";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, Vec<String>>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let identifier = context.field_identifier("value")?.to_owned();
        let record = format!("{} {identifier}", self.prefix);

        context.state_mut().push(record);

        Ok(HandlerControl::Continue)
    }
}

struct RecordIdentifierLocation;

impl ConstructHandler<Vec<(String, Span)>> for RecordIdentifierLocation {
    const CONSTRUCT: &'static str = "record";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, Vec<(String, Span)>>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let identifier = context.field_identifier_capture("value")?;
        context
            .state_mut()
            .push((identifier.text.to_owned(), identifier.span));

        Ok(HandlerControl::Continue)
    }
}

struct RecordSourceTextLocation;

impl ConstructHandler<Vec<(String, Span)>> for RecordSourceTextLocation {
    const CONSTRUCT: &'static str = "record";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, Vec<(String, Span)>>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let captured = context.field_text_capture("value")?;
        context
            .state_mut()
            .push((captured.text.to_owned(), captured.span));

        Ok(HandlerControl::Continue)
    }
}

struct RecordRepeatedFields;

impl ConstructHandler<Vec<String>> for RecordRepeatedFields {
    const CONSTRUCT: &'static str = "record";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, Vec<String>>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let mut values = Vec::new();
        for field in context.fields("value") {
            values.push(context.captured_text(field)?.to_owned());
        }

        context.state_mut().extend(values);

        Ok(HandlerControl::Continue)
    }
}

struct DispatchChildren;

impl ConstructHandler<Vec<String>> for DispatchChildren {
    const CONSTRUCT: &'static str = "block";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, Vec<String>>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        assert_eq!(context.construct().name, "block");

        context.dispatch_children()
    }
}

struct DispatchProgram;

impl ConstructHandler<Vec<String>> for DispatchProgram {
    const CONSTRUCT: &'static str = "program";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, Vec<String>>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        context.state_mut().push("program start".to_owned());
        let control = context.dispatch_children()?;
        context.state_mut().push("program end".to_owned());

        Ok(control)
    }
}

fn record_language() -> LanguageSpec {
    LanguageSpec::new(
        "Example",
        [ConstructSpec::new(
            "record",
            [
                PatternPart::literal("record"),
                PatternPart::identifier("value"),
                PatternPart::literal(";"),
            ],
        )],
    )
    .expect("the test language should be valid")
}

fn program_language() -> LanguageSpec {
    LanguageSpec::new(
        "Example",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["record"],
            ),
            ConstructSpec::new(
                "record",
                [
                    PatternPart::literal("record"),
                    PatternPart::identifier("value"),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .expect("the test language should be valid")
}

fn nested_program_language() -> LanguageSpec {
    LanguageSpec::new(
        "Example",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["block"],
            ),
            ConstructSpec::new("block", [PatternPart::literal("block")]).block(
                "{",
                "}",
                &["record"],
            ),
            ConstructSpec::new(
                "record",
                [
                    PatternPart::literal("record"),
                    PatternPart::identifier("value"),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )
    .expect("the nested test language should be valid")
}

#[test]
fn dispatches_top_level_constructs_from_parse_result() -> Result<(), Box<dyn std::error::Error>> {
    let mut sources = SourceDatabase::default();
    let source_id = sources.insert("main.sample", "record first; record second;")?;
    let source = sources.get(source_id).expect("source was just inserted");
    let parsed = parse(source, &record_language());

    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;

    let mut state = Vec::new();

    let control = handlers
        .dispatch_parse_result(source, &parsed, &mut state)
        .expect("parse result belongs to the supplied source snapshot");

    assert_eq!(control, HandlerControl::Continue);
    assert_eq!(state, ["first", "second"]);

    Ok(())
}

#[test]
fn handler_registry_accepts_a_handler_borrowing_configuration() {
    let language = record_language();
    let source = SourceFile::new("main.sample", "record first;");
    let parsed = parse(&source, &language);

    let prefix = String::from("record");
    let mut handlers = ConstructHandlerRegistry::new();
    handlers
        .register(BorrowedRecordPrefix { prefix: &prefix })
        .expect("borrowed handler registration succeeds");

    let mut state = Vec::new();
    handlers
        .dispatch_parse_result(&source, &parsed, &mut state)
        .expect("borrowed handler dispatches");

    assert_eq!(state, ["record first"]);
    assert_eq!(prefix, "record");
}

#[test]
fn captures_identifier_text_and_span_together() -> Result<(), Box<dyn std::error::Error>> {
    let source = SourceFile::new("main.sample", "record first;");
    let parsed = parse(&source, &record_language());
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifierLocation)?;
    let mut state = Vec::new();

    handlers
        .dispatch_parse_result(&source, &parsed, &mut state)
        .expect("the parsed identifier should dispatch successfully");

    assert_eq!(state, [("first".to_owned(), Span::new(7, 12))]);

    Ok(())
}

#[test]
fn captures_exact_source_text_and_span_together() -> Result<(), Box<dyn std::error::Error>> {
    let language = LanguageSpec::new(
        "Example",
        [ConstructSpec::new(
            "record",
            [
                PatternPart::literal("record"),
                PatternPart::string("value"),
                PatternPart::literal(";"),
            ],
        )],
    )?;
    let source = SourceFile::new("main.sample", "record \"hello\\nworld\";");
    let parsed = parse(&source, &language);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordSourceTextLocation)?;

    let mut state = Vec::new();
    handlers
        .dispatch_parse_result(&source, &parsed, &mut state)
        .expect("the string capture should dispatch successfully");

    assert_eq!(state, [("\"hello\\nworld\"".to_owned(), Span::new(7, 21))]);

    Ok(())
}

#[test]
fn reports_wrong_capture_kind_for_identifier_capture() -> Result<(), Box<dyn std::error::Error>> {
    let language = LanguageSpec::new(
        "Example",
        [ConstructSpec::new(
            "record",
            [
                PatternPart::literal("record"),
                PatternPart::token("value"),
                PatternPart::literal(";"),
            ],
        )],
    )?;
    let source = SourceFile::new("main.sample", "record 42;");
    let parsed = parse(&source, &language);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifierLocation)?;

    let error = handlers
        .dispatch_parse_result(&source, &parsed, &mut Vec::new())
        .expect_err("integer captures are not identifier captures");

    assert_eq!(error.code, "LM-HANDLER-004");
    assert_eq!(
        error
            .location
            .expect("wrong-kind errors should be located")
            .span(),
        Span::new(7, 9)
    );

    Ok(())
}

#[test]
fn dispatches_validated_top_level_children_without_a_root_handler()
-> Result<(), Box<dyn std::error::Error>> {
    let mut sources = SourceDatabase::default();
    let source_id = sources.insert("main.sample", "program { record first; record second; }")?;
    let source = sources.get(source_id).expect("source was just inserted");
    let parsed = parse(source, &program_language());
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;

    let mut state = Vec::new();
    let control = handlers
        .dispatch_top_level_children(source, &parsed, &mut state)
        .expect("parse result belongs to the supplied source snapshot");

    assert_eq!(control, HandlerControl::Continue);
    assert_eq!(state, ["first", "second"]);

    Ok(())
}

#[test]
fn rejects_stale_top_level_children_before_dispatch() -> Result<(), Box<dyn std::error::Error>> {
    let mut sources = SourceDatabase::default();
    let source_id = sources.insert("main.sample", "program { record first; }")?;
    let original_source = sources
        .get(source_id)
        .expect("source was just inserted")
        .clone();
    let parsed = parse(&original_source, &program_language());

    sources.update(source_id, "program { record second; }")?;
    let current_source = sources.get(source_id).expect("source still exists");
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;
    let mut state = Vec::new();

    let error = handlers
        .dispatch_top_level_children(current_source, &parsed, &mut state)
        .expect_err("stale parse results must not dispatch their children");

    assert_eq!(error.code, "LM-HANDLER-005");
    assert!(state.is_empty());

    Ok(())
}

#[test]
fn rejects_parse_results_from_another_source_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let mut sources = SourceDatabase::default();
    let source_id = sources.insert("main.sample", "record first;")?;
    let original_source = sources
        .get(source_id)
        .expect("source was just inserted")
        .clone();
    let parsed = parse(&original_source, &record_language());

    sources.update(source_id, "record second;")?;
    let current_source = sources.get(source_id).expect("source still exists");
    let handlers = ConstructHandlerRegistry::<Vec<String>>::new();

    let mut state = Vec::new();

    let error = handlers
        .dispatch_parse_result(current_source, &parsed, &mut state)
        .expect_err("stale parse results must not be dispatched");

    assert_eq!(error.code, "LM-HANDLER-005");
    assert!(state.is_empty());

    Ok(())
}

#[test]
fn rejects_parse_results_from_another_standalone_snapshot() {
    let parsed_source = SourceFile::new("main.sample", "record first;");
    let current_source = SourceFile::new("main.sample", "record first;");
    let parsed = parse(&parsed_source, &record_language());
    let handlers = ConstructHandlerRegistry::<Vec<String>>::new();
    let mut state = Vec::new();

    let error = handlers
        .dispatch_parse_result(&current_source, &parsed, &mut state)
        .expect_err("parse results from another standalone source must be rejected");
    let direct_error = handlers
        .dispatch_construct(
            &current_source,
            &parsed,
            &parsed.constructs()[0],
            &mut state,
        )
        .expect_err("direct construct dispatch must reject a mismatched source");

    assert_eq!(error.code, "LM-HANDLER-005");
    assert_eq!(direct_error.code, "LM-HANDLER-005");
    assert!(state.is_empty());
}

#[test]
fn dispatches_block_children_in_source_order() -> Result<(), Box<dyn std::error::Error>> {
    let source = SourceFile::new("main.sample", "program { record first; record second; }");
    let parsed = parse(&source, &program_language());
    let block = &parsed.constructs()[0];
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;
    let mut state = Vec::new();

    let result = handlers
        .dispatch_block(&source, &parsed, block, &mut state)
        .expect("registered handlers should process their constructs");

    assert_eq!(result, HandlerControl::Continue);
    assert_eq!(state, ["first", "second"]);

    Ok(())
}

#[test]
fn exposes_repeated_captures_in_source_order() -> Result<(), Box<dyn std::error::Error>> {
    let language = LanguageSpec::new(
        "Example",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["record"],
            ),
            ConstructSpec::new(
                "record",
                [
                    PatternPart::literal("record"),
                    PatternPart::one_or_more([PatternPart::identifier("value")]),
                    PatternPart::literal(";"),
                ],
            ),
        ],
    )?;
    let source = SourceFile::new("main.sample", "program { record alpha beta; }");
    let parsed = parse(&source, &language);
    let block = &parsed.constructs()[0];
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordRepeatedFields)?;
    let mut state = Vec::new();

    handlers
        .dispatch_block(&source, &parsed, block, &mut state)
        .expect("registered handler should process repeated fields");

    assert_eq!(state, ["alpha", "beta"]);

    Ok(())
}

#[test]
fn dispatches_nested_children_when_the_current_handler_requests_it()
-> Result<(), Box<dyn std::error::Error>> {
    let source = SourceFile::new("main.sample", "program { block { record child; } }");
    let parsed = parse(&source, &nested_program_language());
    let nested_block = &parsed.constructs()[0].children[0];
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;
    handlers.register(DispatchChildren)?;
    let mut state = Vec::new();

    handlers
        .dispatch_construct(&source, &parsed, nested_block, &mut state)
        .expect("nested handlers should process their children");

    assert_eq!(state, ["child"]);

    Ok(())
}

#[test]
fn dispatches_a_root_construct_handler_and_its_children() -> Result<(), Box<dyn std::error::Error>>
{
    let source = SourceFile::new("main.sample", "program { record child; }");
    let parsed = parse(&source, &program_language());
    let root = &parsed.constructs()[0];
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;
    handlers.register(DispatchProgram)?;
    let mut state = Vec::new();

    let control = handlers
        .dispatch_construct(&source, &parsed, root, &mut state)
        .expect("root handler should dispatch its children");

    assert_eq!(control, HandlerControl::Continue);
    assert_eq!(state, ["program start", "child", "program end"]);

    Ok(())
}

#[test]
fn reports_unregistered_construct_at_its_source_span() {
    let language = LanguageSpec::new(
        "Example",
        [ConstructSpec::new(
            "unknown",
            [PatternPart::literal("unknown"), PatternPart::literal(";")],
        )],
    )
    .expect("the test language should be valid");
    let source = SourceFile::new("main.sample", "unknown;");
    let parsed = parse(&source, &language);
    let node = &parsed.constructs()[0];
    let handlers = ConstructHandlerRegistry::<()>::new();

    let error = handlers
        .dispatch_construct(&source, &parsed, node, &mut ())
        .expect_err("an unregistered construct should be rejected");

    assert_eq!(error.code, "LM-HANDLER-001");
    assert_eq!(
        error.location.expect("error should have a span").span(),
        Span::new(0, 8)
    );
}

#[test]
fn rejects_duplicate_handler_registration() -> Result<(), Box<dyn std::error::Error>> {
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;

    let error = handlers
        .register(RecordIdentifier)
        .expect_err("duplicate construct handlers should be rejected");

    assert_eq!(
        error,
        HandlerRegistrationError::DuplicateConstruct("record".to_owned())
    );

    Ok(())
}

struct MissingField;

impl ConstructHandler<()> for MissingField {
    const CONSTRUCT: &'static str = "record";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, ()>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        context.field_identifier("value")?;
        Ok(HandlerControl::Continue)
    }
}

#[test]
fn reports_missing_fields_at_the_construct() -> Result<(), Box<dyn std::error::Error>> {
    let language = LanguageSpec::new(
        "Example",
        [ConstructSpec::new(
            "record",
            [PatternPart::literal("record"), PatternPart::literal(";")],
        )],
    )?;
    let source = SourceFile::new("main.sample", "record;");
    let parsed = parse(&source, &language);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(MissingField)?;

    let error = handlers
        .dispatch_parse_result(&source, &parsed, &mut ())
        .expect_err("missing required fields should be diagnosed");

    assert_eq!(error.code, "LM-HANDLER-002");
    assert_eq!(
        error.location.expect("error should have a span").span(),
        Span::new(0, 7)
    );

    Ok(())
}

struct ReturnSignal;

impl ConstructHandler<(), String> for ReturnSignal {
    const CONSTRUCT: &'static str = "return";

    fn handle(
        &self,
        _context: &mut ConstructContext<'_, (), String>,
    ) -> Result<HandlerControl<String>, langmeld_core::Diagnostic> {
        Ok(HandlerControl::Signal("returned".to_owned()))
    }
}

#[test]
fn propagates_host_control_signals_and_stops_the_current_block()
-> Result<(), Box<dyn std::error::Error>> {
    let language = LanguageSpec::new(
        "Example",
        [
            ConstructSpec::new("program", [PatternPart::literal("program")]).block(
                "{",
                "}",
                &["return", "ignored"],
            ),
            ConstructSpec::new(
                "return",
                [PatternPart::literal("return"), PatternPart::literal(";")],
            ),
            ConstructSpec::new(
                "ignored",
                [PatternPart::literal("ignored"), PatternPart::literal(";")],
            ),
        ],
    )?;
    let source = SourceFile::new("main.sample", "program { return; ignored; }");
    let parsed = parse(&source, &language);
    let block = &parsed.constructs()[0];
    let mut handlers = ConstructHandlerRegistry::<(), String>::new();
    handlers.register(ReturnSignal)?;

    let result = handlers
        .dispatch_block(&source, &parsed, block, &mut ())
        .expect("return signal should propagate");

    assert_eq!(result, HandlerControl::Signal("returned".to_owned()));

    Ok(())
}

#[test]
fn rejects_direct_dispatch_of_constructs_from_another_parse_result()
-> Result<(), Box<dyn std::error::Error>> {
    let source = SourceFile::new("main.sample", "program { record value; }");
    let parsed = parse(&source, &program_language());
    let other_parse = parse(&source, &program_language());
    let foreign_root = &other_parse.constructs()[0];
    let foreign_child = &foreign_root.children[0];
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;
    let mut state = Vec::new();

    let construct_error = handlers
        .dispatch_construct(&source, &parsed, foreign_child, &mut state)
        .expect_err("selected nodes must come from the supplied parse result");
    let block_error = handlers
        .dispatch_block(&source, &parsed, foreign_root, &mut state)
        .expect_err("selected blocks must come from the supplied parse result");

    assert_eq!(construct_error.code, "LM-HANDLER-006");
    assert_eq!(block_error.code, "LM-HANDLER-006");
    assert!(state.is_empty());

    Ok(())
}
