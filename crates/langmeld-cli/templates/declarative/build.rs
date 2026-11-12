fn main() {
    if let Err(error) = langmeld_build::generate_project() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
