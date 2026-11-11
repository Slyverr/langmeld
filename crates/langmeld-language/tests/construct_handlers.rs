use langmeld_core::{SourceFile, Span};
use langmeld_language::{
    CapturedField, CapturedValue, ConstructContext, ConstructHandler, ConstructHandlerRegistry,
    HandlerControl, HandlerRegistrationError, ParsedConstruct,
};

struct RecordIdentifier;

impl ConstructHandler<Vec<String>> for RecordIdentifier {
    const CONSTRUCT: &'static str = "record";

    fn handle(
        &self,
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, Vec<String>>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let value = context.field_identifier(construct, "value")?;
        context.state_mut().push(value.to_owned());

        Ok(HandlerControl::Continue)
    }
}

struct RecordRepeatedFields;

impl ConstructHandler<Vec<String>> for RecordRepeatedFields {
    const CONSTRUCT: &'static str = "record";

    fn handle(
        &self,
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, Vec<String>>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        let mut values = Vec::new();
        for field in context.fields(construct, "value") {
            values.push(context.captured_text(field)?.to_owned());
        }

        context.state_mut().extend(values);

        Ok(HandlerControl::Continue)
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
        error.location.expect("error should have a span").span,
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
        construct: &ParsedConstruct,
        context: &mut ConstructContext<'_, ()>,
    ) -> Result<HandlerControl, langmeld_core::Diagnostic> {
        context.field_identifier(construct, "value")?;
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
        error.location.expect("error should have a span").span,
        Span::new(0, 7)
    );

    Ok(())
}

struct ReturnSignal;

impl ConstructHandler<(), String> for ReturnSignal {
    const CONSTRUCT: &'static str = "return";

    fn handle(
        &self,
        _construct: &ParsedConstruct,
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
