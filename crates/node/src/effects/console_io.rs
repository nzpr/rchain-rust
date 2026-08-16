//! Console I/O interface (port of `effects/ConsoleIO.scala`).

use rchain_shared::string_ops::ColoredString;

/// Console I/O (port of `ConsoleIO[F]`; the `F[_]` effect is simplified to synchronous calls).
pub trait ConsoleIo {
    /// Read a line, returning `None` on EOF (the Scala `null`).
    fn read_line(&mut self) -> Option<String>;
    fn read_password(&mut self, prompt: &str) -> String;
    fn println(&mut self, s: &str);
    fn println_colored(&mut self, s: &ColoredString);
    fn update_completion(&mut self, history: &[String]);
    fn close(&mut self);
}

/// A no-op console (port of `NOPConsoleIO`).
#[derive(Default)]
pub struct NopConsoleIo;

impl ConsoleIo for NopConsoleIo {
    fn read_line(&mut self) -> Option<String> {
        Some(String::new())
    }

    fn read_password(&mut self, _prompt: &str) -> String {
        String::new()
    }

    fn println(&mut self, _s: &str) {}

    fn println_colored(&mut self, _s: &ColoredString) {}

    fn update_completion(&mut self, _history: &[String]) {}

    fn close(&mut self) {}
}
