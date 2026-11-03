use langmeld::{
    core::{Diagnostic, SourceFile, Span},
    language::{
        AstNode, CapturedValue, CustomConstructMatch, CustomConstructParser, CustomLexer,
        LanguageToken, LanguageTokenKind, LexedSource, Value,
    },
};
use proc_macro2::{LineColumn, Span as RustSpan};
use quote::ToTokens;
use syn::{FnArg, Item, ReturnType, Type, spanned::Spanned};

use crate::{RustFunctionParameter, RustFunctionSignature};

pub(super) fn callbacks() -> Vec<CustomConstructParser> {
    vec![CustomConstructParser::new("rust_source", parse_rust_source)]
}

pub(super) fn lexer() -> CustomLexer {
    CustomLexer::new(|source| {
        if source.text.is_empty() {
            return LexedSource::default();
        }
        LexedSource::new(
            [LanguageToken::new(
                LanguageTokenKind::Opaque,
                Span::new(0, source.text.len()),
            )],
            [],
        )
    })
}

fn parse_rust_source(
    source: &SourceFile,
    offset: usize,
) -> Result<Option<CustomConstructMatch>, Diagnostic> {
    let file = syn::parse_file(&source.text).map_err(|error| {
        let offsets = SourceOffsets::new(&source.text);
        Diagnostic::error(
            "LMRS1001",
            format!("Rust source parser: {error}"),
            offsets.span(error.span()),
        )
    })?;
    let offsets = SourceOffsets::new(&source.text);
    let children = file
        .items
        .into_iter()
        .filter_map(|item| match item {
            Item::Fn(function) => {
                let name = function.sig.ident.to_string();
                let name_span = offsets.span(function.sig.ident.span());
                let signature_span = offsets.span(function.sig.span());
                let node_span = offsets.span(function.span());
                let signature = function.sig.to_token_stream().to_string();
                let host_signature = RustFunctionSignature {
                    is_async: function.sig.asyncness.is_some(),
                    is_unsafe: function.sig.unsafety.is_some(),
                    generics: function
                        .sig
                        .generics
                        .params
                        .iter()
                        .map(|parameter| parameter.to_token_stream().to_string())
                        .collect(),
                    parameters: function
                        .sig
                        .inputs
                        .iter()
                        .map(|input| match input {
                            FnArg::Receiver(receiver) => RustFunctionParameter {
                                pattern: receiver.self_token.to_token_stream().to_string(),
                                type_text: receiver.ty.to_token_stream().to_string(),
                            },
                            FnArg::Typed(parameter) => RustFunctionParameter {
                                pattern: parameter.pat.to_token_stream().to_string(),
                                type_text: parameter.ty.to_token_stream().to_string(),
                            },
                        })
                        .collect(),
                    return_type: match &function.sig.output {
                        ReturnType::Default => None,
                        ReturnType::Type(_, ty) => Some(match ty.as_ref() {
                            Type::Verbatim(tokens) => tokens.to_string(),
                            ty => ty.to_token_stream().to_string(),
                        }),
                    },
                };
                Some(
                    AstNode::new("function", "default", node_span)
                        .with_field(
                            "name",
                            CapturedValue::new(Value::Identifier(name), name_span),
                        )
                        .with_field(
                            "signature",
                            CapturedValue::new(Value::Text(signature), signature_span),
                        )
                        .with_host_data(host_signature),
                )
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    Ok(Some(
        CustomConstructMatch::new(Span::new(offset, source.text.len()), "default")
            .with_children(children),
    ))
}

struct SourceOffsets<'a> {
    source: &'a str,
    line_starts: Vec<usize>,
}

impl<'a> SourceOffsets<'a> {
    fn new(source: &'a str) -> Self {
        let line_starts = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(index, _)| index + 1))
            .collect();
        Self {
            source,
            line_starts,
        }
    }

    fn span(&self, span: RustSpan) -> Span {
        Span::new(self.offset(span.start()), self.offset(span.end()))
    }

    fn offset(&self, location: LineColumn) -> usize {
        let Some(&line_start) = location
            .line
            .checked_sub(1)
            .and_then(|line| self.line_starts.get(line))
        else {
            return self.source.len();
        };
        let line = self.source[line_start..]
            .split_once('\n')
            .map_or(&self.source[line_start..], |(line, _)| line);
        let column = line
            .char_indices()
            .nth(location.column)
            .map_or(line.len(), |(byte, _)| byte);
        line_start + column
    }
}
