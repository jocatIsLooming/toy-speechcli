use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::as_24_bit_terminal_escaped;

use crate::parser::{BlockStyle, InlineSpan, ParsedBlock};

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_BOLD: &str = "\x1b[1m";
const COLOR_ITALIC: &str = "\x1b[3m";
const COLOR_STRIKETHROUGH: &str = "\x1b[9m";
const COLOR_RED: &str = "\x1b[38;5;203m";
const COLOR_GREEN: &str = "\x1b[38;5;114m";
const COLOR_YELLOW: &str = "\x1b[38;5;221m";
const COLOR_BLUE: &str = "\x1b[38;5;75m";
const COLOR_MAGENTA: &str = "\x1b[38;5;176m";
const COLOR_CYAN: &str = "\x1b[38;5;73m";
const COLOR_GRAY: &str = "\x1b[38;5;245m";
const COLOR_WHITE: &str = "\x1b[97m";
const BG_GRAY: &str = "\x1b[48;5;238m";

pub struct MarkdownRenderer {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownRenderer {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    pub fn render(&self, blocks: &[ParsedBlock]) -> Vec<RenderedLine> {
        let mut lines = Vec::new();

        for block in blocks {
            match block {
                ParsedBlock::Text { spans, style } => {
                    let rendered = self.render_inline_spans(spans, style);
                    if rendered.is_empty() {
                        lines.push(RenderedLine::empty());
                        continue;
                    }

                    let line_style = Style {
                        is_heading: style.heading_level.is_some(),
                        ..Default::default()
                    };

                    lines.push(RenderedLine::styled(rendered, line_style));
                }
                ParsedBlock::CodeBlock { lang, content } => {
                    for line in self.render_code_block(lang, content) {
                        lines.push(RenderedLine::code_block(line));
                    }
                }
                ParsedBlock::Table { rows } => {
                    for line in self.render_table(rows) {
                        lines.push(RenderedLine::table(line));
                    }
                }
                ParsedBlock::Empty => lines.push(RenderedLine::empty()),
                ParsedBlock::Rule => lines.push(RenderedLine::text(format!(
                    "{}──────────────────────────────{}",
                    COLOR_GRAY, COLOR_RESET
                ))),
            }
        }

        lines
    }

    fn render_inline_spans(&self, spans: &[InlineSpan], style: &BlockStyle) -> String {
        let mut output = String::new();
        let heading_color = style
            .heading_level
            .map(|lvl| heading_color(lvl))
            .unwrap_or("");

        for span in spans {
            let mut segment = String::new();

            if style.heading_level.is_some() {
                segment.push_str(COLOR_BOLD);
                segment.push_str(heading_color);
            }

            if span.style.blockquote {
                segment.push_str(COLOR_GRAY);
            }

            if span.style.code {
                segment.push_str(BG_GRAY);
                segment.push_str(&span.text);
                segment.push_str(COLOR_RESET);
                output.push_str(&segment);
                continue;
            }

            if span.style.bold {
                segment.push_str(COLOR_BOLD);
            }
            if span.style.italic {
                segment.push_str(COLOR_ITALIC);
            }
            if span.style.strikethrough {
                segment.push_str(COLOR_STRIKETHROUGH);
            }
            if span.style.link {
                segment.push_str(COLOR_CYAN);
            }

            segment.push_str(&span.text);
            segment.push_str(COLOR_RESET);

            output.push_str(&segment);
        }

        output
    }

    pub fn render_code_block(&self, lang: &str, code: &str) -> Vec<String> {
        let syntax = self
            .syntax_set
            .find_syntax_by_token(lang)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        let mut lines = Vec::new();

        if !lang.is_empty() {
            lines.push(format!("{}┌─ {}{}", COLOR_GRAY, lang, COLOR_RESET));
        } else {
            lines.push(format!("{}┌─{}─{}", COLOR_GRAY, BG_GRAY, COLOR_RESET));
        }

        for line in code.lines() {
            let highlighted = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();
            let escaped = as_24_bit_terminal_escaped(&highlighted, false);
            lines.push(format!("{}│ {}{}", COLOR_GRAY, escaped, COLOR_RESET));
        }

        lines.push(format!(
            "{}└────────────{}─{}─{}",
            COLOR_GRAY, BG_GRAY, COLOR_RESET, COLOR_RESET
        ));

        lines
    }

    pub fn render_table(&self, rows: &[Vec<Vec<InlineSpan>>]) -> Vec<String> {
        if rows.is_empty() {
            return Vec::new();
        }

        let rendered_rows: Vec<Vec<String>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| self.render_inline_spans(cell, &BlockStyle::default()))
                    .collect()
            })
            .collect();

        let max_cols = rendered_rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut col_widths = vec![0; max_cols];

        for row in &rendered_rows {
            for (i, cell) in row.iter().enumerate() {
                let width = strip_ansi(cell).chars().count();
                col_widths[i] = col_widths[i].max(width.min(40));
            }
        }

        let mut lines = Vec::new();

        lines.push(format!(
            "{}┌{}┐{}",
            COLOR_GRAY,
            col_widths
                .iter()
                .map(|&w| "─".repeat(w + 2))
                .collect::<Vec<_>>()
                .join("┬"),
            COLOR_RESET
        ));

        for (row_idx, row) in rendered_rows.iter().enumerate() {
            let mut cells = Vec::new();
            for (i, cell) in row.iter().enumerate() {
                let width = col_widths.get(i).copied().unwrap_or(10);
                let stripped = strip_ansi(cell);
                let padding = width.saturating_sub(stripped.chars().count());
                let styled_cell = if row_idx == 0 {
                    format!(
                        "{}{}{}{}{}",
                        COLOR_BOLD,
                        COLOR_YELLOW,
                        cell,
                        COLOR_RESET,
                        " ".repeat(padding)
                    )
                } else {
                    format!("{}{}{}", cell, " ".repeat(padding), COLOR_RESET)
                };
                cells.push(format!(" {} {}{}", COLOR_GRAY, styled_cell, COLOR_GRAY));
            }
            lines.push(format!("{}│{}│{}", COLOR_GRAY, cells.join("│"), COLOR_RESET));

            if row_idx == 0 && rendered_rows.len() > 1 {
                lines.push(format!(
                    "{}├{}┤{}",
                    COLOR_GRAY,
                    col_widths
                        .iter()
                        .map(|&w| "─".repeat(w + 2))
                        .collect::<Vec<_>>()
                        .join("┼"),
                    COLOR_RESET
                ));
            }
        }

        lines.push(format!(
            "{}└{}┘{}",
            COLOR_GRAY,
            col_widths
                .iter()
                .map(|&w| "─".repeat(w + 2))
                .collect::<Vec<_>>()
                .join("┴"),
            COLOR_RESET
        ));

        lines
    }
}

fn heading_color(level: u8) -> &'static str {
    match level {
        1 => COLOR_RED,
        2 => COLOR_YELLOW,
        3 => COLOR_GREEN,
        4 => COLOR_CYAN,
        5 => COLOR_BLUE,
        6 => COLOR_MAGENTA,
        _ => COLOR_WHITE,
    }
}

#[derive(Clone, Copy, Default)]
pub struct Style {
    pub is_heading: bool,
    pub is_code_block: bool,
    pub is_table: bool,
}

#[derive(Clone)]
pub struct RenderedLine {
    pub content: String,
    pub style: Style,
    pub line_type: LineType,
}

#[derive(Clone, Copy, PartialEq)]
pub enum LineType {
    Text,
    Heading,
    CodeBlock,
    Table,
    Empty,
}

impl Default for LineType {
    fn default() -> Self {
        LineType::Text
    }
}

impl RenderedLine {
    pub fn text(content: String) -> Self {
        Self {
            content,
            style: Style::default(),
            line_type: LineType::Text,
        }
    }

    pub fn styled(content: String, style: Style) -> Self {
        let line_type = if style.is_heading {
            LineType::Heading
        } else {
            LineType::Text
        };
        Self {
            content,
            style,
            line_type,
        }
    }

    pub fn code_block(content: String) -> Self {
        Self {
            content,
            style: Style {
                is_code_block: true,
                ..Default::default()
            },
            line_type: LineType::CodeBlock,
        }
    }

    pub fn table(content: String) -> Self {
        Self {
            content,
            style: Style {
                is_table: true,
                ..Default::default()
            },
            line_type: LineType::Table,
        }
    }

    pub fn empty() -> Self {
        Self {
            content: String::new(),
            style: Style::default(),
            line_type: LineType::Empty,
        }
    }
}

fn strip_ansi(text: &str) -> String {
    let mut result = String::new();
    let mut in_escape = false;

    for ch in text.chars() {
        if in_escape {
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
        } else if ch == '\x1b' {
            in_escape = true;
        } else {
            result.push(ch);
        }
    }

    result
}

pub fn apply_opacity_to_line(line: &RenderedLine, opacity: u8) -> String {
    if opacity >= 255 {
        return line.content.clone();
    }

    let factor = opacity as f32 / 255.0;

    if line.style.is_code_block {
        apply_opacity_to_code(&line.content, factor)
    } else if line.style.is_table {
        apply_opacity_to_table(&line.content, factor)
    } else {
        apply_opacity_to_text(&line.content, factor)
    }
}

fn apply_opacity_to_text(text: &str, factor: f32) -> String {
    let gray = (factor * 23.0) as u8 + 232;
    let gray_code = gray.min(255);
    let gray_prefix = format!("\x1b[38;5;{}m", gray_code);

    let mut result = String::new();
    let mut in_escape = false;
    let mut escape_buffer = String::new();
    let mut started = false;

    for ch in text.chars() {
        if in_escape {
            escape_buffer.push(ch);
            if ch.is_ascii_alphabetic() {
                in_escape = false;
                result.push('\x1b');
                result.push_str(&escape_buffer);
                if escape_buffer.ends_with('m') {
                    result.push_str(&gray_prefix);
                }
                escape_buffer.clear();
            }
        } else if ch == '\x1b' {
            in_escape = true;
        } else {
            if !started {
                result.push_str(&gray_prefix);
                started = true;
            }
            result.push(ch);
        }
    }

    if started {
        result.push_str(COLOR_RESET);
        result
    } else {
        format!("\x1b[38;5;{}m{}\x1b[0m", gray_code, strip_ansi(text))
    }
}

fn apply_opacity_to_code(text: &str, factor: f32) -> String {
    let gray = (factor * 23.0) as u8 + 232;
    let gray_code = gray.min(255);
    format!("\x1b[38;5;{}m{}\x1b[0m", gray_code, strip_ansi(text))
}

fn apply_opacity_to_table(text: &str, factor: f32) -> String {
    let gray = (factor * 23.0) as u8 + 232;
    let gray_code = gray.min(255);

    let (prefix, rest) = split_table_prefix(text);
    if rest.is_empty() {
        return format!("\x1b[38;5;{}m{}\x1b[0m", gray_code, strip_ansi(text));
    }

    let dimmed = apply_opacity_to_text(rest, factor);
    format!("{}{}", prefix, dimmed)
}

fn is_box_drawing(ch: char) -> bool {
    matches!(
        ch,
        '│' | '─' | '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼'
    )
}

fn split_table_prefix(text: &str) -> (String, &str) {
    let mut prefix = String::new();
    let mut in_escape = false;

    for (idx, ch) in text.char_indices() {
        if in_escape {
            prefix.push(ch);
            if ch.is_ascii_alphabetic() {
                in_escape = false;
            }
            continue;
        }

        if ch == '\x1b' {
            in_escape = true;
            prefix.push(ch);
            continue;
        }

        if is_box_drawing(ch) {
            prefix.push(ch);
            continue;
        }

        let rest = &text[idx..];
        return (prefix, rest);
    }

    (prefix, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dims_text_before_first_escape_sequence() {
        let original = format!(
            "I want to talk about something that {}matters{}",
            COLOR_BOLD, COLOR_RESET
        );
        let factor = 0.5;
        let expected_gray = ((factor * 23.0) as u8 + 232).min(255);
        let gray_prefix = format!("\x1b[38;5;{}m", expected_gray);

        let dimmed = apply_opacity_to_text(&original, factor);

        assert!(
            dimmed.contains(&gray_prefix),
            "dimmed text should apply gray coloring"
        );
        assert_eq!(strip_ansi(&dimmed), strip_ansi(&original));
    }

    #[test]
    fn re_applies_gray_after_color_sequences() {
        let original = format!("start {}colored{}", COLOR_RED, COLOR_RESET);
        let factor = 0.5;
        let expected_gray = ((factor * 23.0) as u8 + 232).min(255);
        let gray_prefix = format!("\x1b[38;5;{}m", expected_gray);

        let dimmed = apply_opacity_to_text(&original, factor);

        let color_idx = dimmed
            .find(COLOR_RED)
            .expect("colored sequence should be present");
        let first_gray_after_color = dimmed[color_idx..].find(&gray_prefix);
        assert!(
            first_gray_after_color.is_some(),
            "gray should be re-applied after color code"
        );
        assert_eq!(strip_ansi(&dimmed), "start colored");
    }
}
