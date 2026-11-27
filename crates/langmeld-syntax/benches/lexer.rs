//! Lexing throughput on about 1 MiB of code-like text, with few and with many
//! declared words.

use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use langmeld_syntax::{
    FileId, FloatPreset, IdentStyle, Lexer, Preset, SourceFile, StringPreset, SyntaxKind, TokenSpec,
};

const SIZE: usize = 1 << 20;

fn kind(n: usize) -> SyntaxKind {
    SyntaxKind(u16::try_from(n).expect("few kinds"))
}

/// A lexer with `words` reserved words and `words` operators besides the basic ones.
fn language(words: usize) -> Lexer {
    let float = FloatPreset::default().exponent(true);
    let string = StringPreset::default().double(true);
    let mut specs = vec![
        TokenSpec::regex(kind(1), "space", r"[ \t\r\n]+").skip(),
        TokenSpec::line_comment(kind(2), "//"),
        TokenSpec::preset(kind(3), "identifier", Preset::Ident(IdentStyle::Xid)),
        TokenSpec::regex(kind(4), "integer", "[0-9]+"),
        TokenSpec::preset(kind(5), "float", Preset::Float(float)),
        TokenSpec::preset(kind(6), "string", Preset::String(string)),
    ];
    let punct = [
        "{", "}", "[", "]", "(", ")", ":", ",", ";", "=", "+", "-", "*", "<",
    ];
    specs.extend(
        (10..)
            .zip(punct)
            .map(|(n, text)| TokenSpec::punct(kind(n), text)),
    );
    for n in 0..words {
        specs.push(TokenSpec::keyword(kind(100 + n), format!("word{n}")));
        specs.push(TokenSpec::punct(kind(1000 + n), format!("@{n}")));
    }
    Lexer::builder()
        .tokens(specs)
        .build()
        .expect("the declarations are valid")
}

fn repeated(piece: &str) -> SourceFile {
    let text = piece.repeat(SIZE / piece.len() + 1);
    SourceFile::new(FileId::new(0), "bench", text).expect("small")
}

fn lexing(c: &mut Criterion) {
    let code = repeated(
        "// Adds two numbers.\nword1 add(first, second) { total = first + second * 2.5e3; }\n\
         word2 name = \"text\"; list = [1, 2, 3];\n",
    );
    let small = language(5);
    let large = language(300);

    let mut group = c.benchmark_group("lex");
    group.throughput(Throughput::Bytes(code.text().len() as u64));
    for (name, lexer) in [("5 words", &small), ("300 words", &large)] {
        group.bench_function(name, |b| b.iter(|| lexer.lex(black_box(&code))));
    }
    group.finish();
}

criterion_group!(benches, lexing);
criterion_main!(benches);
