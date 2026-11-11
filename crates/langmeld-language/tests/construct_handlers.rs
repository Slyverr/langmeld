use langmeld_core::{SourceDatabase, SourceFile, Span};
use langmeld_language::{
    CapturedField, CapturedValue, ConstructContext, ConstructHandler, ConstructHandlerRegistry,
    ConstructSpec, HandlerControl, HandlerRegistrationError, LanguageSpec, ParsedConstruct,
    PatternPart, parse,
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

fn construct(name: &str, span: Span, children: Vec<ParsedConstruct>) -> ParsedConstruct {
    ParsedConstruct {
        name: name.to_owned(),
        variant: 0,
        span,
        fields: Vec::new(),
        children,
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

    assert_eq!(error.code, "LM-HANDLER-005");
    assert!(state.is_empty());
}

#[test]
fn dispatches_block_children_in_source_order() -> Result<(), Box<dyn std::error::Error>> {
    let source = SourceFile::new("main.sample", "record first; record second;");
    let first = ParsedConstruct {
        name: "record".to_owned(),
        variant: 0,
        span: Span::new(0, 13),
        fields: vec![CapturedField {
            name: "value".to_owned(),
            value: CapturedValue::Identifier("first".to_owned()),
            span: Span::new(7, 12),
        }],
        children: Vec::new(),
    };
    let second = ParsedConstruct {
        name: "record".to_owned(),
        variant: 0,
        span: Span::new(14, 27),
        fields: vec![CapturedField {
            name: "value".to_owned(),
            value: CapturedValue::Identifier("second".to_owned()),
            span: Span::new(21, 27),
        }],
        children: Vec::new(),
    };
    let block = construct("program", Span::new(0, 27), vec![first, second]);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;
    let mut state = Vec::new();

    let result = handlers
        .dispatch_block(&source, &block, &mut state)
        .expect("registered handlers should process their constructs");

    assert_eq!(result, HandlerControl::Continue);
    assert_eq!(state, ["first", "second"]);

    Ok(())
}

#[test]
fn exposes_repeated_captures_in_source_order() -> Result<(), Box<dyn std::error::Error>> {
    let source = SourceFile::new("main.sample", "record alpha beta");
    let node = ParsedConstruct {
        name: "record".to_owned(),
        variant: 0,
        span: Span::new(0, source.text().len()),
        fields: vec![
            CapturedField {
                name: "value".to_owned(),
                value: CapturedValue::Identifier("alpha".to_owned()),
                span: Span::new(7, 12),
            },
            CapturedField {
                name: "value".to_owned(),
                value: CapturedValue::Identifier("beta".to_owned()),
                span: Span::new(13, 17),
            },
        ],
        children: Vec::new(),
    };
    let block = construct("program", Span::new(0, source.text().len()), vec![node]);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordRepeatedFields)?;
    let mut state = Vec::new();

    handlers
        .dispatch_block(&source, &block, &mut state)
        .expect("registered handler should process repeated fields");

    assert_eq!(state, ["alpha", "beta"]);

    Ok(())
}

#[test]
fn dispatches_nested_children_when_the_current_handler_requests_it()
-> Result<(), Box<dyn std::error::Error>> {
    let source = SourceFile::new("main.sample", "block { record child; }");
    let record = ParsedConstruct {
        name: "record".to_owned(),
        variant: 0,
        span: Span::new(8, 21),
        fields: vec![CapturedField {
            name: "value".to_owned(),
            value: CapturedValue::Identifier("child".to_owned()),
            span: Span::new(15, 20),
        }],
        children: Vec::new(),
    };
    let nested_block = construct("block", Span::new(0, 23), vec![record]);
    let root = construct("program", Span::new(0, 23), vec![nested_block]);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;
    handlers.register(DispatchChildren)?;
    let mut state = Vec::new();

    handlers
        .dispatch_block(&source, &root, &mut state)
        .expect("nested handlers should process their children");

    assert_eq!(state, ["child"]);

    Ok(())
}

#[test]
fn dispatches_a_root_construct_handler_and_its_children() -> Result<(), Box<dyn std::error::Error>>
{
    let source = SourceFile::new("main.sample", "program record child");
    let record = ParsedConstruct {
        name: "record".to_owned(),
        variant: 0,
        span: Span::new(8, 20),
        fields: vec![CapturedField {
            name: "value".to_owned(),
            value: CapturedValue::Identifier("child".to_owned()),
            span: Span::new(15, 20),
        }],
        children: Vec::new(),
    };
    let root = construct("program", Span::new(0, 20), vec![record]);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(RecordIdentifier)?;
    handlers.register(DispatchProgram)?;
    let mut state = Vec::new();

    let control = handlers
        .dispatch_construct(&source, &root, &mut state)
        .expect("root handler should dispatch its children");

    assert_eq!(control, HandlerControl::Continue);
    assert_eq!(state, ["program start", "child", "program end"]);

    Ok(())
}

#[test]
fn reports_unregistered_construct_at_its_source_span() {
    let source = SourceFile::new("main.sample", "unknown;");
    let node = construct("unknown", Span::new(0, 8), Vec::new());
    let block = construct("program", Span::new(0, 8), vec![node]);
    let handlers = ConstructHandlerRegistry::<()>::new();

    let error = handlers
        .dispatch_block(&source, &block, &mut ())
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
    let source = SourceFile::new("main.sample", "record;");
    let node = construct("record", Span::new(0, 7), Vec::new());
    let block = construct("program", Span::new(0, 7), vec![node]);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers.register(MissingField)?;

    let error = handlers
        .dispatch_block(&source, &block, &mut ())
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
    let source = SourceFile::new("main.sample", "return; ignored;");
    let block = construct(
        "program",
        Span::new(0, 16),
        vec![
            construct("return", Span::new(0, 7), Vec::new()),
            construct("ignored", Span::new(8, 16), Vec::new()),
        ],
    );
    let mut handlers = ConstructHandlerRegistry::<(), String>::new();
    handlers.register(ReturnSignal)?;

    let result = handlers
        .dispatch_block(&source, &block, &mut ())
        .expect("return signal should propagate");

    assert_eq!(result, HandlerControl::Signal("returned".to_owned()));

    Ok(())
}
