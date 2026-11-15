fn main() {
    if let Err(error) = langmeld_build::generate_project_with_embedded_sources() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
