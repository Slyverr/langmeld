use langmeld_core::{Diagnostic, SourceFile};
use langmeld_language::CapturedValue;
use langmeld_language::{
    ConstructContext, ConstructHandler, ConstructHandlerRegistry, ConstructSpec, HandlerControl,
    HandlerRegistrationError, LanguageSpec, ParsedConstruct, PatternPart, TypedConstruct,
    TypedConstructHandler, parse,
};

struct NameView<'source> {
    name: &'source str,
}

impl<'source> TypedConstruct<'source> for NameView<'source> {
    const CONSTRUCT: &'static str = "greet";

    fn from_validated_construct(
        source: &'source SourceFile,
        construct: &'source ParsedConstruct,
    ) -> Result<Self, Diagnostic> {
        if construct.name != Self::CONSTRUCT {
            return Err(Diagnostic::error(
                "TEST-TYPED-001",
                "expected a `greet` construct",
                source,
                construct.span,
            ));
        }

        let Some(field) = construct.fields.iter().find(|field| field.name == "name") else {
            return Err(Diagnostic::error(
                "TEST-TYPED-002",
                "missing `name` capture",
                source,
                construct.span,
            ));
        };
        let CapturedValue::Identifier(name) = &field.value else {
            return Err(Diagnostic::error(
                "TEST-TYPED-003",
                "`name` is not an identifier",
                source,
                field.span,
            ));
        };

        Ok(Self { name })
    }
}

struct GreetHandler;

struct BorrowedState<'state> {
    prefix: &'state str,
    output: Vec<String>,
}

struct BorrowedGreetHandler;

impl<'state> TypedConstructHandler<BorrowedState<'state>> for BorrowedGreetHandler {
    type View<'source>
        = NameView<'source>
    where
        BorrowedState<'state>: 'source,
        (): 'source;

    fn handle<'source>(
        &self,
        context: &mut ConstructContext<'source, BorrowedState<'state>>,
        greet: Self::View<'source>,
    ) -> Result<HandlerControl, Diagnostic>
    where
        BorrowedState<'state>: 'source,
        (): 'source,
    {
        let greeting = format!("{} {}", context.state().prefix, greet.name);
        context.state_mut().output.push(greeting);

        Ok(HandlerControl::Continue)
    }
}

impl TypedConstructHandler<Vec<String>> for GreetHandler {
    type View<'source> = NameView<'source>;

    fn handle<'source>(
        &self,
        context: &mut ConstructContext<'source, Vec<String>>,
        greet: Self::View<'source>,
    ) -> Result<HandlerControl, Diagnostic>
    where
        Vec<String>: 'source,
        (): 'source,
    {
        context.state_mut().push(greet.name.to_owned());

        Ok(HandlerControl::Continue)
    }
}

struct MismatchedTypedViewHandler;

impl ConstructHandler<Vec<String>> for MismatchedTypedViewHandler {
    const CONSTRUCT: &'static str = "other";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, Vec<String>>,
    ) -> Result<HandlerControl, Diagnostic> {
        context.typed::<NameView<'_>>()?;

        Ok(HandlerControl::Continue)
    }
}

fn greeting_language() -> LanguageSpec {
    LanguageSpec::new(
        "Greeting",
        [
            ConstructSpec::new(
                "greet",
                [
                    PatternPart::literal("hello"),
                    PatternPart::identifier("name"),
                ],
            ),
            ConstructSpec::new(
                "other",
                [
                    PatternPart::literal("goodbye"),
                    PatternPart::identifier("name"),
                ],
            ),
        ],
    )
    .expect("the test grammar is valid")
}

#[test]
fn typed_handler_registry_creates_and_passes_the_registered_view() {
    let language = greeting_language();
    let source = SourceFile::new("greeting.txt", "hello Ada");
    let parsed = parse(&source, &language);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers
        .register_typed(GreetHandler)
        .expect("handler registration succeeds");
    let mut state = Vec::new();

    handlers
        .dispatch_parse_result(&source, &parsed, &mut state)
        .expect("typed capture access succeeds");

    assert_eq!(state, ["Ada"]);
}

#[test]
fn typed_handler_registration_rejects_duplicate_construct_names() {
    let mut handlers: ConstructHandlerRegistry<Vec<String>> = ConstructHandlerRegistry::new();
    handlers
        .register_typed(GreetHandler)
        .expect("first typed handler registration succeeds");

    let error = handlers
        .register_typed(GreetHandler)
        .expect_err("a second handler for the same view must be rejected");

    assert_eq!(
        error,
        HandlerRegistrationError::DuplicateConstruct("greet".to_owned())
    );
}

#[test]
fn typed_handler_reports_runtime_definition_capture_mismatch() {
    let language = LanguageSpec::new(
        "Greeting",
        [ConstructSpec::new("greet", [PatternPart::literal("hello")])],
    )
    .expect("the runtime grammar is valid");
    let source = SourceFile::new("greeting.txt", "hello");
    let parsed = parse(&source, &language);
    let mut handlers: ConstructHandlerRegistry<Vec<String>> = ConstructHandlerRegistry::new();
    handlers
        .register_typed(GreetHandler)
        .expect("typed handler registration succeeds");
    let mut state = Vec::new();

    let diagnostic = handlers
        .dispatch_parse_result(&source, &parsed, &mut state)
        .expect_err("runtime captures must be validated against the typed view");

    assert_eq!(diagnostic.code, "TEST-TYPED-002");
    assert!(state.is_empty());
}

#[test]
fn typed_handler_registration_supports_borrowing_host_state() {
    let language = greeting_language();
    let source = SourceFile::new("greeting.txt", "hello Ada");
    let parsed = parse(&source, &language);
    let prefix = "Hello";
    let mut state = BorrowedState {
        prefix,
        output: Vec::new(),
    };
    let mut handlers: ConstructHandlerRegistry<BorrowedState<'_>> = ConstructHandlerRegistry::new();
    handlers
        .register_typed(BorrowedGreetHandler)
        .expect("typed handler registration succeeds");

    handlers
        .dispatch_parse_result(&source, &parsed, &mut state)
        .expect("typed capture access succeeds with borrowed host state");

    assert_eq!(state.output, ["Hello Ada"]);
}

#[test]
fn handler_context_rejects_a_typed_view_for_another_construct() {
    let language = greeting_language();
    let source = SourceFile::new("greeting.txt", "goodbye Ada");
    let parsed = parse(&source, &language);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers
        .register(MismatchedTypedViewHandler)
        .expect("handler registration succeeds");
    let mut state = Vec::new();

    let diagnostic = handlers
        .dispatch_parse_result(&source, &parsed, &mut state)
        .expect_err("the handler must not receive a view for another construct");

    assert_eq!(diagnostic.code, "LM-TYPED-001");
    let location = diagnostic
        .location
        .expect("the mismatch should point to the handled construct");
    assert_eq!(location.source_name(), source.name());
    assert_eq!(location.span(), parsed.constructs()[0].span);
    assert!(location.is_valid_for(&source));
    assert!(state.is_empty());
}
