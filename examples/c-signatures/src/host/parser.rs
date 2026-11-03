use langmeld::{
    core::{Diagnostic, SourceFile, Span},
    language::{AstNode, CapturedValue, CustomConstructMatch, CustomConstructParser, Value},
};

/// Registers the parser declared by `translation_unit` in the `.lm` model.
pub fn custom_parser_callbacks() -> Vec<CustomConstructParser> {
    vec![CustomConstructParser::new(
        "c_translation_unit",
        parse_translation_unit,
    )]
}

fn parse_translation_unit(
    source: &SourceFile,
    offset: usize,
) -> Result<Option<CustomConstructMatch>, Diagnostic> {
    if !source
        .text
        .get(..offset)
        .is_some_and(|prefix| prefix.trim().is_empty())
    {
        return Ok(None);
    }

    let mut children = Vec::new();
    let mut line_offset = 0;
    for raw_line in source.text.split_inclusive('\n') {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line_offset += raw_line.len();
            continue;
        }
        let leading = line.len() - line.trim_start().len();
        let start = line_offset + leading;
        children.push(parse_prototype(start, trimmed)?);
        line_offset += raw_line.len();
    }

    if children.is_empty() {
        return Err(parse_error(
            "translation unit must contain at least one supported prototype",
            Span::new(0, source.text.len()),
        ));
    }

    Ok(Some(
        CustomConstructMatch::new(Span::new(0, source.text.len()), "default")
            .with_children(children),
    ))
}

fn parse_prototype(start: usize, line: &str) -> Result<AstNode, Diagnostic> {
    let mut cursor = Cursor::new(line, start);
    let return_type = cursor.identifier()?;
    cursor.space();
    let name = cursor.identifier()?;
    cursor.space();
    cursor.expect("(")?;
    cursor.space();

    if return_type.value == "void" {
        cursor.expect("void")?;
        cursor.space();
        cursor.expect(")")?;
        cursor.space();
        cursor.expect(";")?;
        cursor.finish()?;
        return Ok(AstNode::new(
            "prototype0",
            "signature",
            Span::new(start, start + line.len()),
        )
        .with_field(
            "name",
            CapturedValue::new(Value::Identifier(name.value), name.span),
        ));
    }
    if return_type.value != "int" {
        return Err(parse_error(
            format!("unsupported C return type `{}`", return_type.value),
            Span::new(start, start + line.len()),
        ));
    }

    let first_type = cursor.identifier()?;
    if first_type.value != "int" {
        return Err(parse_error(
            format!("unsupported C parameter type `{}`", first_type.value),
            Span::new(start, start + line.len()),
        ));
    }
    cursor.space();
    let left = cursor.identifier()?;
    cursor.space();
    cursor.expect(",")?;
    cursor.space();
    let second_type = cursor.identifier()?;
    if second_type.value != "int" {
        return Err(parse_error(
            format!("unsupported C parameter type `{}`", second_type.value),
            Span::new(start, start + line.len()),
        ));
    }
    cursor.space();
    let right = cursor.identifier()?;
    cursor.space();
    cursor.expect(")")?;
    cursor.space();
    cursor.expect(";")?;
    cursor.finish()?;

    if left.value == right.value {
        return Err(parse_error(
            "parameter names must be distinct in this C signature subset",
            Span::new(left.span.start, right.span.end),
        ));
    }

    Ok(AstNode::new(
        "prototype2",
        "signature",
        Span::new(start, start + line.len()),
    )
    .with_field(
        "name",
        CapturedValue::new(Value::Identifier(name.value), name.span),
    )
    .with_field(
        "left",
        CapturedValue::new(Value::Identifier(left.value), left.span),
    )
    .with_field(
        "right",
        CapturedValue::new(Value::Identifier(right.value), right.span),
    ))
}

struct Identifier {
    value: String,
    span: Span,
}

struct Cursor<'a> {
    line: &'a str,
    base: usize,
    position: usize,
}

impl<'a> Cursor<'a> {
    fn new(line: &'a str, base: usize) -> Self {
        Self {
            line,
            base,
            position: 0,
        }
    }

    fn space(&mut self) {
        while self
            .line
            .as_bytes()
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn identifier(&mut self) -> Result<Identifier, Diagnostic> {
        let start = self.position;
        let bytes = self.line.as_bytes();
        let Some(first) = bytes.get(self.position).copied() else {
            return Err(self.expected("identifier"));
        };
        if !is_identifier_start(first) {
            return Err(self.expected("identifier"));
        }
        self.position += 1;
        while bytes
            .get(self.position)
            .copied()
            .is_some_and(is_identifier_continue)
        {
            self.position += 1;
        }
        Ok(Identifier {
            value: self.line[start..self.position].to_owned(),
            span: Span::new(self.base + start, self.base + self.position),
        })
    }

    fn expect(&mut self, expected: &str) -> Result<(), Diagnostic> {
        if self.line[self.position..].starts_with(expected) {
            self.position += expected.len();
            Ok(())
        } else {
            Err(self.expected(expected))
        }
    }

    fn finish(&self) -> Result<(), Diagnostic> {
        if self.position == self.line.len() {
            Ok(())
        } else {
            Err(self.expected("end of declaration"))
        }
    }

    fn expected(&self, expected: &str) -> Diagnostic {
        let position = (self.base + self.position).min(self.base + self.line.len());
        Diagnostic::error(
            "LMCS1001",
            format!("expected {expected} in C function prototype"),
            Span::new(position, position),
        )
    }
}

fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn parse_error(message: impl Into<String>, span: Span) -> Diagnostic {
    Diagnostic::error("LMCS1001", message, span)
}
