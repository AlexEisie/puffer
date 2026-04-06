use crate::{SessionMetadata, SessionRecord, SessionSummary, TranscriptEvent};
use anyhow::Context;
use anyhow::Result;
use puffer_config::ConfigPaths;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

/// Stores and retrieves append-only Puffer sessions.
#[derive(Debug, Clone)]
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    /// Creates a session store rooted under the workspace configuration directory.
    pub fn from_paths(paths: &ConfigPaths) -> Result<Self> {
        let root = paths.workspace_config_dir.join("sessions");
        fs::create_dir_all(&root)
            .with_context(|| format!("failed to create session dir {}", root.display()))?;
        Ok(Self { root })
    }

    /// Returns the on-disk root directory used by this session store.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Creates a new session and writes its metadata stub to disk.
    pub fn create_session(&self, cwd: PathBuf) -> Result<SessionMetadata> {
        let now = unix_timestamp_ms();
        let metadata = SessionMetadata {
            id: Uuid::new_v4(),
            display_name: None,
            cwd,
            created_at_ms: now,
            updated_at_ms: now,
            parent_session_id: None,
            slug: Some(format!("session-{}", Uuid::new_v4().simple())),
            tags: Vec::new(),
            note: None,
        };
        let path = self.session_path(metadata.id);
        let file = SessionFile {
            metadata: metadata.clone(),
        };
        fs::write(&path, serde_json::to_vec(&file)?)
            .with_context(|| format!("failed to create session file {}", path.display()))?;
        fs::write(path.with_extension("jsonl"), b"")?;
        Ok(metadata)
    }

    /// Appends a transcript event to the session log.
    pub fn append_event(&self, session_id: Uuid, event: TranscriptEvent) -> Result<()> {
        let path = self.session_path(session_id).with_extension("jsonl");
        let mut file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .with_context(|| format!("failed to open session log {}", path.display()))?;
        let line = serde_json::to_string(&event)?;
        writeln!(file, "{line}")?;
        self.touch_session(session_id)?;
        Ok(())
    }

    /// Updates a session display name and records the rename in the event log.
    pub fn rename_session(&self, session_id: Uuid, name: String) -> Result<()> {
        self.update_metadata(session_id, |metadata| {
            metadata.display_name = Some(name.clone());
        })?;
        self.append_event(session_id, TranscriptEvent::SessionRenamed { name })?;
        Ok(())
    }

    /// Replaces the stored slug for a session.
    pub fn set_slug(&self, session_id: Uuid, slug: Option<String>) -> Result<()> {
        self.update_metadata(session_id, |metadata| {
            metadata.slug = slug;
        })
    }

    /// Sets or clears a free-form note on a session.
    pub fn set_note(&self, session_id: Uuid, note: Option<String>) -> Result<()> {
        self.update_metadata(session_id, |metadata| {
            metadata.note = note;
        })
    }

    /// Adds a tag to a session if it is not already present.
    pub fn add_tag(&self, session_id: Uuid, tag: impl Into<String>) -> Result<()> {
        let tag = tag.into();
        self.update_metadata(session_id, |metadata| {
            if !metadata.tags.iter().any(|existing| existing == &tag) {
                metadata.tags.push(tag);
                metadata.tags.sort();
            }
        })
    }

    /// Removes a tag from a session if present.
    pub fn remove_tag(&self, session_id: Uuid, tag: &str) -> Result<()> {
        self.update_metadata(session_id, |metadata| {
            metadata.tags.retain(|existing| existing != tag);
        })
    }

    /// Loads a session metadata record and its transcript events from disk.
    pub fn load_session(&self, session_id: Uuid) -> Result<SessionRecord> {
        let path = self.session_path(session_id);
        let file: SessionFile = serde_json::from_slice(&fs::read(&path)?)?;
        let events = self.load_events(session_id)?;
        Ok(SessionRecord {
            metadata: file.metadata,
            events,
        })
    }

    /// Lists all sessions sorted by most recently updated first.
    pub fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
        let mut sessions = Vec::new();
        for entry in fs::read_dir(&self.root)
            .with_context(|| format!("failed to read session dir {}", self.root.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if !matches!(
                path.file_name().and_then(|value| value.to_str()),
                Some(name) if name.ends_with(".session.json")
            ) {
                continue;
            }
            let file: SessionFile = serde_json::from_slice(&fs::read(&path)?)?;
            let event_count = self
                .load_events(file.metadata.id)
                .map(|events| events.len())
                .unwrap_or(0);
            sessions.push(SessionSummary {
                id: file.metadata.id,
                display_name: file.metadata.display_name.clone(),
                cwd: file.metadata.cwd.clone(),
                created_at_ms: file.metadata.created_at_ms,
                updated_at_ms: file.metadata.updated_at_ms,
                event_count,
                parent_session_id: file.metadata.parent_session_id,
                slug: file.metadata.slug.clone(),
                tags: file.metadata.tags.clone(),
                note: file.metadata.note.clone(),
            });
        }
        sessions.sort_by(|left, right| right.updated_at_ms.cmp(&left.updated_at_ms));
        Ok(sessions)
    }

    /// Creates a new session by forking an existing session and copying its transcript.
    pub fn fork_session(&self, source_session_id: Uuid, cwd: PathBuf) -> Result<SessionMetadata> {
        let source = self.load_session(source_session_id)?;
        let now = unix_timestamp_ms();
        let metadata = SessionMetadata {
            id: Uuid::new_v4(),
            display_name: source
                .metadata
                .display_name
                .as_ref()
                .map(|name| format!("Fork of {name}")),
            cwd,
            created_at_ms: now,
            updated_at_ms: now,
            parent_session_id: Some(source_session_id),
            slug: Some(format!("session-{}", Uuid::new_v4().simple())),
            tags: source.metadata.tags.clone(),
            note: source.metadata.note.clone(),
        };

        let path = self.session_path(metadata.id);
        fs::write(
            &path,
            serde_json::to_vec(&SessionFile {
                metadata: metadata.clone(),
            })?,
        )?;
        let events_path = path.with_extension("jsonl");
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(events_path)?;
        for event in source.events {
            writeln!(file, "{}", serde_json::to_string(&event)?)?;
        }
        Ok(metadata)
    }

    fn load_events(&self, session_id: Uuid) -> Result<Vec<TranscriptEvent>> {
        let events_path = self.session_path(session_id).with_extension("jsonl");
        let mut events = Vec::new();
        if events_path.exists() {
            let reader = BufReader::new(fs::File::open(&events_path)?);
            for line in reader.lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                events.push(serde_json::from_str(&line)?);
            }
        }
        Ok(events)
    }

    fn touch_session(&self, session_id: Uuid) -> Result<()> {
        self.update_metadata(session_id, |metadata| {
            metadata.updated_at_ms = unix_timestamp_ms();
        })
    }

    fn update_metadata(
        &self,
        session_id: Uuid,
        updater: impl FnOnce(&mut SessionMetadata),
    ) -> Result<()> {
        let path = self.session_path(session_id);
        let mut file: SessionFile = serde_json::from_slice(&fs::read(&path)?)?;
        updater(&mut file.metadata);
        fs::write(&path, serde_json::to_vec(&file)?)?;
        Ok(())
    }

    fn session_path(&self, session_id: Uuid) -> PathBuf {
        self.root.join(format!("{session_id}.session.json"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionFile {
    metadata: SessionMetadata,
}

fn unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use puffer_config::ConfigPaths;
    use tempfile::tempdir;

    #[test]
    fn list_and_fork_sessions_work() {
        let tempdir = tempdir().unwrap();
        let paths = ConfigPaths::discover(tempdir.path());
        fs::create_dir_all(&paths.workspace_config_dir).unwrap();
        let store = SessionStore::from_paths(&paths).unwrap();

        let source = store.create_session(tempdir.path().join("src")).unwrap();
        store
            .append_event(
                source.id,
                TranscriptEvent::UserMessage {
                    text: "hello".to_string(),
                },
            )
            .unwrap();

        let fork = store
            .fork_session(source.id, tempdir.path().join("fork"))
            .unwrap();
        let listed = store.list_sessions().unwrap();

        assert_eq!(listed.len(), 2);
        assert!(listed.iter().any(|session| session.id == source.id));
        assert!(listed.iter().any(|session| session.id == fork.id));

        let fork_record = store.load_session(fork.id).unwrap();
        assert_eq!(fork_record.metadata.parent_session_id, Some(source.id));
        assert_eq!(fork_record.events.len(), 1);
    }

    #[test]
    fn session_tags_and_slug_can_be_updated() {
        let tempdir = tempdir().unwrap();
        let paths = ConfigPaths::discover(tempdir.path());
        fs::create_dir_all(&paths.workspace_config_dir).unwrap();
        let store = SessionStore::from_paths(&paths).unwrap();

        let session = store.create_session(tempdir.path().join("src")).unwrap();
        store.add_tag(session.id, "review").unwrap();
        store.add_tag(session.id, "auth").unwrap();
        store.remove_tag(session.id, "review").unwrap();
        store
            .set_slug(session.id, Some("custom-slug".to_string()))
            .unwrap();

        let loaded = store.load_session(session.id).unwrap();
        assert_eq!(loaded.metadata.slug.as_deref(), Some("custom-slug"));
        assert_eq!(loaded.metadata.tags, vec!["auth".to_string()]);

        let listed = store.list_sessions().unwrap();
        let summary = listed
            .into_iter()
            .find(|entry| entry.id == session.id)
            .unwrap();
        assert_eq!(summary.slug.as_deref(), Some("custom-slug"));
        assert_eq!(summary.tags, vec!["auth".to_string()]);
    }

    #[test]
    fn session_note_can_be_set_and_cleared() {
        let tempdir = tempdir().unwrap();
        let paths = ConfigPaths::discover(tempdir.path());
        fs::create_dir_all(&paths.workspace_config_dir).unwrap();
        let store = SessionStore::from_paths(&paths).unwrap();

        let session = store.create_session(tempdir.path().join("src")).unwrap();
        store
            .set_note(session.id, Some("important follow-up".to_string()))
            .unwrap();

        let loaded = store.load_session(session.id).unwrap();
        assert_eq!(loaded.metadata.note.as_deref(), Some("important follow-up"));

        let summary = store
            .list_sessions()
            .unwrap()
            .into_iter()
            .find(|entry| entry.id == session.id)
            .unwrap();
        assert_eq!(summary.note.as_deref(), Some("important follow-up"));

        store.set_note(session.id, None).unwrap();
        let cleared = store.load_session(session.id).unwrap();
        assert!(cleared.metadata.note.is_none());
    }
}
