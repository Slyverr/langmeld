//! What carries the messages between a client and the server.

use std::io::{self, BufReader, Stdin};

use langmeld_meta::Editor;
use langmeld_syntax::Language;
use lsp_server::Message;

use super::{LanguageHooks, Server};

/// A way to exchange messages with a client.
pub trait Transport {
    /// The next message, or `None` once the client is gone.
    fn receive(&mut self) -> io::Result<Option<Message>>;

    /// Sends a message.
    fn send(&mut self, message: Message) -> io::Result<()>;
}

/// Messages on the standard input and output, as editors start language servers.
pub struct StdioTransport {
    input: BufReader<Stdin>,
}

impl StdioTransport {
    /// The transport of this process.
    pub fn new() -> Self {
        Self {
            input: BufReader::new(io::stdin()),
        }
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for StdioTransport {
    fn receive(&mut self) -> io::Result<Option<Message>> {
        Message::read(&mut self.input)
    }

    fn send(&mut self, message: Message) -> io::Result<()> {
        message.write(&mut io::stdout().lock())
    }
}

/// Gives `server` every message of `transport` until the client asks it to exit or
/// goes away, and returns the exit code: 0 after a proper shutdown, 1 otherwise.
pub fn serve_with<T: Transport, H: LanguageHooks>(
    mut transport: T,
    mut server: Server<'_, H>,
) -> io::Result<u8> {
    while let Some(message) = transport.receive()? {
        for reply in server.handle(message) {
            transport.send(reply)?;
        }
        if let Some(code) = server.exit_code() {
            return Ok(code);
        }
    }
    Ok(1)
}

/// Serves `language` on the standard input and output. See [`serve_with`]. `editor` is
/// what the language tells editors, as `Loaded::editor` has it.
pub fn serve<H: LanguageHooks>(language: &Language, editor: &Editor, hooks: H) -> io::Result<u8> {
    serve_with(StdioTransport::new(), Server::new(language, editor, hooks))
}
