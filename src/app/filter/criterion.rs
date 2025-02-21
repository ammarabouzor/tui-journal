use aho_corasick::AhoCorasick;
use backend::Entry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagFilterOption {
    Tag(String),
    NoTags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterCriterion {
    Tag(TagFilterOption),
    Title(String),
    Content(String),
    Priority(u32),
}

impl FilterCriterion {
    /// Checks if the entry meets the criterion
    pub fn check_entry(&self, entry: &Entry) -> bool {
        match self {
            FilterCriterion::Tag(TagFilterOption::Tag(tag)) => entry.tags.contains(tag),
            FilterCriterion::Tag(TagFilterOption::NoTags) => entry.tags.is_empty(),
            FilterCriterion::Title(search) => {
                // Use simple smart-case search for title
                if search.chars().any(|c| c.is_uppercase()) {
                    entry.title.contains(search)
                } else {
                    entry.title.to_lowercase().contains(search)
                }
            }
            FilterCriterion::Content(search) => {
                if search.chars().any(|c| c.is_uppercase()) {
                    // Use simple search when pattern already has uppercase
                    entry.content.contains(search)
                } else {
                    // Otherwise use case insensitive pattern matcher
                    let ac = match AhoCorasick::builder()
                        .ascii_case_insensitive(true)
                        .build([&search])
                    {
                        Ok(ac) => ac,
                        Err(err) => {
                            log::error!(
                                "Build AhoCorasick with pattern {search} failed with error: {err}"
                            );
                            return false;
                        }
                    };

                    ac.find(&entry.content).is_some()
                }
            }
            FilterCriterion::Priority(prio) => entry.priority.is_some_and(|pr| pr == *prio),
        }
    }
}
