use owo_colors::OwoColorize;

// #FCF434 — yellow: server names, headings, active indicator
pub fn yellow(s: &str) -> String {
    format!("{}", s.truecolor(252, 244, 52))
}

// #9C59D1 — purple: success states, completions
#[allow(dead_code)]
pub fn purple(s: &str) -> String {
    format!("{}", s.truecolor(156, 89, 209))
}

pub fn success(msg: &str) {
    println!("{}  {msg}", "✓".truecolor(156, 89, 209));
}

pub fn error(msg: &str) {
    println!("{}  {msg}", "✗".red());
}

pub fn warn(msg: &str) {
    println!("{}  {msg}", "⚠".truecolor(252, 244, 52));
}
