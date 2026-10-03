//! Streaming emoji cell allocation. Ordinary text keeps the existing VT placement.

use unicode_properties::UnicodeEmoji as _;
use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};
use unicode_width::{UnicodeWidthChar as _, UnicodeWidthStr as _};

use super::{Cell, Dimensions, EventListener, Flags, Point, Term, TermMode};
use crate::vte::ansi::Handler as _;

pub(super) struct EmojiInput {
    cell: Point,
    next: Point,
    wrap: bool,
    segmenter: GraphemeCursor,
    bytes: usize,
    context: String,
    last: char,
    graphic: char,
    joined: bool,
}

impl EmojiInput {
    fn new(c: char, cell: Point, next: Point, wrap: bool) -> Option<Self> {
        // The only ASCII emoji bases are the standardized keycap bases.
        if (c.is_ascii() && !matches!(c, '#' | '*' | '0'..='9')) || !c.is_emoji_char() {
            return None;
        }
        let mut buffer = [0; 4];
        let text = c.encode_utf8(&mut buffer);
        // An open-ended cursor defers the boundary until the next UTF-8 chunk.
        let mut segmenter = GraphemeCursor::new(0, usize::MAX, true);
        let result = segmenter.next_boundary(text, 0);
        debug_assert_eq!(result, Err(GraphemeIncomplete::NextChunk));
        Some(Self {
            cell,
            next,
            wrap,
            segmenter,
            bytes: text.len(),
            context: String::new(),
            last: c,
            graphic: c,
            joined: false,
        })
    }

    fn continues(&mut self, c: char, cell: &Cell) -> bool {
        let mut buffer = [0; 8];
        let (text, chunk_start) = if cell.zerowidth().is_none_or(|tail| tail.is_empty()) {
            let first_len = cell.c.encode_utf8(&mut buffer).len();
            let next_len = c.encode_utf8(&mut buffer[first_len..]).len();
            (std::str::from_utf8(&buffer[..first_len + next_len]).unwrap(), 0)
        } else {
            (c.encode_utf8(&mut buffer) as &str, self.bytes)
        };
        loop {
            match self.segmenter.next_boundary(text, chunk_start) {
                Err(GraphemeIncomplete::NextChunk) => {
                    self.bytes += c.len_utf8();
                    if !self.context.is_empty() {
                        self.context.push(c);
                    }
                    return true;
                },
                Err(GraphemeIncomplete::PreContext(end)) => {
                    // Context is built only when GB11/Indic lookbehind asks for it;
                    // later extensions append, rather than rescan/rebuild a long cluster.
                    if self.context.is_empty() {
                        self.context.push(cell.c);
                        self.context.extend(cell.zerowidth().into_iter().flatten());
                    }
                    self.segmenter.provide_context(&self.context[..end], 0);
                },
                Ok(Some(_)) => return false,
                other => unreachable!("complete UTF-8 input chunk: {other:?}"),
            }
        }
    }
}

fn sequence_width(first: char, tail: &[char]) -> usize {
    let mut buffer = [0; 12];
    let mut len = first.encode_utf8(&mut buffer).len();
    for c in tail {
        len += c.encode_utf8(&mut buffer[len..]).len();
    }
    std::str::from_utf8(&buffer[..len]).unwrap().width()
}

impl<T: EventListener> Term<T> {
    pub(super) fn input_character(&mut self, c: char) {
        // Number of cells the char will occupy.
        let width = match c.width() {
            Some(width) => width,
            None => return,
        };

        if !c.is_ascii() && self.extend_emoji_input(c, width) {
            return;
        }
        self.input_cluster = None;

        // Preserve the existing placement of standalone combining characters.
        if width == 0 {
            // Get previous column.
            let mut column = self.grid.cursor.point.column;
            if !self.grid.cursor.input_needs_wrap {
                column.0 = column.saturating_sub(1);
            }

            // Put zerowidth characters over first fullwidth character cell.
            let line = self.grid.cursor.point.line;
            if self.grid[line][column].flags.contains(Flags::WIDE_CHAR_SPACER) {
                column.0 = column.saturating_sub(1);
            }

            self.grid[line][column].push_zerowidth(c);
            return;
        }

        // Move cursor to next line.
        if self.grid.cursor.input_needs_wrap {
            self.wrapline();
        }

        // If in insert mode, first shift cells to the right.
        let columns = self.columns();
        if self.mode.contains(TermMode::INSERT) && self.grid.cursor.point.column + width < columns {
            let line = self.grid.cursor.point.line;
            let col = self.grid.cursor.point.column;
            let row = &mut self.grid[line][..];

            for col in (col.0..(columns - width)).rev() {
                row.swap(col + width, col);
            }
        }

        let glyph_point;
        if width == 1 {
            glyph_point = self.grid.cursor.point;
            self.write_at_cursor(c);
        } else {
            if self.grid.cursor.point.column + 1 >= columns {
                if self.mode.contains(TermMode::LINE_WRAP) {
                    // Insert placeholder before wide char if glyph does not fit in this row.
                    self.grid.cursor.template.flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);
                    self.write_at_cursor(' ');
                    self.grid.cursor.template.flags.remove(Flags::LEADING_WIDE_CHAR_SPACER);
                    self.wrapline();
                    if self.mode.contains(TermMode::INSERT) {
                        self.insert_blank(2);
                    }
                } else {
                    // Prevent out of bounds crash when linewrapping is disabled.
                    self.grid.cursor.input_needs_wrap = true;
                    return;
                }
            }

            glyph_point = self.grid.cursor.point;
            // Write full width glyph to current cursor cell.
            self.grid.cursor.template.flags.insert(Flags::WIDE_CHAR);
            self.write_at_cursor(c);
            self.grid.cursor.template.flags.remove(Flags::WIDE_CHAR);

            // Write spacer to cell following the wide glyph.
            self.grid.cursor.point.column += 1;
            self.grid.cursor.template.flags.insert(Flags::WIDE_CHAR_SPACER);
            self.write_at_cursor(' ');
            self.grid.cursor.template.flags.remove(Flags::WIDE_CHAR_SPACER);
        }

        if self.grid.cursor.point.column + 1 < columns {
            self.grid.cursor.point.column += 1;
        } else {
            self.grid.cursor.input_needs_wrap = true;
        }
        self.input_cluster = EmojiInput::new(
            self.grid[glyph_point].c,
            glyph_point,
            self.grid.cursor.point,
            self.grid.cursor.input_needs_wrap,
        );
    }

    fn extend_emoji_input(&mut self, c: char, char_width: usize) -> bool {
        let Some(mut cluster) = self.input_cluster.take() else { return false };
        if cluster.next != self.grid.cursor.point || cluster.wrap != self.grid.cursor.input_needs_wrap {
            return false;
        }
        let cell = &self.grid[cluster.cell];
        let old_width = if cell.flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 };
        let width = if char_width > 0 {
            if !c.is_emoji_char() {
                return false;
            }
            if cluster.last == '\u{200d}' {
                2
            } else {
                let width = sequence_width(cluster.graphic, &[c]);
                if width > 2 {
                    return false;
                }
                old_width.max(width)
            }
        } else if !cluster.joined && cell.zerowidth().is_none_or(|tail| tail.is_empty()) {
            sequence_width(cell.c, &[c]).max(1)
        } else if c == '\u{20e3}' && cell.zerowidth() == Some(&['\u{fe0f}'][..]) {
            // Standard keycaps have one optional VS16 before the enclosing mark.
            sequence_width(cell.c, &['\u{fe0f}', c])
        } else {
            old_width
        };
        if !cluster.continues(c, cell) {
            return false;
        }
        self.grid[cluster.cell].push_zerowidth(c);
        if width != old_width {
            self.resize_emoji_cell(&mut cluster.cell, width);
        }
        self.damage.damage_point(Point::new(cluster.cell.line.0 as usize, cluster.cell.column));
        if char_width > 0 {
            cluster.graphic = c;
            cluster.joined = true;
        }
        cluster.last = c;
        cluster.next = self.grid.cursor.point;
        cluster.wrap = self.grid.cursor.input_needs_wrap;
        self.input_cluster = Some(cluster);
        true
    }

    fn resize_emoji_cell(&mut self, point: &mut Point, width: usize) {
        let mut cell = self.grid[*point].clone();
        if width == 1 {
            cell.flags.remove(Flags::WIDE_CHAR);
            self.grid[*point] = cell;
            self.grid[point.line][point.column + 1] = self.grid.cursor.template.bg.into();
            self.grid.cursor.point = Point::new(point.line, point.column + 1);
            if self.mode.contains(TermMode::INSERT) {
                self.delete_chars(1);
            }
            self.grid.cursor.input_needs_wrap = false;
            return;
        }
        if point.column == self.last_column() {
            if !self.mode.contains(TermMode::LINE_WRAP) {
                return;
            }
            let mut placeholder: Cell = cell.bg.into();
            placeholder.flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);
            self.grid[*point] = placeholder;
            self.grid.cursor.point = *point;
            self.wrapline();
            *point = self.grid.cursor.point;
            if self.mode.contains(TermMode::INSERT) {
                self.insert_blank(2);
            }
        } else if self.mode.contains(TermMode::INSERT) {
            self.grid.cursor.point = Point::new(point.line, point.column + 1);
            self.insert_blank(1);
        }
        // Clear any overwritten wide-cell partners before assigning the new head.
        self.grid.cursor.point = *point;
        self.write_at_cursor(' ');
        self.grid.cursor.point.column += 1;
        self.write_at_cursor(' ');
        let mut spacer = cell.clone();
        cell.flags.insert(Flags::WIDE_CHAR);
        spacer.clear_wide();
        spacer.flags.insert(Flags::WIDE_CHAR_SPACER);
        self.grid[*point] = cell;
        self.grid[point.line][point.column + 1] = spacer;
        self.grid.cursor.point.column = point.column + 1;
        if self.grid.cursor.point.column < self.last_column() {
            self.grid.cursor.point.column += 1;
            self.grid.cursor.input_needs_wrap = false;
        } else {
            self.grid.cursor.input_needs_wrap = true;
        }
        self.damage.damage_point(Point::new(point.line.0 as usize, point.column + 1));
    }
}

#[cfg(test)]
#[path = "emoji_input_tests.rs"]
mod tests;
