use std::fs;
use std::io::{self, Write};

use crossterm::{
    ExecutableCommand, QueueableCommand,
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::layout::{Layout, Rect};
use crate::parser::{InlineSpan, MarkdownParser, ParsedBlock};
use crate::renderer::{
    MarkdownRenderer, RenderedEntity, RenderedLine, apply_opacity_to_line, strip_ansi,
};

const LOW_OPACITY: u8 = 55;
const FOCUS_BAND_RADIUS: usize = 2;
const MIN_VIEWPORT_WIDTH: usize = 10;
const ANSI_RESET: &str = "\x1b[0m";
const FRAME_COLOR: &str = "\x1b[38;5;240m";
const SIDEBAR_TITLE_COLOR: &str = "\x1b[38;5;252m";
const SIDEBAR_ACTIVE_COLOR: &str = "\x1b[38;5;223m";
const SIDEBAR_INACTIVE_COLOR: &str = "\x1b[38;5;245m";

#[derive(Clone)]
struct SidebarHeading {
    entity_idx: usize,
    level: u8,
    title: String,
}

pub fn run() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let text = if args.len() > 1 {
        fs::read_to_string(&args[1])?
    } else {
        fs::read_to_string("speech.txt")?
    };

    let parser = MarkdownParser::new();
    let renderer = MarkdownRenderer::new();
    let parsed = parser.parse(&text);
    let entities = renderer.render(&parsed);
    let headings = collect_sidebar_headings(&parsed);

    if entities.is_empty() {
        println!("No text to display");
        return Ok(());
    }

    let mut stdout = io::stdout();
    terminal::enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(Hide)?;

    let mut current_entity_idx = first_focusable_entity(&entities).unwrap_or(0);

    loop {
        stdout.queue(Clear(ClearType::All))?;

        let (cols, rows) = terminal::size()?;
        let layout = Layout::centered_panel(cols, rows);
        draw_frame(&mut stdout, layout.sidebar_frame)?;
        draw_sidebar(&mut stdout, layout.sidebar_content, &headings, current_entity_idx)?;

        let (display_rows, entity_row_offsets, entity_row_counts) =
            build_display_rows(&entities, layout.content.width as usize);

        let focus_row = entity_row_offsets
            .get(current_entity_idx)
            .copied()
            .unwrap_or(0)
            + entity_row_counts
                .get(current_entity_idx)
                .copied()
                .unwrap_or(1)
                .saturating_sub(1)
                / 2;

        let viewport_rows =
            centered_viewport_rows(focus_row, display_rows.len(), layout.content.height as usize);

        for (row, maybe_row_idx) in viewport_rows.iter().enumerate() {
            stdout.queue(MoveTo(layout.content.x, layout.content.y + row as u16))?;

            let Some(display_row_idx) = *maybe_row_idx else {
                continue;
            };

            let (entity_idx, line, segment) = &display_rows[display_row_idx];
            let in_focus_band =
                is_in_focus_band(current_entity_idx, *entity_idx, FOCUS_BAND_RADIUS);
            let opacity = if in_focus_band { 255 } else { LOW_OPACITY };

            let wrapped_line = RenderedLine {
                content: segment.clone(),
                style: line.style,
                line_type: line.line_type,
            };
            let styled_line = apply_opacity_to_line(&wrapped_line, opacity);
            print!("{}", styled_line);
        }

        stdout.flush()?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Char(' ') => {
                        current_entity_idx = move_down(&entities, current_entity_idx);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        current_entity_idx = move_up(&entities, current_entity_idx);
                    }
                    KeyCode::Home => {
                        current_entity_idx = first_focusable_entity(&entities).unwrap_or(0)
                    }
                    KeyCode::End => {
                        current_entity_idx = last_focusable_entity(&entities)
                            .unwrap_or_else(|| entities.len().saturating_sub(1))
                    }
                    _ => {}
                }
            }
        }
    }

    stdout.execute(Show)?;
    stdout.execute(LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;

    Ok(())
}

fn draw_frame(stdout: &mut io::Stdout, rect: Rect) -> io::Result<()> {
    if rect.width < 2 || rect.height < 2 {
        return Ok(());
    }

    let inner_width = rect.width.saturating_sub(2) as usize;
    let top = format!("{}┌{}┐{}", FRAME_COLOR, "─".repeat(inner_width), ANSI_RESET);
    let middle = format!("{}│{}│{}", FRAME_COLOR, " ".repeat(inner_width), ANSI_RESET);
    let bottom = format!("{}└{}┘{}", FRAME_COLOR, "─".repeat(inner_width), ANSI_RESET);

    stdout.queue(MoveTo(rect.x, rect.y))?;
    print!("{}", top);

    for y in rect.y + 1..rect.bottom() {
        stdout.queue(MoveTo(rect.x, y))?;
        print!("{}", middle);
    }

    stdout.queue(MoveTo(rect.x, rect.bottom()))?;
    print!("{}", bottom);

    Ok(())
}

fn draw_sidebar(
    stdout: &mut io::Stdout,
    rect: Rect,
    headings: &[SidebarHeading],
    current_entity_idx: usize,
) -> io::Result<()> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(());
    }

    let sidebar_rows = build_sidebar_rows(headings, rect.width as usize, current_entity_idx);
    let active_heading_idx = active_heading_index(headings, current_entity_idx).unwrap_or(0);
    let viewport_rows = centered_viewport_rows(
        active_heading_idx,
        sidebar_rows.len(),
        rect.height as usize,
    );

    for (row, maybe_row_idx) in viewport_rows.iter().enumerate() {
        stdout.queue(MoveTo(rect.x, rect.y + row as u16))?;

        let Some(row_idx) = *maybe_row_idx else {
            print!("{}", " ".repeat(rect.width as usize));
            continue;
        };

        let row_text = sidebar_rows
            .get(row_idx)
            .cloned()
            .unwrap_or_else(String::new);
        print!("{}{}", pad_visible(&row_text, rect.width as usize), ANSI_RESET);
    }

    Ok(())
}

fn centered_viewport_rows(
    focus_row: usize,
    total_rows: usize,
    viewport_height: usize,
) -> Vec<Option<usize>> {
    if viewport_height == 0 {
        return Vec::new();
    }

    let center_offset = viewport_height / 2;

    (0..viewport_height)
        .map(|row| {
            let offset = row as isize - center_offset as isize;
            let idx = focus_row as isize + offset;

            if idx >= 0 && idx < total_rows as isize {
                Some(idx as usize)
            } else {
                None
            }
        })
        .collect()
}

fn build_sidebar_rows(
    headings: &[SidebarHeading],
    max_width: usize,
    current_entity_idx: usize,
) -> Vec<String> {
    let usable_width = max_width.max(1);
    let mut rows = wrap_plain_text("Outline", usable_width);
    if !rows.is_empty() {
        rows[0] = format!("{}{}{}", SIDEBAR_TITLE_COLOR, rows[0], ANSI_RESET);
    }
    rows.push(String::new());

    if headings.is_empty() {
        rows.extend(wrap_plain_text("No headings found", usable_width).into_iter().map(|line| {
            format!("{}{}{}", SIDEBAR_INACTIVE_COLOR, line, ANSI_RESET)
        }));
        return rows;
    }

    let active_idx = active_heading_index(headings, current_entity_idx);
    for (idx, heading) in headings.iter().enumerate() {
        let indent = "  ".repeat(heading.level.saturating_sub(1).min(3) as usize);
        let marker = if Some(idx) == active_idx { ">" } else { " " };
        let prefix = format!("{}{} ", marker, indent);
        let wrapped = wrap_plain_text(
            &format!("{}{}", prefix, heading.title),
            usable_width,
        );
        let continuation_indent = " ".repeat(prefix.chars().count());
        let color = if Some(idx) == active_idx {
            SIDEBAR_ACTIVE_COLOR
        } else {
            SIDEBAR_INACTIVE_COLOR
        };

        for (line_idx, line) in wrapped.into_iter().enumerate() {
            let display = if line_idx == 0 {
                line
            } else {
                format!("{}{}", continuation_indent, line.trim_start())
            };
            rows.push(format!("{}{}{}", color, display, ANSI_RESET));
        }
    }

    rows
}

fn is_in_focus_band(focus_idx: usize, entity_idx: usize, radius: usize) -> bool {
    let start = focus_idx.saturating_sub(radius);
    let end = focus_idx.saturating_add(radius);
    entity_idx >= start && entity_idx <= end
}

fn active_heading_index(headings: &[SidebarHeading], current_entity_idx: usize) -> Option<usize> {
    headings
        .iter()
        .rposition(|heading| heading.entity_idx <= current_entity_idx)
        .or_else(|| (!headings.is_empty()).then_some(0))
}

fn build_display_rows(
    entities: &[RenderedEntity],
    max_width: usize,
) -> (Vec<(usize, RenderedLine, String)>, Vec<usize>, Vec<usize>) {
    let mut rows = Vec::new();
    let mut offsets = Vec::with_capacity(entities.len());
    let mut counts = Vec::with_capacity(entities.len());
    let max_width = max_width.max(MIN_VIEWPORT_WIDTH);

    for (entity_idx, entity) in entities.iter().enumerate() {
        offsets.push(rows.len());

        let start_len = rows.len();
        for line in &entity.lines {
            for segment in wrap_rendered_line(line, max_width) {
                rows.push((entity_idx, line.clone(), segment));
            }
        }

        counts.push(rows.len().saturating_sub(start_len).max(1));
    }

    (rows, offsets, counts)
}

fn collect_sidebar_headings(blocks: &[ParsedBlock]) -> Vec<SidebarHeading> {
    blocks
        .iter()
        .enumerate()
        .filter_map(|(entity_idx, block)| match block {
            ParsedBlock::Text { spans, style } => style.heading_level.map(|level| SidebarHeading {
                entity_idx,
                level,
                title: plain_text_from_spans(spans),
            }),
            _ => None,
        })
        .filter(|heading| !heading.title.is_empty())
        .collect()
}

fn plain_text_from_spans(spans: &[InlineSpan]) -> String {
    spans
        .iter()
        .flat_map(|span| span.text.chars())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn wrap_rendered_line(line: &RenderedLine, max_width: usize) -> Vec<String> {
    let plain = strip_ansi(&line.content);
    let indent_len = list_indent_length(&plain).unwrap_or(0);
    let indent_prefix = if indent_len > 0 {
        Some(" ".repeat(indent_len))
    } else {
        None
    };
    let indent_visible = indent_prefix
        .as_ref()
        .map(|s| s.chars().count())
        .unwrap_or(0);

    let mut wrapped = Vec::new();
    let mut current = String::new();
    let mut visible = 0;
    let mut in_escape = false;
    let mut escape_buf = String::new();
    let mut active_prefix = String::new();
    let max_width = max_width.max(indent_visible + 1).max(MIN_VIEWPORT_WIDTH);

    for ch in line.content.chars() {
        if in_escape {
            escape_buf.push(ch);
            if ch.is_ascii_alphabetic() {
                in_escape = false;
                let seq = format!("\x1b{}", escape_buf);
                current.push_str(&seq);
                if seq.contains("[0m") {
                    active_prefix.clear();
                } else {
                    active_prefix.push_str(&seq);
                }
                escape_buf.clear();
            }
            continue;
        }

        if ch == '\x1b' {
            in_escape = true;
            escape_buf.clear();
            continue;
        }

        if visible >= max_width {
            current.push_str(ANSI_RESET);
            wrapped.push(current);
            current = active_prefix.clone();
            if let Some(indent) = &indent_prefix {
                current.push_str(indent);
                visible = indent_visible;
            } else {
                visible = 0;
            }
        }

        current.push(ch);
        visible += 1;
    }

    if current.is_empty() {
        wrapped.push(String::new());
    } else {
        current.push_str(ANSI_RESET);
        wrapped.push(current);
    }

    wrapped
}

fn wrap_plain_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return Vec::new();
    }

    let mut rows = Vec::new();
    let mut current = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let word_width = word.chars().count();
        let separator = usize::from(!current.is_empty());

        if current_width + separator + word_width <= max_width {
            if separator == 1 {
                current.push(' ');
                current_width += 1;
            }
            current.push_str(word);
            current_width += word_width;
            continue;
        }

        if !current.is_empty() {
            rows.push(current);
            current = String::new();
        }

        if word_width <= max_width {
            current.push_str(word);
            current_width = word_width;
            continue;
        }

        let mut chunk = String::new();
        let mut chunk_width = 0;
        for ch in word.chars() {
            if chunk_width == max_width {
                rows.push(chunk);
                chunk = String::new();
                chunk_width = 0;
            }
            chunk.push(ch);
            chunk_width += 1;
        }
        current = chunk;
        current_width = chunk_width;
    }

    if !current.is_empty() {
        rows.push(current);
    }

    if rows.is_empty() {
        rows.push(String::new());
    }

    rows
}

fn pad_visible(text: &str, width: usize) -> String {
    let visible = strip_ansi(text).chars().count();
    if visible >= width {
        return text.to_string();
    }

    format!("{}{}", text, " ".repeat(width - visible))
}

fn list_indent_length(plain: &str) -> Option<usize> {
    let first_non_space = plain
        .char_indices()
        .find(|(_, ch)| !ch.is_whitespace())
        .map(|(idx, _)| idx)?;

    let rest = &plain[first_non_space..];

    if rest.starts_with("• ") || rest.starts_with("- ") || rest.starts_with("* ") {
        return Some(first_non_space + 2);
    }

    let mut chars = rest.chars().peekable();
    let mut digits = 0;
    while matches!(chars.peek(), Some(ch) if ch.is_ascii_digit()) {
        digits += 1;
        chars.next();
    }

    if digits > 0 && chars.next() == Some('.') && chars.next() == Some(' ') {
        return Some(first_non_space + digits + 2);
    }

    None
}

fn move_down(entities: &[RenderedEntity], current_idx: usize) -> usize {
    if entities.is_empty() {
        return 0;
    }

    find_focusable_forward(entities, current_idx.saturating_add(1)).unwrap_or(current_idx)
}

fn move_up(entities: &[RenderedEntity], current_idx: usize) -> usize {
    if entities.is_empty() || current_idx == 0 {
        return current_idx.min(entities.len().saturating_sub(1));
    }

    find_focusable_backward(entities, current_idx - 1).unwrap_or(current_idx)
}

fn first_focusable_entity(entities: &[RenderedEntity]) -> Option<usize> {
    find_focusable_forward(entities, 0)
}

fn last_focusable_entity(entities: &[RenderedEntity]) -> Option<usize> {
    entities.iter().rposition(RenderedEntity::is_focusable)
}

fn find_focusable_forward(entities: &[RenderedEntity], start: usize) -> Option<usize> {
    (start..entities.len()).find(|&idx| entities[idx].is_focusable())
}

fn find_focusable_backward(entities: &[RenderedEntity], start: usize) -> Option<usize> {
    let mut idx = start;
    loop {
        if entities[idx].is_focusable() {
            return Some(idx);
        }
        if idx == 0 {
            break;
        }
        idx -= 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_headings_for_sidebar() {
        let parser = MarkdownParser::new();
        let parsed = parser.parse("# Intro\n\n## Details\nBody");

        let headings = collect_sidebar_headings(&parsed);

        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].title, "Intro");
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[1].title, "Details");
    }

    #[test]
    fn active_heading_tracks_current_entity() {
        let headings = vec![
            SidebarHeading {
                entity_idx: 0,
                level: 1,
                title: "Intro".to_string(),
            },
            SidebarHeading {
                entity_idx: 5,
                level: 2,
                title: "Details".to_string(),
            },
        ];

        assert_eq!(active_heading_index(&headings, 0), Some(0));
        assert_eq!(active_heading_index(&headings, 4), Some(0));
        assert_eq!(active_heading_index(&headings, 5), Some(1));
    }
}
