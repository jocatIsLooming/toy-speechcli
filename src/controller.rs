use std::fs;
use std::io::{self, Write};

use crossterm::{
    ExecutableCommand, QueueableCommand,
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::parser::MarkdownParser;
use crate::renderer::{
    MarkdownRenderer, RenderedEntity, RenderedLine, apply_opacity_to_line, strip_ansi,
};

const LOW_OPACITY: u8 = 55;
const FOCUS_BAND_RADIUS: usize = 2;
const MIN_VIEWPORT_WIDTH: usize = 10;
const ANSI_RESET: &str = "\x1b[0m";

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

    if entities.is_empty() {
        println!("No text to display");
        return Ok(());
    }

    let mut stdout = io::stdout();
    let (cols, rows) = terminal::size()?;
    let viewport_height = rows as usize;
    let viewport_width = cols.max(MIN_VIEWPORT_WIDTH as u16) as usize;

    terminal::enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(Hide)?;

    let mut current_entity_idx = first_focusable_entity(&entities).unwrap_or(0);

    loop {
        stdout.queue(Clear(ClearType::All))?;

        let (display_rows, entity_row_offsets, entity_row_counts) =
            build_display_rows(&entities, viewport_width);

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

        let viewport_rows = centered_viewport_rows(focus_row, display_rows.len(), viewport_height);

        for (row, maybe_row_idx) in viewport_rows.iter().enumerate() {
            stdout.queue(MoveTo(0, row as u16))?;

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

fn is_in_focus_band(focus_idx: usize, entity_idx: usize, radius: usize) -> bool {
    let start = focus_idx.saturating_sub(radius);
    let end = focus_idx.saturating_add(radius);
    entity_idx >= start && entity_idx <= end
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
