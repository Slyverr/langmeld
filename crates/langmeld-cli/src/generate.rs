use std::{error::Error, path::PathBuf};

use clap::{Args, Subcommand};
use langmeld_textmate::{TextMateOptions, generate as generate_textmate};
use langmeld_tree_sitter::{TreeSitterOptions, generate as generate_tree_sitter};

use crate::generate_handler::{HandlerArgs, generate_handler};
use crate::project::load_language_definition;

#[derive(Debug, Args)]
pub(crate) struct GenerateArgs {
    #[command(subcommand)]
    command: GenerateCommand,
}

#[derive(Debug, Subcommand)]
enum GenerateCommand {
    /// Generate a Rust handler for a declared construct in an initialized language project.
    Handler(HandlerArgs),
    /// Generate a lexical TextMate grammar as JSON on standard output.
    Textmate(TextmateArgs),
    /// Generate a Tree-sitter grammar.js file on standard output.
    TreeSitter(TreeSitterArgs),
}

#[derive(Debug, Args)]
struct TextmateArgs {
    /// Path to langmeld.toml.
    #[arg(long, default_value = "langmeld.toml")]
    manifest_path: PathBuf,
    /// Unique TextMate scope name, such as `source.widget`.
    #[arg(long)]
    scope_name: String,
    /// Source file extension without a leading dot. May be repeated.
    #[arg(long = "file-type")]
    file_types: Vec<String>,
}

#[derive(Debug, Args)]
struct TreeSitterArgs {
    /// Path to langmeld.toml.
    #[arg(long, default_value = "langmeld.toml")]
    manifest_path: PathBuf,
    /// Tree-sitter grammar name, such as `widget_script`.
    #[arg(long)]
    grammar_name: String,
}

pub(crate) fn run(args: GenerateArgs) -> Result<(), Box<dyn Error>> {
    match args.command {
        GenerateCommand::Handler(args) => generate_handler(args),
        GenerateCommand::Textmate(args) => generate_textmate_grammar(args),
        GenerateCommand::TreeSitter(args) => generate_tree_sitter_grammar(args),
    }
}

fn generate_textmate_grammar(args: TextmateArgs) -> Result<(), Box<dyn Error>> {
    let language = load_language_definition(&args.manifest_path)?;
    let options = TextMateOptions::new(args.scope_name).file_types(args.file_types);
    let grammar = generate_textmate(&language, &options)?;
    println!("{grammar}");
    Ok(())
}

fn generate_tree_sitter_grammar(args: TreeSitterArgs) -> Result<(), Box<dyn Error>> {
    let language = load_language_definition(&args.manifest_path)?;
    let options = TreeSitterOptions::new(args.grammar_name);
    let grammar = generate_tree_sitter(&language, &options)?;
    print!("{grammar}");
    Ok(())
}
