//! The code written for a grammar compiles, and when the grammar and the code drift
//! apart, the compiler says what to update.

#[test]
fn the_types_compile_and_the_compiler_finds_drift() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/pass/*.rs");
    cases.compile_fail("tests/ui/drift/*.rs");
}
