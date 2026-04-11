use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

#[derive(Clone, Default)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub link: bool,
    pub code: bool,
    pub blockquote: bool,
}

#[derive(Clone)]
pub struct InlineSpan {
    pub text: String,
    pub style: InlineStyle,
}

#[derive(Clone, Default)]
pub struct BlockStyle {
    pub heading_level: Option<u8>,
}

#[derive(Clone)]
pub enum ParsedBlock {
    Text { spans: Vec<InlineSpan>, style: BlockStyle },
    CodeBlock { lang: String, content: String },
    Table { rows: Vec<Vec<Vec<InlineSpan>>> },
    Empty,
    Rule,
}

#[derive(Default)]
struct InlineState {
    bold: usize,
    italic: usize,
    strikethrough: usize,
    link: usize,
    blockquote: usize,
}

impl InlineState {
    fn current_style(&self) -> InlineStyle {
        InlineStyle {
            bold: self.bold > 0,
            italic: self.italic > 0,
            strikethrough: self.strikethrough > 0,
            link: self.link > 0,
            blockquote: self.blockquote > 0,
            code: false,
        }
    }
}

#[derive(Clone, Copy)]
enum ListKind {
    Unordered,
    Ordered(u64),
}

pub struct MarkdownParser;

impl MarkdownParser {
    pub fn new() -> Self {
        Self
    }

    pub fn parse(&self, text: &str) -> Vec<ParsedBlock> {
        let mut blocks = Vec::new();
        let mut parser = Parser::new_ext(text, Self::parser_options());

        let mut in_code_block = false;
        let mut code_lang = String::new();
        let mut code_content = String::new();

        let mut in_table = false;
        let mut table_rows: Vec<Vec<Vec<InlineSpan>>> = Vec::new();
        let mut current_row: Vec<Vec<InlineSpan>> = Vec::new();

        let mut current_spans: Vec<InlineSpan> = Vec::new();
        let mut block_style = BlockStyle::default();
        let mut inline_state = InlineState::default();
        let mut list_stack: Vec<ListKind> = Vec::new();

        while let Some(event) = parser.next() {
            match event {
                Event::Start(Tag::CodeBlock(kind)) => {
                    Self::push_text_block(&mut blocks, &mut current_spans, &block_style);
                    in_code_block = true;
                    code_lang = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    code_content.clear();
                }
                Event::End(TagEnd::CodeBlock) => {
                    in_code_block = false;
                    blocks.push(ParsedBlock::CodeBlock {
                        lang: code_lang.clone(),
                        content: code_content.clone(),
                    });
                    code_content.clear();
                    code_lang.clear();
                }
                Event::Text(text) if in_code_block => {
                    code_content.push_str(&text);
                }
                Event::Start(Tag::Table(_)) => {
                    Self::push_text_block(&mut blocks, &mut current_spans, &block_style);
                    in_table = true;
                    table_rows.clear();
                }
                Event::End(TagEnd::Table) => {
                    in_table = false;
                    if !current_row.is_empty() {
                        table_rows.push(current_row.clone());
                        current_row.clear();
                    }
                    blocks.push(ParsedBlock::Table {
                        rows: table_rows.clone(),
                    });
                    table_rows.clear();
                }
                Event::Start(Tag::TableHead) => {
                    current_row.clear();
                }
                Event::End(TagEnd::TableHead) => {
                    if !current_row.is_empty() {
                        table_rows.push(current_row.clone());
                        current_row.clear();
                    }
                }
                Event::Start(Tag::TableRow) => {
                    current_row.clear();
                }
                Event::End(TagEnd::TableRow) => {
                    table_rows.push(current_row.clone());
                    current_row.clear();
                }
                Event::Start(Tag::TableCell) => {}
                Event::End(TagEnd::TableCell) => {
                    current_row.push(current_spans.clone());
                    current_spans.clear();
                }
                Event::Start(Tag::List(start_num)) => {
                    let kind = start_num.map(ListKind::Ordered).unwrap_or(ListKind::Unordered);
                    list_stack.push(kind);
                }
                Event::End(TagEnd::List(_)) => {
                    list_stack.pop();
                }
                Event::Start(Tag::Item) => {
                    let indent = "  ".repeat(list_stack.len().saturating_sub(1));
                    if let Some(last) = list_stack.last_mut() {
                        match last {
                            ListKind::Ordered(next) => {
                                let prefix = format!("{}{}. ", indent, *next);
                                *next += 1;
                                current_spans.push(InlineSpan {
                                    text: prefix,
                                    style: inline_state.current_style(),
                                });
                            }
                            ListKind::Unordered => {
                                current_spans.push(InlineSpan {
                                    text: format!("{}• ", indent),
                                    style: inline_state.current_style(),
                                });
                            }
                        }
                    }
                }
                Event::End(TagEnd::Item) => {
                    Self::push_text_block(&mut blocks, &mut current_spans, &block_style);
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    Self::push_text_block(&mut blocks, &mut current_spans, &block_style);
                    block_style.heading_level = Some(level as u8);
                }
                Event::End(TagEnd::Heading(_)) => {
                    Self::push_text_block(&mut blocks, &mut current_spans, &block_style);
                    block_style.heading_level = None;
                }
                Event::Start(Tag::Paragraph) => {}
                Event::End(TagEnd::Paragraph) => {
                    Self::push_text_block(&mut blocks, &mut current_spans, &block_style);
                    blocks.push(ParsedBlock::Empty);
                }
                Event::Start(Tag::Strong) => inline_state.bold += 1,
                Event::End(TagEnd::Strong) => inline_state.bold = inline_state.bold.saturating_sub(1),
                Event::Start(Tag::Emphasis) => inline_state.italic += 1,
                Event::End(TagEnd::Emphasis) => inline_state.italic = inline_state.italic.saturating_sub(1),
                Event::Start(Tag::Strikethrough) => inline_state.strikethrough += 1,
                Event::End(TagEnd::Strikethrough) => {
                    inline_state.strikethrough = inline_state.strikethrough.saturating_sub(1);
                }
                Event::Start(Tag::Link { .. }) => inline_state.link += 1,
                Event::End(TagEnd::Link) => inline_state.link = inline_state.link.saturating_sub(1),
                Event::Start(Tag::BlockQuote(_)) => {
                    inline_state.blockquote += 1;
                    current_spans.push(InlineSpan {
                        text: "> ".to_string(),
                        style: inline_state.current_style(),
                    });
                }
                Event::End(TagEnd::BlockQuote(_)) => inline_state.blockquote = inline_state.blockquote.saturating_sub(1),
                Event::Code(code) => {
                    let mut style = inline_state.current_style();
                    style.code = true;
                    current_spans.push(InlineSpan {
                        text: code.to_string(),
                        style,
                    });
                }
                Event::Text(text) => {
                    if in_table {
                        current_spans.push(InlineSpan {
                            text: text.to_string(),
                            style: inline_state.current_style(),
                        });
                    } else {
                        current_spans.push(InlineSpan {
                            text: text.to_string(),
                            style: inline_state.current_style(),
                        });
                    }
                }
                Event::SoftBreak => current_spans.push(InlineSpan {
                    text: " ".to_string(),
                    style: inline_state.current_style(),
                }),
                Event::HardBreak => {
                    if in_table {
                        current_spans.push(InlineSpan {
                            text: " ".to_string(),
                            style: inline_state.current_style(),
                        });
                    } else {
                        Self::push_text_block(&mut blocks, &mut current_spans, &block_style);
                    }
                }
                Event::Rule => blocks.push(ParsedBlock::Rule),
                _ => {}
            }
        }

        Self::push_text_block(&mut blocks, &mut current_spans, &block_style);

        blocks
    }

    fn push_text_block(blocks: &mut Vec<ParsedBlock>, spans: &mut Vec<InlineSpan>, style: &BlockStyle) {
        if spans.is_empty() {
            return;
        }
        blocks.push(ParsedBlock::Text {
            spans: spans.clone(),
            style: style.clone(),
        });
        spans.clear();
    }

    fn parser_options() -> Options {
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH
    }
}
