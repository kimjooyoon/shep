//! A sheep's `dogs` tab: one row per dog that can hold a table on this
//! sheep, what state its table is in, and under an open dog its table.
//!
//! A closed row names its table's keys and never a value. A set table's
//! secrets are marked by a schema the row does not read, and a read-only
//! table has no schema at all, so a row stops at key names; the open table
//! below it is where values are drawn, masked.

use ratatui::text::{Line, Span};

use super::super::super::pane::{ConfigPane, DogTableState, PaneRow};
use super::super::super::theme::Palette;
use super::super::flock::{fit, mark};
use super::super::scroll::{self, Attempt};
use super::body::row_line;
use super::field_row::open_glyph;
use super::layout::body_width;

/// The widest a dog's name column grows before it is cut.
const NAME_W: u16 = 16;
/// `read-only`, the widest state word.
const STATE_W: u16 = 9;
/// How far an open table's rows sit in from the dog's own.
const INDENT: u16 = 2;

/// The tab's body: the armed removal's question when there is one, then the
/// rows, laid out through [`scroll::to_cursor`] so the cursor is drawn at
/// every height.
pub(super) fn dogs_body_lines(
    pane: &ConfigPane,
    palette: Palette,
    width: u16,
    budget: usize,
    show_lands: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(dog) = pane.dogs_tab().armed() {
        let sheep = pane.target().name();
        lines.push(Line::from(Span::styled(
            format!(
                "  {}",
                fit(
                    &format!("remove {sheep}'s {dog} table? enter confirms, any other key cancels"),
                    body_width(width)
                )
            ),
            palette.attention(),
        )));
    }
    let body_budget = budget.saturating_sub(lines.len());
    if body_budget == 0 {
        return lines;
    }
    let rows = pane.rows();
    if rows.is_empty() {
        let text = if pane.dogs_tab().is_listed() {
            "no dog publishes a sheep schema, and this sheep carries no table"
        } else {
            "listing dogs..."
        };
        lines.push(Line::from(Span::styled(
            format!("  {}", fit(text, body_width(width))),
            palette.muted(),
        )));
        return lines;
    }
    let cursor_row = pane.view().cursor().min(rows.len() - 1);
    lines.extend(scroll::to_cursor(
        cursor_row,
        pane.view().offset(),
        |offset| dogs_body_from(pane, &rows, palette, width, body_budget, offset, show_lands),
        || {
            vec![tab_row_line(
                pane,
                rows[cursor_row],
                true,
                width,
                palette,
                show_lands,
            )]
        },
    ));
    lines
}

/// One row of the tab: a dog's own, or one of its open table's.
fn tab_row_line(
    pane: &ConfigPane,
    row: PaneRow,
    selected: bool,
    width: u16,
    palette: Palette,
    show_lands: bool,
) -> Line<'static> {
    match row {
        PaneRow::Dog(index) => dog_line(pane, index, selected, width, palette),
        PaneRow::Under { dog, at } => {
            let Some(table) = pane.dogs_tab().rows().get(dog).and_then(|row| row.table()) else {
                return Line::default();
            };
            let Some(inner) = table.rows().get(at).copied() else {
                return Line::default();
            };
            let mut line = row_line(
                table,
                inner,
                selected,
                width.saturating_sub(INDENT),
                palette,
                show_lands,
            );
            // After the selection mark, so the mark stays in its column.
            line.spans.insert(
                1.min(line.spans.len()),
                Span::raw(" ".repeat(usize::from(INDENT))),
            );
            line
        }
        PaneRow::Field(_)
        | PaneRow::Table(_)
        | PaneRow::Entry(_)
        | PaneRow::Choice { .. }
        | PaneRow::Arm { .. }
        | PaneRow::Env(_)
        | PaneRow::AddEnv => Line::default(),
    }
}

/// One dog's row: the selection mark, whether it is open, its name, its
/// table's state, and what the table holds, by key name only.
fn dog_line(
    pane: &ConfigPane,
    index: usize,
    selected: bool,
    width: u16,
    palette: Palette,
) -> Line<'static> {
    let Some(row) = pane.dogs_tab().rows().get(index) else {
        return Line::default();
    };
    let body = body_width(width);
    let name_w = NAME_W.min(body.saturating_sub(2));
    let state_w = STATE_W.min(body.saturating_sub(name_w + 4));
    let rest_w = body.saturating_sub(name_w + state_w + 6);
    let keys = row.keys().join(", ");
    let (state, rest, muted) = if row.is_waiting() {
        ("asking", String::new(), true)
    } else {
        match row.state() {
            DogTableState::Set => ("set", keys, false),
            DogTableState::Unset => ("-", "(no table)".to_owned(), true),
            DogTableState::ReadOnly => (
                "read-only",
                format!("{keys}  (publishes no sheep schema)"),
                true,
            ),
            DogTableState::NoSchema => ("-", "(publishes no sheep schema)".to_owned(), true),
        }
    };
    let mut text = format!("{} ", open_glyph(row.is_open()));
    text.push_str(&fit(row.name(), name_w));
    if state_w > 0 {
        text.push_str("  ");
        text.push_str(&fit(state, state_w));
    }
    if rest_w > 0 {
        text.push_str("  ");
        text.push_str(&fit(&rest, rest_w));
    }
    let text = format!("{}{text}", mark(selected));
    if muted {
        Line::from(Span::styled(text, palette.muted()))
    } else {
        Line::from(Span::raw(text))
    }
}

/// Lays the rows out from `offset`, spending at most `budget` lines, both
/// markers reserved before a row is admitted, as the list sub-screen does.
fn dogs_body_from(
    pane: &ConfigPane,
    rows: &[PaneRow],
    palette: Palette,
    width: u16,
    budget: usize,
    offset: usize,
    show_lands: bool,
) -> Attempt {
    let total = rows.len();
    let cursor_row = pane.view().cursor().min(total.saturating_sub(1));
    let above = usize::from(offset > 0);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut drawn = 0usize;
    for (index, row) in rows.iter().enumerate().skip(offset) {
        if lines.len() + 1 + above + usize::from(index + 1 < total) > budget {
            break;
        }
        lines.push(tab_row_line(
            pane,
            *row,
            index == cursor_row,
            width,
            palette,
            show_lands,
        ));
        drawn += 1;
    }
    let hidden_below = total.saturating_sub(offset + drawn);
    if hidden_below > 0 {
        lines.push(Line::from(Span::styled(
            format!("  ... {hidden_below} below"),
            palette.muted(),
        )));
    }
    if offset > 0 {
        lines.insert(
            0,
            Line::from(Span::styled(
                format!("  ... {offset} above"),
                palette.muted(),
            )),
        );
    }
    Attempt {
        cursor_drawn: drawn > 0 && (offset..offset + drawn).contains(&cursor_row),
        lines,
    }
}
