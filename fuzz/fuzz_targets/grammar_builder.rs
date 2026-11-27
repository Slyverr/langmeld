//! Any grammar is either refused with explanations or parses any text: building and
//! parsing never panic and always finish.

#![no_main]

use langmeld_syntax::pattern::{
    attempt, choice, field, lit, look, many, many1, not, opt, r, sep, sep_strict, seq, terminated,
};
use langmeld_syntax::{
    Assoc, ExprSpec, FileId, IdentStyle, IntPreset, Language, Pattern, Preset, SourceFile, Token,
    parse,
};
use libfuzzer_sys::fuzz_target;

const RULES: [&str; 5] = ["A", "B", "C", "D", "E"];
const LITERALS: [&str; 10] = ["a", "from", "(", ")", "{", "}", ",", ";", ">", ">>"];
const PIECES: [&str; 14] = [
    "a", "from", "(", ")", "{", "}", ",", ";", ">", "x", "1", " ", "\n", "@",
];

/// Reads choices from the input, and zeros once it is used up.
struct Choices<'a>(&'a [u8]);

impl Choices<'_> {
    fn next(&mut self, below: usize) -> usize {
        let Some((&byte, rest)) = self.0.split_first() else {
            return 0;
        };
        self.0 = rest;
        usize::from(byte) % below
    }

    fn literal(&mut self) -> &'static str {
        LITERALS[self.next(LITERALS.len())]
    }

    fn reference(&mut self) -> Pattern {
        match self.next(RULES.len() + 4) {
            n if n < RULES.len() => r(RULES[n]),
            n if n == RULES.len() => r("name"),
            n if n == RULES.len() + 1 => r("int"),
            n if n == RULES.len() + 2 => r("Expr"),
            _ => lit(self.literal()),
        }
    }

    fn pattern(&mut self, depth: u32) -> Pattern {
        let kinds = if depth > 2 { 2 } else { 15 };
        match self.next(kinds) {
            0 => lit(self.literal()),
            1 => self.reference(),
            2 => {
                let parts = 1 + self.next(3);
                seq((0..parts).map(|_| self.pattern(depth + 1)))
            }
            3 => {
                let parts = 1 + self.next(3);
                choice((0..parts).map(|_| self.pattern(depth + 1)))
            }
            4 => opt(self.pattern(depth + 1)),
            5 => many(self.pattern(depth + 1)),
            6 => many1(self.pattern(depth + 1)),
            7 => {
                let item = self.pattern(depth + 1);
                sep(item, self.literal())
            }
            8 => {
                let item = self.pattern(depth + 1);
                sep_strict(item, self.literal())
            }
            9 => terminated(self.pattern(depth + 1)),
            10 => field("part", self.pattern(depth + 1)),
            11 => look(self.pattern(depth + 1)),
            12 => not(self.pattern(depth + 1)),
            13 => attempt(self.pattern(depth + 1)),
            _ => self.reference(),
        }
    }
}

fn language(choices: &mut Choices<'_>) -> Option<Language> {
    let mut builder = Language::builder("Fuzz")
        .token(Token::skip("space", "[ \t\r\n]+"))
        .token(Token::preset("name", Preset::Ident(IdentStyle::Ascii)))
        .token(Token::preset("int", Preset::Int(IntPreset::default())))
        .contextual(["from"]);
    if choices.next(3) == 0 {
        builder = builder.terminators([";"]);
    }
    let rules = 1 + choices.next(RULES.len());
    for name in &RULES[..rules] {
        builder = builder.rule(*name, choices.pattern(0));
    }
    let mut expr = ExprSpec::new("Expr")
        .atom("Number", r("int"))
        .atom("Custom", choices.pattern(1));
    for n in 0..choices.next(4) {
        let name = format!("Operator{n}");
        let symbol = choices.literal();
        let precedence = 1 + choices.next(4) as u16;
        expr = match choices.next(5) {
            0 => expr.prefix(name, [symbol], precedence),
            1 => expr.infix(name, [symbol], precedence, Assoc::Left),
            2 => expr.infix(name, [symbol], precedence, Assoc::Right),
            3 => expr.infix(name, [symbol], precedence, Assoc::None),
            _ => {
                let body = seq([lit(symbol), choices.pattern(2)]);
                expr.postfix(name, body, precedence)
            }
        };
    }
    builder.expr(expr).build().ok()
}

fuzz_target!(|data: &[u8]| {
    let split = data.len() / 2;
    let (grammar, input) = data.split_at(split);
    let Some(language) = language(&mut Choices(grammar)) else {
        return;
    };
    let text: String = input
        .iter()
        .take(200)
        .map(|&byte| PIECES[usize::from(byte) % PIECES.len()])
        .collect();
    let file = SourceFile::new(FileId::new(0), "fuzz", text.as_str()).expect("small");
    let parsed = parse(&language, &file);
    assert_eq!(
        parsed.syntax().text().to_string(),
        text,
        "the tree is lossless"
    );
    for diagnostic in &parsed.diagnostics {
        assert!(usize::from(diagnostic.range.end()) <= text.len());
        // Declared grammars never loop without taking a token.
        assert_ne!(diagnostic.code, "LM-P004", "the parser stalled");
    }
});
