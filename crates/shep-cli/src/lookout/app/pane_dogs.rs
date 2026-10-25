//! The dogs tab under a sheep pane: the listing that fills it, a dog's row
//! opening to its table, the keys on both, and the table write the tab's
//! removal sends.
//!
//! An open dog's table is a pane of its own, held by the tab. A key that
//! lands inside one runs against that pane through [`App::in_table`], which
//! seats it as the body for the length of the handler, so every handler
//! written for a pane edits a table inline as it did on a pane of its own.

use super::*;

impl App {
    /// The ask for the dogs listing, the first time the `dogs` tab is up.
    /// [`Effect::None`] on any other tab, and once it has been asked.
    pub(super) fn dogs_tab_effect(&mut self) -> Effect {
        let Some(pane) = self.config_pane() else {
            return Effect::None;
        };
        if !pane.shows_dogs() || !pane.dogs_tab().needs_listing() {
            return Effect::None;
        }
        let sheep = pane.target().name().to_owned();
        if self.refuse_over_host() {
            return Effect::None;
        }
        if let Some(pane) = self.config_pane_mut() {
            pane.dogs_tab_mut().start_listing();
        }
        self.sheep_dogs_ask += 1;
        Effect::ListSheepDogs {
            sheep,
            ask: self.sheep_dogs_ask,
        }
    }

    /// One [`Msg::SheepDogNames`]: lists the dogs on the tab that asked, every
    /// row closed.
    ///
    /// Guarded on the pane's own sheep the way [`Self::on_dog_section`] is
    /// guarded on its dog, and on the ask: closing the pane moves it on, so
    /// an answer outlives neither the pane nor a newer ask.
    pub(super) fn on_sheep_dog_names(
        &mut self,
        sheep: &str,
        ask: u64,
        dogs: Vec<ListedDog>,
    ) -> Effect {
        let latest = ask == self.sheep_dogs_ask;
        if let Some(pane) = self.config_pane_mut()
            && latest
            && matches!(pane.target(), PaneTarget::Sheep { name } if name == sheep)
        {
            pane.list_dogs(dogs);
        }
        Effect::None
    }

    /// One [`Msg::SheepDog`]: the dog's schema has come, so its row, if it
    /// was waiting, opens. A dog that publishes no sheep schema opens
    /// nothing and says so.
    pub(super) fn on_sheep_dog(&mut self, sheep: &str, entry: SheepDogEntry) -> Effect {
        let Some(pane) = self.config_pane_mut() else {
            return Effect::None;
        };
        if !matches!(pane.target(), PaneTarget::Sheep { name } if name == sheep) {
            return Effect::None;
        }
        if let Some(DogToggle::NoSchema(dog)) = pane.answer_dog(entry) {
            self.notice = Some(no_schema_notice(&dog));
        }
        Effect::None
    }

    /// The `dogs` tab's own keys, ahead of the pane's: an armed removal, a
    /// dog's row, and a key landing inside an open table. [`None`] for every
    /// key the pane's keymap answers the same way here as on any tab:
    /// movement, `Escape`, `r`, the tab keys.
    ///
    /// `d` arms a removal, which `Enter` confirms and every other key but a
    /// quit cancels, the secrets pane's rule. An armed removal eats the first
    /// `Escape`, as it does there.
    pub(super) fn on_dogs_tab_key(&mut self, key: KeyPress) -> Option<Effect> {
        let pane = self.config_pane()?;
        if !pane.shows_dogs() {
            return None;
        }
        let armed = pane.dogs_tab().armed().is_some();
        if armed && !matches!(key, KeyPress::Confirm | KeyPress::Quit) {
            self.config_pane_mut()?.dogs_tab_mut().disarm();
            if key == KeyPress::Escape {
                return Some(Effect::None);
            }
        }
        match self.config_pane()?.cursor()? {
            PaneRow::Dog(dog) => self.on_dog_row_key(key, dog),
            PaneRow::Under { .. } if lands_in_table(key) => Some(
                self.in_table(|app| app.on_pane_key(key))
                    .unwrap_or(Effect::None),
            ),
            _ => None,
        }
    }

    /// A key with the cursor on `dog`'s row.
    fn on_dog_row_key(&mut self, key: KeyPress, dog: usize) -> Option<Effect> {
        let armed = self.config_pane()?.dogs_tab().armed().is_some();
        let effect = match key {
            KeyPress::Confirm if armed => self.confirm_table_removal(),
            KeyPress::Confirm | KeyPress::Edit => self.toggle_dog_row(dog, None),
            KeyPress::TabNext => self.toggle_dog_row(dog, Some(true)),
            KeyPress::TabPrev => self.toggle_dog_row(dog, Some(false)),
            KeyPress::Remove => self.arm_table_removal(dog),
            // `space` files nothing on a row that holds no value.
            KeyPress::Cycle => Effect::None,
            _ => return None,
        };
        Some(effect)
    }

    /// Opens the row at `dog` if it is closed, else closes it; with `want`
    /// set, only toward that state. An adopted dog's first opening sends its
    /// probe.
    fn toggle_dog_row(&mut self, dog: usize, want: Option<bool>) -> Effect {
        let Some(pane) = self.config_pane() else {
            return Effect::None;
        };
        let Some(row) = pane.dogs_tab().rows().get(dog) else {
            return Effect::None;
        };
        if want.is_some_and(|open| open == row.is_open()) {
            return Effect::None;
        }
        let probes = !row.is_open() && row.needs_probe();
        let sheep = pane.target().name().to_owned();
        // An adopted dog's binary is on the shepherd's machine.
        if probes && self.refuse_over_host() {
            return Effect::None;
        }
        let Some(pane) = self.config_pane_mut() else {
            return Effect::None;
        };
        match pane.toggle_dog(dog) {
            DogToggle::Ask { dog, adopted_path } => Effect::LoadSheepDog {
                sheep,
                dog,
                adopted_path,
            },
            DogToggle::NoSchema(dog) => {
                self.notice = Some(no_schema_notice(&dog));
                Effect::None
            }
            DogToggle::Closed | DogToggle::Opened | DogToggle::Waiting => Effect::None,
        }
    }

    /// `d` on `dog`'s row: arms the removal of its table. A row with no
    /// table arms nothing and says nothing; a closed gate refuses in its own
    /// words.
    fn arm_table_removal(&mut self, dog: usize) -> Effect {
        let has_table = self
            .config_pane()
            .and_then(|pane| pane.dogs_tab().rows().get(dog))
            .is_some_and(DogRow::has_table);
        if !has_table || self.authorize_write().is_none() {
            return Effect::None;
        }
        if let Some(pane) = self.config_pane_mut() {
            pane.dogs_tab_mut().arm_removal(dog);
        }
        Effect::None
    }

    /// `Enter` with a removal armed: sends it and disarms.
    fn confirm_table_removal(&mut self) -> Effect {
        let Some(pane) = self.config_pane_mut() else {
            return Effect::None;
        };
        let PaneTarget::Sheep { name } = pane.target().clone() else {
            return Effect::None;
        };
        let tab = pane.dogs_tab_mut();
        let Some(dog) = tab.armed().map(str::to_owned) else {
            return Effect::None;
        };
        tab.disarm();
        let Some(authority) = self.authorize_write() else {
            return Effect::None;
        };
        let ticket = self.take_write_ticket();
        Effect::Send(Sent::SetSheepDogTable {
            name,
            dog,
            ticket,
            table: None,
            authority,
        })
    }

    /// Runs `run` with the open table the cursor is inside seated as the
    /// body, then puts it back and moves the sheep pane's cursor to the row
    /// the table's own is on. [`None`] when the cursor is not in a table.
    pub(super) fn in_table<R>(&mut self, run: impl FnOnce(&mut Self) -> R) -> Option<R> {
        let dog = self.config_pane()?.focused_dog()?;
        self.in_dog_table(dog, run)
    }

    /// [`Self::in_table`] for the table of the dog at `dog`.
    pub(super) fn in_dog_table<R>(
        &mut self,
        dog: usize,
        run: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        let table = self.config_pane_mut()?.dogs_tab_mut().take_table(dog)?;
        let sheep = core::mem::replace(&mut self.body, Body::ConfigPane(*table));
        let out = run(self);
        if let Body::ConfigPane(table) = core::mem::replace(&mut self.body, sheep)
            && let Some(pane) = self.config_pane_mut()
        {
            pane.dogs_tab_mut().seat_table(dog, Box::new(table));
            pane.pull_focus(dog);
        }
        Some(out)
    }

    /// One `Request::SetSheepDogSettings` reply, for a removal or for a
    /// table's write on close.
    ///
    /// A success re-reads the sheep, so an open tab shows the table
    /// gone; [`Self::on_sheep_config`] drops the answer when no pane is
    /// waiting for it. Neither arm can print a value: the reply carries
    /// none, and `removed` comes off the request.
    pub(super) fn on_table_set(
        &mut self,
        name: &str,
        dog: &str,
        removed: bool,
        result: Result<Response, RequestError>,
    ) -> Effect {
        let why = match result {
            Ok(Response::SheepDogSettingsSet { .. }) => {
                let verb = if removed { "removed" } else { "written" };
                self.notice = Some(Notice::info(format!(
                    "{name}: its {dog} table is {verb}, and {dog} is told"
                )));
                return Effect::Send(Sent::SheepConfig {
                    name: name.to_owned(),
                });
            }
            other => failed_reply(other),
        };
        self.notice = Some(Notice::grave(format!("{name}: {dog}: {why}")));
        Effect::None
    }

    /// A table write the link task never took.
    pub(super) fn on_table_unsent(&mut self, name: &str, dog: &str) -> Effect {
        self.notice = Some(Notice::grave(format!(
            "{name}: its {dog} table was not sent"
        )));
        Effect::None
    }
}

/// Whether a key with the cursor inside an open table is the table's to
/// answer. Movement, `Escape`, `r`, the help overlay and the tab keys are the
/// sheep pane's, because the table is only part of what they walk.
fn lands_in_table(key: KeyPress) -> bool {
    !matches!(
        key,
        KeyPress::Quit
            | KeyPress::Escape
            | KeyPress::Refresh
            | KeyPress::Help
            | KeyPress::SelectUp
            | KeyPress::SelectDown
            | KeyPress::SelectFirst
            | KeyPress::SelectLast
            | KeyPress::NextGroup
            | KeyPress::PrevGroup
            | KeyPress::Group(_)
    )
}

fn no_schema_notice(dog: &str) -> Notice {
    Notice::grave(format!(
        "{dog} publishes no sheep schema, so its table is not edited here"
    ))
}
