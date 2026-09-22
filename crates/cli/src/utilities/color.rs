use std::{io::IsTerminal, sync::OnceLock};

static COLOR_ENABLED: OnceLock<bool> = OnceLock::new();

fn color_enabled() -> bool {
    *COLOR_ENABLED.get_or_init(|| {
        std::env::var_os("NO_COLOR").is_none()
            && std::io::stdout().is_terminal()
            && std::io::stderr().is_terminal()
    })
}

fn ansi(code: &'static str) -> &'static str {
    if color_enabled() { code } else { "" }
}

pub(crate) fn reset() -> &'static str {
    ansi("\x1b[0m")
}

pub(crate) fn green() -> &'static str {
    ansi("\x1b[1;32m")
}

pub(crate) fn yellow() -> &'static str {
    ansi("\x1b[1;33m")
}

pub(crate) fn red() -> &'static str {
    ansi("\x1b[1;31m")
}

pub(crate) fn blue() -> &'static str {
    ansi("\x1b[1;34m")
}

pub(crate) fn bold() -> &'static str {
    ansi("\x1b[1m")
}

pub(crate) fn dim() -> &'static str {
    ansi("\x1b[2m")
}
