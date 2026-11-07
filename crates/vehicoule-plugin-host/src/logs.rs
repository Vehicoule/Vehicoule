//! What plugins log (docs/plugin-host-spec.md, section 5): bounded in size and count per call, tagged with the
//! plugin's id, and handed to the edge in the call's report. Never shown to the user as app text.

use crate::bindings::base::vehicoule::plugin::log;
use crate::{ByteSize, LogBudget, PluginId};

/// How much a plugin's message matters, as the plugin says.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LogLevel {
    /// Detail for whoever debugs the plugin.
    Debug,
    /// What the plugin did.
    Info,
    /// Something went wrong, and the plugin went on.
    Warn,
    /// Something failed.
    Error,
}

impl From<log::Level> for LogLevel {
    fn from(level: log::Level) -> Self {
        match level {
            log::Level::Debug => Self::Debug,
            log::Level::Info => Self::Info,
            log::Level::Warn => Self::Warn,
            log::Level::Error => Self::Error,
        }
    }
}

/// One message a plugin logged during a call.
///
/// The message is text from untrusted code, one line long: every control character and every line or paragraph
/// separator is replaced with U+FFFD, so it cannot forge another line where the edge writes it.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogEntry {
    /// The plugin that logged it.
    pub plugin: PluginId,
    /// Its level.
    pub level: LogLevel,
    /// The message, at most [`LogBudget::max_message`] bytes.
    pub message: String,
    /// Whether the message was cut to fit [`LogBudget::max_message`].
    pub truncated: bool,
}

/// The messages of one call, within its budget.
#[derive(Debug)]
pub(crate) struct Logs {
    plugin: PluginId,
    budget: LogBudget,
    pub(crate) entries: Vec<LogEntry>,
    pub(crate) dropped: u32,
}

impl Logs {
    pub(crate) fn new(plugin: PluginId, budget: LogBudget) -> Self {
        Self {
            plugin,
            budget,
            entries: Vec::new(),
            dropped: 0,
        }
    }

    /// Keeps `message`, cut to the budget, or counts it as dropped when the call already kept its most messages.
    pub(crate) fn push(&mut self, level: LogLevel, message: &str) {
        let max_messages = usize::try_from(self.budget.max_messages).unwrap_or(usize::MAX);
        if self.entries.len() >= max_messages {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        let (message, truncated) = one_line(message, self.budget.max_message);
        self.entries.push(LogEntry {
            plugin: self.plugin.clone(),
            level,
            message,
            truncated,
        });
    }
}

/// Whether `c` could break or rewrite a line where the edge writes it: a control character (Unicode `Cc`: C0, DEL,
/// C1, which hold LF, CR, VT, FF and NEL), or U+2028 LINE SEPARATOR and U+2029 PARAGRAPH SEPARATOR, which are not
/// `Cc`. Together they are every mandatory line break of Unicode's line breaking algorithm (UAX #14, class BK, CR, LF
/// and NL).
fn breaks_a_line(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}')
}

/// `text` with every character that [breaks a line](breaks_a_line) replaced with U+FFFD, cut at the last character
/// that fits in `max` bytes, and whether it was cut.
fn one_line(text: &str, max: ByteSize) -> (String, bool) {
    let max = max.saturating_usize();
    let mut line = String::new();
    for c in text.chars() {
        let c = if breaks_a_line(c) { '\u{FFFD}' } else { c };
        if line.len().saturating_add(c.len_utf8()) > max {
            return (line, true);
        }
        line.push(c);
    }
    (line, false)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn logs(max_messages: u32, max_message: u64) -> Logs {
        Logs::new(
            PluginId::parse("fixture").unwrap(),
            LogBudget::new(max_messages, ByteSize::new(max_message)),
        )
    }

    #[test]
    fn every_level_maps_to_its_own_level() {
        assert_eq!(LogLevel::from(log::Level::Debug), LogLevel::Debug);
        assert_eq!(LogLevel::from(log::Level::Info), LogLevel::Info);
        assert_eq!(LogLevel::from(log::Level::Warn), LogLevel::Warn);
        assert_eq!(LogLevel::from(log::Level::Error), LogLevel::Error);
    }

    #[test]
    fn a_message_exactly_at_its_limit_is_kept_whole_and_one_byte_more_is_cut() {
        assert_eq!(
            one_line("abcd", ByteSize::new(4)),
            ("abcd".to_owned(), false)
        );
        assert_eq!(
            one_line("abcde", ByteSize::new(4)),
            ("abcd".to_owned(), true)
        );
    }

    #[test]
    fn a_message_is_cut_before_a_character_that_does_not_fit() {
        // `é` is 2 bytes: with 3 bytes, `aé` fits and the next `é` does not.
        assert_eq!(one_line("aéé", ByteSize::new(3)), ("aé".to_owned(), true));
        assert_eq!(one_line("aéé", ByteSize::new(4)), ("aé".to_owned(), true));
    }

    #[test]
    fn control_characters_become_replacement_characters() {
        assert_eq!(
            one_line("a\nb\r\u{1b}[2J\u{85}", ByteSize::new(64)),
            ("a\u{FFFD}b\u{FFFD}\u{FFFD}[2J\u{FFFD}".to_owned(), false)
        );
    }

    #[test]
    fn line_and_paragraph_separators_become_replacement_characters() {
        assert_eq!(
            one_line("a\u{2028}b\u{2029}c", ByteSize::new(64)),
            ("a\u{FFFD}b\u{FFFD}c".to_owned(), false)
        );
    }

    #[test]
    fn messages_over_the_count_are_dropped_and_counted() {
        let mut logs = logs(2, 64);
        for message in ["one", "two", "three", "four"] {
            logs.push(LogLevel::Info, message);
        }
        let kept: Vec<&str> = logs.entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(kept, ["one", "two"]);
        assert_eq!(logs.dropped, 2);
    }

    #[test]
    fn every_entry_is_tagged_with_the_plugin_and_its_level() {
        let mut logs = logs(2, 64);
        logs.push(LogLevel::Warn, "slow");
        assert_eq!(
            logs.entries,
            [LogEntry {
                plugin: PluginId::parse("fixture").unwrap(),
                level: LogLevel::Warn,
                message: "slow".to_owned(),
                truncated: false,
            }]
        );
    }

    #[test]
    fn a_budget_of_no_message_keeps_none() {
        let mut logs = logs(0, 64);
        logs.push(LogLevel::Error, "lost");
        assert_eq!(logs.entries, Vec::<LogEntry>::new());
        assert_eq!(logs.dropped, 1);
    }

    proptest! {
        #[test]
        fn a_kept_message_is_one_line_within_its_limit_and_a_prefix_of_the_cleaned_text(
            text in "(?s)(.|\u{2028}|\u{2029}){0,40}",
            max in 0u64..48,
        ) {
            let (line, truncated) = one_line(&text, ByteSize::new(max));
            // The rule written out, apart from `breaks_a_line`: Cc and the two separators.
            let breaks = |c: char| c.is_control() || c == '\u{2028}' || c == '\u{2029}';
            let cleaned: String = text.chars().map(|c| if breaks(c) { '\u{FFFD}' } else { c }).collect();
            prop_assert!(line.len() <= usize::try_from(max).unwrap());
            prop_assert!(!line.chars().any(breaks));
            prop_assert!(cleaned.starts_with(&line));
            prop_assert_eq!(truncated, line.len() < cleaned.len());
        }
    }
}
