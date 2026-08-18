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

/// A stdin/stdout console (port of `effects.consoleIO`/`JLineConsoleIO`; the jline line-editing
/// and prompt are not reproduced — only plain line reads/writes).
#[derive(Default)]
pub struct StdioConsole;

impl ConsoleIo for StdioConsole {
    fn read_line(&mut self) -> Option<String> {
        let mut line = String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => {
                if line.ends_with('\n') {
                    line.pop();
                    if line.ends_with('\r') {
                        line.pop();
                    }
                }
                Some(line)
            }
            Err(_) => None,
        }
    }

    fn read_password(&mut self, prompt: &str) -> String {
        use std::io::Write;
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(prompt.as_bytes());
        let _ = stdout.flush();
        let mut line = String::new();
        let _ = std::io::stdin().read_line(&mut line);
        line.trim().to_string()
    }

    fn println(&mut self, s: &str) {
        println!("{s}");
    }

    fn println_colored(&mut self, s: &ColoredString) {
        println!("{}", s.colorize());
    }

    fn update_completion(&mut self, _history: &[String]) {}

    fn close(&mut self) {}
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
