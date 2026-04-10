use std::fs;
use std::io::{self, Write};

use crossterm::{
    ExecutableCommand, QueueableCommand,
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::parser::MarkdownParser;
use crate::renderer::{LineType, MarkdownRenderer, RenderedLine, apply_opacity_to_line};

const LOW_OPACITY: u8 = 50;
const FOCUS_BAND_RADIUS: usize = 2;

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
    let (_cols, rows) = terminal::size()?;
    let viewport_height = rows as usize;

    terminal::enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(Hide)?;

    let mut current_idx = 0;

    loop {
        stdout.queue(Clear(ClearType::All))?;

        let focus_idx = focus_index_for_line(&lines, current_idx);
        let viewport_indices = centered_viewport_indices(focus_idx, lines.len(), viewport_height);

        let active_block = active_block_range(&lines, current_idx);

        for (row, maybe_line_idx) in viewport_indices.iter().enumerate() {
            stdout.queue(MoveTo(0, row as u16))?;

            let Some(line_idx) = *maybe_line_idx else {
                continue;
            };

            let line = &lines[line_idx];
            let in_active_block = active_block
                .map(|(start, end)| line_idx >= start && line_idx <= end)
                .unwrap_or(false);

            let opacity = if in_active_block || is_in_focus_band(focus_idx, line_idx, 0) {
                255
            // } else if is_in_focus_band(focus_idx, line_idx, 1) {
            //     155
            } else {
                55
            };

            let styled_line = apply_opacity_to_line(line, opacity);
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

fn centered_viewport_indices(
    current: usize,
    total: usize,
    viewport_height: usize,
) -> Vec<Option<usize>> {
    if viewport_height == 0 {
        return Vec::new();
    }

    let center_row = viewport_height / 2;

    (0..viewport_height)
        .map(|row| {
            let offset = row as isize - center_row as isize;
            let idx = current as isize + offset;

            if idx >= 0 && idx < total as isize {
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
