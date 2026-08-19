use anyhow::{anyhow, Result};
use chrono::Utc;
use log::{debug, info};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use ulid::Ulid;

/// Database migrations for the meetings store. One migration creates every
/// table for M8; later milestones (M9/M10) add migrations rather than
/// editing this one, matching the pattern in `history.rs`.
static MIGRATIONS: &[M] = &[M::up(
    "CREATE TABLE meetings (
      id TEXT PRIMARY KEY, title TEXT NOT NULL, status TEXT NOT NULL,
      source TEXT NOT NULL, started_at INTEGER, ended_at INTEGER, language TEXT,
      mic_audio_path TEXT, system_audio_path TEXT, duration_ms INTEGER,
      consent_confirmed_at INTEGER, audio_retention_until INTEGER,
      metadata_json TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
    CREATE TABLE meeting_documents (
      id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, kind TEXT NOT NULL, template_id TEXT,
      title TEXT, body_format TEXT NOT NULL, body TEXT NOT NULL,
      generation_metadata_json TEXT, version INTEGER NOT NULL DEFAULT 1,
      created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
    CREATE TABLE transcripts (
      id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL UNIQUE, provider TEXT, model TEXT, language TEXT,
      granularity TEXT NOT NULL DEFAULT 'segment@1', segments_json TEXT NOT NULL DEFAULT '[]',
      speaker_hints_json TEXT, content_revision INTEGER NOT NULL DEFAULT 0,
      created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
    CREATE TABLE transcript_deltas (
      transcript_id TEXT NOT NULL, sequence INTEGER NOT NULL, delta_json TEXT NOT NULL,
      created_at INTEGER NOT NULL, PRIMARY KEY (transcript_id, sequence));
    CREATE TABLE speakers (
      id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, channel INTEGER NOT NULL,
      speaker_index INTEGER, human_id TEXT, display_name TEXT, consent_state TEXT,
      created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
    CREATE TABLE humans (
      id TEXT PRIMARY KEY, name TEXT NOT NULL, email TEXT, memo TEXT,
      created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
    CREATE TABLE action_items (
      id TEXT PRIMARY KEY, meeting_id TEXT NOT NULL, text TEXT NOT NULL,
      assignee_human_id TEXT, due_at INTEGER, status TEXT NOT NULL DEFAULT 'todo',
      source TEXT NOT NULL, kind TEXT NOT NULL,
      created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);
    CREATE TABLE meeting_templates (
      id TEXT PRIMARY KEY, title TEXT NOT NULL, sections_json TEXT NOT NULL,
      pinned INTEGER NOT NULL DEFAULT 0,
      created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, deleted_at INTEGER);",
)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeetingSource {
    Live,
    Import,
    Subtitle,
}

impl MeetingSource {
    fn as_str(&self) -> &'static str {
        match self {
            MeetingSource::Live => "live",
            MeetingSource::Import => "import",
            MeetingSource::Subtitle => "subtitle",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeetingStatus {
    Recording,
    Processing,
    Ready,
    Failed,
}

impl MeetingStatus {
    fn as_str(&self) -> &'static str {
        match self {
            MeetingStatus::Recording => "recording",
            MeetingStatus::Processing => "processing",
            MeetingStatus::Ready => "ready",
            MeetingStatus::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub status: String,
    pub source: String,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub language: Option<String>,
    pub mic_audio_path: Option<String>,
    pub system_audio_path: Option<String>,
    pub duration_ms: Option<u64>,
    pub consent_confirmed_at: Option<i64>,
    pub audio_retention_until: Option<i64>,
    pub created_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct StoredSegment {
    pub segment_index: u32,
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub channel: u8, // 0=DirectMic, 1=RemoteParty, 2=MixedCapture
    pub speaker_index: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct TranscriptDelta {
    pub new_segments: Vec<StoredSegment>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MeetingDocument {
    pub id: String,
    pub meeting_id: String,
    pub kind: String,
    pub body_format: String,
    pub body: String,
    pub version: u32,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct MeetingTemplate {
    pub id: String,
    pub title: String,
    pub sections_json: String,
    pub pinned: bool,
}

pub struct MeetingStore {
    db_path: PathBuf,
}

impl MeetingStore {
    /// Opens (and, on first run, creates + migrates) `<appdata>/meetings/meetings.db`.
    pub fn new(app: &tauri::AppHandle) -> Result<Self> {
        let app_data_dir = crate::portable::app_data_dir(app)?;
        let meetings_dir = app_data_dir.join("meetings");
        if !meetings_dir.exists() {
            std::fs::create_dir_all(&meetings_dir)?;
            debug!("Created meetings directory: {:?}", meetings_dir);
        }
        let db_path = meetings_dir.join("meetings.db");
        Self::open_at(&db_path)
    }

    /// Opens a store at an explicit path, running migrations. Used directly by tests.
    pub fn open_at(path: &Path) -> Result<Self> {
        let store = Self {
            db_path: path.to_path_buf(),
        };
        store.init_database()?;
        store.seed_default_template()?;
        Ok(store)
    }

    fn init_database(&self) -> Result<()> {
        info!("Initializing meetings database at {:?}", self.db_path);
        let mut conn = Connection::open(&self.db_path)?;

        let migrations = Migrations::new(MIGRATIONS.to_vec());
        #[cfg(debug_assertions)]
        migrations.validate().expect("Invalid migrations");

        migrations.to_latest(&mut conn)?;
        Ok(())
    }

    fn seed_default_template(&self) -> Result<()> {
        let conn = self.get_connection()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM meeting_templates", [], |row| {
            row.get(0)
        })?;
        if count > 0 {
            return Ok(());
        }

        let sections = serde_json::json!([
            "summary",
            "scope",
            "speakers",
            "speaking_shares",
            "decisions",
            "tasks",
            "next_steps",
            "follow_ups",
            "open_questions"
        ])
        .to_string();
        let now = Utc::now().timestamp();
        conn.execute(
            "INSERT INTO meeting_templates (id, title, sections_json, pinned, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, ?4, ?4)",
            params![Ulid::new().to_string(), "Standardprotokoll", sections, now],
        )?;
        Ok(())
    }

    fn get_connection(&self) -> Result<Connection> {
        Ok(Connection::open(&self.db_path)?)
    }

    /// Guards write paths that key off a `meeting_id` but don't otherwise
    /// touch the `meetings` row (`append_delta`, `upsert_document`): without
    /// this, a typo'd id or a write arriving after `soft_delete_meeting`
    /// would silently create orphaned/invisible rows instead of failing.
    fn ensure_meeting_is_live(conn: &Connection, meeting_id: &str) -> Result<()> {
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM meetings WHERE id = ?1 AND deleted_at IS NULL)",
            params![meeting_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(anyhow!("Meeting {} not found", meeting_id));
        }
        Ok(())
    }

    fn map_meeting(row: &rusqlite::Row<'_>) -> rusqlite::Result<Meeting> {
        Ok(Meeting {
            id: row.get("id")?,
            title: row.get("title")?,
            status: row.get("status")?,
            source: row.get("source")?,
            started_at: row.get("started_at")?,
            ended_at: row.get("ended_at")?,
            language: row.get("language")?,
            mic_audio_path: row.get("mic_audio_path")?,
            system_audio_path: row.get("system_audio_path")?,
            duration_ms: row.get::<_, Option<i64>>("duration_ms")?.map(|v| v as u64),
            consent_confirmed_at: row.get("consent_confirmed_at")?,
            audio_retention_until: row.get("audio_retention_until")?,
            created_at: row.get("created_at")?,
            deleted_at: row.get("deleted_at")?,
        })
    }

    pub fn create_meeting(
        &self,
        title: &str,
        source: MeetingSource,
        consent_confirmed_at: Option<i64>,
    ) -> Result<Meeting> {
        let status = match source {
            MeetingSource::Live => MeetingStatus::Recording,
            MeetingSource::Import | MeetingSource::Subtitle => MeetingStatus::Processing,
        };

        let id = Ulid::new().to_string();
        let now = Utc::now().timestamp();
        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO meetings (id, title, status, source, consent_confirmed_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![id, title, status.as_str(), source.as_str(), consent_confirmed_at, now],
        )?;

        Ok(Meeting {
            id,
            title: title.to_string(),
            status: status.as_str().to_string(),
            source: source.as_str().to_string(),
            started_at: None,
            ended_at: None,
            language: None,
            mic_audio_path: None,
            system_audio_path: None,
            duration_ms: None,
            consent_confirmed_at,
            audio_retention_until: None,
            created_at: now,
            deleted_at: None,
        })
    }

    pub fn set_status(&self, id: &str, status: MeetingStatus) -> Result<()> {
        let conn = self.get_connection()?;
        let now = Utc::now().timestamp();
        let updated = conn.execute(
            "UPDATE meetings SET status = ?1, updated_at = ?2 WHERE id = ?3 AND deleted_at IS NULL",
            params![status.as_str(), now, id],
        )?;
        if updated == 0 {
            return Err(anyhow!("Meeting {} not found", id));
        }
        Ok(())
    }

    pub fn set_audio_paths(
        &self,
        id: &str,
        mic: Option<&str>,
        system: Option<&str>,
        duration_ms: Option<u64>,
    ) -> Result<()> {
        let conn = self.get_connection()?;
        let now = Utc::now().timestamp();
        let updated = conn.execute(
            "UPDATE meetings SET mic_audio_path = ?1, system_audio_path = ?2, duration_ms = ?3, updated_at = ?4
             WHERE id = ?5 AND deleted_at IS NULL",
            params![mic, system, duration_ms.map(|v| v as i64), now, id],
        )?;
        if updated == 0 {
            return Err(anyhow!("Meeting {} not found", id));
        }
        Ok(())
    }

    /// Sets (or clears) the audio expiry timestamp computed by
    /// `retention::retention_until`. Task 12.
    pub fn set_retention_until(&self, id: &str, until: Option<i64>) -> Result<()> {
        let conn = self.get_connection()?;
        let now = Utc::now().timestamp();
        let updated = conn.execute(
            "UPDATE meetings SET audio_retention_until = ?1, updated_at = ?2
             WHERE id = ?3 AND deleted_at IS NULL",
            params![until, now, id],
        )?;
        if updated == 0 {
            return Err(anyhow!("Meeting {} not found", id));
        }
        Ok(())
    }

    /// Meetings whose audio is due for hard-deletion: not soft-deleted (that
    /// cascade already hard-deletes on its own path) and past their
    /// `audio_retention_until`. Task 12 (`retention::purge_due_audio`).
    pub fn meetings_with_due_audio(&self, now_unix: i64) -> Result<Vec<Meeting>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, status, source, started_at, ended_at, language,
                    mic_audio_path, system_audio_path, duration_ms, consent_confirmed_at,
                    audio_retention_until, created_at, deleted_at
             FROM meetings
             WHERE deleted_at IS NULL
               AND audio_retention_until IS NOT NULL
               AND audio_retention_until <= ?1",
        )?;
        let meetings = stmt
            .query_map(params![now_unix], Self::map_meeting)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(meetings)
    }

    pub fn get_meeting(&self, id: &str) -> Result<Option<Meeting>> {
        let conn = self.get_connection()?;
        let meeting = conn
            .query_row(
                "SELECT id, title, status, source, started_at, ended_at, language,
                        mic_audio_path, system_audio_path, duration_ms, consent_confirmed_at,
                        audio_retention_until, created_at, deleted_at
                 FROM meetings WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                Self::map_meeting,
            )
            .optional()?;
        Ok(meeting)
    }

    pub fn list_meetings(&self, offset: u32, limit: u32) -> Result<Vec<Meeting>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, status, source, started_at, ended_at, language,
                    mic_audio_path, system_audio_path, duration_ms, consent_confirmed_at,
                    audio_retention_until, created_at, deleted_at
             FROM meetings WHERE deleted_at IS NULL
             ORDER BY created_at DESC
             LIMIT ?1 OFFSET ?2",
        )?;
        let meetings = stmt
            .query_map(params![limit, offset], Self::map_meeting)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(meetings)
    }

    /// Soft-deletes a meeting and all of its child rows. Returns the audio
    /// file paths that existed on the meeting so the caller can delete them
    /// from disk (this store never touches the filesystem itself).
    pub fn soft_delete_meeting(&self, id: &str) -> Result<Vec<String>> {
        let mut conn = self.get_connection()?;
        let now = Utc::now().timestamp();

        let tx = conn.transaction()?;

        let paths: Option<(Option<String>, Option<String>)> = tx
            .query_row(
                "SELECT mic_audio_path, system_audio_path FROM meetings WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        let Some((mic, system)) = paths else {
            return Err(anyhow!("Meeting {} not found", id));
        };

        tx.execute(
            "UPDATE meetings SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
        tx.execute(
            "UPDATE meeting_documents SET deleted_at = ?1, updated_at = ?1 WHERE meeting_id = ?2",
            params![now, id],
        )?;
        tx.execute(
            "UPDATE transcripts SET deleted_at = ?1, updated_at = ?1 WHERE meeting_id = ?2",
            params![now, id],
        )?;
        tx.execute(
            "UPDATE speakers SET deleted_at = ?1, updated_at = ?1 WHERE meeting_id = ?2",
            params![now, id],
        )?;
        tx.execute(
            "UPDATE action_items SET deleted_at = ?1, updated_at = ?1 WHERE meeting_id = ?2",
            params![now, id],
        )?;

        tx.commit()?;

        let mut audio_paths = Vec::new();
        if let Some(mic) = mic {
            audio_paths.push(mic);
        }
        if let Some(system) = system {
            audio_paths.push(system);
        }
        Ok(audio_paths)
    }

    /// Appends a transcript delta in a single transaction: computes the next
    /// sequence number, persists the raw delta, and materializes the new
    /// segments into `transcripts.segments_json` so `get_segments` stays a
    /// simple read and later crash-replay can diff deltas against it.
    pub fn append_delta(&self, meeting_id: &str, delta: &TranscriptDelta) -> Result<u64> {
        let mut conn = self.get_connection()?;
        let now = Utc::now().timestamp();
        let tx = conn.transaction()?;

        Self::ensure_meeting_is_live(&tx, meeting_id)?;

        // Lazily create the transcript row on first delta.
        let transcript_id: Option<String> = tx
            .query_row(
                "SELECT id FROM transcripts WHERE meeting_id = ?1",
                params![meeting_id],
                |row| row.get(0),
            )
            .optional()?;

        let transcript_id = match transcript_id {
            Some(id) => id,
            None => {
                let id = Ulid::new().to_string();
                tx.execute(
                    "INSERT INTO transcripts (id, meeting_id, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3)",
                    params![id, meeting_id, now],
                )?;
                id
            }
        };

        let next_sequence: i64 = tx.query_row(
            "SELECT 1 + COALESCE(MAX(sequence), 0) FROM transcript_deltas WHERE transcript_id = ?1",
            params![transcript_id],
            |row| row.get(0),
        )?;

        let delta_json = serde_json::to_string(delta)?;
        tx.execute(
            "INSERT INTO transcript_deltas (transcript_id, sequence, delta_json, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![transcript_id, next_sequence, delta_json, now],
        )?;

        let existing_json: String = tx.query_row(
            "SELECT segments_json FROM transcripts WHERE id = ?1",
            params![transcript_id],
            |row| row.get(0),
        )?;
        let mut segments: Vec<StoredSegment> = serde_json::from_str(&existing_json)?;
        segments.extend(delta.new_segments.iter().cloned());

        let segments_json = serde_json::to_string(&segments)?;
        tx.execute(
            "UPDATE transcripts SET segments_json = ?1, content_revision = content_revision + 1, updated_at = ?2
             WHERE id = ?3",
            params![segments_json, now, transcript_id],
        )?;

        tx.commit()?;
        Ok(next_sequence as u64)
    }

    pub fn get_segments(&self, meeting_id: &str) -> Result<Vec<StoredSegment>> {
        let conn = self.get_connection()?;
        let segments_json: Option<String> = conn
            .query_row(
                "SELECT segments_json FROM transcripts WHERE meeting_id = ?1 AND deleted_at IS NULL",
                params![meeting_id],
                |row| row.get(0),
            )
            .optional()?;

        match segments_json {
            Some(json) => Ok(serde_json::from_str(&json)?),
            None => Ok(Vec::new()),
        }
    }

    pub fn update_segment_text(
        &self,
        meeting_id: &str,
        segment_index: u32,
        text: &str,
    ) -> Result<()> {
        let conn = self.get_connection()?;
        let now = Utc::now().timestamp();

        let (transcript_id, segments_json): (String, String) = conn
            .query_row(
                "SELECT id, segments_json FROM transcripts WHERE meeting_id = ?1 AND deleted_at IS NULL",
                params![meeting_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| anyhow!("No transcript for meeting {}", meeting_id))?;

        let mut segments: Vec<StoredSegment> = serde_json::from_str(&segments_json)?;
        let segment = segments
            .iter_mut()
            .find(|s| s.segment_index == segment_index)
            .ok_or_else(|| {
                anyhow!(
                    "Segment {} not found for meeting {}",
                    segment_index,
                    meeting_id
                )
            })?;
        segment.text = text.to_string();

        let updated_json = serde_json::to_string(&segments)?;
        conn.execute(
            "UPDATE transcripts SET segments_json = ?1, content_revision = content_revision + 1, updated_at = ?2
             WHERE id = ?3",
            params![updated_json, now, transcript_id],
        )?;
        Ok(())
    }

    pub fn upsert_document(
        &self,
        meeting_id: &str,
        kind: &str,
        body_format: &str,
        body: &str,
        generation_metadata: Option<&str>,
    ) -> Result<String> {
        let conn = self.get_connection()?;
        let now = Utc::now().timestamp();

        Self::ensure_meeting_is_live(&conn, meeting_id)?;

        let current_max_version: Option<i64> = conn
            .query_row(
                "SELECT MAX(version) FROM meeting_documents WHERE meeting_id = ?1 AND kind = ?2 AND deleted_at IS NULL",
                params![meeting_id, kind],
                |row| row.get(0),
            )
            .optional()?
            .flatten();

        let version = current_max_version.unwrap_or(0) + 1;
        let id = Ulid::new().to_string();
        conn.execute(
            "INSERT INTO meeting_documents (id, meeting_id, kind, body_format, body, generation_metadata_json, version, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![id, meeting_id, kind, body_format, body, generation_metadata, version, now],
        )?;
        Ok(id)
    }

    pub fn get_documents(&self, meeting_id: &str) -> Result<Vec<MeetingDocument>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, meeting_id, kind, body_format, body, version, created_at
             FROM meeting_documents WHERE meeting_id = ?1 AND deleted_at IS NULL
             ORDER BY version ASC",
        )?;
        let docs = stmt
            .query_map(params![meeting_id], |row| {
                Ok(MeetingDocument {
                    id: row.get("id")?,
                    meeting_id: row.get("meeting_id")?,
                    kind: row.get("kind")?,
                    body_format: row.get("body_format")?,
                    body: row.get("body")?,
                    version: row.get::<_, i64>("version")? as u32,
                    created_at: row.get("created_at")?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(docs)
    }

    pub fn list_templates(&self) -> Result<Vec<MeetingTemplate>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, title, sections_json, pinned FROM meeting_templates
             WHERE deleted_at IS NULL
             ORDER BY created_at ASC",
        )?;
        let templates = stmt
            .query_map([], |row| {
                Ok(MeetingTemplate {
                    id: row.get("id")?,
                    title: row.get("title")?,
                    sections_json: row.get("sections_json")?,
                    pinned: row.get("pinned")?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(templates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> MeetingStore {
        // open_at mit Tempdir-Datei — In-Memory geht nicht, weil der Store pro Aufruf öffnet (History-Muster)
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meetings.db");
        let s = MeetingStore::open_at(&path).unwrap();
        std::mem::forget(dir); // Tempdir bis Prozessende behalten
        s
    }

    #[test]
    fn a_meeting_without_consent_timestamp_is_storable_but_marked() {
        let s = store();
        let m = s
            .create_meeting("Jour fixe", MeetingSource::Import, None)
            .unwrap();
        assert!(m.consent_confirmed_at.is_none());
        assert_eq!(m.status, "processing");
    }

    #[test]
    fn live_meetings_start_in_recording_state_with_consent() {
        let s = store();
        let m = s
            .create_meeting("Standup", MeetingSource::Live, Some(1_755_600_000))
            .unwrap();
        assert_eq!(m.status, "recording");
        assert_eq!(m.consent_confirmed_at, Some(1_755_600_000));
    }

    #[test]
    fn deltas_are_sequenced_and_segments_materialize_in_order() {
        let s = store();
        let m = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();
        let d1 = TranscriptDelta {
            new_segments: vec![StoredSegment {
                segment_index: 0,
                text: "Hallo.".into(),
                start_ms: 0,
                end_ms: 900,
                channel: 0,
                speaker_index: None,
            }],
        };
        let d2 = TranscriptDelta {
            new_segments: vec![StoredSegment {
                segment_index: 1,
                text: "Guten Morgen.".into(),
                start_ms: 950,
                end_ms: 2100,
                channel: 1,
                speaker_index: None,
            }],
        };
        assert_eq!(s.append_delta(&m.id, &d1).unwrap(), 1);
        assert_eq!(s.append_delta(&m.id, &d2).unwrap(), 2);
        let segs = s.get_segments(&m.id).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[1].text, "Guten Morgen.");
        assert_eq!(segs[1].channel, 1);
    }

    #[test]
    fn segment_text_can_be_corrected() {
        let s = store();
        let m = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();
        s.append_delta(
            &m.id,
            &TranscriptDelta {
                new_segments: vec![StoredSegment {
                    segment_index: 0,
                    text: "Falsch erkannt".into(),
                    start_ms: 0,
                    end_ms: 800,
                    channel: 0,
                    speaker_index: None,
                }],
            },
        )
        .unwrap();
        s.update_segment_text(&m.id, 0, "Richtig erkannt").unwrap();
        assert_eq!(s.get_segments(&m.id).unwrap()[0].text, "Richtig erkannt");
    }

    #[test]
    fn soft_delete_hides_the_meeting_and_returns_audio_paths() {
        let s = store();
        let m = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();
        s.set_audio_paths(
            &m.id,
            Some("C:/x/mic.wav"),
            Some("C:/x/system.wav"),
            Some(60_000),
        )
        .unwrap();
        let paths = s.soft_delete_meeting(&m.id).unwrap();
        assert_eq!(
            paths,
            vec!["C:/x/mic.wav".to_string(), "C:/x/system.wav".to_string()]
        );
        assert!(s.list_meetings(0, 50).unwrap().is_empty());
        assert!(s.get_meeting(&m.id).unwrap().is_none());
    }

    #[test]
    fn the_default_template_is_seeded_once() {
        let s = store();
        let t = s.list_templates().unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].title, "Standardprotokoll");
        // Alle Spec-Sektionen enthalten:
        for key in [
            "summary",
            "scope",
            "decisions",
            "tasks",
            "next_steps",
            "follow_ups",
            "open_questions",
        ] {
            assert!(
                t[0].sections_json.contains(key),
                "Sektion {key} fehlt im Seed"
            );
        }
    }

    #[test]
    fn documents_version_instead_of_overwrite() {
        let s = store();
        let m = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();
        s.upsert_document(&m.id, "minutes", "markdown@1", "# V1", None)
            .unwrap();
        s.upsert_document(&m.id, "minutes", "markdown@1", "# V2", None)
            .unwrap();
        let docs = s.get_documents(&m.id).unwrap();
        assert_eq!(
            docs.len(),
            2,
            "Regenerieren erzeugt neue Version statt Überschreiben (Spec M10-Vorgriff)"
        );
        assert_eq!(docs.iter().map(|d| d.version).max(), Some(2));
    }

    #[test]
    fn append_delta_to_a_deleted_or_unknown_meeting_is_an_error() {
        let s = store();
        let m = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();
        s.soft_delete_meeting(&m.id).unwrap();

        let delta = TranscriptDelta {
            new_segments: vec![StoredSegment {
                segment_index: 0,
                text: "Zu spät.".into(),
                start_ms: 0,
                end_ms: 500,
                channel: 0,
                speaker_index: None,
            }],
        };
        assert!(s.append_delta(&m.id, &delta).is_err());
        assert!(s.append_delta(&Ulid::new().to_string(), &delta).is_err());
    }

    #[test]
    fn upsert_document_requires_a_live_meeting() {
        let s = store();
        let m = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();
        s.soft_delete_meeting(&m.id).unwrap();

        assert!(s
            .upsert_document(&m.id, "minutes", "markdown@1", "# V1", None)
            .is_err());
        assert!(s
            .upsert_document(
                &Ulid::new().to_string(),
                "minutes",
                "markdown@1",
                "# V1",
                None
            )
            .is_err());
    }
}
