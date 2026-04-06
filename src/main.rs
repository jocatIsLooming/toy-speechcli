mod markdown;

use std::fs;
use std::io::{self, Write};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand, QueueableCommand,
};
use markdown::{MarkdownRenderer, apply_opacity_to_line};

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let text = if args.len() > 1 {
        fs::read_to_string(&args[1])?
    } else {
        fs::read_to_string("speech.txt")?
    };

    let renderer = MarkdownRenderer::new();
    let lines = renderer.parse_and_render(&text);

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

        let visible_range = calculate_visible_range(current_idx, lines.len(), viewport_height);

        for (i, line_idx) in visible_range.iter().enumerate() {
            let line = &lines[*line_idx];
            let distance = (*line_idx as isize - current_idx as isize).abs() as usize;
            let opacity = calculate_opacity(distance);

            let row = i as u16;
            stdout.queue(MoveTo(0, row))?;

            let styled_line = apply_opacity_to_line(line, opacity);
            print!("{}", styled_line);
        }

        stdout.flush()?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Down | KeyCode::Char('j') | KeyCode::Char(' ') => {
                        if current_idx < lines.len() - 1 {
                            current_idx += 1;
                        }
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if current_idx > 0 {
                            current_idx -= 1;
                        }
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

fn calculate_visible_range(
    current: usize,
    total: usize,
    viewport_height: usize,
) -> Vec<usize> {
    let half_viewport = viewport_height / 2;

    let start = current.saturating_sub(half_viewport);
    let end = (start + viewport_height).min(total);
    let start = end.saturating_sub(viewport_height);

    (start..end).collect()
}

fn calculate_opacity(distance: usize) -> u8 {
    match distance {
        0 => 255,
        1 => 180,
        2 => 120,
        3 => 80,
        4 => 50,
        _ => 30,
    }
}
