use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, CodeBlockKind, HeadingLevel};
use syntect::easy::HighlightLines;
use syntect::parsing::SyntaxSet;
use syntect::highlighting::ThemeSet;
use syntect::util::as_24_bit_terminal_escaped;

const COLOR_RESET: &str = "\x1b[0m";
const COLOR_BOLD: &str = "\x1b[1m";
const COLOR_DIM: &str = "\x1b[2m";
const COLOR_ITALIC: &str = "\x1b[3m";
const COLOR_UNDERLINE: &str = "\x1b[4m";
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

    pub fn render_line(&self, line: &str) -> String {
        if line.trim().is_empty() {
            return String::new();
        }

        let mut output = String::new();
        let parser = Parser::new_ext(line, Self::parser_options());

        for event in parser {
            match event {
                Event::Text(text) => output.push_str(&text.to_string()),
                Event::Code(code) => {
                    output.push_str(&format!("{}{}{}", BG_GRAY, code, COLOR_RESET));
                }
                Event::Start(tag) => {
                    self.handle_start_tag(&mut output, &tag);
                }
                Event::End(tag_end) => {
                    self.handle_end_tag(&mut output, &tag_end);
                }
                Event::SoftBreak | Event::HardBreak => output.push(' '),
                Event::Rule => output.push_str(&format!("{}──────────────────────────────{}", COLOR_GRAY, COLOR_RESET)),
                _ => {}
            }
        }

        if output.is_empty() {
            output = line.to_string();
        }

        output
    }

    fn handle_start_tag(&self, output: &mut String, tag: &Tag) {
        match tag {
            Tag::Strong => output.push_str(COLOR_BOLD),
            Tag::Emphasis => output.push_str(COLOR_ITALIC),
            Tag::Strikethrough => output.push_str(COLOR_STRIKETHROUGH),
            Tag::Link { .. } => output.push_str(COLOR_CYAN),
            Tag::Heading { level, .. } => {
                let color = match level {
                    HeadingLevel::H1 => COLOR_RED,
                    HeadingLevel::H2 => COLOR_YELLOW,
                    HeadingLevel::H3 => COLOR_GREEN,
                    HeadingLevel::H4 => COLOR_CYAN,
                    HeadingLevel::H5 => COLOR_BLUE,
                    HeadingLevel::H6 => COLOR_MAGENTA,
                };
                output.push_str(&format!("{}{}", COLOR_BOLD, color));
            }
            Tag::BlockQuote(_) => output.push_str(&format!("{}> ", COLOR_GRAY)),
            Tag::List(_) => {}
            Tag::Item => output.push_str("  • "),
            Tag::Table(_) => output.push_str(COLOR_BOLD),
            Tag::TableCell => output.push_str(" "),
            Tag::TableHead => output.push_str(&format!("{}{}", COLOR_BOLD, COLOR_YELLOW)),
            Tag::Paragraph => {}
            Tag::CodeBlock(_) => {}
            _ => {}
        }
    }

    fn handle_end_tag(&self, output: &mut String, tag_end: &TagEnd) {
        match tag_end {
            TagEnd::Strong
            | TagEnd::Emphasis
            | TagEnd::Strikethrough
            | TagEnd::Heading(_)
            | TagEnd::TableHead
            | TagEnd::Table
            | TagEnd::Link => {
                output.push_str(COLOR_RESET);
            }
            _ => {}
        }
    }

    pub fn render_code_block(&self, lang: &str, code: &str) -> Vec<String> {
        let syntax = self.syntax_set.find_syntax_by_token(lang)
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
            let highlighted = highlighter.highlight_line(line, &self.syntax_set).unwrap_or_default();
            let escaped = as_24_bit_terminal_escaped(&highlighted, false);
            lines.push(format!("{}│ {}{}", COLOR_GRAY, escaped, COLOR_RESET));
        }

        lines.push(format!("{}└────────────{}─{}─{}", COLOR_GRAY, BG_GRAY, COLOR_RESET, COLOR_RESET));

        lines
    }

    pub fn parse_and_render(&self, text: &str) -> Vec<RenderedLine> {
        let mut lines = Vec::new();
        let parser = Parser::new_ext(text, Self::parser_options());

        let mut in_code_block = false;
        let mut code_lang = String::new();
        let mut code_content = String::new();
        let mut in_table = false;
        let mut table_rows: Vec<Vec<String>> = Vec::new();
        let mut current_row: Vec<String> = Vec::new();
        let mut in_table_head = false;
        let mut list_stack: Vec<u64> = Vec::new();

        let mut current_text = String::new();
        let mut current_style = Style::default();

        for event in parser {
            match event {
                Event::Start(Tag::CodeBlock(kind)) => {
                    if !current_text.is_empty() {
                        lines.push(RenderedLine::text(current_text.clone()));
                        current_text.clear();
                    }
                    in_code_block = true;
                    code_lang = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    code_content.clear();
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                    let code_lines = self.render_code_block(&code_lang, &code_content);
                    for line in code_lines {
                        lines.push(RenderedLine::code_block(line));
                    }
                    code_content.clear();
                    code_lang.clear();
                }
                Event::Text(text) if in_code_block => {
                    code_content.push_str(&text);
                }
                Event::Start(Tag::Table(_)) => {
                    if !current_text.is_empty() {
                        lines.push(RenderedLine::text(current_text.clone()));
                        current_text.clear();
                    }
                    in_table = true;
                    table_rows.clear();
                }
                Event::End(TagEnd::Table) => {
                    in_table = false;
                    let table_lines = self.render_table(&table_rows);
                    for line in table_lines {
                        lines.push(RenderedLine::table(line));
                    }
                }
                Event::Start(Tag::TableHead) => {
                    in_table_head = true;
                    current_row.clear();
                }
                Event::End(TagEnd::TableHead) => {
                    in_table_head = false;
                    table_rows.push(current_row.clone());
                }
                Event::Start(Tag::TableRow) => {
                    current_row.clear();
                }
                Event::End(TagEnd::TableRow) => {
                    table_rows.push(current_row.clone());
                }
                Event::Start(Tag::TableCell) => {
                    current_style.is_table_head = in_table_head;
                }
                Event::End(TagEnd::TableCell) => {
                    current_row.push(current_text.clone());
                    current_text.clear();
                }
                Event::Start(Tag::List(start_num)) => {
                    list_stack.push(start_num.unwrap_or(0));
                }
                Event::End(TagEnd::List(_)) => {
                    list_stack.pop();
                }
                Event::Start(Tag::Item) => {
                    let indent = "  ".repeat(list_stack.len().saturating_sub(1));
                    if let Some(&start) = list_stack.last() {
                        if start > 0 {
                            let idx = start + current_row.len() as u64;
                            current_text.push_str(&format!("{}{}. ", indent, idx));
                        } else {
                            current_text.push_str(&format!("{}• ", indent));
                        }
                    }
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    if !current_text.is_empty() {
                        lines.push(RenderedLine::text(current_text.clone()));
                        current_text.clear();
                    }
                    current_style.is_heading = true;
                    current_style.heading_level = level as u8;
                }
                Event::End(TagEnd::Heading(_)) => {
                    lines.push(RenderedLine::styled(current_text.clone(), current_style));
                    current_text.clear();
                    current_style = Style::default();
                }
                Event::Start(Tag::Paragraph) => {}
                Event::End(TagEnd::Paragraph) => {
                    if !current_text.is_empty() {
                        lines.push(RenderedLine::text(current_text.clone()));
                        current_text.clear();
                    }
                    lines.push(RenderedLine::empty());
                }
                Event::Start(Tag::Strong) => current_text.push_str(COLOR_BOLD),
                Event::End(TagEnd::Strong) => current_text.push_str(COLOR_RESET),
                Event::Start(Tag::Emphasis) => current_text.push_str(COLOR_ITALIC),
                Event::End(TagEnd::Emphasis) => current_text.push_str(COLOR_RESET),
                Event::Code(code) => {
                    current_text.push_str(BG_GRAY);
                    current_text.push_str(&code);
                    current_text.push_str(COLOR_RESET);
                }
                Event::Start(Tag::Link { .. }) => current_text.push_str(COLOR_CYAN),
                Event::End(TagEnd::Link) => current_text.push_str(COLOR_RESET),
                Event::Start(Tag::BlockQuote(_)) => current_text.push_str(&format!("{}> ", COLOR_GRAY)),
                Event::End(TagEnd::BlockQuote(_)) => current_text.push_str(COLOR_RESET),
                Event::Text(text) => {
                    if in_table {
                        current_text.push_str(&text);
                    } else {
                        current_text.push_str(&self.apply_inline_styles(&text, &current_style));
                    }
                }
                Event::SoftBreak => current_text.push(' '),
                Event::HardBreak => {
                    lines.push(RenderedLine::text(current_text.clone()));
                    current_text.clear();
                }
                Event::Rule => {
                    lines.push(RenderedLine::text(format!("{}──────────────────────────────{}", COLOR_GRAY, COLOR_RESET)));
                }
                _ => {}
            }
        }

        if !current_text.is_empty() {
            lines.push(RenderedLine::text(current_text));
        }

        lines
    }

    fn parser_options() -> Options {
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH
    }

    fn apply_inline_styles(&self, text: &str, style: &Style) -> String {
        if style.is_heading {
            let color = match style.heading_level {
                1 => COLOR_RED,
                2 => COLOR_YELLOW,
                3 => COLOR_GREEN,
                4 => COLOR_CYAN,
                5 => COLOR_BLUE,
                6 => COLOR_MAGENTA,
                _ => COLOR_WHITE,
            };
            format!("{}{}{}{}", COLOR_BOLD, color, text, COLOR_RESET)
        } else {
            text.to_string()
        }
    }

    fn render_table(&self, rows: &[Vec<String>]) -> Vec<String> {
        if rows.is_empty() {
            return Vec::new();
        }

        let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut col_widths = vec![0; max_cols];

        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                let width = strip_ansi(cell).chars().count();
                col_widths[i] = col_widths[i].max(width.min(40));
            }
        }

        let mut lines = Vec::new();

        lines.push(format!("{}┌{}┐{}",
            COLOR_GRAY,
            col_widths.iter().map(|&w| "─".repeat(w + 2)).collect::<Vec<_>>().join("┬"),
            COLOR_RESET
        ));

        for (row_idx, row) in rows.iter().enumerate() {
            let mut cells = Vec::new();
            for (i, cell) in row.iter().enumerate() {
                let width = col_widths.get(i).copied().unwrap_or(10);
                let stripped = strip_ansi(cell);
                let padding = width.saturating_sub(stripped.chars().count());
                let styled_cell = if row_idx == 0 {
                    format!("{}{}{}{}{}", COLOR_BOLD, COLOR_YELLOW, cell, COLOR_RESET, " ".repeat(padding))
                } else {
                    format!("{}{}{}", cell, " ".repeat(padding), COLOR_RESET)
                };
                cells.push(format!(" {} {}{}", COLOR_GRAY, styled_cell, COLOR_GRAY));
            }
            lines.push(format!("{}│{}│{}", COLOR_GRAY, cells.join("│"), COLOR_RESET));

            if row_idx == 0 && rows.len() > 1 {
                lines.push(format!("{}├{}┤{}",
                    COLOR_GRAY,
                    col_widths.iter().map(|&w| "─".repeat(w + 2)).collect::<Vec<_>>().join("┼"),
                    COLOR_RESET
                ));
            }
        }

        lines.push(format!("{}└{}┘{}",
            COLOR_GRAY,
            col_widths.iter().map(|&w| "─".repeat(w + 2)).collect::<Vec<_>>().join("┴"),
            COLOR_RESET
        ));

        lines
    }
}

#[derive(Clone, Copy, Default)]
pub struct Style {
    pub is_heading: bool,
    pub heading_level: u8,
    pub is_code_block: bool,
    pub is_table: bool,
    pub is_table_head: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dims_text_before_first_escape_sequence() {
        let original = format!("I want to talk about something that {}matters{}", COLOR_BOLD, COLOR_RESET);
        let factor = 0.5;
        let expected_gray = ((factor * 23.0) as u8 + 232).min(255);
        let gray_prefix = format!("\x1b[38;5;{}m", expected_gray);

        let dimmed = apply_opacity_to_text(&original, factor);

        assert!(dimmed.contains(&gray_prefix), "dimmed text should apply gray coloring");
        assert_eq!(strip_ansi(&dimmed), strip_ansi(&original));
    }

    #[test]
    fn re_applies_gray_after_color_sequences() {
        let original = format!("start {}colored{}", COLOR_RED, COLOR_RESET);
        let factor = 0.5;
        let expected_gray = ((factor * 23.0) as u8 + 232).min(255);
        let gray_prefix = format!("\x1b[38;5;{}m", expected_gray);

        let dimmed = apply_opacity_to_text(&original, factor);

        // After the explicit color code, gray should be re-applied.
        let color_idx = dimmed.find(COLOR_RED).expect("colored sequence should be present");
        let first_gray_after_color = dimmed[color_idx..].find(&gray_prefix);
        assert!(first_gray_after_color.is_some(), "gray should be re-applied after color code");
        assert_eq!(strip_ansi(&dimmed), "start colored");
    }
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
    matches!(ch, '│' | '─' | '┌' | '┐' | '└' | '┘' | '├' | '┤' | '┬' | '┴' | '┼')
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

        // First visible non-box character; split here.
        let rest = &text[idx..];
        return (prefix, rest);
    }

    (prefix, "")
}
