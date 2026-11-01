//! Folder contents for stacks: sorting and how old a file is, in words.

use crate::config::Sort;

#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    pub name: String,
    /// Seconds since the epoch.
    pub modified: i64,
    /// Content type (`image/png`, `inode/directory`).
    pub kind: String,
    pub hidden: bool,
}

/// Sort for a stack: hidden files left out, folders and files mixed (as in a Downloads
/// stack), newest first by date.
pub fn sorted(mut entries: Vec<FileEntry>, sort: Sort) -> Vec<FileEntry> {
    entries.retain(|e| !e.hidden && !e.name.starts_with('.') && !is_partial(&e.name));
    let by_name = |a: &FileEntry, b: &FileEntry| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    entries.sort_by(|a, b| match sort {
        Sort::Date => b.modified.cmp(&a.modified).then_with(|| by_name(a, b)),
        Sort::Name => by_name(a, b),
        Sort::Kind => a.kind.cmp(&b.kind).then_with(|| by_name(a, b)),
    });
    entries
}

/// A download still in progress (browsers' partial files).
fn is_partial(name: &str) -> bool {
    [".part", ".crdownload", ".download", ".partial"].iter().any(|ext| name.ends_with(ext))
}

/// "Just now", "5 min ago", "3 h ago", "Yesterday", "Tue", or a date.
/// `now` and `then` are seconds since the epoch; `days_ago` is how many calendar days
/// back `then` is (0 = today), worked out by the caller in local time.
pub fn ago(now: i64, then: i64, days_ago: i64, weekday: &str, date: &str) -> String {
    let secs = (now - then).max(0);
    match days_ago {
        0 if secs < 60 => "Just now".into(),
        0 if secs < 3600 => format!("{} min ago", secs / 60),
        0 => format!("{} h ago", secs / 3600),
        1 => "Yesterday".into(),
        2..=6 => weekday.into(),
        _ => date.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, modified: i64, kind: &str) -> FileEntry {
        FileEntry { name: name.into(), modified, kind: kind.into(), hidden: false }
    }

    #[test]
    fn sorts_and_skips() {
        let list = vec![
            e("b.png", 30, "image/png"),
            e("A.pdf", 10, "application/pdf"),
            e(".hidden", 99, "text/plain"),
            e("movie.mkv.part", 98, "video/x-matroska"),
            e("c.zip", 20, "application/zip"),
        ];
        let names = |v: Vec<FileEntry>| v.into_iter().map(|e| e.name).collect::<Vec<_>>();
        assert_eq!(names(sorted(list.clone(), Sort::Date)), vec!["b.png", "c.zip", "A.pdf"]);
        assert_eq!(names(sorted(list.clone(), Sort::Name)), vec!["A.pdf", "b.png", "c.zip"]);
        assert_eq!(names(sorted(list, Sort::Kind)), vec!["A.pdf", "c.zip", "b.png"]);
    }

    #[test]
    fn ages() {
        let now = 1_000_000;
        assert_eq!(ago(now, now - 5, 0, "", ""), "Just now");
        assert_eq!(ago(now, now - 125, 0, "", ""), "2 min ago");
        assert_eq!(ago(now, now - 7300, 0, "", ""), "2 h ago");
        assert_eq!(ago(now, now - 90_000, 1, "", ""), "Yesterday");
        assert_eq!(ago(now, now - 300_000, 3, "Thu", ""), "Thu");
        assert_eq!(ago(now, now - 3_000_000, 34, "Mon", "Sep 28"), "Sep 28");
    }
}
