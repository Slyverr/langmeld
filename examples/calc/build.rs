//! Checks the grammar when the example is built, writes the Rust types of its trees and
//! compiles it, so that the program starts without reading it.

fn main() {
    if let Err(error) = langmeld::build::generate() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
