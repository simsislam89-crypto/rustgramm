//! Парсер Telegram Layer 233 `RichMessage` (`richMessage#baf39d8b`) для современных ботов
//! (таблицы `PageBlockTable`, цитаты `PageBlockBlockquote`, заголовки, списки с чекбоксами,
//! встроенные фото/видео/GIF и пользовательские эмодзи `textCustomEmoji`).

use grammers_client::tl;

use crate::engine::{Entity, EntityKind};

/// Ячейка таблицы `PageBlockTable`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RichTableCell {
    pub header: bool,
    /// 0 — влево, 1 — по центру, 2 — вправо.
    pub align: u8,
    pub text: String,
    pub entities: Vec<Entity>,
}

/// Пункт списка `PageBlockList` / `PageBlockOrderedList`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RichListItem {
    pub checkbox: bool,
    pub checked: bool,
    pub text: String,
    pub entities: Vec<Entity>,
}

/// Структурированный блок `RichMessage` для отрисовки в пузыре сообщения.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RichBlock {
    Heading {
        level: u8,
        text: String,
        entities: Vec<Entity>,
    },
    Paragraph {
        text: String,
        entities: Vec<Entity>,
    },
    Blockquote {
        text: String,
        entities: Vec<Entity>,
        caption: String,
    },
    Preformatted {
        code: String,
        language: String,
    },
    Table {
        title: String,
        bordered: bool,
        striped: bool,
        rows: Vec<Vec<RichTableCell>>,
    },
    List {
        ordered: bool,
        items: Vec<RichListItem>,
    },
    Media {
        is_video: bool,
        autoplay: bool,
        media_id: i64,
        caption: String,
    },
    Divider,
    Footer {
        text: String,
        entities: Vec<Entity>,
    },
}

/// Извлекает текст и сущности из дерева `tl::enums::RichText`.
pub fn flatten_rich_text(rt: &tl::enums::RichText) -> (String, Vec<Entity>) {
    let mut text = String::new();
    let mut ents = Vec::new();
    let mut char_pos = 0usize;
    walk_rich_text(rt, &mut text, &mut ents, &mut char_pos);
    (text, ents)
}

fn walk_rich_text(
    rt: &tl::enums::RichText,
    out: &mut String,
    ents: &mut Vec<Entity>,
    char_pos: &mut usize,
) {
    use tl::enums::RichText as R;
    match rt {
        R::TextEmpty => {}
        R::TextPlain(x) => {
            out.push_str(&x.text);
            *char_pos += x.text.chars().count();
        }
        R::TextConcat(x) => {
            for part in &x.texts {
                walk_rich_text(part, out, ents, char_pos);
            }
        }
        R::TextBold(x) => wrap_entity(&x.text, EntityKind::Bold, None, out, ents, char_pos),
        R::TextItalic(x) => wrap_entity(&x.text, EntityKind::Italic, None, out, ents, char_pos),
        R::TextUnderline(x) => wrap_entity(&x.text, EntityKind::Underline, None, out, ents, char_pos),
        R::TextStrike(x) => wrap_entity(&x.text, EntityKind::Strike, None, out, ents, char_pos),
        R::TextFixed(x) => wrap_entity(&x.text, EntityKind::Code, None, out, ents, char_pos),
        R::TextSpoiler(x) => wrap_entity(&x.text, EntityKind::Spoiler, None, out, ents, char_pos),
        R::TextMarked(x) => wrap_entity(&x.text, EntityKind::Quote, None, out, ents, char_pos),
        R::TextUrl(x) => {
            wrap_entity(&x.text, EntityKind::Link, Some(x.url.clone()), out, ents, char_pos);
        }
        R::TextEmail(x) => {
            wrap_entity(&x.text, EntityKind::Link, Some(format!("mailto:{}", x.email)), out, ents, char_pos);
        }
        R::TextMention(x) => wrap_entity(&x.text, EntityKind::Mention, None, out, ents, char_pos),
        R::TextMentionName(x) => {
            wrap_entity(&x.text, EntityKind::Mention, None, out, ents, char_pos);
        }
        R::TextHashtag(x) => wrap_entity(&x.text, EntityKind::Hashtag, None, out, ents, char_pos),
        R::TextBotCommand(x) => wrap_entity(&x.text, EntityKind::Command, None, out, ents, char_pos),
        R::TextAutoUrl(x) => wrap_entity(&x.text, EntityKind::Link, None, out, ents, char_pos),
        R::TextCustomEmoji(x) => {
            let start = *char_pos;
            let alt = if x.alt.is_empty() { "✨" } else { x.alt.as_str() };
            out.push_str(alt);
            *char_pos += alt.chars().count();
            ents.push(Entity {
                kind: EntityKind::CustomEmoji(x.document_id),
                start,
                end: *char_pos,
                url: None,
            });
        }
        R::TextSubscript(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextSuperscript(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextPhone(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextAnchor(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextMath(x) => {
            let start = *char_pos;
            out.push_str(&x.source);
            *char_pos += x.source.chars().count();
            ents.push(Entity {
                kind: EntityKind::Code,
                start,
                end: *char_pos,
                url: None,
            });
        }
        R::TextCashtag(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextAutoEmail(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextAutoPhone(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextBankCard(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextDate(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextDiff(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextButton(x) => walk_rich_text(&x.text, out, ents, char_pos),
        R::TextTonAddress(x) => wrap_entity(&x.text, EntityKind::Code, None, out, ents, char_pos),
        R::TextImage(x) => {
            let start = *char_pos;
            out.push('▪');
            *char_pos += 1;
            ents.push(Entity {
                kind: EntityKind::CustomEmoji(x.document_id),
                start,
                end: *char_pos,
                url: None,
            });
        }
    }
}

fn wrap_entity(
    inner: &tl::enums::RichText,
    kind: EntityKind,
    url: Option<String>,
    out: &mut String,
    ents: &mut Vec<Entity>,
    char_pos: &mut usize,
) {
    let start = *char_pos;
    walk_rich_text(inner, out, ents, char_pos);
    let end = *char_pos;
    if end > start {
        ents.push(Entity { kind, start, end, url });
    }
}

fn caption_text(cap: &tl::enums::PageCaption) -> String {
    let tl::enums::PageCaption::Caption(c) = cap;
    let (a, _) = flatten_rich_text(&c.text);
    let (b, _) = flatten_rich_text(&c.credit);
    match (a.trim().is_empty(), b.trim().is_empty()) {
        (true, true) => String::new(),
        (false, true) => a,
        (true, false) => b,
        (false, false) => format!("{a} — {b}"),
    }
}

fn parse_blocks(blocks: &[tl::enums::PageBlock], out: &mut Vec<RichBlock>) {
    use tl::enums::PageBlock as B;
    for blk in blocks {
        match blk {
            B::Title(x) => push_heading(1, &x.text, out),
            B::Subtitle(x) => push_heading(2, &x.text, out),
            B::Header(x) => push_heading(1, &x.text, out),
            B::Heading1(x) => push_heading(1, &x.text, out),
            B::Subheader(x) => push_heading(2, &x.text, out),
            B::Heading2(x) => push_heading(2, &x.text, out),
            B::Heading3(x) => push_heading(3, &x.text, out),
            B::Kicker(x) => push_heading(3, &x.text, out),
            B::Heading4(x) => push_heading(4, &x.text, out),
            B::Heading5(x) => push_heading(4, &x.text, out),
            B::Heading6(x) => push_heading(4, &x.text, out),
            B::Paragraph(x) => {
                let (text, entities) = flatten_rich_text(&x.text);
                if !text.is_empty() {
                    out.push(RichBlock::Paragraph { text, entities });
                }
            }
            B::Preformatted(x) => {
                let (code, _) = flatten_rich_text(&x.text);
                out.push(RichBlock::Preformatted {
                    code,
                    language: x.language.clone(),
                });
            }
            B::Blockquote(x) => {
                let (text, entities) = flatten_rich_text(&x.text);
                let (caption, _) = flatten_rich_text(&x.caption);
                out.push(RichBlock::Blockquote { text, entities, caption });
            }
            B::Pullquote(x) => {
                let (text, entities) = flatten_rich_text(&x.text);
                let (caption, _) = flatten_rich_text(&x.caption);
                out.push(RichBlock::Blockquote { text, entities, caption });
            }
            B::BlockquoteBlocks(x) => {
                let mut inner = Vec::new();
                parse_blocks(&x.blocks, &mut inner);
                let (caption, _) = flatten_rich_text(&x.caption);
                let mut combined = String::new();
                let mut all_ents = Vec::new();
                let mut off = 0usize;
                for ib in inner {
                    let (t, es) = match ib {
                        RichBlock::Paragraph { text, entities }
                        | RichBlock::Heading { text, entities, .. }
                        | RichBlock::Footer { text, entities }
                        | RichBlock::Blockquote { text, entities, .. } => (text, entities),
                        RichBlock::Preformatted { code, .. } => (code, Vec::new()),
                        _ => continue,
                    };
                    if !combined.is_empty() {
                        combined.push('\n');
                        off += 1;
                    }
                    combined.push_str(&t);
                    for mut e in es {
                        e.start += off;
                        e.end += off;
                        all_ents.push(e);
                    }
                    off += t.chars().count();
                }
                if !combined.is_empty() {
                    out.push(RichBlock::Blockquote {
                        text: combined,
                        entities: all_ents,
                        caption,
                    });
                }
            }
            B::Table(x) => {
                let (title, _) = flatten_rich_text(&x.title);
                let mut rows = Vec::with_capacity(x.rows.len());
                for r in &x.rows {
                    let tl::enums::PageTableRow::Row(tr) = r;
                    let mut cells = Vec::with_capacity(tr.cells.len());
                    for c in &tr.cells {
                        let tl::enums::PageTableCell::Cell(tc) = c;
                        let (text, entities) = tc
                            .text
                            .as_ref()
                            .map(flatten_rich_text)
                            .unwrap_or_default();
                        let align = if tc.align_center {
                            1
                        } else if tc.align_right {
                            2
                        } else {
                            0
                        };
                        cells.push(RichTableCell {
                            header: tc.header,
                            align,
                            text,
                            entities,
                        });
                    }
                    if !cells.is_empty() {
                        rows.push(cells);
                    }
                }
                if !rows.is_empty() {
                    out.push(RichBlock::Table {
                        title,
                        bordered: x.bordered,
                        striped: x.striped,
                        rows,
                    });
                }
            }
            B::List(x) => {
                let mut items = Vec::with_capacity(x.items.len());
                for it in &x.items {
                    match it {
                        tl::enums::PageListItem::Text(li) => {
                            let (text, entities) = flatten_rich_text(&li.text);
                            items.push(RichListItem {
                                checkbox: li.checkbox,
                                checked: li.checked,
                                text,
                                entities,
                            });
                        }
                        tl::enums::PageListItem::Blocks(li) => {
                            let mut sub = Vec::new();
                            parse_blocks(&li.blocks, &mut sub);
                            let (text, entities) = flatten_blocks_to_line(&sub);
                            items.push(RichListItem {
                                checkbox: li.checkbox,
                                checked: li.checked,
                                text,
                                entities,
                            });
                        }
                    }
                }
                if !items.is_empty() {
                    out.push(RichBlock::List { ordered: false, items });
                }
            }
            B::OrderedList(x) => {
                let mut items = Vec::with_capacity(x.items.len());
                for it in &x.items {
                    match it {
                        tl::enums::PageListOrderedItem::Text(li) => {
                            let (text, entities) = flatten_rich_text(&li.text);
                            items.push(RichListItem {
                                checkbox: false,
                                checked: false,
                                text,
                                entities,
                            });
                        }
                        tl::enums::PageListOrderedItem::Blocks(li) => {
                            let mut sub = Vec::new();
                            parse_blocks(&li.blocks, &mut sub);
                            let (text, entities) = flatten_blocks_to_line(&sub);
                            items.push(RichListItem {
                                checkbox: false,
                                checked: false,
                                text,
                                entities,
                            });
                        }
                    }
                }
                if !items.is_empty() {
                    out.push(RichBlock::List { ordered: true, items });
                }
            }
            B::Photo(x) => {
                out.push(RichBlock::Media {
                    is_video: false,
                    autoplay: false,
                    media_id: x.photo_id,
                    caption: caption_text(&x.caption),
                });
            }
            B::Video(x) => {
                out.push(RichBlock::Media {
                    is_video: true,
                    autoplay: x.autoplay || x.r#loop,
                    media_id: x.video_id,
                    caption: caption_text(&x.caption),
                });
            }
            B::Document(x) => {
                out.push(RichBlock::Media {
                    is_video: true,
                    autoplay: false,
                    media_id: x.document_id,
                    caption: caption_text(&x.caption),
                });
            }
            B::Cover(x) => {
                parse_blocks(std::slice::from_ref(&x.cover), out);
            }
            B::Collage(x) => parse_blocks(&x.items, out),
            B::Slideshow(x) => parse_blocks(&x.items, out),
            B::Details(x) => {
                push_heading(3, &x.title, out);
                parse_blocks(&x.blocks, out);
            }
            B::Divider => out.push(RichBlock::Divider),
            B::Footer(x) => {
                let (text, entities) = flatten_rich_text(&x.text);
                if !text.is_empty() {
                    out.push(RichBlock::Footer { text, entities });
                }
            }
            B::Math(x) => {
                out.push(RichBlock::Preformatted {
                    code: x.source.clone(),
                    language: "math".to_owned(),
                });
            }
            B::Thinking(x) => {
                let (text, entities) = flatten_rich_text(&x.text);
                if !text.is_empty() {
                    out.push(RichBlock::Blockquote {
                        text,
                        entities,
                        caption: "Размышление".to_owned(),
                    });
                }
            }
            _ => {}
        }
    }
}

fn push_heading(level: u8, rt: &tl::enums::RichText, out: &mut Vec<RichBlock>) {
    let (text, entities) = flatten_rich_text(rt);
    if !text.is_empty() {
        out.push(RichBlock::Heading { level, text, entities });
    }
}

fn flatten_blocks_to_line(blocks: &[RichBlock]) -> (String, Vec<Entity>) {
    let mut out = String::new();
    let mut ents = Vec::new();
    let mut off = 0usize;
    for b in blocks {
        let (t, es) = match b {
            RichBlock::Paragraph { text, entities }
            | RichBlock::Heading { text, entities, .. }
            | RichBlock::Footer { text, entities }
            | RichBlock::Blockquote { text, entities, .. } => (text.as_str(), entities.as_slice()),
            RichBlock::Preformatted { code, .. } => (code.as_str(), &[][..]),
            _ => continue,
        };
        if !out.is_empty() {
            out.push(' ');
            off += 1;
        }
        out.push_str(t);
        for e in es {
            ents.push(Entity {
                kind: e.kind,
                start: e.start + off,
                end: e.end + off,
                url: e.url.clone(),
            });
        }
        off += t.chars().count();
    }
    (out, ents)
}

/// Разбирает `tl::enums::RichMessage` в список блоков `RichBlock`, сводный текст и
/// список всех `custom_emoji` document_id для предзагрузки их текстур.
pub fn parse_rich_message(rm: &tl::enums::RichMessage) -> (Vec<RichBlock>, String) {
    let tl::enums::RichMessage::Message(m) = rm;
    let mut blocks = Vec::new();
    parse_blocks(&m.blocks, &mut blocks);
    let summary = summarize_blocks(&blocks);
    (blocks, summary)
}

/// Собирает текстовую выжимку из `RichBlock` (для поиска, копирования и предпросмотра в списке чатов).
pub fn summarize_blocks(blocks: &[RichBlock]) -> String {
    let mut lines = Vec::new();
    for b in blocks {
        match b {
            RichBlock::Heading { text, .. }
            | RichBlock::Paragraph { text, .. }
            | RichBlock::Footer { text, .. } => {
                if !text.trim().is_empty() {
                    lines.push(text.trim().to_owned());
                }
            }
            RichBlock::Blockquote { text, .. } => {
                if !text.trim().is_empty() {
                    lines.push(format!("▎ {}", text.trim()));
                }
            }
            RichBlock::Preformatted { code, .. } => {
                if !code.trim().is_empty() {
                    lines.push(code.trim().to_owned());
                }
            }
            RichBlock::Table { rows, .. } => {
                for r in rows {
                    let row_s: Vec<&str> = r.iter().map(|c| c.text.trim()).collect();
                    lines.push(row_s.join(" | "));
                }
            }
            RichBlock::List { ordered, items } => {
                for (i, it) in items.iter().enumerate() {
                    let mark = if it.checkbox {
                        if it.checked { "☑".to_owned() } else { "☐".to_owned() }
                    } else if *ordered {
                        format!("{}.", i + 1)
                    } else {
                        "•".to_owned()
                    };
                    lines.push(format!("{mark} {}", it.text.trim()));
                }
            }
            RichBlock::Media { caption, .. } if !caption.trim().is_empty() => {
                lines.push(caption.trim().to_owned());
            }
            _ => {}
        }
    }
    lines.join("\n")
}

/// Собирает все `CustomEmoji(doc_id)` внутри блоков `RichMessage` для загрузки их иконок.
pub fn collect_rich_custom_emojis(blocks: &[RichBlock], out: &mut Vec<i64>) {
    let push_ents = |ents: &[Entity], out: &mut Vec<i64>| {
        for e in ents {
            if let EntityKind::CustomEmoji(id) = e.kind {
                out.push(id);
            }
        }
    };
    for b in blocks {
        match b {
            RichBlock::Heading { entities, .. }
            | RichBlock::Paragraph { entities, .. }
            | RichBlock::Blockquote { entities, .. }
            | RichBlock::Footer { entities, .. } => push_ents(entities, out),
            RichBlock::Table { rows, .. } => {
                for r in rows {
                    for c in r {
                        push_ents(&c.entities, out);
                    }
                }
            }
            RichBlock::List { items, .. } => {
                for it in items {
                    push_ents(&it.entities, out);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_table_blockquote_and_custom_emoji() {
        let rm = tl::enums::RichMessage::Message(tl::types::RichMessage {
            rtl: false,
            part: false,
            blocks: vec![
                tl::enums::PageBlock::Header(tl::types::PageBlockHeader {
                    text: tl::enums::RichText::TextConcat(tl::types::TextConcat {
                        texts: vec![
                            tl::enums::RichText::TextCustomEmoji(tl::types::TextCustomEmoji {
                                document_id: 555_001,
                                alt: "💎".to_owned(),
                            }),
                            tl::enums::RichText::TextPlain(tl::types::TextPlain {
                                text: " eroot".to_owned(),
                            }),
                        ],
                    }),
                }),
                tl::enums::PageBlock::Table(tl::types::PageBlockTable {
                    bordered: true,
                    striped: true,
                    compact: false,
                    title: tl::enums::RichText::TextEmpty,
                    rows: vec![tl::enums::PageTableRow::Row(tl::types::PageTableRow {
                        cells: vec![
                            tl::enums::PageTableCell::Cell(tl::types::PageTableCell {
                                header: true,
                                align_center: false,
                                align_right: false,
                                valign_middle: false,
                                valign_bottom: false,
                                text: Some(tl::enums::RichText::TextPlain(tl::types::TextPlain {
                                    text: "Товар".to_owned(),
                                })),
                                colspan: None,
                                rowspan: None,
                            }),
                            tl::enums::PageTableCell::Cell(tl::types::PageTableCell {
                                header: true,
                                align_center: false,
                                align_right: false,
                                valign_middle: false,
                                valign_bottom: false,
                                text: Some(tl::enums::RichText::TextPlain(tl::types::TextPlain {
                                    text: "Цена".to_owned(),
                                })),
                                colspan: None,
                                rowspan: None,
                            }),
                        ],
                    })],
                }),
            ],
            photos: Vec::new(),
            documents: Vec::new(),
        });
        let (blocks, summary) = parse_rich_message(&rm);
        assert_eq!(blocks.len(), 2);
        assert!(summary.contains("eroot"));
        assert!(summary.contains("Товар | Цена"));
        let mut ids = Vec::new();
        collect_rich_custom_emojis(&blocks, &mut ids);
        assert_eq!(ids, vec![555_001]);
    }
}
