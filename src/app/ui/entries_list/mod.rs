use chrono::Datelike;

use ratatui::{
    layout::{Alignment, Rect},
    prelude::Margin,
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Wrap,
    },
    Frame,
};

use backend::DataProvider;

use crate::app::App;
use crate::{app::keymap::Keymap, settings::DatumVisibility};

use super::INACTIVE_CONTROL_COLOR;
use super::{UICommand, ACTIVE_CONTROL_COLOR};

const LIST_INNER_MARGIN: usize = 5;
const SELECTED_FOREGROUND_COLOR: Color = Color::Yellow;

#[derive(Debug)]
pub struct EntriesList {
    pub state: ListState,
    is_active: bool,
    pub multi_select_mode: bool,
}

impl<'a> EntriesList {
    pub fn new() -> Self {
        Self {
            state: ListState::default(),
            is_active: false,
            multi_select_mode: false,
        }
    }

    fn render_list<D: DataProvider>(&mut self, frame: &mut Frame, app: &App<D>, area: Rect) {
        let (foreground_color, highlight_bg) = if self.is_active {
            (ACTIVE_CONTROL_COLOR, Color::LightGreen)
        } else {
            (INACTIVE_CONTROL_COLOR, Color::LightBlue)
        };

        let mut lines_count = 0;

        let items: Vec<ListItem> = app
            .get_active_entries()
            .map(|entry| {
                let highlight_selected =
                    self.multi_select_mode && app.selected_entries.contains(&entry.id);

                // *** Title ***
                let mut title = entry.title.to_string();

                if highlight_selected {
                    title.insert_str(0, "* ");
                }

                // Text wrapping
                let title_lines = textwrap::wrap(&title, area.width as usize - LIST_INNER_MARGIN);

                // tilte lines
                lines_count += title_lines.len();

                let fg_color = if highlight_selected {
                    SELECTED_FOREGROUND_COLOR
                } else {
                    foreground_color
                };

                let mut spans: Vec<Line> = title_lines
                    .iter()
                    .map(|line| {
                        Line::from(Span::styled(
                            line.to_string(),
                            Style::default().fg(fg_color).add_modifier(Modifier::BOLD),
                        ))
                    })
                    .collect();

                // *** Date & Priority ***
                let date_priority_lines = match (app.settings.datum_visibility, entry.priority) {
                    (DatumVisibility::Show, Some(prio)) => {
                        let one_liner = format!(
                            "{},{},{} | Priority: {}",
                            entry.date.day(),
                            entry.date.month(),
                            entry.date.year(),
                            prio
                        );

                        if one_liner.len() > area.width as usize - LIST_INNER_MARGIN {
                            vec![
                                format!(
                                    "{},{},{}",
                                    entry.date.day(),
                                    entry.date.month(),
                                    entry.date.year()
                                ),
                                format!("Priority: {prio}"),
                            ]
                        } else {
                            vec![one_liner]
                        }
                    }
                    (DatumVisibility::Show, None) => {
                        vec![format!(
                            "{},{},{}",
                            entry.date.day(),
                            entry.date.month(),
                            entry.date.year()
                        )]
                    }
                    (DatumVisibility::Hide, None) => Vec::new(),
                    (DatumVisibility::EmptyLine, None) => vec![String::new()],
                    (_, Some(prio)) => {
                        vec![format!("Priority: {}", prio)]
                    }
                };

                let date_lines = date_priority_lines.iter().map(|line| {
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default()
                            .fg(Color::LightBlue)
                            .remove_modifier(Modifier::BOLD),
                    ))
                });
                spans.extend(date_lines);

                // date & priority lines
                lines_count += date_priority_lines.len();

                // *** Tags ***
                if !entry.tags.is_empty() {
                    const TAGS_SEPARATOR: &str = " | ";
                    let tags_default_style: Style = Style::default()
                        .fg(Color::LightCyan)
                        .add_modifier(Modifier::DIM);

                    let mut added_lines = 1;
                    spans.push(Line::default());

                    for tag in entry.tags.iter() {
                        let mut last_line = spans.last_mut().unwrap();
                        let allowd_width = area.width as usize - LIST_INNER_MARGIN;
                        if !last_line.spans.is_empty() {
                            if last_line.width() + TAGS_SEPARATOR.len() > allowd_width {
                                added_lines += 1;
                                spans.push(Line::default());
                                last_line = spans.last_mut().unwrap();
                            }
                            last_line.push_span(Span::styled(TAGS_SEPARATOR, tags_default_style))
                        }

                        let style = app
                            .get_color_for_tag(tag)
                            .map(|c| Style::default().bg(c.background).fg(c.foreground))
                            .unwrap_or(tags_default_style);
                        let span_to_add = Span::styled(tag.to_owned(), style);

                        if last_line.width() + tag.len() < allowd_width {
                            last_line.push_span(span_to_add);
                        } else {
                            added_lines += 1;
                            let line = Line::from(span_to_add);
                            spans.push(line);
                        }
                    }

                    lines_count += added_lines;
                }

                ListItem::new(spans)
            })
            .collect();

        let items_count = items.len();

        let list = List::new(items)
            .block(self.get_list_block(app.filter.is_some(), Some(items_count)))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(highlight_bg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, &mut self.state);

        let lines_count = lines_count;

        if lines_count > area.height as usize - 2 {
            let avg_item_height = lines_count / items_count;

            self.render_scrollbar(
                frame,
                area,
                self.state.selected().unwrap_or(0),
                items_count,
                avg_item_height,
            );
        }
    }

    fn render_scrollbar(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        pos: usize,
        items_count: usize,
        avg_item_height: usize,
    ) {
        const VIEWPORT_ADJUST: u16 = 4;
        let viewport_len = (area.height / avg_item_height as u16).saturating_sub(VIEWPORT_ADJUST);

        let mut state = ScrollbarState::default()
            .content_length(items_count)
            .viewport_content_length(viewport_len as usize)
            .position(pos);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some(symbols::line::VERTICAL))
            .thumb_symbol(symbols::block::FULL);

        let scroll_area = area.inner(Margin {
            horizontal: 0,
            vertical: 1,
        });

        frame.render_stateful_widget(scrollbar, scroll_area, &mut state);
    }

    fn render_place_holder(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        list_keymaps: &[Keymap],
        has_filter: bool,
    ) {
        let keys_text: Vec<String> = list_keymaps
            .iter()
            .filter(|keymap| keymap.command == UICommand::CreateEntry)
            .map(|keymap| format!("'{}'", keymap.key))
            .collect();

        let place_holder_text = if self.multi_select_mode {
            String::from("\nNo entries to select")
        } else {
            format!("\n Use {} to create new entry ", keys_text.join(","))
        };

        let place_holder = Paragraph::new(place_holder_text)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Center)
            .block(self.get_list_block(has_filter, None));

        frame.render_widget(place_holder, area);
    }

    fn get_list_block(&self, has_filter: bool, entries_len: Option<usize>) -> Block<'a> {
        let title = match (self.multi_select_mode, has_filter) {
            (true, true) => "Journals - Multi-Select - Filtered",
            (true, false) => "Journals - Multi-Select",
            (false, true) => "Journals - Filtered",
            (false, false) => "Journals",
        };

        let border_style = match (self.is_active, self.multi_select_mode) {
            (_, true) => Style::default()
                .fg(SELECTED_FOREGROUND_COLOR)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::ITALIC),
            (true, _) => Style::default()
                .fg(ACTIVE_CONTROL_COLOR)
                .add_modifier(Modifier::BOLD),
            (false, _) => Style::default().fg(INACTIVE_CONTROL_COLOR),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border_style);

        match (entries_len, self.state.selected().map(|v| v + 1)) {
            (Some(entries_len), Some(selected)) => {
                block.title_bottom(Line::from(format!("{selected}/{entries_len}")).right_aligned())
            }
            _ => block,
        }
    }

    pub fn render_widget<D: DataProvider>(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        app: &App<D>,
        list_keymaps: &[Keymap],
    ) {
        if app.get_active_entries().next().is_none() {
            self.render_place_holder(frame, area, list_keymaps, app.filter.is_some());
        } else {
            self.render_list(frame, app, area);
        }
    }

    pub fn set_active(&mut self, active: bool) {
        self.is_active = active;
    }
}
