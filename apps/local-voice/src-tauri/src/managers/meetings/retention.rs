//! M8 meetings: audio retention policy and the hard-delete cascade (Spec A2).
//!
//! `retention_until` is pure — it only decides *when* a meeting's audio
//! expires, never touches the filesystem or the store. `purge_due_audio` and
//! `delete_audio_files` do the actual I/O: the former sweeps the store for
//! meetings whose `audio_retention_until` is in the past (startup + the
//! `AfterMinutes` fast path right after a protocol is generated), the latter
//! is the shared file-deletion primitive also used by the soft-delete
//! cascade (a tombstoned meeting must never leave an orphaned WAV behind).

use log::warn;

use super::store::MeetingStore;

/// How long a meeting's audio survives after the meeting ends.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum MeetingAudioRetention {
    /// Delete the audio as soon as a minutes document exists for the
    /// meeting (the spec default — the transcript plus protocol are the
    /// durable record, the raw audio is not).
    AfterMinutes,
    /// Keep the audio for a fixed number of days after the meeting ended.
    Days(u32),
    /// Never delete the audio automatically.
    Forever,
}

const SECONDS_PER_DAY: i64 = 86_400;

/// Pure: determines the audio's expiry timestamp, or `None` if it must not
/// be deleted (yet). `AfterMinutes` only expires once a minutes document
/// exists — before that, `has_minutes` is `false` and the audio is kept
/// indefinitely, same as `Forever`.
pub fn retention_until(
    policy: &MeetingAudioRetention,
    now_unix: i64,
    ended_at: i64,
    has_minutes: bool,
) -> Option<i64> {
    match policy {
        MeetingAudioRetention::AfterMinutes => has_minutes.then_some(now_unix),
        MeetingAudioRetention::Days(days) => Some(ended_at + (*days as i64) * SECONDS_PER_DAY),
        MeetingAudioRetention::Forever => None,
    }
}

/// Hard-deletes every audio file whose meeting is due (`audio_retention_until
/// <= now_unix`) and nulls the paths + retention marker in the store so a
/// meeting is only ever swept once. Returns the number of files actually
/// deleted. Missing files (already gone) are not an error — the store state
/// is still cleared so the meeting stops showing up as due.
pub fn purge_due_audio(store: &MeetingStore, now_unix: i64) -> anyhow::Result<u32> {
    let due = store.meetings_with_due_audio(now_unix)?;
    let mut deleted = 0u32;

    for meeting in due {
        let mut paths = Vec::new();
        if let Some(mic) = &meeting.mic_audio_path {
            paths.push(mic.clone());
        }
        if let Some(system) = &meeting.system_audio_path {
            paths.push(system.clone());
        }

        deleted += delete_audio_files(&paths);

        if let Err(e) = store.set_audio_paths(&meeting.id, None, None, meeting.duration_ms) {
            warn!(
                "meetings: retention purge could not clear audio paths for {}: {e}",
                meeting.id
            );
        }
        if let Err(e) = store.set_retention_until(&meeting.id, None) {
            warn!(
                "meetings: retention purge could not clear retention marker for {}: {e}",
                meeting.id
            );
        }
    }

    Ok(deleted)
}

/// Hard-deletes the given audio files from disk. Used both by
/// `purge_due_audio` and directly by the soft-delete cascade (`meetings_delete`),
/// which already has the paths from `soft_delete_meeting`'s return value.
/// A file that is already missing is not an error — deleting is idempotent.
/// Returns the number of files actually removed.
pub fn delete_audio_files(paths: &[String]) -> u32 {
    let mut deleted = 0u32;
    for path in paths {
        match std::fs::remove_file(path) {
            Ok(()) => deleted += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => warn!("meetings: could not delete audio file {path}: {e}"),
        }
    }
    deleted
}

#[cfg(test)]
mod tests {
    use super::super::store::MeetingSource;
    use super::*;

    fn store() -> (MeetingStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meetings.db");
        let s = MeetingStore::open_at(&path).unwrap();
        (s, dir)
    }

    #[test]
    fn after_minutes_policy_expires_once_minutes_exist() {
        let p = MeetingAudioRetention::AfterMinutes;
        assert_eq!(retention_until(&p, 1_000, 900, false), None);
        assert_eq!(retention_until(&p, 1_000, 900, true), Some(1_000));
    }

    #[test]
    fn days_policy_counts_from_meeting_end() {
        let p = MeetingAudioRetention::Days(3);
        assert_eq!(
            retention_until(&p, 0, 1_000_000, false),
            Some(1_000_000 + 3 * 86_400)
        );
    }

    #[test]
    fn forever_never_expires() {
        assert_eq!(retention_until(&MeetingAudioRetention::Forever, 5, 1, true), None);
    }

    /// Regression for the review finding: a `Days(n)` expiry must stay
    /// anchored to the meeting's real `ended_at` (persisted via
    /// `set_ended_at`, mirroring what `stop()`/import do) even when it is
    /// recomputed much later — e.g. once minutes are generated well after
    /// the meeting ended. Recomputing with "now" instead of the stored
    /// `ended_at` would silently push the expiry further into the future
    /// every time it's recomputed.
    #[test]
    fn days_retention_stays_anchored_to_stored_ended_at_when_recomputed_later() {
        let (s, _dir) = store();
        let meeting = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();

        let ended_at = 1_000_000;
        s.set_ended_at(&meeting.id, ended_at).unwrap();

        let policy = MeetingAudioRetention::Days(3);
        let expected = Some(ended_at + 3 * 86_400);

        // Computed right at meeting end (mirrors `stop()`).
        let now_at_end = ended_at + 100;
        let until_at_end = retention_until(&policy, now_at_end, ended_at, false);
        s.set_retention_until(&meeting.id, until_at_end).unwrap();
        assert_eq!(until_at_end, expected);

        // Recomputed much later (mirrors minutes generated after a delay):
        // reading `ended_at` back from the store, not substituting "now".
        let now_much_later = ended_at + 500_000;
        let stored = s.get_meeting(&meeting.id).unwrap().unwrap();
        let until_later = retention_until(
            &policy,
            now_much_later,
            stored.ended_at.unwrap(),
            true,
        );
        assert_eq!(
            until_later, expected,
            "Days(n) expiry must not drift when recomputed later"
        );
    }

    #[test]
    fn purge_deletes_files_and_clears_paths() {
        let (s, dir) = store();
        let meeting = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();

        let mic_path = dir.path().join("mic.wav");
        let system_path = dir.path().join("system.wav");
        std::fs::write(&mic_path, b"RIFF....WAVEfmt ").unwrap();
        std::fs::write(&system_path, b"RIFF....WAVEfmt ").unwrap();

        s.set_audio_paths(
            &meeting.id,
            mic_path.to_str(),
            system_path.to_str(),
            Some(1_000),
        )
        .unwrap();
        // Due in the past relative to the `now_unix` passed to purge below.
        s.set_retention_until(&meeting.id, Some(500)).unwrap();

        let deleted = purge_due_audio(&s, 1_000).unwrap();
        assert_eq!(deleted, 2);
        assert!(!mic_path.exists());
        assert!(!system_path.exists());

        let stored = s.get_meeting(&meeting.id).unwrap().unwrap();
        assert_eq!(stored.mic_audio_path, None);
        assert_eq!(stored.system_audio_path, None);
    }

    #[test]
    fn soft_delete_cascade_removes_files_from_disk() {
        let (s, dir) = store();
        let meeting = s.create_meeting("T", MeetingSource::Live, Some(1)).unwrap();

        let mic_path = dir.path().join("mic.wav");
        std::fs::write(&mic_path, b"RIFF....WAVEfmt ").unwrap();
        s.set_audio_paths(&meeting.id, mic_path.to_str(), None, Some(1_000))
            .unwrap();

        let paths = s.soft_delete_meeting(&meeting.id).unwrap();
        let deleted = delete_audio_files(&paths);

        assert_eq!(deleted, 1);
        assert!(!mic_path.exists());
    }
}
