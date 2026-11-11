fn main() {
    match langmeld_embedded_definitions_example::parse_example() {
        Ok(parsed) => {
            let shape = parsed
                .constructs
                .first()
                .and_then(|program| program.children.first())
                .and_then(|draw| draw.fields.first())
                .and_then(|field| match &field.value {
                    langmeld_language::CapturedValue::Token { text, .. } => Some(text),
                    _ => None,
                });

            match shape {
                Some(shape) => println!("Geometry: draw({shape})"),
                None => {
                    eprintln!("parsed program did not contain a draw shape");
                    std::process::exit(1);
                }
            }
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
