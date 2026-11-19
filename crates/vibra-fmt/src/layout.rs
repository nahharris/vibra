//! Line breaking, indentation, and comments.
//!
//! A grammar's formatter lowers a tree to a flat list of [`Item`]s and this
//! module lays it out by the rules of the source chapter's "Canonical
//! format": two-space indentation, a limit of 88 columns, and a form on one
//! line when it holds no comment and fits, otherwise its opener, its entries
//! one per line one step in, and its closer on its own line.
//!
//! A group, `Begin` to `End`, is laid out as a whole: flat, when it is inside
//! a flat group, or when it holds no comment and no line break and its flat
//! width, with the text that must follow it on the same line, fits in the
//! columns left; broken otherwise. Both passes are iterative.
//!
//! A line never starts or ends with a space, and a break at the start of a
//! line writes nothing, so a comment's line break and the break of the group
//! around it make one line break, never a blank line; only [`Item::Hard`]
//! writes a line break wherever it is.

/// The column limit.
pub(crate) const WIDTH: usize = 88;

/// One indentation step.
const INDENT: &str = "  ";

/// One element of a layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Item {
    /// Text written as is.
    Text(String),
    /// A break between two entries: one space in a flat group, a new line in
    /// a broken one.
    Space,
    /// A break after an opener or before a closer: nothing in a flat group, a
    /// new line in a broken one.
    Soft,
    /// A new line, in any group, even at the start of a line.
    Hard,
    /// A new line, in any group, unless the line is empty.
    Line,
    /// One more indentation step for the lines that follow.
    Indent,
    /// One indentation step less.
    Dedent,
    /// The start of a group laid out as a whole.
    Begin,
    /// The end of the innermost group.
    End,
    /// A comment at the end of the line before it, with the spaces written
    /// before it ([`trailing_comment`]).
    TrailingComment(String),
    /// A comment on a line of its own.
    Comment(String),
    /// Nothing written; the group it is in breaks, and so does every group
    /// around it.
    Force,
}

/// What the layout needs to know about one group.
#[derive(Clone, Copy, Debug, Default)]
struct Group {
    /// The index of its `End`.
    end: usize,
    /// Its width on one line.
    width: usize,
    /// Whether it must break: it holds a comment or a line break.
    forced: bool,
}

fn width(text: &str) -> usize {
    text.chars().count()
}

/// A comment at the end of a line: the spaces and tabs written between the
/// token before it and the comment, or one space when none was written, then
/// the comment without trailing whitespace. A comment starts no token, so
/// the one space between tokens does not apply to it.
pub(crate) fn trailing_comment(text: &str, start: usize) -> Item {
    let before = text.get(..start).unwrap_or_default();
    let gap_start = before.trim_end_matches([' ', '\t']).len();
    let gap = before.get(gap_start..).unwrap_or_default();
    let comment = text
        .get(start..)
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default()
        .trim_end();
    let gap = if gap.is_empty() { " " } else { gap };
    Item::TrailingComment(format!("{gap}{comment}"))
}

/// Measures every group.
fn measure(items: &[Item]) -> Vec<Option<Group>> {
    let mut groups = vec![None; items.len()];
    let mut open: Vec<(usize, Group)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match item {
            Item::Begin => open.push((index, Group::default())),
            Item::End => {
                if let Some((begin, mut group)) = open.pop() {
                    group.end = index;
                    if let Some(slot) = groups.get_mut(begin) {
                        *slot = Some(group);
                    }
                    if let Some((_, parent)) = open.last_mut() {
                        parent.width += group.width;
                        parent.forced |= group.forced;
                    }
                }
            }
            _ => {
                let Some((_, group)) = open.last_mut() else {
                    continue;
                };
                match item {
                    Item::Text(text) => {
                        group.width += width(text);
                        group.forced |= text.contains(['\n', '\r']);
                    }
                    Item::Space => group.width += 1,
                    Item::Hard
                    | Item::Line
                    | Item::TrailingComment(_)
                    | Item::Comment(_)
                    | Item::Force => {
                        group.forced = true;
                    }
                    Item::Soft
                    | Item::Indent
                    | Item::Dedent
                    | Item::Begin
                    | Item::End => {}
                }
            }
        }
    }
    groups
}

/// The width of what must follow position `from` on the same line: text up
/// to the next break.
fn trailing(items: &[Item], from: usize) -> usize {
    let mut total = 0;
    for item in items.iter().skip(from) {
        match item {
            Item::Text(text) => total += width(text),
            Item::TrailingComment(comment) => total += width(comment),
            Item::End | Item::Indent | Item::Dedent | Item::Force => {}
            Item::Space
            | Item::Soft
            | Item::Hard
            | Item::Line
            | Item::Begin
            | Item::Comment(_) => {
                break;
            }
        }
    }
    total
}

/// The output being written.
struct Writer {
    out: String,
    column: usize,
    indent: usize,
    /// A line was started and its indentation is not written yet, so a line
    /// never ends in indentation.
    at_line_start: bool,
    /// Spaces written at the end of the text so far, held back until more
    /// text follows on the same line, so a line never ends in a space.
    pending: usize,
}

impl Writer {
    fn column(&self) -> usize {
        if self.at_line_start {
            self.indent * INDENT.len()
        } else {
            self.column + self.pending
        }
    }

    fn write(&mut self, text: &str) {
        let text = if self.at_line_start {
            text.trim_start_matches(' ')
        } else {
            text
        };
        let body = text.trim_end_matches(' ');
        let spaces = text.len() - body.len();
        if body.is_empty() {
            if !self.at_line_start {
                self.pending += spaces;
            }
            return;
        }
        if self.at_line_start {
            for _ in 0..self.indent {
                self.out.push_str(INDENT);
            }
            self.column = self.indent * INDENT.len();
            self.at_line_start = false;
        }
        for _ in 0..self.pending {
            self.out.push(' ');
        }
        self.column += self.pending;
        self.out.push_str(body);
        match body.rfind('\n') {
            Some(position) => {
                self.column = width(body.get(position + 1..).unwrap_or_default())
            }
            None => self.column += width(body),
        }
        self.pending = spaces;
    }

    /// `text`, its leading spaces included, at the end of the current line.
    fn write_trailing(&mut self, text: &str) {
        if self.at_line_start {
            self.write(text);
            return;
        }
        self.pending = 0;
        self.out.push_str(text);
        self.column += width(text);
    }

    fn newline(&mut self) {
        self.out.push('\n');
        self.column = 0;
        self.pending = 0;
        self.at_line_start = true;
    }

    /// A new line, unless the current line is empty.
    fn line(&mut self) {
        if !self.at_line_start {
            self.newline();
        }
    }
}

/// Lays out `items`.
pub(crate) fn render(items: &[Item]) -> String {
    let groups = measure(items);
    let mut writer = Writer {
        out: String::new(),
        column: 0,
        indent: 0,
        at_line_start: true,
        pending: 0,
    };
    // Whether each open group is broken; outside every group, lines break.
    let mut broken: Vec<bool> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let in_broken = broken.last().copied().unwrap_or(true);
        match item {
            Item::Begin => {
                let breaks = in_broken
                    && groups.get(index).copied().flatten().is_some_and(|group| {
                        group.forced
                            || writer.column()
                                + group.width
                                + trailing(items, group.end + 1)
                                > WIDTH
                    });
                broken.push(breaks);
            }
            Item::End => {
                broken.pop();
            }
            Item::Text(text) => writer.write(text),
            Item::Space if in_broken => writer.line(),
            Item::Space => writer.write(" "),
            Item::Soft if in_broken => writer.line(),
            Item::Soft => {}
            Item::Hard => writer.newline(),
            Item::Line => writer.line(),
            Item::Indent => writer.indent += 1,
            Item::Dedent => writer.indent = writer.indent.saturating_sub(1),
            Item::Force => {}
            Item::TrailingComment(comment) => writer.write_trailing(comment),
            Item::Comment(comment) => writer.write(comment),
        }
    }
    writer.out
}

#[cfg(test)]
mod tests {
    use super::{Item, WIDTH, render};

    fn group(entries: &[&str]) -> Vec<Item> {
        let mut items = vec![Item::Begin, Item::Text("[".to_owned()), Item::Indent];
        for (index, entry) in entries.iter().enumerate() {
            items.push(if index == 0 { Item::Soft } else { Item::Space });
            items.push(Item::Text((*entry).to_owned()));
        }
        items.extend([
            Item::Dedent,
            Item::Soft,
            Item::Text("]".to_owned()),
            Item::End,
        ]);
        items
    }

    #[test]
    fn a_group_that_fits_is_one_line() {
        assert_eq!(render(&group(&["a", "b"])), "[a b]");
        assert_eq!(render(&group(&[])), "[]");
    }

    #[test]
    fn a_group_that_does_not_fit_is_one_entry_per_line() {
        let long = "x".repeat(WIDTH);
        assert_eq!(
            render(&group(&[&long, "b"])),
            format!("[\n  {long}\n  b\n]")
        );
    }

    #[test]
    fn exactly_the_limit_fits() {
        let entry = "x".repeat(WIDTH - 2);
        assert_eq!(render(&group(&[&entry])), format!("[{entry}]"));
        let entry = "x".repeat(WIDTH - 1);
        assert_eq!(render(&group(&[&entry])), format!("[\n  {entry}\n]"));
    }

    #[test]
    fn a_comment_breaks_its_group_and_every_group_around_it() {
        let mut items = vec![
            Item::Begin,
            Item::Text("{".to_owned()),
            Item::Indent,
            Item::Soft,
        ];
        items.extend(group(&["a"]));
        items.push(Item::TrailingComment(" // c".to_owned()));
        items.extend([
            Item::Dedent,
            Item::Soft,
            Item::Text("}".to_owned()),
            Item::End,
        ]);
        assert_eq!(render(&items), "{\n  [a] // c\n}");
    }

    #[test]
    fn a_force_breaks_every_group_around_it_and_writes_nothing() {
        let mut items = vec![
            Item::Begin,
            Item::Text("{".to_owned()),
            Item::Indent,
            Item::Soft,
            Item::Force,
        ];
        items.extend(group(&["a"]));
        items.extend([
            Item::Dedent,
            Item::Soft,
            Item::Text("}".to_owned()),
            Item::End,
        ]);
        assert_eq!(render(&items), "{\n  [a]\n}");
    }
}
