fn main() {
    if let Err(error) = langmeld_build::generate_project() {
        panic!("{error}");
    }
}
