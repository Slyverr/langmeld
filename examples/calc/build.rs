//! Compiles the language file when the example is built, for the tests that compare the
//! compiled language with the others.

fn main() {
    if let Err(error) = langmeld::build::embed() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
