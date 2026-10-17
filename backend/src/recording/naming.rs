//! Sortable output names. Group time is a label, never a segment's actual start time.
use chrono::{DateTime, Duration, Local, NaiveDateTime, Utc};
use std::{fs, io, path::{Path, PathBuf}};

const FORMAT: &str = "%Y%m%d_%H%M%S";

#[derive(Debug, Clone)]
pub struct PartName {
    pub group_started_at: DateTime<Utc>,
    pub stamp: NaiveDateTime,
    pub index: u8,
}

/// Parse from the right so usernames may contain underscores, dates, and `part`.
pub fn parse(stem: &str) -> Option<(NaiveDateTime, Option<u8>)> {
    let (base, index) = match stem.rsplit_once("_part") {
        Some((base, digits)) if digits.len() == 2 && digits.bytes().all(|b| b.is_ascii_digit()) => {
            let index = digits.parse::<u8>().ok()?;
            if index == 0 { return None; }
            (base, Some(index))
        }
        _ => (stem, None),
    };
    let mut pieces = base.rsplitn(3, '_');
    let time = pieces.next()?;
    let date = pieces.next()?;
    if pieces.next()?.is_empty() || date.len() != 8 || time.len() != 6 { return None; }
    let stamp = NaiveDateTime::parse_from_str(&format!("{date}_{time}"), FORMAT).ok()?;
    Some((stamp, index))
}

impl PartName {
    pub fn stem(&self, username: &str) -> String {
        let name = username.strip_prefix("chaturbate~").unwrap_or(username);
        format!("{}_{}_part{:02}", name, self.stamp.format(FORMAT), self.index)
    }

    pub fn first(parent: &Path, username: &str, now: DateTime<Utc>) -> io::Result<(Self, PathBuf)> {
        Self::new_group(parent, username, now, None)
    }

    fn new_group(parent: &Path, username: &str, now: DateTime<Utc>, after: Option<NaiveDateTime>) -> io::Result<(Self, PathBuf)> {
        fs::create_dir_all(parent)?;
        let local = now.with_timezone(&Local).naive_local();
        let mut name = Self {
            group_started_at: now,
            stamp: after.map(|previous| local.max(previous + Duration::seconds(1))).unwrap_or(local),
            index: 1,
        };
        loop {
            if let Some(path) = name.reserve(parent, username)? { return Ok((name, path)); }
            // Logical labels advance without sleeping or changing real metadata timestamps.
            name.stamp += Duration::seconds(1);
        }
    }

    pub fn next(&self, parent: &Path, username: &str, now: DateTime<Utc>) -> io::Result<(Self, PathBuf)> {
        if self.index < 99 {
            let mut name = self.clone();
            name.index += 1;
            if let Some(path) = name.reserve(parent, username)? { return Ok((name, path)); }
        }
        Self::new_group(parent, username, now, Some(self.stamp))
    }

    fn reserve(&self, parent: &Path, username: &str) -> io::Result<Option<PathBuf>> {
        let stem = self.stem(username);
        // Reserve the entire group for a first part, including orphan sidecars/old formats.
        let prefix = if self.index == 1 {
            stem.strip_suffix("_part01").unwrap().to_owned()
        } else { stem.clone() };
        for entry in fs::read_dir(parent)? {
            let filename = entry?.file_name().to_string_lossy().into_owned();
            let filename = filename.strip_prefix('.').unwrap_or(&filename);
            if filename == prefix || filename.starts_with(&format!("{prefix}.")) || filename.starts_with(&format!("{prefix}_")) {
                return Ok(None);
            }
        }
        let path = parent.join(stem);
        match fs::create_dir(&path) {
            Ok(()) => Ok(Some(path)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> PathBuf {
        let p = std::env::temp_dir().join(format!("str-name-{}", rand::random::<u64>()));
        fs::create_dir_all(&p).unwrap(); p
    }
    #[test]
    fn ordering_rollover_midnight_and_restart_collision() {
        let root = root();
        let now = Utc::now();
        let (mut name, first) = PartName::first(&root, "chaturbate~a_room", now).unwrap();
        assert!(first.file_name().unwrap().to_str().unwrap().starts_with("a_room_"));
        let stamp = name.stamp;
        let mut names = vec![name.stem("chaturbate~a_room")];
        for i in 2..=99 {
            let (next, _) = name.next(&root, "chaturbate~a_room", now + Duration::hours(i)).unwrap();
            assert_eq!(next.stamp, stamp); // includes midnight and several calendar days
            assert_eq!(next.group_started_at, now);
            name = next; names.push(name.stem("chaturbate~a_room"));
        }
        let mut sorted = names.clone(); sorted.sort(); assert_eq!(names, sorted);
        let (next, _) = name.next(&root, "chaturbate~a_room", now).unwrap();
        assert_eq!(next.index, 1); assert!(next.stamp > name.stamp);
        let (restart, _) = PartName::first(&root, "chaturbate~a_room", now).unwrap();
        assert_ne!(restart.stamp.format(FORMAT).to_string(), stamp.format(FORMAT).to_string());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn parses_old_new_and_underscores_and_preserves_existing_files() {
        assert_eq!(parse("a_part_room_20261017_235959_part09").unwrap().1, Some(9));
        assert_eq!(parse("chaturbate~a_room_20261017_235959").unwrap().1, None);
        assert!(parse("a_20260230_235959_part01").is_none());
        assert!(parse("a_20261017_235959_part00").is_none());
        let root = root(); let now = Utc::now();
        let dummy = PartName { group_started_at: now, stamp: now.with_timezone(&Local).naive_local(), index: 1 };
        let occupied = root.join(format!("{}.mkv", dummy.stem("alice")));
        fs::write(&occupied, b"keep").unwrap();
        let (name, _) = PartName::first(&root, "alice", now).unwrap();
        assert!(name.stamp > dummy.stamp);
        assert_eq!(fs::read(&occupied).unwrap(), b"keep");
        fs::remove_dir_all(root).unwrap();
    }
}
