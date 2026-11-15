fn main() {
    if let Err(error) = langmeld_build::generate_embedded_sources() {
        panic!("{error}");
    }
}
