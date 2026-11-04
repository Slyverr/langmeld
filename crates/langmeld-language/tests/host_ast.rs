use langmeld_core::{SourceFile, Span};
use langmeld_language::{AstNode, CustomConstructMatch, CustomConstructParser, HostData};

#[derive(Debug, PartialEq, Eq)]
struct NativeExpression {
    operator: &'static str,
}

#[test]
fn generic_nodes_retain_and_downcast_host_owned_ast_values() {
    let node = AstNode::new("expression", "binary", Span::new(0, 5))
        .with_host_data(NativeExpression { operator: "+" });
    let clone = node.clone();

    assert_eq!(
        node.host_data::<NativeExpression>(),
        Some(&NativeExpression { operator: "+" })
    );
    assert_eq!(node, clone);
    assert!(node.host_data::<String>().is_none());

    let separate = AstNode::new("expression", "binary", Span::new(0, 5))
        .with_host_data(NativeExpression { operator: "+" });
    assert_ne!(node.host_data, separate.host_data);
}

#[test]
fn custom_construct_parser_can_return_a_host_ast_projection() {
    let parser = CustomConstructParser::new("native-node", |source, offset| {
        let Some(rest) = source.text.get(offset..) else {
            return Err(langmeld_core::Diagnostic::error(
                "TEST0001",
                "offset is not a character boundary",
                Span::new(offset.min(source.text.len()), source.text.len()),
            ));
        };
        Ok(rest.starts_with("node").then(|| {
            CustomConstructMatch::new(Span::new(offset, offset + 4), "native-node")
                .with_host_data(NativeExpression { operator: "custom" })
        }))
    });
    let source = SourceFile::new("custom.source", "node value");

    let matched = (parser.parse)(&source, 0)
        .expect("custom parser runs")
        .expect("construct matches");
    assert_eq!(matched.span, Span::new(0, 4));
    assert_eq!(
        matched
            .host_data
            .as_ref()
            .and_then(HostData::downcast_ref::<NativeExpression>),
        Some(&NativeExpression { operator: "custom" })
    );
}
