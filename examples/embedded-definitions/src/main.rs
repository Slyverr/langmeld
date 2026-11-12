fn main() {
    match langmeld_embedded_definitions_example::parse_example() {
        Ok(parsed) => {
            let statements = parsed
                .constructs()
                .first()
                .map_or(&[][..], |program| program.children.as_slice());
            let shape = statements
                .iter()
                .find(|statement| statement.name == "draw")
                .and_then(|draw| draw.fields.first())
                .and_then(|field| match &field.value {
                    langmeld_language::CapturedValue::Token { text, .. } => Some(text),
                    _ => None,
                });
            let message = statements
                .iter()
                .find(|statement| statement.name == "host_note")
                .and_then(|note| note.fields.first())
                .and_then(|field| match &field.value {
                    langmeld_language::CapturedValue::String(text) => Some(text),
                    _ => None,
                });

            let (Some(shape), Some(message)) = (shape, message) else {
                eprintln!("parsed program did not contain both expected statements");
                std::process::exit(1);
            };

            println!("Geometry: draw({shape})");
            println!("Rust-authored: note({message})");
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
