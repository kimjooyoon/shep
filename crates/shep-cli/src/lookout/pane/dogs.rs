//! The dogs tab of a sheep pane: every dog that can hold a table on this
//! sheep, each a row that opens, and an open dog's table drawn inline under
//! its own row.
//!
//! A dog is listed when it is a built-in that publishes a sheep schema, when
//! it is adopted (its binary is asked only when its row is opened), and when
//! the sheep carries a table for it that no schema describes. The last kind
//! is read-only: with no schema nothing says which of its values is a
//! secret, so it shows key names and never a value.

use std::path::PathBuf;

use serde_json::{Map, Value};
use shep_core::config::DogTable;
use shep_core::protocol::SheepConfigView;

use super::{ConfigPane, PaneRow, PaneTarget};

/// What one dog answered when it was probed.
///
/// `Debug` is manual (IR-41): a schema answer carries the dog's own
/// defaults, which the secret marker exists to keep off a screen, so it
/// prints only whether there is one.
#[derive(Clone, PartialEq, Eq)]
pub struct SheepDogEntry {
    /// The dog.
    pub name: String,
    /// The adopted binary, or [`None`] for a built-in.
    pub adopted_path: Option<PathBuf>,
    /// Its `x-shep-sheep` value with the root's `$defs` attached, or
    /// [`None`] for a dog that publishes no per-sheep schema.
    pub schema: Option<Value>,
}

impl core::fmt::Debug for SheepDogEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "SheepDogEntry {{ name: {:?}, adopted_path: {:?}, schema: {} }}",
            self.name,
            self.adopted_path,
            self.schema.as_ref().map_or("None", |_| "Some(..)")
        )
    }
}

/// One dog the tab can list, before it is asked anything.
///
/// `Debug` is derived (IR-41): [`SheepDogEntry`] redacts its own schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedDog {
    /// The dog. Its schema is an answer only when [`Self::asked`] says so.
    pub entry: SheepDogEntry,
    /// Whether `entry` already answers: a built-in is this binary, so its
    /// schema is read without spawning anything. An adopted dog is asked
    /// when its row is opened.
    pub asked: bool,
}

/// Whether the sheep carries a table for a listed dog, and whether the pane
/// can edit it.
///
/// `Debug` is derived (IR-41): a bare variant name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DogTableState {
    /// A table, and a schema to edit it through or none asked for yet.
    Set,
    /// No table yet; the schema is what lists the dog.
    Unset,
    /// A table and no schema: shown by key name, never edited.
    ReadOnly,
    /// The dog answered without a sheep schema, and the sheep carries no
    /// table for it.
    NoSchema,
}

/// How far a dog's schema is along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Asked {
    /// Nothing sent; opening the row sends the probe.
    No,
    /// The probe is out.
    Waiting,
    /// [`SheepDogEntry::schema`] is the answer.
    Yes,
}

/// One row of the tab.
///
/// `Debug` is manual (IR-41), for [`SheepDogEntry`]'s reason.
#[derive(Clone)]
pub struct DogRow {
    entry: SheepDogEntry,
    asked: Asked,
    /// The top-level key names of the sheep's table for this dog, or [`None`]
    /// for no table.
    keys: Option<Vec<String>>,
    /// Listed only because the sheep carries a table for it. Dropped again
    /// when the table goes.
    table_only: bool,
    /// Whether the row is open. A collapsed row keeps its table pane, so
    /// the edits filed in it survive the collapse.
    open: bool,
    /// The dog's table, built when the row is first opened with a schema.
    table: Option<Box<ConfigPane>>,
}

impl core::fmt::Debug for DogRow {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "DogRow {{ name: {:?}, state: {:?}, open: {}, keys: {:?} }}",
            self.entry.name,
            self.state(),
            self.open,
            self.keys()
        )
    }
}

impl DogRow {
    /// The dog.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.entry.name
    }

    /// Set, unset, read-only or without a schema.
    #[must_use]
    pub fn state(&self) -> DogTableState {
        match (self.asked, self.entry.schema.is_some(), self.keys.is_some()) {
            (Asked::Yes, false, true) => DogTableState::ReadOnly,
            (Asked::Yes, false, false) => DogTableState::NoSchema,
            (_, _, true) => DogTableState::Set,
            (_, _, false) => DogTableState::Unset,
        }
    }

    /// The table's top-level key names, in key order. Empty for no table.
    #[must_use]
    pub fn keys(&self) -> &[String] {
        self.keys.as_deref().unwrap_or_default()
    }

    /// Whether the row is open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether the sheep carries a table for this dog.
    #[must_use]
    pub fn has_table(&self) -> bool {
        self.keys.is_some()
    }

    /// Whether opening this row sends the dog's probe: it has not been asked.
    #[must_use]
    pub fn needs_probe(&self) -> bool {
        self.asked == Asked::No
    }

    /// Whether a probe for this dog is out.
    #[must_use]
    pub fn is_waiting(&self) -> bool {
        self.asked == Asked::Waiting
    }

    /// The dog's table pane, once its row has been opened with a schema.
    #[must_use]
    pub fn table(&self) -> Option<&ConfigPane> {
        self.table.as_deref()
    }
}

/// How far the dogs listing is along.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Listing {
    #[default]
    Unasked,
    Waiting,
    Listed,
}

/// The tab's state: its rows, and at most one armed removal.
///
/// `Debug` is manual (IR-41), for [`SheepDogEntry`]'s reason.
#[derive(Clone, Default)]
pub struct DogsTab {
    listing: Listing,
    rows: Vec<DogRow>,
    armed: Option<String>,
}

impl core::fmt::Debug for DogsTab {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "DogsTab {{ listing: {:?}, rows: {:?}, armed: {:?} }}",
            self.listing, self.rows, self.armed
        )
    }
}

impl DogsTab {
    /// Every row, in display order.
    #[must_use]
    pub fn rows(&self) -> &[DogRow] {
        &self.rows
    }

    /// Whether the listing has landed.
    #[must_use]
    pub fn is_listed(&self) -> bool {
        self.listing == Listing::Listed
    }

    /// The dog whose table an `Enter` would remove, or [`None`].
    #[must_use]
    pub fn armed(&self) -> Option<&str> {
        self.armed.as_deref()
    }

    /// The tab's rows as the cursor walks them: each dog, then the rows of
    /// its table while it is open.
    fn layout(&self) -> Vec<PaneRow> {
        let mut rows = Vec::new();
        for (dog, row) in self.rows.iter().enumerate() {
            rows.push(PaneRow::Dog(dog));
            if let (true, Some(table)) = (row.open, row.table.as_deref()) {
                rows.extend((0..table.rows().len()).map(|at| PaneRow::Under { dog, at }));
            }
        }
        rows
    }

    /// Whether the listing has not been asked yet.
    #[must_use]
    pub fn needs_listing(&self) -> bool {
        self.listing == Listing::Unasked
    }

    /// Marks the listing as asked.
    pub(in crate::lookout) fn start_listing(&mut self) {
        self.listing = Listing::Waiting;
    }

    pub(in crate::lookout) fn disarm(&mut self) -> bool {
        self.armed.take().is_some()
    }

    /// Arms the removal of `dog`'s table, and says whether there was one.
    pub(in crate::lookout) fn arm_removal(&mut self, dog: usize) -> bool {
        let Some(row) = self.rows.get(dog).filter(|row| row.has_table()) else {
            return false;
        };
        self.armed = Some(row.name().to_owned());
        true
    }

    /// The dogs whose table pane holds filed edits.
    pub(in crate::lookout) fn edited(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&dog| {
                self.rows[dog]
                    .table
                    .as_deref()
                    .is_some_and(|table| !table.edits().is_empty())
            })
            .collect()
    }

    /// Lends `dog`'s table pane out, so it can stand in for the sheep pane.
    pub(in crate::lookout) fn take_table(&mut self, dog: usize) -> Option<Box<ConfigPane>> {
        self.rows.get_mut(dog)?.table.take()
    }

    /// Gives `dog`'s table pane back.
    pub(in crate::lookout) fn seat_table(&mut self, dog: usize, table: Box<ConfigPane>) {
        if let Some(row) = self.rows.get_mut(dog) {
            row.table = Some(table);
        }
    }

    pub(super) fn tables(&self) -> impl Iterator<Item = &ConfigPane> {
        self.rows.iter().filter_map(|row| row.table.as_deref())
    }

    /// Re-reads each row's table off `tables`, the sheep's current ones: an
    /// open table pane takes its table in place, its edits and cursor kept.
    fn refresh(&mut self, tables: &Map<String, Value>) {
        self.armed = None;
        for row in &mut self.rows {
            let table = tables.get(&row.entry.name);
            row.keys = table.map(key_names);
            if let Some(pane) = row.table.as_deref_mut() {
                pane.adopt_table(
                    table
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default(),
                );
            }
        }
        self.rows
            .retain(|row| !(row.table_only && row.keys.is_none()));
        self.add_table_only(tables);
    }

    /// A read-only row for each table no listed dog carries, then the rows
    /// in name order.
    fn add_table_only(&mut self, tables: &Map<String, Value>) {
        for (name, table) in tables {
            if self.rows.iter().any(|row| &row.entry.name == name) {
                continue;
            }
            self.rows.push(DogRow {
                entry: SheepDogEntry {
                    name: name.clone(),
                    adopted_path: None,
                    schema: None,
                },
                asked: Asked::Yes,
                keys: Some(key_names(table)),
                table_only: true,
                open: false,
                table: None,
            });
        }
        self.rows.sort_by(|a, b| a.entry.name.cmp(&b.entry.name));
    }
}

/// A table's top-level key names.
fn key_names(table: &Value) -> Vec<String> {
    table
        .as_object()
        .map(|table| table.keys().cloned().collect())
        .unwrap_or_default()
}

/// What opening or closing a dog's row asks of the caller.
///
/// `Debug` is derived (IR-41): a dog's name and its binary's path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::lookout) enum DogToggle {
    /// The row was open and is closed.
    Closed,
    /// The row is open, its table drawn under it.
    Opened,
    /// The row is open as soon as this dog answers: the probe goes out.
    Ask {
        /// The dog to probe.
        dog: String,
        /// Its adopted binary, or [`None`] for a built-in.
        adopted_path: Option<PathBuf>,
    },
    /// A probe is already out for this dog.
    Waiting,
    /// The dog publishes no sheep schema: nothing opens.
    NoSchema(String),
}

impl ConfigPane {
    /// The dogs tab's state.
    #[must_use]
    pub fn dogs_tab(&self) -> &DogsTab {
        &self.dogs_tab
    }

    pub(in crate::lookout) fn dogs_tab_mut(&mut self) -> &mut DogsTab {
        &mut self.dogs_tab
    }

    /// Whether the tab the pane is on is a sheep's `dogs` tab.
    #[must_use]
    pub fn shows_dogs(&self) -> bool {
        matches!(self.target, PaneTarget::Sheep { .. }) && self.group() == "dogs"
    }

    /// The rows of the `dogs` tab: see [`DogsTab::layout`].
    pub(super) fn dog_rows(&self) -> Vec<PaneRow> {
        self.dogs_tab.layout()
    }

    /// Lists `listed` over this sheep's tables, every row closed.
    pub(in crate::lookout) fn list_dogs(&mut self, listed: Vec<ListedDog>) {
        let tables = self.sheep_tables();
        let mut rows: Vec<DogRow> = listed
            .into_iter()
            .map(|dog| DogRow {
                keys: tables.get(&dog.entry.name).map(key_names),
                asked: if dog.asked { Asked::Yes } else { Asked::No },
                entry: dog.entry,
                table_only: false,
                open: false,
                table: None,
            })
            .collect();
        // A built-in with no sheep schema has nothing to hold, unless the
        // sheep carries a table for it anyway.
        rows.retain(|row| row.asked != Asked::Yes || row.entry.schema.is_some() || row.has_table());
        self.dogs_tab.rows = rows;
        self.dogs_tab.listing = Listing::Listed;
        self.dogs_tab.add_table_only(&tables);
        self.clamp_view();
    }

    /// Opens the dog at `dog` if it is closed, else closes it.
    pub(in crate::lookout) fn toggle_dog(&mut self, dog: usize) -> DogToggle {
        let Some(row) = self.dogs_tab.rows.get_mut(dog) else {
            return DogToggle::Closed;
        };
        if row.open {
            row.open = false;
            return DogToggle::Closed;
        }
        match row.asked {
            Asked::No => {
                row.asked = Asked::Waiting;
                row.open = true;
                DogToggle::Ask {
                    dog: row.entry.name.clone(),
                    adopted_path: row.entry.adopted_path.clone(),
                }
            }
            Asked::Waiting => {
                row.open = true;
                DogToggle::Waiting
            }
            Asked::Yes => self.open_dog(dog),
        }
    }

    /// Opens an answered dog, building its table pane the first time.
    fn open_dog(&mut self, dog: usize) -> DogToggle {
        let sheep = self.target.name().to_owned();
        let Some(row) = self.dogs_tab.rows.get(dog) else {
            return DogToggle::Closed;
        };
        let name = row.entry.name.clone();
        let Some(schema) = row.entry.schema.clone() else {
            return DogToggle::NoSchema(name);
        };
        if row.table.is_none() {
            let pane = Self::sheep_dog(sheep, name, &schema, self.sheep_table(&row.entry.name));
            self.dogs_tab.rows[dog].table = Some(Box::new(pane));
        }
        self.dogs_tab.rows[dog].open = true;
        DogToggle::Opened
    }

    /// Takes one dog's probe answer. Opens its row when it was waiting for
    /// the answer, or says the dog has no schema to open. [`None`] for a
    /// dog the tab does not list.
    pub(in crate::lookout) fn answer_dog(&mut self, entry: SheepDogEntry) -> Option<DogToggle> {
        let dog = self
            .dogs_tab
            .rows
            .iter()
            .position(|row| row.entry.name == entry.name)?;
        let row = &mut self.dogs_tab.rows[dog];
        row.entry = entry;
        row.asked = Asked::Yes;
        if !row.open {
            return Some(DogToggle::Closed);
        }
        row.open = false;
        Some(self.open_dog(dog))
    }

    /// Carries a previous pane's tab onto this one's tables, for a re-read
    /// that rebuilt the pane under it.
    pub(in crate::lookout) fn adopt_dogs_tab(&mut self, previous: DogsTab) {
        *self.dogs_tab = previous;
        let tables = self.sheep_tables();
        self.dogs_tab.refresh(&tables);
        self.clamp_view();
    }

    fn clamp_view(&mut self) {
        let len = self.rows().len();
        self.view.clamp(len);
    }

    /// The dog the cursor is inside the table of, if any.
    #[must_use]
    pub(in crate::lookout) fn focused_dog(&self) -> Option<usize> {
        match self.cursor()? {
            PaneRow::Under { dog, .. } => Some(dog),
            _ => None,
        }
    }

    /// The pane a field key lands on: an open dog's table while the cursor
    /// is inside one, else this pane.
    #[must_use]
    pub fn focus(&self) -> &Self {
        self.focused_dog()
            .and_then(|dog| self.dogs_tab.rows.get(dog))
            .and_then(|row| row.table.as_deref())
            .unwrap_or(self)
    }

    /// Whether a text editor, the sheep's or an open table's, owns the
    /// keyboard.
    #[must_use]
    pub(in crate::lookout) fn owns_text(&self) -> bool {
        let pane = self.focus();
        pane.typing().is_some() || pane.env_typing().is_some()
    }

    /// The dogs whose open table no longer matches what `view` says the
    /// sheep carries for them, among those with edits filed.
    #[must_use]
    pub(in crate::lookout) fn moved_tables(&self, view: &SheepConfigView) -> Vec<String> {
        self.dogs_tab
            .edited()
            .into_iter()
            .filter_map(|dog| {
                let row = self.dogs_tab.rows.get(dog)?;
                let fresh = view.config.dogs.get(row.name()).map(DogTable::as_map);
                (!row.table.as_deref()?.holds_table(fresh)).then(|| row.name().to_owned())
            })
            .collect()
    }

    /// Moves the focused table's own cursor to the row this pane's cursor is
    /// on, after this pane's cursor moved.
    pub(in crate::lookout) fn push_focus(&mut self) {
        let Some(PaneRow::Under { dog, at }) = self.cursor() else {
            return;
        };
        if let Some(table) = self
            .dogs_tab
            .rows
            .get_mut(dog)
            .and_then(|row| row.table.as_deref_mut())
        {
            let len = table.rows().len();
            table.view.move_to(at, len);
        }
    }

    /// Moves this pane's cursor to the row `dog`'s table has its own on,
    /// after a key moved the table's.
    pub(in crate::lookout) fn pull_focus(&mut self, dog: usize) {
        let Some(row) = self.dogs_tab.rows.get(dog) else {
            return;
        };
        let Some(table) = row.table.as_deref() else {
            return;
        };
        let within = table.view.cursor();
        let Some(head) = self.rows().iter().position(|r| *r == PaneRow::Dog(dog)) else {
            return;
        };
        let len = self.rows().len();
        self.view.move_to(head + 1 + within, len);
    }

    /// The dog whose row the cursor is on.
    #[must_use]
    pub(in crate::lookout) fn cursor_dog_name(&self) -> Option<String> {
        let PaneRow::Dog(dog) = self.cursor()? else {
            return None;
        };
        Some(self.dogs_tab.rows.get(dog)?.name().to_owned())
    }

    /// Puts the cursor on `name`'s row, if it is listed. A re-read can add
    /// a table above the cursor, and a cursor carried by index would then
    /// land on a neighbour.
    pub(in crate::lookout) fn adopt_dog_cursor(&mut self, name: &str) {
        let Some(dog) = self.dogs_tab.rows.iter().position(|row| row.name() == name) else {
            return;
        };
        let rows = self.rows();
        if let Some(at) = rows.iter().position(|row| *row == PaneRow::Dog(dog)) {
            self.view.move_to(at, rows.len());
        }
    }

    /// `dog`'s table on this sheep, or an empty one when it carries none.
    #[must_use]
    pub(in crate::lookout) fn sheep_table(&self, dog: &str) -> Map<String, Value> {
        self.values
            .get("dogs")
            .and_then(|dogs| dogs.get(dog))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }

    fn sheep_tables(&self) -> Map<String, Value> {
        self.values
            .get("dogs")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    }
}

/// A sheep's `dogs` field as its row draws it: the dog names, comma
/// separated, or `none`. Never a value from a table: without a schema
/// nothing says which of them is a secret.
pub(crate) fn dog_names(dogs: Option<&Value>) -> String {
    let names: Vec<&str> = dogs
        .and_then(Value::as_object)
        .map(|tables| tables.keys().map(String::as_str).collect())
        .unwrap_or_default();
    if names.is_empty() {
        "none".to_owned()
    } else {
        names.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use shep_core::config::AppConfig;

    use super::*;

    /// `web`'s tables: `jobs` with a credential in it, and a `legacy` one
    /// for a dog that publishes no sheep schema.
    fn tables() -> Map<String, Value> {
        json!({
            "jobs": { "concurrency": 2, "token": "sk-live-51Hx9Qa" },
            "legacy": { "url": "https://ops:hunter2@example.test", "retries": 3 },
        })
        .as_object()
        .cloned()
        .expect("an object")
    }

    fn jobs_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "concurrency": { "type": "integer" },
                "token": { "type": "string", "x-shep-secret": true },
            },
        })
    }

    fn listed(name: &str, schema: Option<Value>, asked: bool) -> ListedDog {
        ListedDog {
            entry: SheepDogEntry {
                name: name.to_owned(),
                adopted_path: Some(PathBuf::from(format!("/opt/{name}"))),
                schema,
            },
            asked,
        }
    }

    fn plain_schema() -> Option<Value> {
        Some(json!({ "type": "object", "properties": {} }))
    }

    /// `web`, on its `dogs` tab, carrying [`tables`].
    fn web_on_dogs() -> ConfigPane {
        let mut config = AppConfig {
            name: "web".into(),
            ..AppConfig::default()
        };
        for (name, table) in tables() {
            let table = table.as_object().cloned().expect("a table");
            config.dogs.insert(name, table.into());
        }
        let mut pane = ConfigPane::sheep(SheepConfigView::new(config, Vec::new(), Vec::new()));
        pane.set_group(9);
        pane
    }

    /// `web_on_dogs`, listing `jobs` (answered, with a schema) and `legacy`.
    fn listed_web() -> ConfigPane {
        let mut pane = web_on_dogs();
        pane.list_dogs(vec![
            listed("jobs", Some(jobs_schema()), true),
            listed("legacy", None, true),
        ]);
        pane
    }

    fn states(pane: &ConfigPane) -> Vec<(&str, DogTableState)> {
        pane.dogs_tab()
            .rows()
            .iter()
            .map(|row| (row.name(), row.state()))
            .collect()
    }

    #[test]
    fn a_sheep_with_no_tables_shows_none() {
        let view = SheepConfigView::new(
            AppConfig {
                name: "web".into(),
                ..AppConfig::default()
            },
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(ConfigPane::sheep(view).value("dogs"), "none");
    }

    #[test]
    fn the_tab_marks_set_unset_and_read_only_and_sorts_by_name() {
        let mut pane = web_on_dogs();
        pane.list_dogs(vec![
            listed("jobs", Some(jobs_schema()), true),
            listed("deploy", plain_schema(), true),
            listed("legacy", None, true),
            listed("metrics", None, true),
        ]);
        assert_eq!(
            states(&pane),
            [
                ("deploy", DogTableState::Unset),
                ("jobs", DogTableState::Set),
                ("legacy", DogTableState::ReadOnly),
            ],
            "a dog with neither a schema nor a table is not listed"
        );
    }

    #[test]
    fn a_table_whose_dog_was_never_listed_is_a_read_only_row() {
        let mut pane = web_on_dogs();
        pane.list_dogs(vec![listed("jobs", Some(jobs_schema()), true)]);
        assert_eq!(
            states(&pane),
            [
                ("jobs", DogTableState::Set),
                ("legacy", DogTableState::ReadOnly)
            ]
        );
        assert_eq!(pane.dogs_tab().rows()[1].keys(), ["retries", "url"]);
    }

    #[test]
    fn an_adopted_dog_is_listed_closed_and_unasked() {
        let mut pane = web_on_dogs();
        pane.list_dogs(vec![listed("deploy", None, false)]);
        let deploy = &pane.dogs_tab().rows()[0];
        assert_eq!(deploy.name(), "deploy");
        assert!(deploy.needs_probe() && !deploy.is_open());
        assert_eq!(deploy.state(), DogTableState::Unset);
    }

    #[test]
    fn a_closed_row_carries_key_names_and_no_value() {
        let pane = listed_web();
        let debug = format!("{:?}", pane.dogs_tab());
        assert!(debug.contains("retries"), "{debug}");
        assert!(!debug.contains("hunter2"), "{debug}");
        assert!(!debug.contains("sk-live-51Hx9Qa"), "{debug}");
    }

    #[test]
    fn an_empty_tab_moves_nowhere_and_arms_nothing() {
        let mut pane = ConfigPane::sheep(SheepConfigView::new(
            AppConfig {
                name: "web".into(),
                ..AppConfig::default()
            },
            Vec::new(),
            Vec::new(),
        ));
        pane.set_group(9);
        pane.list_dogs(Vec::new());
        pane.dogs_tab_mut().disarm();
        pane.move_by(1);
        pane.move_to_last();
        pane.move_by(-1);
        assert_eq!(pane.view().cursor(), 0);
        assert_eq!(pane.cursor(), None);
        assert!(!pane.dogs_tab_mut().arm_removal(0));
    }

    #[test]
    fn arming_a_removal_on_an_unset_row_does_nothing() {
        let mut pane = web_on_dogs();
        pane.list_dogs(vec![
            listed("deploy", plain_schema(), true),
            listed("jobs", Some(jobs_schema()), true),
        ]);
        assert_eq!(pane.cursor_dog_name().as_deref(), Some("deploy"));
        assert!(!pane.dogs_tab_mut().arm_removal(0));
        assert_eq!(pane.dogs_tab().armed(), None);
        assert!(pane.dogs_tab_mut().arm_removal(2), "a read-only table too");
        assert_eq!(pane.dogs_tab().armed(), Some("legacy"));
        assert!(pane.dogs_tab_mut().disarm());
        assert_eq!(pane.dogs_tab().armed(), None);
    }

    #[test]
    fn every_dog_lists_closed() {
        let pane = listed_web();
        assert!(pane.dogs_tab().rows().iter().all(|row| !row.is_open()));
        assert_eq!(pane.rows(), [PaneRow::Dog(0), PaneRow::Dog(1)]);
    }

    #[test]
    fn opening_an_answered_dog_draws_its_table_under_its_row() {
        let mut pane = listed_web();
        assert_eq!(pane.toggle_dog(0), DogToggle::Opened);
        assert_eq!(
            pane.rows(),
            [
                PaneRow::Dog(0),
                PaneRow::Under { dog: 0, at: 0 },
                PaneRow::Under { dog: 0, at: 1 },
                PaneRow::Dog(1),
            ],
            "jobs's two fields sit between its row and the next dog's"
        );
        assert_eq!(pane.toggle_dog(0), DogToggle::Closed);
        assert_eq!(pane.rows(), [PaneRow::Dog(0), PaneRow::Dog(1)]);
    }

    #[test]
    fn a_closed_table_keeps_the_edit_filed_in_it() {
        let mut pane = listed_web();
        let _ = pane.toggle_dog(0);
        pane.dogs_tab_mut()
            .take_table(0)
            .map(|mut table| {
                table.begin_typing();
                table.type_char('9');
                let _ = table.apply_typing();
                pane.dogs_tab_mut().seat_table(0, table);
            })
            .expect("an open table");
        assert_eq!(pane.dogs_tab().edited(), [0]);
        let _ = pane.toggle_dog(0);
        let _ = pane.toggle_dog(0);
        assert_eq!(pane.dogs_tab().edited(), [0], "collapse is not undo");
    }

    #[test]
    fn an_unasked_dog_asks_once_and_opens_on_its_answer() {
        let mut pane = web_on_dogs();
        pane.list_dogs(vec![listed("deploy", None, false)]);
        let ask = DogToggle::Ask {
            dog: "deploy".into(),
            adopted_path: Some(PathBuf::from("/opt/deploy")),
        };
        assert_eq!(pane.toggle_dog(0), ask);
        assert!(pane.dogs_tab().rows()[0].is_waiting());
        assert_eq!(
            pane.toggle_dog(0),
            DogToggle::Closed,
            "a second Enter closes"
        );
        assert_eq!(pane.toggle_dog(0), DogToggle::Waiting, "and opens again");

        let answered = pane.answer_dog(SheepDogEntry {
            name: "deploy".into(),
            adopted_path: Some(PathBuf::from("/opt/deploy")),
            schema: plain_schema(),
        });
        assert_eq!(answered, Some(DogToggle::Opened));
        assert!(pane.dogs_tab().rows()[0].is_open());
        assert!(pane.dogs_tab().rows()[0].table().is_some());
    }

    #[test]
    fn a_dog_that_answers_with_no_schema_opens_nothing() {
        let mut pane = web_on_dogs();
        pane.list_dogs(vec![listed("deploy", None, false)]);
        let _ = pane.toggle_dog(0);
        let answered = pane.answer_dog(SheepDogEntry {
            name: "deploy".into(),
            adopted_path: Some(PathBuf::from("/opt/deploy")),
            schema: None,
        });
        assert_eq!(answered, Some(DogToggle::NoSchema("deploy".into())));
        let deploy = &pane.dogs_tab().rows()[0];
        assert!(!deploy.is_open());
        assert_eq!(deploy.state(), DogTableState::NoSchema);
    }

    #[test]
    fn an_answer_for_an_unlisted_dog_is_dropped() {
        let mut pane = listed_web();
        let answered = pane.answer_dog(SheepDogEntry {
            name: "ghost".into(),
            adopted_path: None,
            schema: plain_schema(),
        });
        assert_eq!(answered, None);
    }

    /// The tab's cursor is the one cursor: walking it into a table moves the
    /// table's own, and a key moving the table's moves the tab's.
    #[test]
    fn the_cursor_walks_into_a_table_and_the_table_follows() {
        let mut pane = listed_web();
        let _ = pane.toggle_dog(0);
        pane.move_by(2);
        assert_eq!(pane.cursor(), Some(PaneRow::Under { dog: 0, at: 1 }));
        assert_eq!(pane.focused_dog(), Some(0));
        assert_eq!(pane.focus().view().cursor(), 1);
        assert_eq!(pane.focus().target().name(), "web");

        pane.move_to_first();
        assert_eq!(pane.focused_dog(), None);
        pane.move_to_last();
        assert_eq!(pane.cursor(), Some(PaneRow::Dog(1)));
    }

    /// A table added above the cursor moves its dog down a row, so a cursor
    /// carried by index would land on a neighbour.
    #[test]
    fn a_refresh_keeps_the_cursor_on_its_dog() {
        let mut pane = listed_web();
        pane.move_by(1);
        assert_eq!(pane.cursor_dog_name().as_deref(), Some("legacy"));
        let mut more = tables();
        more.insert("alpha".into(), json!({ "on": true }));
        let mut fresh = web_on_dogs();
        fresh.values = [("dogs".to_owned(), Value::Object(more))]
            .into_iter()
            .collect();
        fresh.adopt_dogs_tab(pane.dogs_tab().clone());
        fresh.adopt_dog_cursor("legacy");
        assert_eq!(fresh.cursor_dog_name().as_deref(), Some("legacy"));
    }

    /// A schema answer carries a dog's own defaults (IR-41).
    #[test]
    fn the_debug_of_an_entry_names_no_schema() {
        let entry = SheepDogEntry {
            name: "jobs".into(),
            adopted_path: None,
            schema: Some(json!({ "default": "sk-live-51Hx9Qa" })),
        };
        assert_eq!(
            format!("{entry:?}"),
            r#"SheepDogEntry { name: "jobs", adopted_path: None, schema: Some(..) }"#
        );
    }
}
