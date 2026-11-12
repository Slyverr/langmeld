use langmeld_core::{Diagnostic, SourceFile};
use langmeld_language::CapturedValue;
use langmeld_language::{
    ConstructContext, ConstructHandler, ConstructHandlerRegistry, ConstructSpec, HandlerControl,
    LanguageSpec, ParsedConstruct, PatternPart, TypedConstruct, parse,
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

impl ConstructHandler<Vec<String>> for GreetHandler {
    const CONSTRUCT: &'static str = "greet";

    fn handle(
        &self,
        context: &mut ConstructContext<'_, Vec<String>>,
    ) -> Result<HandlerControl, Diagnostic> {
        let greet = context.typed::<NameView<'_>>()?;
        context.state_mut().push(greet.name.to_owned());

        Ok(HandlerControl::Continue)
    }
}

#[test]
fn handler_context_creates_a_typed_view_of_the_current_construct() {
    let language = LanguageSpec::new(
        "Greeting",
        [ConstructSpec::new(
            "greet",
            [
                PatternPart::literal("hello"),
                PatternPart::identifier("name"),
            ],
        )],
    )
    .expect("the test grammar is valid");
    let source = SourceFile::new("greeting.txt", "hello Ada");
    let parsed = parse(&source, &language);
    let mut handlers = ConstructHandlerRegistry::new();
    handlers
        .register(GreetHandler)
        .expect("handler registration succeeds");
    let mut state = Vec::new();

    handlers
        .dispatch_parse_result(&source, &parsed, &mut state)
        .expect("typed capture access succeeds");

    assert_eq!(state, ["Ada"]);
}
