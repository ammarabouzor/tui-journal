use std::collections::HashSet;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};
use tui_textarea::{CursorMove, TextArea};

use crate::app::{
    filter::{criterion::TagFilterOption, CriteriaRelation, Filter, FilterCriterion},
    keymap::Input,
};

use super::{ui_functions::centered_rect, PopupReturn, Styles};

type FilterPopupReturn = PopupReturn<Option<Filter>>;

const FOOTER_TEXT: &str = r"Tab: Change focused control | Enter or <Ctrl-m>: Confirm | Esc or <Ctrl-c>: Cancel | <Ctrl-r>: Change Matching Logic | <Space>: Tags Toggle Selected";
const FOOTER_MARGIN: usize = 8;

pub struct FilterPopup<'a> {
    active_control: FilterControl,
    tags_state: ListState,
    tags: Vec<String>,
    relation: CriteriaRelation,
    selected_tags: HashSet<String>,
    title_txt: TextArea<'a>,
    content_txt: TextArea<'a>,
    priority_txt: TextArea<'a>,
    priority_err_msg: String,
}

#[derive(Debug, PartialEq, Eq)]
enum FilterControl {
    TitleTxt,
    ContentTxt,
    PriorityTxt,
    TagsList,
}

impl FilterPopup<'_> {
    pub fn new(tags: Vec<String>, filter: Option<Filter>) -> Self {
        let filter = filter.unwrap_or_default();

        let relation = filter.relation;

        let mut selected_tags = HashSet::new();
        let mut title_text = String::default();
        let mut content_text = String::default();
        let mut priority_text = String::default();

        filter.criteria.into_iter().for_each(|cr| match cr {
            FilterCriterion::Tag(TagFilterOption::Tag(tag)) => {
                selected_tags.insert(tag);
            }
            FilterCriterion::Tag(TagFilterOption::NoTags) => {
                selected_tags.clear();
            }
            FilterCriterion::Title(title_search) => title_text = title_search,
            FilterCriterion::Content(content_search) => content_text = content_search,
            FilterCriterion::Priority(prio) => priority_text = prio.to_string(),
        });

        let mut title_txt = TextArea::new(vec![title_text]);
        title_txt.move_cursor(CursorMove::End);

        let mut content_txt = TextArea::new(vec![content_text]);
        content_txt.move_cursor(CursorMove::End);

        let mut priority_txt = TextArea::new(vec![priority_text]);
        priority_txt.move_cursor(CursorMove::End);

        let active_control = FilterControl::TitleTxt;

        let mut filter_popup = FilterPopup {
            active_control,
            tags_state: ListState::default(),
            tags,
            relation,
            selected_tags,
            title_txt,
            content_txt,
            priority_txt,
            priority_err_msg: String::default(),
        };

        filter_popup.cycle_next_tag();

        filter_popup.validate_priority();

        filter_popup
    }

    pub fn render_widget(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let area = centered_rect(70, 80, area);

        let block = Block::default().borders(Borders::ALL).title("Filter");
        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let footer_height = textwrap::fill(FOOTER_TEXT, (area.width as usize) - FOOTER_MARGIN)
            .lines()
            .count();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .horizontal_margin(4)
            .vertical_margin(2)
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(4),
                    Constraint::Length(footer_height.try_into().unwrap()),
                ]
                .as_ref(),
            )
            .split(area);

        self.render_relations(frame, chunks[0]);

        self.render_text_boxes(frame, chunks[1], chunks[2], chunks[3], styles);

        if self.tags.is_empty() {
            self.render_tags_place_holder(frame, chunks[4], styles);
        } else {
            self.render_tags_list(frame, chunks[4], styles);
        }

        self.render_footer(frame, chunks[5]);
    }

    fn render_relations(&mut self, frame: &mut Frame, area: Rect) {
        let relation_text = match self.relation {
            CriteriaRelation::And => "Journals must meet all criteria",
            CriteriaRelation::Or => "Journals must meet any of the criteria",
        };

        let relation = Paragraph::new(relation_text)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("Matching Logic"),
            );

        frame.render_widget(relation, area);
    }

    fn render_text_boxes(
        &mut self,
        frame: &mut Frame,
        title_area: Rect,
        content_area: Rect,
        priority_area: Rect,
        styles: &Styles,
    ) {
        let gstyles = &styles.general;
        let active_cursor_style: Style = gstyles.input_corsur_active.into();
        let invalid_cursor_style: Style = gstyles.input_corsur_invalid.into();
        let deactivate_cursor_style = Style::default().bg(Color::Reset);

        let mut title_txt_block = Block::default().title("Title").borders(Borders::ALL);
        let mut content_txt_block = Block::default().title("Content").borders(Borders::ALL);
        let mut priority_txt_block = if self.priority_err_msg.is_empty() {
            Block::default().title("Priority").borders(Borders::ALL)
        } else {
            Block::default()
                .title(format!("Priority : {}", self.priority_err_msg))
                .borders(Borders::ALL)
                .style(gstyles.input_block_invalid)
        };

        match self.active_control {
            FilterControl::TitleTxt => {
                self.title_txt.set_cursor_style(active_cursor_style);
                self.content_txt.set_cursor_style(deactivate_cursor_style);
                self.priority_txt.set_cursor_style(deactivate_cursor_style);
                title_txt_block = title_txt_block.style(gstyles.input_block_active);
            }
            FilterControl::ContentTxt => {
                self.title_txt.set_cursor_style(deactivate_cursor_style);
                self.content_txt.set_cursor_style(active_cursor_style);
                self.priority_txt.set_cursor_style(deactivate_cursor_style);
                content_txt_block = content_txt_block.style(gstyles.input_block_active);
            }
            FilterControl::TagsList => {
                self.title_txt.set_cursor_style(deactivate_cursor_style);
                self.content_txt.set_cursor_style(deactivate_cursor_style);
                self.priority_txt.set_cursor_style(deactivate_cursor_style);
            }
            FilterControl::PriorityTxt => {
                self.title_txt.set_cursor_style(deactivate_cursor_style);
                self.content_txt.set_cursor_style(deactivate_cursor_style);
                if self.priority_err_msg.is_empty() {
                    self.priority_txt.set_cursor_style(active_cursor_style);
                    priority_txt_block = priority_txt_block.style(gstyles.input_block_active);
                } else {
                    self.priority_txt.set_cursor_style(invalid_cursor_style);
                }
            }
        }

        self.title_txt.set_cursor_line_style(Style::default());
        self.content_txt.set_cursor_line_style(Style::default());
        self.priority_txt.set_cursor_line_style(Style::default());

        self.title_txt.set_block(title_txt_block);
        self.content_txt.set_block(content_txt_block);
        self.priority_txt.set_block(priority_txt_block);

        frame.render_widget(&self.title_txt, title_area);
        frame.render_widget(&self.content_txt, content_area);
        frame.render_widget(&self.priority_txt, priority_area);
    }

    fn render_tags_list(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let gstyles = &styles.general;
        let selected_style = Style::from(gstyles.list_item_selected);
        let items: Vec<ListItem> = self
            .tags
            .iter()
            .map(|tag| {
                let is_selected = self.selected_tags.contains(tag);

                let (tag_text, style) = if is_selected {
                    (format!("* {tag}"), selected_style)
                } else {
                    (tag.to_owned(), Style::reset())
                };

                ListItem::new(tag_text).style(style)
            })
            .collect();

        let highlight_style = match self.active_control {
            FilterControl::TagsList => gstyles.list_highlight_active,
            _ => gstyles.list_highlight_inactive,
        };

        let list = List::new(items)
            .block(self.get_list_block(styles))
            .highlight_style(highlight_style)
            .highlight_symbol(">> ");

        frame.render_stateful_widget(list, area, &mut self.tags_state);
    }

    fn render_tags_place_holder(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let place_holder_text = String::from("\nNo journals with tags provided");

        let place_holder = Paragraph::new(place_holder_text)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Center)
            .block(self.get_list_block(styles));

        frame.render_widget(place_holder, area);
    }

    fn get_list_block<'b>(&self, styles: &Styles) -> Block<'b> {
        let style = match self.active_control {
            FilterControl::TagsList => Style::from(styles.general.input_block_active),
            _ => Style::default(),
        };
        Block::default()
            .borders(Borders::ALL)
            .title("Tags")
            .border_type(BorderType::Rounded)
            .style(style)
    }

    fn render_footer(&mut self, frame: &mut Frame, area: Rect) {
        let footer = Paragraph::new(FOOTER_TEXT)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::NONE)
                    .style(Style::default()),
            );

        frame.render_widget(footer, area);
    }

    pub fn handle_input(&mut self, input: &Input) -> FilterPopupReturn {
        let has_control = input.modifiers.contains(KeyModifiers::CONTROL);

        if self.active_control != FilterControl::TagsList {
            match input.key_code {
                KeyCode::Tab => self.cycle_next_control(),
                KeyCode::Esc => FilterPopupReturn::Cancel,
                KeyCode::Char('c') if has_control => FilterPopupReturn::Cancel,
                KeyCode::Enter => self.confirm(),
                KeyCode::Char('m') if has_control => self.confirm(),
                KeyCode::Char('r') if has_control => {
                    self.change_relation();
                    FilterPopupReturn::KeepPopup
                }
                _ => {
                    match self.active_control {
                        FilterControl::TitleTxt => _ = self.title_txt.input(KeyEvent::from(input)),
                        FilterControl::ContentTxt => {
                            _ = self.content_txt.input(KeyEvent::from(input))
                        }
                        FilterControl::PriorityTxt => {
                            if self.priority_txt.input(KeyEvent::from(input)) {
                                self.validate_priority();
                            }
                        }
                        FilterControl::TagsList => unreachable!("Tags List is unreachable here"),
                    };
                    FilterPopupReturn::KeepPopup
                }
            }
        } else {
            match input.key_code {
                KeyCode::Tab => self.cycle_next_control(),
                KeyCode::Char('j') | KeyCode::Down => {
                    self.cycle_next_tag();
                    FilterPopupReturn::KeepPopup
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.cycle_prev_tag();
                    FilterPopupReturn::KeepPopup
                }
                KeyCode::Char(' ') => {
                    self.toggle_selected();
                    FilterPopupReturn::KeepPopup
                }
                KeyCode::Char('r') => {
                    self.change_relation();
                    FilterPopupReturn::KeepPopup
                }
                KeyCode::Esc | KeyCode::Char('q') => FilterPopupReturn::Cancel,
                KeyCode::Char('c') if has_control => FilterPopupReturn::Cancel,
                KeyCode::Enter => self.confirm(),
                KeyCode::Char('m') if has_control => self.confirm(),
                _ => FilterPopupReturn::KeepPopup,
            }
        }
    }

    fn cycle_next_control(&mut self) -> FilterPopupReturn {
        self.active_control = match self.active_control {
            FilterControl::TitleTxt => FilterControl::ContentTxt,
            FilterControl::ContentTxt => FilterControl::PriorityTxt,
            FilterControl::PriorityTxt => FilterControl::TagsList,
            FilterControl::TagsList => FilterControl::TitleTxt,
        };

        FilterPopupReturn::KeepPopup
    }

    fn cycle_next_tag(&mut self) {
        if self.tags.is_empty() {
            return;
        }

        let last_index = self.tags.len() - 1;
        let new_index = self
            .tags_state
            .selected()
            .map(|idx| if idx >= last_index { 0 } else { idx + 1 })
            .unwrap_or(0);

        self.tags_state.select(Some(new_index));
    }

    fn cycle_prev_tag(&mut self) {
        if self.tags.is_empty() {
            return;
        }

        let last_index = self.tags.len() - 1;
        let new_index = self
            .tags_state
            .selected()
            .map(|idx| idx.checked_sub(1).unwrap_or(last_index))
            .unwrap_or(last_index);

        self.tags_state.select(Some(new_index));
    }

    fn change_relation(&mut self) {
        self.relation = match self.relation {
            CriteriaRelation::And => CriteriaRelation::Or,
            CriteriaRelation::Or => CriteriaRelation::And,
        }
    }

    fn toggle_selected(&mut self) {
        if let Some(idx) = self.tags_state.selected() {
            let tag = self
                .tags
                .get(idx)
                .expect("tags has the index of the selected item in list");

            if self.selected_tags.contains(tag) {
                self.selected_tags.remove(tag);
            } else {
                self.selected_tags.insert(tag.to_owned());
            }
        }
    }

    fn validate_priority(&mut self) {
        let prio_text = self.priority_txt.lines().first().unwrap();
        if !prio_text.is_empty() && prio_text.parse::<u32>().is_err() {
            self.priority_err_msg = String::from("Priority must be a positive number");
        } else {
            self.priority_err_msg.clear();
        }
    }

    fn is_valid_input(&self) -> bool {
        self.priority_err_msg.is_empty()
    }

    fn confirm(&mut self) -> FilterPopupReturn {
        self.validate_priority();
        if !self.is_valid_input() {
            return FilterPopupReturn::KeepPopup;
        }

        let mut critria: Vec<_> = self
            .selected_tags
            .iter()
            .map(|tag| FilterCriterion::Tag(TagFilterOption::Tag(tag.into())))
            .collect();

        let title_filter = self
            .title_txt
            .lines()
            .first()
            .expect("Title TextBox has one line");

        if !title_filter.is_empty() {
            critria.push(FilterCriterion::Title(title_filter.to_owned()));
        }

        let content_filter = self
            .content_txt
            .lines()
            .first()
            .expect("Content TextBox has one line");

        if !content_filter.is_empty() {
            critria.push(FilterCriterion::Content(content_filter.to_owned()));
        }

        let priority_filter = self
            .priority_txt
            .lines()
            .first()
            .expect("Priority text box has one line");
        if !priority_filter.is_empty() {
            let prio = priority_filter
                .parse()
                .expect("Priority text is validated at this point");
            critria.push(FilterCriterion::Priority(prio));
        }

        if critria.is_empty() {
            FilterPopupReturn::Apply(None)
        } else {
            let filter = Filter {
                relation: self.relation,
                criteria: critria,
            };

            FilterPopupReturn::Apply(Some(filter))
        }
    }
}
