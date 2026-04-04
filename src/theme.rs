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

/// Gradient: yellow → white → purple across the string
pub fn gradient(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    if len == 0 {
        return String::new();
    }

    // Colour stops: yellow → white → purple
    let stops: &[(u8, u8, u8)] = &[
        (252, 244,  52),  // #FCF434 yellow
        (255, 255, 255),  // #FFFFFF white
        (156,  89, 209),  // #9C59D1 purple
    ];

    let mut out = String::new();
    for (i, ch) in chars.iter().enumerate() {
        let t = i as f32 / (len - 1).max(1) as f32; // 0.0 → 1.0
        let scaled = t * (stops.len() - 1) as f32;
        let seg = (scaled as usize).min(stops.len() - 2);
        let local = scaled - seg as f32;

        let (r0, g0, b0) = stops[seg];
        let (r1, g1, b1) = stops[seg + 1];
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * local) as u8;

        let r = lerp(r0, r1);
        let g = lerp(g0, g1);
        let b = lerp(b0, b1);
        out.push_str(&format!("{}", ch.to_string().truecolor(r, g, b)));
    }
    out
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
