use std::fs;
use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand, QueueableCommand,
};

use crate::parser::MarkdownParser;
use crate::renderer::{
    apply_opacity_to_line, strip_ansi, LineType, MarkdownRenderer, RenderedLine,
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
    let lines = renderer.render(&parsed);

    if lines.is_empty() {
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

    let mut current_idx = 0;

    loop {
        stdout.queue(Clear(ClearType::All))?;

        let focus_idx = focus_index_for_line(&lines, current_idx);
        let (display_rows, line_row_offsets, line_row_counts) =
            build_display_rows(&lines, viewport_width);

        let focus_row = line_row_offsets
            .get(focus_idx)
            .copied()
            .unwrap_or(0)
            + line_row_counts
                .get(focus_idx)
                .copied()
                .unwrap_or(1)
                .saturating_sub(1)
                / 2;

        let viewport_rows =
            centered_viewport_rows(focus_row, display_rows.len(), viewport_height);

        let active_block = active_block_range(&lines, current_idx);

        for (row, maybe_row_idx) in viewport_rows.iter().enumerate() {
            stdout.queue(MoveTo(0, row as u16))?;

            let Some(display_row_idx) = *maybe_row_idx else {
                continue;
            };

            let (line_idx, segment) = &display_rows[display_row_idx];
            let line = &lines[*line_idx];
            let in_active_block = active_block
                .map(|(start, end)| *line_idx >= start && *line_idx <= end)
                .unwrap_or(false);
            let in_focus_band = is_in_focus_band(focus_idx, *line_idx, FOCUS_BAND_RADIUS);

            let opacity = if in_active_block || in_focus_band {
                255
            } else {
                LOW_OPACITY
            };

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
                        current_idx = move_down(&lines, current_idx);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        current_idx = move_up(&lines, current_idx);
                    }
                    KeyCode::Home => current_idx = 0,
                    KeyCode::End => current_idx = lines.len().saturating_sub(1),
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

fn active_block_range(lines: &[RenderedLine], current_idx: usize) -> Option<(usize, usize)> {
    block_range_at(lines, current_idx)
}

fn block_range_at(lines: &[RenderedLine], idx: usize) -> Option<(usize, usize)> {
    if lines.is_empty() || idx >= lines.len() {
        return None;
    }

    let line_type = lines[idx].line_type;
    if line_type != LineType::CodeBlock && line_type != LineType::Table {
        return None;
    }

    let mut start = idx;
    while start > 0 && lines[start - 1].line_type == line_type {
        start -= 1;
    }

    let mut end = idx;
    while end + 1 < lines.len() && lines[end + 1].line_type == line_type {
        end += 1;
    }

    Some((start, end))
}

fn focus_index_for_line(lines: &[RenderedLine], idx: usize) -> usize {
    if let Some((start, end)) = block_range_at(lines, idx) {
        start + (end - start) / 2
    } else {
        idx
    }
}

fn is_in_focus_band(focus_idx: usize, line_idx: usize, radius: usize) -> bool {
    let start = focus_idx.saturating_sub(radius);
    let end = focus_idx.saturating_add(radius);
    line_idx >= start && line_idx <= end
}

fn build_display_rows(
    lines: &[RenderedLine],
    max_width: usize,
) -> (Vec<(usize, String)>, Vec<usize>, Vec<usize>) {
    let mut rows = Vec::new();
    let mut offsets = Vec::with_capacity(lines.len());
    let mut counts = Vec::with_capacity(lines.len());
    let max_width = max_width.max(MIN_VIEWPORT_WIDTH);

    for (idx, line) in lines.iter().enumerate() {
        offsets.push(rows.len());

        let wrapped = wrap_rendered_line(line, max_width);
        let count = wrapped.len().max(1);
        counts.push(count);

        for segment in wrapped {
            rows.push((idx, segment));
        }
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

    if digits > 0 {
        if chars.next() == Some('.') && chars.next() == Some(' ') {
            return Some(first_non_space + digits + 2);
        }
    }

    None
}

fn move_down(lines: &[RenderedLine], current_idx: usize) -> usize {
    if lines.is_empty() {
        return 0;
    }

    if let Some((_, end)) = active_block_range(lines, current_idx) {
        let target = end.saturating_add(1);
        return find_nonempty_forward(lines, target).unwrap_or(current_idx);
    }

    if current_idx < lines.len() - 1 {
        let target = current_idx + 1;
        find_nonempty_forward(lines, target)
            .map(|idx| focus_index_for_line(lines, idx))
            .unwrap_or(current_idx)
    } else {
        current_idx
    }
}

fn move_up(lines: &[RenderedLine], current_idx: usize) -> usize {
    if lines.is_empty() {
        return 0;
    }

    if let Some((start, _)) = active_block_range(lines, current_idx) {
        if start > 0 {
            let target = start - 1;
            return find_nonempty_backward(lines, target).unwrap_or(current_idx);
        }
        return 0;
    }

    if current_idx > 0 {
        let target = current_idx - 1;
        find_nonempty_backward(lines, target)
            .map(|idx| focus_index_for_line(lines, idx))
            .unwrap_or(current_idx)
    } else {
        current_idx
    }
}

fn is_nonempty(line: &RenderedLine) -> bool {
    line.line_type != LineType::Empty && !line.content.trim().is_empty()
}

fn find_nonempty_forward(lines: &[RenderedLine], start: usize) -> Option<usize> {
    for idx in start..lines.len() {
        if is_nonempty(&lines[idx]) {
            return Some(idx);
        }
    }
    None
}

fn find_nonempty_backward(lines: &[RenderedLine], start: usize) -> Option<usize> {
    let mut idx = start;
    loop {
        if is_nonempty(&lines[idx]) {
            return Some(idx);
        }
        if idx == 0 {
            break;
        }
        idx -= 1;
    }
    None
}
