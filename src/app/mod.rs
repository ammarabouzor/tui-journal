use self::{
    filter::{Filter, FilterCriterion},
    sorter::{SortCriteria, SortOrder, Sorter},
    state::AppState,
};
use crate::settings::Settings;
use anyhow::{anyhow, bail, Context};
use backend::{DataProvider, EntriesDTO, Entry, EntryDraft};
use chrono::{DateTime, Utc};
use colored_tags::ColoredTagsManager;
use filter::criterion::TagFilterOption;
use history::{Change, HistoryManager, HistoryStack};
use rayon::prelude::*;
use std::{
    collections::{BTreeSet, HashSet},
    fs::File,
    path::PathBuf,
};

mod colored_tags;
mod external_editor;
mod filter;
mod history;
mod keymap;
mod runner;
mod sorter;
pub mod state;
#[cfg(test)]
mod test;
pub mod ui;

pub use runner::run;
pub use runner::HandleInputReturnType;
pub use ui::UIComponents;

pub use colored_tags::TagColors;

pub struct App<D>
where
    D: DataProvider,
{
    pub data_provide: D,
    pub entries: Vec<Entry>,
    pub current_entry_id: Option<u32>,
    /// Selected entries' IDs in multi-select mode
    pub selected_entries: HashSet<u32>,
    /// Inactive entries' IDs due to not meeting the filter criteria
    pub filtered_out_entries: HashSet<u32>,
    pub settings: Settings,
    pub redraw_after_restore: bool,
    pub filter: Option<Filter>,
    state: AppState,
    /// Keeps history of the changes on entries, enabling undo & redo operations
    history: HistoryManager,
    colored_tags: Option<ColoredTagsManager>,
}

impl<D> App<D>
where
    D: DataProvider,
{
    pub fn new(data_provide: D, settings: Settings) -> Self {
        let entries = Vec::new();
        let selected_entries = HashSet::new();
        let filtered_out_entries = HashSet::new();
        let history = HistoryManager::new(settings.history_limit);
        let colored_tags = settings.colored_tags.then(ColoredTagsManager::new);

        Self {
            data_provide,
            entries,
            current_entry_id: None,
            selected_entries,
            filtered_out_entries,
            settings,
            redraw_after_restore: false,
            filter: None,
            state: Default::default(),
            history,
            colored_tags,
        }
    }

    /// Get entries that meet the filter criteria if any otherwise it returns all entries
    pub fn get_active_entries(&self) -> impl DoubleEndedIterator<Item = &Entry> {
        self.entries
            .iter()
            .filter(|entry| !self.filtered_out_entries.contains(&entry.id))
    }

    pub fn get_entry(&self, entry_id: u32) -> Option<&Entry> {
        self.get_active_entries().find(|e| e.id == entry_id)
    }

    /// Gives a mutable reference to the entry with given id if exist, registering it in
    /// the history according to the given [`EntryEditPart`] and [`HistoryStack`]
    fn get_entry_mut(
        &mut self,
        entry_id: u32,
        edit_target: EntryEditPart,
        history_target: HistoryStack,
    ) -> Option<&mut Entry> {
        let entry_opt = self.entries.iter_mut().find(|e| e.id == entry_id);

        if let Some(entry) = entry_opt.as_ref() {
            match edit_target {
                EntryEditPart::Attributes => self
                    .history
                    .register_change_attributes(history_target, entry),
                EntryEditPart::Content => {
                    self.history.register_change_content(history_target, entry)
                }
            };
        }

        entry_opt
    }

    /// Gets' the selected entry currently.
    pub fn get_current_entry(&self) -> Option<&Entry> {
        self.current_entry_id
            .and_then(|id| self.get_active_entries().find(|entry| entry.id == id))
    }

    pub async fn load_entries(&mut self) -> anyhow::Result<()> {
        log::trace!("Loading entries");

        self.entries = self.data_provide.load_all_entries().await?;

        self.sort_entries();

        self.update_filtered_out_entries();

        self.update_colored_tags();

        Ok(())
    }

    pub async fn add_entry(
        &mut self,
        title: String,
        date: DateTime<Utc>,
        tags: Vec<String>,
        priority: Option<u32>,
    ) -> anyhow::Result<u32> {
        self.add_entry_intern(title, date, tags, priority, None, HistoryStack::Undo)
            .await
    }

    /// Creates an [`Entry`] from the given arguments, registering the change to the provided
    /// [`HistoryStack`].
    async fn add_entry_intern(
        &mut self,
        title: String,
        date: DateTime<Utc>,
        tags: Vec<String>,
        priority: Option<u32>,
        content: Option<String>,
        history_target: HistoryStack,
    ) -> anyhow::Result<u32> {
        log::trace!("Adding entry");

        let mut draft = EntryDraft::new(date, title, tags, priority);
        if let Some(content) = content {
            draft = draft.with_content(content);
        }

        let entry = self.data_provide.add_entry(draft).await?;
        let entry_id = entry.id;

        self.history.register_add(history_target, &entry);

        self.entries.push(entry);

        self.sort_entries();
        self.update_filtered_out_entries();
        self.update_colored_tags();

        Ok(entry_id)
    }

    /// Updates the attributes of the currently selected [`Entry`]
    pub async fn update_current_entry_attributes(
        &mut self,
        title: String,
        date: DateTime<Utc>,
        tags: Vec<String>,
        priority: Option<u32>,
    ) -> anyhow::Result<()> {
        let current_entry_id = self
            .current_entry_id
            .expect("Current entry id must have value when updating entry attributes");
        self.update_entry_attributes(
            current_entry_id,
            title,
            date,
            tags,
            priority,
            HistoryStack::Undo,
        )
        .await
    }

    /// Updates the attributes of the given [`Entry`], registering its state before the change on
    /// the given [`HistoryStack`]
    async fn update_entry_attributes(
        &mut self,
        entry_id: u32,
        title: String,
        date: DateTime<Utc>,
        tags: Vec<String>,
        priority: Option<u32>,
        history_target: HistoryStack,
    ) -> anyhow::Result<()> {
        log::trace!("Updating entry");

        assert!(self.current_entry_id.is_some());

        let entry = self
            .get_entry_mut(entry_id, EntryEditPart::Attributes, history_target)
            .expect("Current entry must have value when updating entry attributes");

        entry.title = title;
        entry.date = date;
        entry.tags = tags;
        entry.priority = priority;

        let clone = entry.clone();

        self.data_provide.update_entry(clone).await?;

        self.sort_entries();

        self.update_filter();
        self.update_filtered_out_entries();
        self.update_colored_tags();

        Ok(())
    }

    /// Updates the content of the currently selected [`Entry`]
    pub async fn update_current_entry_content(
        &mut self,
        entry_content: String,
    ) -> anyhow::Result<()> {
        let current_entry_id = self
            .current_entry_id
            .expect("Current entry id must have value when updating entry content");
        self.update_entry_content(current_entry_id, entry_content, HistoryStack::Undo)
            .await
    }

    /// Update the content of the given [`Entry`], registering its previous content to the given
    /// [`HistoryStack`]
    pub async fn update_entry_content(
        &mut self,
        entry_id: u32,
        entry_content: String,
        history_target: HistoryStack,
    ) -> anyhow::Result<()> {
        log::trace!("Updating entry content");

        let entry = self
            .get_entry_mut(entry_id, EntryEditPart::Content, history_target)
            .expect("Current entry id must have value when updating entry content");

        entry.content = entry_content;

        let clone = entry.clone();

        self.data_provide.update_entry(clone).await?;

        self.update_filtered_out_entries();

        Ok(())
    }

    pub async fn delete_entry(&mut self, entry_id: u32) -> anyhow::Result<()> {
        self.delete_entry_intern(entry_id, HistoryStack::Undo).await
    }

    /// Removes the given entry, registering it to the given [`HistoryStack`]
    pub async fn delete_entry_intern(
        &mut self,
        entry_id: u32,
        history_target: HistoryStack,
    ) -> anyhow::Result<()> {
        log::trace!("Deleting entry with id: {entry_id}");

        self.data_provide.remove_entry(entry_id).await?;
        let removed_entry = self
            .entries
            .iter()
            .position(|entry| entry.id == entry_id)
            .map(|index| self.entries.remove(index))
            .expect("entry must be in the entries list");

        self.history.register_remove(history_target, removed_entry);

        self.update_filter();
        self.update_filtered_out_entries();
        self.update_colored_tags();

        Ok(())
    }

    async fn export_entry_content(&self, entry_id: u32, path: PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let entry = self.get_entry(entry_id).expect("Entry should exist");

        tokio::fs::write(path, entry.content.to_owned()).await?;

        Ok(())
    }

    async fn export_entries(&self, path: PathBuf) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let selected_ids: Vec<u32> = self.selected_entries.iter().cloned().collect();

        let entries_dto = self.data_provide.get_export_object(&selected_ids).await?;

        let file = File::create(path)?;
        serde_json::to_writer_pretty(&file, &entries_dto)?;

        Ok(())
    }

    async fn import_entries(&self, file_path: PathBuf) -> anyhow::Result<()> {
        if !file_path.exists() {
            bail!("Import file doesn't exist: path {}", file_path.display())
        }

        let file = File::open(file_path)
            .map_err(|err| anyhow!("Error while opening import file: Error: {err}"))?;

        let entries_dto: EntriesDTO = serde_json::from_reader(&file)
            .map_err(|err| anyhow!("Error while parsing import file. Error: {err}"))?;

        self.data_provide
            .import_entries(entries_dto)
            .await
            .map_err(|err| anyhow!("Error while importing the entry. Error: {err}"))?;

        Ok(())
    }

    pub fn get_all_tags(&self) -> Vec<String> {
        let mut tags = BTreeSet::new();

        for tag in self.entries.iter().flat_map(|entry| &entry.tags) {
            tags.insert(tag);
        }

        tags.into_iter().map(String::from).collect()
    }

    /// Sets and applies the given filter on the entries
    pub fn apply_filter(&mut self, filter: Option<Filter>) {
        self.filter = filter;
        self.update_filtered_out_entries();
    }

    /// Checks if the filter criteria still valid and update them if needed
    fn update_filter(&mut self) {
        if self.filter.is_some() {
            let all_tags = self.get_all_tags();
            let filter = self.filter.as_mut().unwrap();

            filter.criteria.retain(|cr| match cr {
                FilterCriterion::Tag(TagFilterOption::Tag(tag)) => all_tags.contains(tag),
                FilterCriterion::Tag(TagFilterOption::NoTags) => true,
                FilterCriterion::Title(_) => true,
                FilterCriterion::Content(_) => true,
                FilterCriterion::Priority(_) => true,
            });

            if filter.criteria.is_empty() {
                self.filter = None;
            }
        }
    }

    /// Applies filter on the entries and filter out the ones who don't meet the filter's criteria
    fn update_filtered_out_entries(&mut self) {
        if let Some(filter) = self.filter.as_ref() {
            self.filtered_out_entries = self
                .entries
                .par_iter()
                .filter(|entry| !filter.check_entry(entry))
                .map(|entry| entry.id)
                .collect();
        } else {
            self.filtered_out_entries.clear();
        }
    }

    /// Updates the colors tags mapping, assigning colors to new one and removing the non existing
    /// tags from the colors map.
    fn update_colored_tags(&mut self) {
        if self.colored_tags.is_none() {
            return;
        }

        let tags = { self.get_all_tags() };
        if let Some(colored_tags) = self.colored_tags.as_mut() {
            colored_tags.update_tags(tags);
        }
    }

    /// Gets the matching color for the giving tag if colored tags are enabled and tag exists.
    pub fn get_color_for_tag(&self, tag: &str) -> Option<TagColors> {
        self.colored_tags
            .as_ref()
            .and_then(|c| c.get_tag_color(tag))
    }

    pub fn cycle_tags_in_filter(&mut self) {
        let mut all_tags: Vec<FilterCriterion> = self
            .get_all_tags()
            .iter()
            .map(|v| FilterCriterion::Tag(TagFilterOption::Tag(v.to_string())))
            .collect();
        if all_tags.len() <= 1 {
            return;
        }
        all_tags.push(FilterCriterion::Tag(TagFilterOption::NoTags));

        if let Some(mut filter) = self.filter.take() {
            let applied_tags: Vec<String> = filter
                .criteria
                .iter()
                .filter_map(|c| match c {
                    FilterCriterion::Tag(TagFilterOption::Tag(tag)) => Some(tag.to_owned()),
                    _ => None,
                })
                .collect();
            match applied_tags.len() {
                // No existing tags => apply the first one.
                0 => {
                    filter.criteria.push(
                        all_tags
                            .into_iter()
                            .next()
                            .expect("Bound check done at the beginning"),
                    );
                }
                // One tag exist only => Cycle to the next one.
                1 => {
                    let current_tag = filter
                        .criteria
                        .iter_mut()
                        .find_map(|c| match c {
                            FilterCriterion::Tag(TagFilterOption::Tag(tag)) => Some(tag),
                            _ => None,
                        })
                        .expect("Criteria checked for having one Tag only");

                    let tag_pos = all_tags
                        .iter()
                        .position(|t| {
                            *t == FilterCriterion::Tag(TagFilterOption::Tag(current_tag.clone()))
                        })
                        .unwrap_or_default();

                    let next_index = (tag_pos + 1) % all_tags.len();
                    match all_tags.get(next_index) {
                        Some(FilterCriterion::Tag(TagFilterOption::Tag(next_tag))) => {
                            *current_tag = next_tag.clone();
                        }
                        _ => {
                            filter
                                .criteria
                                .retain(|c| !matches!(c, FilterCriterion::Tag(_)));
                            filter
                                .criteria
                                .push(FilterCriterion::Tag(TagFilterOption::NoTags));
                        }
                    }
                }
                // Many tags exist => Clean them and apply the first one.
                _ => {
                    filter
                        .criteria
                        .retain(|c| !matches!(c, FilterCriterion::Tag(_)));
                    filter.criteria.push(
                        all_tags
                            .into_iter()
                            .next()
                            .expect("Bound check done at the beginning"),
                    );
                }
            }

            self.apply_filter(Some(filter));
        } else {
            // Apply filter with the first criteria
            let mut filter = Filter::default();
            filter.criteria.push(
                all_tags
                    .into_iter()
                    .next()
                    .expect("Bound check done at the beginning"),
            );
            self.apply_filter(Some(filter));
        }
    }

    /// Assigns priority to all entries that don't have a priority assigned to
    async fn assign_priority_to_entries(&self, priority: u32) -> anyhow::Result<()> {
        self.data_provide
            .assign_priority_to_entries(priority)
            .await?;

        Ok(())
    }

    pub fn apply_sort(&mut self, criteria: Vec<SortCriteria>, order: SortOrder) {
        self.state.sorter.set_criteria(criteria);
        self.state.sorter.order = order;

        self.sort_entries();
    }

    fn sort_entries(&mut self) {
        self.entries
            .sort_by(|entry1, entry2| self.state.sorter.sort(entry1, entry2));
    }

    pub fn load_state(&mut self, ui_components: &mut UIComponents) {
        let state = match AppState::load(&self.settings) {
            Ok(state) => state,
            Err(err) => {
                ui_components.show_err_msg(format!(
                    "Loading state failed. Falling back to default state\n\rError Info: {err}"
                ));
                AppState::default()
            }
        };

        self.state = state;
    }

    pub fn persist_state(&self) -> anyhow::Result<()> {
        self.state.save(&self.settings)?;

        Ok(())
    }

    /// Apply undo on entries returning the id of the effected entry.
    pub async fn undo(&mut self) -> anyhow::Result<Option<u32>> {
        match self.history.pop_undo() {
            Some(change) => self.apply_history_change(change, HistoryStack::Redo).await,
            None => Ok(None),
        }
    }

    /// Apply redo on entries returning the id of the effected entry.
    pub async fn redo(&mut self) -> anyhow::Result<Option<u32>> {
        match self.history.pop_redo() {
            Some(change) => self.apply_history_change(change, HistoryStack::Undo).await,
            None => Ok(None),
        }
    }

    async fn apply_history_change(
        &mut self,
        change: Change,
        history_target: HistoryStack,
    ) -> anyhow::Result<Option<u32>> {
        match change {
            Change::AddEntry { id } => {
                log::trace!("History Apply: Add Entry: ID {id}");
                self.delete_entry_intern(id, history_target).await?;
                Ok(None)
            }
            Change::RemoveEntry(entry) => {
                log::trace!("History Apply: Remove Entry: {entry:?}");
                let id = self
                    .add_entry_intern(
                        entry.title,
                        entry.date,
                        entry.tags,
                        entry.priority,
                        Some(entry.content),
                        history_target,
                    )
                    .await?;

                Ok(Some(id))
            }
            Change::EntryAttribute(attr) => {
                log::trace!("History Apply: Change Attributes: {attr:?}");
                self.update_entry_attributes(
                    attr.id,
                    attr.title,
                    attr.date,
                    attr.tags,
                    attr.priority,
                    history_target,
                )
                .await?;

                Ok(Some(attr.id))
            }
            Change::EntryContent { id, content } => {
                log::trace!("History Apply: Change Content: ID: {id}");
                self.update_entry_content(id, content, history_target)
                    .await?;
                Ok(Some(id))
            }
        }
    }
}

/// Represents what part of [`Entry`] will be changed.
enum EntryEditPart {
    /// The attributes (Name, Date...) of the entry will be changed
    Attributes,
    /// The content of the entry will be changed.
    Content,
}
