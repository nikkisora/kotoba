use ratatui::style::Color;
use serde::Deserialize;
use std::path::Path;

/// Application theme — all colors used throughout the UI.
///
/// Colors are organized into semantic categories:
/// - Base: backgrounds and foregrounds
/// - Palette: semantic colors (accent, info, success, warning, error, muted)
/// - Vocabulary: status-based highlighting in reader
/// - Review vocabulary: subtler versions for review sentence context
/// - UI elements: progress bars, status bar, cloze cards, chapter states
#[derive(Debug, Clone)]
pub struct Theme {
    // ── Base ──
    pub bg: Color,
    pub fg: Color,
    pub title_bar_bg: Color,
    pub title_bar_fg: Color,

    // ── Semantic palette ──
    pub accent: Color,  // Brand color — app name, selected items, cursor marker
    pub info: Color,    // Informational — borders, new-card state
    pub success: Color, // Positive — import, good rating, known words
    pub warning: Color, // Attention — due cards, learning, preprocessing
    pub error: Color,   // Destructive — delete, again rating, skipped
    pub muted: Color,   // De-emphasized — hints, help text, furigana
    pub subtle: Color,  // Secondary accent — group headers

    // ── Vocabulary status (reader — inline text highlighting) ──
    pub vocab_new_bg: Color,
    pub vocab_new_fg: Color,
    pub vocab_l1_bg: Color,
    pub vocab_l1_fg: Color,
    pub vocab_l2_bg: Color,
    pub vocab_l2_fg: Color,
    pub vocab_l3_bg: Color,
    pub vocab_l3_fg: Color,
    pub vocab_l4_bg: Color,
    pub vocab_l4_fg: Color,
    pub vocab_ignored_fg: Color,

    // ── Vocabulary status (review context — more subtle) ──
    pub review_vocab_new_bg: Color,
    pub review_vocab_l1_bg: Color,
    pub review_vocab_l2_bg: Color,
    pub review_vocab_l3_bg: Color,
    pub review_vocab_l4_bg: Color,

    // ── UI elements ──
    pub progress_bar: Color,
    pub stats_text: Color,
    pub status_msg_fg: Color,
    pub status_msg_bg: Color,
    pub cloze_reveal_fg: Color,
    pub cloze_reveal_bg: Color,
    pub chapter_in_progress: Color,
}

impl Theme {
    const BUILTIN_NAMES: &'static [&'static str] =
        &["tokyo-night", "light", "solarized-light", "gruvbox"];

    /// Get a built-in theme by name. Returns `None` if not a built-in.
    pub fn builtin(name: &str) -> Option<Self> {
        match name {
            "tokyo-night" => Some(Self::tokyo_night()),
            "light" => Some(Self::light()),
            "solarized-light" => Some(Self::solarized_light()),
            "gruvbox" => Some(Self::gruvbox()),
            _ => None,
        }
    }

    /// Directory where custom theme .toml files are stored.
    /// Same parent as the database: ~/.local/share/kotoba/themes/
    pub fn themes_dir() -> std::path::PathBuf {
        dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("kotoba")
            .join("themes")
    }

    /// List all available theme names: built-ins first, then custom .toml files
    /// discovered in the themes directory.
    pub fn available_themes() -> Vec<String> {
        let mut names: Vec<String> = Self::BUILTIN_NAMES.iter().map(|s| s.to_string()).collect();

        let themes_dir = Self::themes_dir();
        if let Ok(entries) = std::fs::read_dir(&themes_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        let name = stem.to_string();
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
            }
        }

        names
    }

    /// Tokyo Night (dark) — the default theme.
    pub fn tokyo_night() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::Reset,
            title_bar_bg: Color::Rgb(30, 30, 50),
            title_bar_fg: Color::White,

            accent: Color::Cyan,
            info: Color::Blue,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            muted: Color::DarkGray,
            subtle: Color::Magenta,

            vocab_new_bg: Color::Blue,
            vocab_new_fg: Color::White,
            vocab_l1_bg: Color::Yellow,
            vocab_l1_fg: Color::Black,
            vocab_l2_bg: Color::Rgb(200, 180, 60),
            vocab_l2_fg: Color::Black,
            vocab_l3_bg: Color::Rgb(160, 150, 80),
            vocab_l3_fg: Color::Black,
            vocab_l4_bg: Color::Rgb(120, 120, 100),
            vocab_l4_fg: Color::White,
            vocab_ignored_fg: Color::DarkGray,

            review_vocab_new_bg: Color::Rgb(60, 80, 160),
            review_vocab_l1_bg: Color::Rgb(120, 100, 40),
            review_vocab_l2_bg: Color::Rgb(100, 85, 30),
            review_vocab_l3_bg: Color::Rgb(80, 70, 20),
            review_vocab_l4_bg: Color::Rgb(60, 55, 15),

            progress_bar: Color::Rgb(100, 180, 100),
            stats_text: Color::Rgb(100, 140, 180),
            status_msg_fg: Color::Black,
            status_msg_bg: Color::Yellow,
            cloze_reveal_fg: Color::Green,
            cloze_reveal_bg: Color::Rgb(30, 60, 30),
            chapter_in_progress: Color::Rgb(100, 150, 255),
        }
    }

    /// Light theme — white background, black text, high contrast.
    pub fn light() -> Self {
        let white = Color::Rgb(255, 255, 255);
        let black = Color::Rgb(30, 30, 30);
        let gray50 = Color::Rgb(128, 128, 128);
        let gray80 = Color::Rgb(200, 200, 200);
        let gray93 = Color::Rgb(237, 237, 237);

        Self {
            bg: white,
            fg: black,
            title_bar_bg: Color::Rgb(230, 230, 230),
            title_bar_fg: Color::Rgb(30, 30, 30),

            accent: Color::Rgb(0, 95, 175),   // strong blue
            info: Color::Rgb(30, 120, 190),   // medium blue
            success: Color::Rgb(30, 130, 50), // dark green
            warning: Color::Rgb(170, 110, 0), // amber
            error: Color::Rgb(190, 30, 30),   // dark red
            muted: gray50,
            subtle: Color::Rgb(130, 60, 160), // purple

            vocab_new_bg: Color::Rgb(210, 230, 255), // light blue
            vocab_new_fg: Color::Rgb(0, 60, 120),
            vocab_l1_bg: Color::Rgb(255, 240, 200), // light amber
            vocab_l1_fg: Color::Rgb(120, 70, 0),
            vocab_l2_bg: Color::Rgb(255, 245, 220), // lighter amber
            vocab_l2_fg: Color::Rgb(100, 60, 0),
            vocab_l3_bg: Color::Rgb(230, 245, 230), // very light green
            vocab_l3_fg: Color::Rgb(30, 90, 30),
            vocab_l4_bg: gray93, // near-white
            vocab_l4_fg: Color::Rgb(60, 60, 60),
            vocab_ignored_fg: gray80,

            review_vocab_new_bg: Color::Rgb(225, 238, 255),
            review_vocab_l1_bg: Color::Rgb(255, 245, 225),
            review_vocab_l2_bg: Color::Rgb(255, 248, 235),
            review_vocab_l3_bg: Color::Rgb(240, 248, 240),
            review_vocab_l4_bg: Color::Rgb(245, 245, 245),

            progress_bar: Color::Rgb(30, 130, 50),
            stats_text: gray50,
            status_msg_fg: white,
            status_msg_bg: Color::Rgb(170, 110, 0),
            cloze_reveal_fg: Color::Rgb(30, 130, 50),
            cloze_reveal_bg: Color::Rgb(230, 245, 230),
            chapter_in_progress: Color::Rgb(0, 95, 175),
        }
    }

    /// Solarized Light theme.
    pub fn solarized_light() -> Self {
        // Solarized palette
        let _base03 = Color::Rgb(0, 43, 54);
        let base02 = Color::Rgb(7, 54, 66);
        let base01 = Color::Rgb(88, 110, 117);
        let _base0 = Color::Rgb(131, 148, 150);
        let base1 = Color::Rgb(147, 161, 161);
        let _base2 = Color::Rgb(238, 232, 213);
        let base3 = Color::Rgb(253, 246, 227);
        let yellow = Color::Rgb(181, 137, 0);
        let orange = Color::Rgb(203, 75, 22);
        let red = Color::Rgb(220, 50, 47);
        let magenta = Color::Rgb(211, 54, 130);
        let violet = Color::Rgb(108, 113, 196);
        let blue = Color::Rgb(38, 139, 210);
        let cyan = Color::Rgb(42, 161, 152);
        let green = Color::Rgb(133, 153, 0);

        Self {
            bg: base3,
            fg: base02,
            title_bar_bg: base02,
            title_bar_fg: base3,

            accent: cyan,
            info: blue,
            success: green,
            warning: yellow,
            error: red,
            muted: base1,
            subtle: magenta,

            vocab_new_bg: blue,
            vocab_new_fg: base3,
            vocab_l1_bg: yellow,
            vocab_l1_fg: base3,
            vocab_l2_bg: orange,
            vocab_l2_fg: base3,
            vocab_l3_bg: Color::Rgb(160, 140, 60),
            vocab_l3_fg: base3,
            vocab_l4_bg: base01,
            vocab_l4_fg: base3,
            vocab_ignored_fg: base1,

            review_vocab_new_bg: violet,
            review_vocab_l1_bg: Color::Rgb(150, 130, 50),
            review_vocab_l2_bg: Color::Rgb(130, 110, 40),
            review_vocab_l3_bg: Color::Rgb(110, 100, 50),
            review_vocab_l4_bg: base01,

            progress_bar: green,
            stats_text: blue,
            status_msg_fg: base3,
            status_msg_bg: yellow,
            cloze_reveal_fg: green,
            cloze_reveal_bg: Color::Rgb(220, 240, 210),
            chapter_in_progress: blue,
        }
    }

    /// Gruvbox (dark) theme.
    pub fn gruvbox() -> Self {
        let bg0 = Color::Rgb(40, 40, 40);
        let _bg1 = Color::Rgb(60, 56, 54);
        let _bg2 = Color::Rgb(80, 73, 69);
        let _bg3 = Color::Rgb(102, 92, 84);
        let fg0 = Color::Rgb(251, 241, 199);
        let _fg1 = Color::Rgb(235, 219, 178);
        let fg4 = Color::Rgb(168, 153, 132);
        let red = Color::Rgb(204, 36, 29);
        let green = Color::Rgb(152, 151, 26);
        let yellow = Color::Rgb(215, 153, 33);
        let blue = Color::Rgb(69, 133, 136);
        let purple = Color::Rgb(177, 98, 134);
        let aqua = Color::Rgb(104, 157, 106);
        let orange = Color::Rgb(214, 93, 14);

        Self {
            bg: Color::Reset,
            fg: Color::Reset,
            title_bar_bg: bg0,
            title_bar_fg: fg0,

            accent: aqua,
            info: blue,
            success: green,
            warning: yellow,
            error: red,
            muted: fg4,
            subtle: purple,

            vocab_new_bg: blue,
            vocab_new_fg: fg0,
            vocab_l1_bg: yellow,
            vocab_l1_fg: bg0,
            vocab_l2_bg: orange,
            vocab_l2_fg: bg0,
            vocab_l3_bg: Color::Rgb(170, 140, 50),
            vocab_l3_fg: bg0,
            vocab_l4_bg: Color::Rgb(130, 120, 80),
            vocab_l4_fg: fg0,
            vocab_ignored_fg: fg4,

            review_vocab_new_bg: Color::Rgb(50, 80, 90),
            review_vocab_l1_bg: Color::Rgb(100, 80, 30),
            review_vocab_l2_bg: Color::Rgb(90, 65, 20),
            review_vocab_l3_bg: Color::Rgb(75, 60, 15),
            review_vocab_l4_bg: Color::Rgb(60, 50, 10),

            progress_bar: green,
            stats_text: blue,
            status_msg_fg: bg0,
            status_msg_bg: yellow,
            cloze_reveal_fg: green,
            cloze_reveal_bg: Color::Rgb(40, 60, 30),
            chapter_in_progress: blue,
        }
    }

    /// Load a theme by name.
    ///
    /// Resolution order:
    /// 1. Built-in theme if `name` matches one of the built-in names.
    /// 2. Custom theme file at `<themes_dir>/<name>.toml`.
    ///    Custom themes start from tokyo-night defaults, then apply all
    ///    fields specified in the TOML file.
    /// 3. Falls back to tokyo-night if nothing matches.
    pub fn load(name: &str, _path: Option<&Path>) -> Self {
        // 1. Try built-in
        if let Some(theme) = Self::builtin(name) {
            return theme;
        }

        // 2. Try custom theme file
        let theme_path = Self::themes_dir().join(format!("{}.toml", name));
        if theme_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&theme_path) {
                if let Ok(overrides) = toml::from_str::<ThemeOverrides>(&content) {
                    // Start from tokyo-night as a safe base, then apply all custom colors
                    let mut theme = Self::tokyo_night();
                    overrides.apply(&mut theme);
                    return theme;
                }
            }
        }

        // 3. Fallback
        Self::tokyo_night()
    }

    /// Downgrade RGB colors to 256-color or 16-color palette based on terminal
    /// capabilities. Call this after loading the theme.
    pub fn apply_color_fallback(&mut self) {
        // Windows Terminal, cmd.exe, and PowerShell on Windows 10+ all support
        // truecolor via the virtual terminal sequences that crossterm enables.
        // They don't set COLORTERM/TERM, so skip detection on Windows entirely.
        if cfg!(target_os = "windows") {
            return;
        }

        // Check if the terminal advertises color support via COLORTERM
        let colorterm = std::env::var("COLORTERM").unwrap_or_default();
        let supports_truecolor = colorterm == "truecolor" || colorterm == "24bit";

        if supports_truecolor {
            return; // No fallback needed
        }

        // Check TERM for 256color support
        let term = std::env::var("TERM").unwrap_or_default();
        let supports_256 = term.contains("256color") || !colorterm.is_empty();

        if supports_256 {
            // Convert RGB to nearest 256-color
            self.map_colors(|c| rgb_to_256(c));
        } else {
            // Convert to 16-color ANSI palette
            self.map_colors(|c| rgb_to_16(c));
        }
    }

    /// Apply a color mapping function to all theme colors.
    fn map_colors(&mut self, f: impl Fn(Color) -> Color) {
        self.bg = f(self.bg);
        self.fg = f(self.fg);
        self.title_bar_bg = f(self.title_bar_bg);
        self.title_bar_fg = f(self.title_bar_fg);
        self.accent = f(self.accent);
        self.info = f(self.info);
        self.success = f(self.success);
        self.warning = f(self.warning);
        self.error = f(self.error);
        self.muted = f(self.muted);
        self.subtle = f(self.subtle);
        self.vocab_new_bg = f(self.vocab_new_bg);
        self.vocab_new_fg = f(self.vocab_new_fg);
        self.vocab_l1_bg = f(self.vocab_l1_bg);
        self.vocab_l1_fg = f(self.vocab_l1_fg);
        self.vocab_l2_bg = f(self.vocab_l2_bg);
        self.vocab_l2_fg = f(self.vocab_l2_fg);
        self.vocab_l3_bg = f(self.vocab_l3_bg);
        self.vocab_l3_fg = f(self.vocab_l3_fg);
        self.vocab_l4_bg = f(self.vocab_l4_bg);
        self.vocab_l4_fg = f(self.vocab_l4_fg);
        self.vocab_ignored_fg = f(self.vocab_ignored_fg);
        self.review_vocab_new_bg = f(self.review_vocab_new_bg);
        self.review_vocab_l1_bg = f(self.review_vocab_l1_bg);
        self.review_vocab_l2_bg = f(self.review_vocab_l2_bg);
        self.review_vocab_l3_bg = f(self.review_vocab_l3_bg);
        self.review_vocab_l4_bg = f(self.review_vocab_l4_bg);
        self.progress_bar = f(self.progress_bar);
        self.stats_text = f(self.stats_text);
        self.status_msg_fg = f(self.status_msg_fg);
        self.status_msg_bg = f(self.status_msg_bg);
        self.cloze_reveal_fg = f(self.cloze_reveal_fg);
        self.cloze_reveal_bg = f(self.cloze_reveal_bg);
        self.chapter_in_progress = f(self.chapter_in_progress);
    }
}

// ── Theme overrides from theme.toml ──────────────────────────────────────

/// Partial theme overrides loaded from a TOML file.
/// All fields are optional — only specified fields override the built-in theme.
#[derive(Debug, Default, Deserialize)]
struct ThemeOverrides {
    #[serde(default)]
    base: Option<BaseOverrides>,
    #[serde(default)]
    palette: Option<PaletteOverrides>,
    #[serde(default)]
    vocabulary: Option<VocabOverrides>,
    #[serde(default)]
    ui: Option<UiOverrides>,
}

#[derive(Debug, Default, Deserialize)]
struct BaseOverrides {
    bg: Option<String>,
    fg: Option<String>,
    title_bar_bg: Option<String>,
    title_bar_fg: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct PaletteOverrides {
    accent: Option<String>,
    info: Option<String>,
    success: Option<String>,
    warning: Option<String>,
    error: Option<String>,
    muted: Option<String>,
    subtle: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct VocabOverrides {
    new_bg: Option<String>,
    new_fg: Option<String>,
    l1_bg: Option<String>,
    l1_fg: Option<String>,
    l2_bg: Option<String>,
    l2_fg: Option<String>,
    l3_bg: Option<String>,
    l3_fg: Option<String>,
    l4_bg: Option<String>,
    l4_fg: Option<String>,
    ignored_fg: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct UiOverrides {
    progress_bar: Option<String>,
    stats_text: Option<String>,
    status_msg_fg: Option<String>,
    status_msg_bg: Option<String>,
    cloze_reveal_fg: Option<String>,
    cloze_reveal_bg: Option<String>,
    chapter_in_progress: Option<String>,
}

impl ThemeOverrides {
    fn apply(&self, theme: &mut Theme) {
        if let Some(ref base) = self.base {
            apply_color(&base.bg, &mut theme.bg);
            apply_color(&base.fg, &mut theme.fg);
            apply_color(&base.title_bar_bg, &mut theme.title_bar_bg);
            apply_color(&base.title_bar_fg, &mut theme.title_bar_fg);
        }
        if let Some(ref palette) = self.palette {
            apply_color(&palette.accent, &mut theme.accent);
            apply_color(&palette.info, &mut theme.info);
            apply_color(&palette.success, &mut theme.success);
            apply_color(&palette.warning, &mut theme.warning);
            apply_color(&palette.error, &mut theme.error);
            apply_color(&palette.muted, &mut theme.muted);
            apply_color(&palette.subtle, &mut theme.subtle);
        }
        if let Some(ref vocab) = self.vocabulary {
            apply_color(&vocab.new_bg, &mut theme.vocab_new_bg);
            apply_color(&vocab.new_fg, &mut theme.vocab_new_fg);
            apply_color(&vocab.l1_bg, &mut theme.vocab_l1_bg);
            apply_color(&vocab.l1_fg, &mut theme.vocab_l1_fg);
            apply_color(&vocab.l2_bg, &mut theme.vocab_l2_bg);
            apply_color(&vocab.l2_fg, &mut theme.vocab_l2_fg);
            apply_color(&vocab.l3_bg, &mut theme.vocab_l3_bg);
            apply_color(&vocab.l3_fg, &mut theme.vocab_l3_fg);
            apply_color(&vocab.l4_bg, &mut theme.vocab_l4_bg);
            apply_color(&vocab.l4_fg, &mut theme.vocab_l4_fg);
            apply_color(&vocab.ignored_fg, &mut theme.vocab_ignored_fg);
        }
        if let Some(ref ui) = self.ui {
            apply_color(&ui.progress_bar, &mut theme.progress_bar);
            apply_color(&ui.stats_text, &mut theme.stats_text);
            apply_color(&ui.status_msg_fg, &mut theme.status_msg_fg);
            apply_color(&ui.status_msg_bg, &mut theme.status_msg_bg);
            apply_color(&ui.cloze_reveal_fg, &mut theme.cloze_reveal_fg);
            apply_color(&ui.cloze_reveal_bg, &mut theme.cloze_reveal_bg);
            apply_color(&ui.chapter_in_progress, &mut theme.chapter_in_progress);
        }
    }
}

/// Parse a hex color string (e.g. "#1E1E32" or "1E1E32") into a `Color::Rgb`.
fn parse_hex_color(s: &str) -> Option<Color> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some(Color::Rgb(r, g, b))
}

/// Apply a color override if the string is Some and parseable.
fn apply_color(value: &Option<String>, target: &mut Color) {
    if let Some(ref s) = value {
        if let Some(color) = parse_hex_color(s) {
            *target = color;
        }
    }
}

// ── Color fallback (256-color and 16-color) ──────────────────────────────

/// Convert an RGB color to the nearest xterm 256-color index.
fn rgb_to_256(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            // Use the 6x6x6 color cube (indices 16-231)
            let ri = color_cube_index(r);
            let gi = color_cube_index(g);
            let bi = color_cube_index(b);
            let cube_index = 16 + 36 * ri + 6 * gi + bi;

            // Also check the grayscale ramp (indices 232-255)
            let gray_level = (r as u16 + g as u16 + b as u16) / 3;
            let gray_index = if gray_level < 8 {
                232
            } else if gray_level > 238 {
                255
            } else {
                232 + ((gray_level - 8) as u8 / 10)
            };

            // Pick whichever is closer
            let cube_r = CUBE_VALUES[ri as usize];
            let cube_g = CUBE_VALUES[gi as usize];
            let cube_b = CUBE_VALUES[bi as usize];
            let cube_dist = color_distance(r, g, b, cube_r, cube_g, cube_b);

            let gray_val = if gray_index == 232 {
                8
            } else {
                8 + (gray_index - 232) * 10
            };
            let gray_dist = color_distance(r, g, b, gray_val, gray_val, gray_val);

            if gray_dist < cube_dist {
                Color::Indexed(gray_index)
            } else {
                Color::Indexed(cube_index)
            }
        }
        other => other, // Already a non-RGB color, keep as-is
    }
}

/// Convert an RGB color to the nearest ANSI 16-color.
fn rgb_to_16(color: Color) -> Color {
    match color {
        Color::Rgb(r, g, b) => {
            // Map to the closest ANSI color
            let brightness = (r as u16 + g as u16 + b as u16) / 3;
            let is_bright = brightness > 127;

            // Determine dominant channel(s)
            let max = r.max(g).max(b);
            let threshold = max / 2;

            let has_r = r > threshold;
            let has_g = g > threshold;
            let has_b = b > threshold;

            let base = match (has_r, has_g, has_b) {
                (false, false, false) => {
                    if brightness > 200 {
                        return Color::White;
                    }
                    return Color::DarkGray;
                }
                (true, false, false) => Color::Red,
                (false, true, false) => Color::Green,
                (false, false, true) => Color::Blue,
                (true, true, false) => Color::Yellow,
                (true, false, true) => Color::Magenta,
                (false, true, true) => Color::Cyan,
                (true, true, true) => {
                    if is_bright {
                        return Color::White;
                    }
                    return Color::Gray;
                }
            };

            if is_bright {
                Color::LightRed // Use Light* variants for brighter colors
                                // Note: ratatui maps LightRed etc to bright ANSI codes
            } else {
                base
            }
        }
        other => other,
    }
}

const CUBE_VALUES: [u8; 6] = [0, 0x5f, 0x87, 0xaf, 0xd7, 0xff];

fn color_cube_index(value: u8) -> u8 {
    match value {
        0..=0x2f => 0,
        0x30..=0x72 => 1,
        0x73..=0x9b => 2,
        0x9c..=0xc3 => 3,
        0xc4..=0xeb => 4,
        _ => 5,
    }
}

fn color_distance(r1: u8, g1: u8, b1: u8, r2: u8, g2: u8, b2: u8) -> u32 {
    let dr = (r1 as i32 - r2 as i32).unsigned_abs();
    let dg = (g1 as i32 - g2 as i32).unsigned_abs();
    let db = (b1 as i32 - b2 as i32).unsigned_abs();
    dr * dr + dg * dg + db * db
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hex_color() {
        assert_eq!(parse_hex_color("#1E1E32"), Some(Color::Rgb(30, 30, 50)));
        assert_eq!(parse_hex_color("FF0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(parse_hex_color("#00FF00"), Some(Color::Rgb(0, 255, 0)));
        assert_eq!(parse_hex_color("nope"), None);
        assert_eq!(parse_hex_color("#FFF"), None); // Too short
    }

    #[test]
    fn test_builtin_themes_load() {
        let _t1 = Theme::tokyo_night();
        let _t2 = Theme::solarized_light();
        let _t3 = Theme::gruvbox();
    }

    #[test]
    fn test_by_name() {
        let t = Theme::by_name("gruvbox");
        assert!(matches!(t.accent, Color::Rgb(104, 157, 106)));
    }
}
