# toy-speechcli

Terminal teleprompter that renders Markdown with syntax highlighting and a subtle focus gradient so you can rehearse speeches or talks without losing your place.

## Why this exists

- Keeps a crisp spotlight on the current line while gently dimming context above and below.
- Understands real Markdown: headings, lists, emphasis, blockquotes, code fences, and tables.
- Syntax-highlights fenced code blocks using `syntect` for quick technical demos.
- Uses `crossterm` for smooth scrolling and alternate-screen rendering that will not mangle your shell.

## Quick start

```bash
cargo run --release -- speech_example.md   # run with the bundled sample script
```

You need Rust with edition 2024 support (Rust 1.79+ recommended).

## Controls

- `j`, `↓`, or `Space` — advance one line
- `k` or `↑` — move back one line
- `Home` — jump to the top
- `End` — jump to the bottom
- `q` or `Esc` — quit

The app opens in an alternate screen; when you quit, your terminal returns to normal.

## Preparing your script

1. Write your talk as Markdown (tables, code fences, quotes, lists all work).
2. Save it as `speech.txt` in the project root, or pass a path as the first argument.
3. Optional: start from `speech_example.md` to see supported formatting.

## How it works

`MarkdownRenderer` (see `src/markdown.rs`) parses the document with `pulldown-cmark`, renders inline styling, highlights code fences via `syntect`, and draws tables with box-drawing characters. The main loop (`src/main.rs`) keeps track of the “current” line and re-renders the visible viewport while applying a fade based on distance from the focus line.

## Limitations

- No pagination or time-based auto-scroll yet; it is manual only.
- Long lines do not wrap; they will flow off the right edge of your terminal.
- Rendering relies on ANSI escape codes; some terminals without truecolor support may look less vibrant.

