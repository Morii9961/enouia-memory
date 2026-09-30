//! The index projection (MV-4.1): `indexes/memory.sqlite`.
//!
//! A disposable, rebuildable projection of the Vault. Each memory revision
//! is one row with the commit sequences between which the Vault knew it as
//! the record's latest revision, so one index answers "what did the system
//! know at commit k" (`known_at`) as well as the head. The watermark (commit
//! ID and sequence) advances with the commits of each SQLite transaction
//! (up to 256), so a crash or cancellation leaves a consistent, merely
//! older index. Content named
//! by a purge tombstone is removed from the rows and both full-text indexes,
//! the FTS segments are merged so no deleted term survives, and the file is
//! vacuumed with `secure_delete` on. Every failure of the index is reported
//! as `index_not_ready`; it never touches the Vault.

use crate::fold::{FOLD_VERSION, fold};
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::catalog::StoredCommit;
use enouia_memory_contract::commit::{DeleteMode, Tombstone};
use enouia_memory_contract::ids::CommitId;
use enouia_memory_contract::memory::{CanonicalMemory, MemoryBody, ProjectEntity};
use enouia_memory_contract::ports::CommitPin;
use enouia_memory_contract::record::{Record, RecordKind, RecordRef, parse_record};
use enouia_memory_vault::{Fault, Vault, VaultError};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;
use std::sync::Arc;

pub type Result<T> = std::result::Result<T, VaultError>;

pub const INDEX_FORMAT: &str = "enouia-index/1";
/// Commits applied per SQLite transaction; cancellation is checked between.
pub const COMMITS_PER_TRANSACTION: usize = 256;
pub const INDEX_FILE: &str = enouia_memory_contract::layout::INDEX_FILE;

pub(crate) fn not_ready(what: &'static str) -> VaultError {
    VaultError::new(MemoryErrorCode::IndexNotReady, Fault::Corrupt(what))
}

pub(crate) fn db(_: rusqlite::Error) -> VaultError {
    not_ready("index database")
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS commits(sequence INTEGER PRIMARY KEY, commit_id TEXT NOT NULL UNIQUE);
CREATE TABLE IF NOT EXISTS memories(
    row INTEGER PRIMARY KEY,
    memory_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    known_from INTEGER NOT NULL,
    known_until INTEGER,
    type TEXT NOT NULL,
    project_id TEXT,
    folded TEXT NOT NULL,
    record TEXT NOT NULL,
    UNIQUE(memory_id, revision));
CREATE INDEX IF NOT EXISTS memories_known ON memories(known_from, known_until);
CREATE INDEX IF NOT EXISTS memories_project ON memories(project_id);
CREATE VIRTUAL TABLE IF NOT EXISTS memories_tri USING fts5(
    folded, content='memories', content_rowid='row', tokenize='trigram');
CREATE VIRTUAL TABLE IF NOT EXISTS memories_word USING fts5(
    folded, content='memories', content_rowid='row', tokenize='unicode61');
CREATE TABLE IF NOT EXISTS projects(
    project_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    known_from INTEGER NOT NULL,
    known_until INTEGER,
    names TEXT NOT NULL,
    record TEXT NOT NULL,
    PRIMARY KEY(project_id, revision));
CREATE TABLE IF NOT EXISTS tombstones(
    delete_id TEXT PRIMARY KEY,
    known_from INTEGER NOT NULL,
    record TEXT NOT NULL);
";

/// Where the index stands.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Watermark {
    pub commit_id: CommitId,
    pub sequence: u64,
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
}

/// What one `update` call did.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UpdateReport {
    pub commits_applied: u64,
    pub revisions_indexed: u64,
    pub rows_purged: u64,
    /// False when cancelled before reaching the head.
    pub reached_head: bool,
}

pub struct Index {
    pub(crate) conn: Connection,
    path: PathBuf,
}

fn index_path(vault: &Vault) -> Result<PathBuf> {
    vault.managed_root().ensure_dir("indexes")?;
    let mut path = vault.managed_root().root().to_path_buf();
    path.extend(INDEX_FILE.split('/'));
    Ok(path)
}

/// Text a memory revision is found by: what the owner reads (title,
/// content, tags, category, and the type's descriptive fields), folded.
/// Internal keys (claim keys, preference scopes) are metadata, not text: in
/// the full text they would match words the owner never wrote.
fn searchable(memory: &CanonicalMemory) -> String {
    let mut parts = vec![memory.title.clone(), memory.content.clone()];
    parts.extend(memory.tags.iter().cloned());
    parts.extend(memory.category.iter().cloned());
    match &memory.body {
        MemoryBody::Fact(_) | MemoryBody::Preference(_) => {}
        MemoryBody::Episode(e) => parts.push(e.summary.clone()),
        MemoryBody::ProjectState(p) => {
            parts.extend(p.state.iter().chain(&p.decisions).map(|i| i.claim.clone()));
            parts.extend(p.open_loops.iter().map(|l| l.description.clone()));
        }
        MemoryBody::SessionCheckpoint(s) => parts.push(s.last_state.clone()),
    }
    fold(&parts.join("\n"))
}

fn type_name(memory: &CanonicalMemory) -> String {
    serde_json::to_value(memory.memory_type())
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

impl Index {
    /// Open the index of this Vault, creating an empty one if absent. An
    /// index of another Vault, format, or folding version, or one that
    /// fails SQLite's quick check, is refused (`index_not_ready`): rebuild.
    pub fn open(vault: &Vault) -> Result<Self> {
        let path = index_path(vault)?;
        let conn = Connection::open(&path).map_err(db)?;
        conn.execute_batch(
            "PRAGMA journal_mode = DELETE; PRAGMA secure_delete = ON; PRAGMA synchronous = FULL;",
        )
        .map_err(db)?;
        let check: String = conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(db)?;
        if check != "ok" {
            return Err(not_ready("index failed its integrity check"));
        }
        conn.execute_batch(SCHEMA).map_err(db)?;
        let index = Self { conn, path };
        let expected = [
            ("format", INDEX_FORMAT.to_owned()),
            ("fold", FOLD_VERSION.to_owned()),
            ("vault_id", vault.vault_id().to_string()),
        ];
        for (key, value) in expected {
            match index.meta(key)? {
                None => index.set_meta(key, &value)?,
                Some(found) if found == value => {}
                Some(_) => return Err(not_ready("index of another vault or version")),
            }
        }
        Ok(index)
    }

    /// Delete the index file and build it again from the Vault's history.
    pub fn rebuild(vault: &Vault) -> Result<(Self, UpdateReport)> {
        let path = index_path(vault)?;
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let file = PathBuf::from(format!("{}{suffix}", path.display()));
            match std::fs::remove_file(&file) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        let mut index = Self::open(vault)?;
        let report = index.update(vault, &|| false)?;
        Ok((index, report))
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    fn meta(&self, key: &str) -> Result<Option<String>> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(db)
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO meta(key, value) VALUES(?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [key, value],
            )
            .map(|_| ())
            .map_err(db)
    }

    pub fn watermark(&self) -> Result<Option<Watermark>> {
        let Some(commit) = self.meta("commit_id")? else {
            return Ok(None);
        };
        let number = |key: &str| -> Result<u64> {
            self.meta(key)?
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| not_ready("index watermark"))
        };
        Ok(Some(Watermark {
            commit_id: CommitId::parse(&commit).map_err(|_| not_ready("index watermark"))?,
            sequence: number("sequence")?,
            policy_epoch: number("policy_epoch")?,
            deletion_epoch: number("deletion_epoch")?,
        }))
    }

    /// The sequence of a commit the index has applied, if any.
    pub fn sequence_of(&self, commit_id: &CommitId) -> Result<Option<u64>> {
        self.conn
            .query_row(
                "SELECT sequence FROM commits WHERE commit_id = ?1",
                [commit_id.as_str()],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map(|s| s.map(|s| s as u64))
            .map_err(db)
    }

    /// Apply every commit after the watermark up to the Vault's head, one
    /// transaction per commit. `cancel` is checked between commits.
    pub fn update(&mut self, vault: &Vault, cancel: &dyn Fn() -> bool) -> Result<UpdateReport> {
        let pin = vault.pin_current()?;
        let head = vault.stored_commit(&pin)?;
        let mark = self.watermark()?;
        let from = mark.as_ref().map_or(0, |m| m.sequence);
        if from > head.sequence {
            return Err(not_ready("index ahead of the vault"));
        }
        // The commits after the watermark, oldest first; the watermark
        // itself must be on the head's chain (a recovery may have adopted
        // another one).
        let mut chain: Vec<Arc<StoredCommit>> = Vec::new();
        let mut current = head.clone();
        while current.sequence > from {
            let parent = current.parent_commit_id.clone();
            chain.push(current.clone());
            match parent {
                Some(id) => {
                    current = vault.stored_commit(&CommitPin {
                        commit_id: id,
                        sequence: chain.last().expect("pushed").sequence - 1,
                        policy_epoch: 0,
                        deletion_epoch: 0,
                    })?;
                }
                None => break,
            }
        }
        if let Some(mark) = &mark
            && (current.sequence != mark.sequence || current.commit_id != mark.commit_id)
        {
            return Err(not_ready("index on another chain"));
        }
        chain.reverse();
        let mut report = UpdateReport::default();
        let mut purged = false;
        // Several commits per transaction (the watermark moves with them),
        // so a rebuild does not pay one synchronous flush per commit.
        for chunk in chain.chunks(COMMITS_PER_TRANSACTION) {
            if cancel() {
                return Ok(report);
            }
            let tx = self.conn.transaction().map_err(db)?;
            for commit in chunk {
                let sequence = commit.sequence as i64;
                for reference in &commit.receipt.records {
                    match reference.record_kind {
                        RecordKind::Memory => {
                            if let Some((memory, text)) =
                                read::<CanonicalMemory>(vault, &pin, reference)?
                            {
                                tx.execute(
                                    "UPDATE memories SET known_until = ?1
                                 WHERE memory_id = ?2 AND known_until IS NULL",
                                    params![sequence, reference.record_id],
                                )
                                .map_err(db)?;
                                let folded = searchable(&memory);
                                tx.execute(
                                    "INSERT INTO memories(memory_id, revision, known_from, type,
                                     project_id, folded, record)
                                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                                    params![
                                        reference.record_id,
                                        reference.revision.get() as i64,
                                        sequence,
                                        type_name(&memory),
                                        memory.project_id.as_ref().map(|p| p.to_string()),
                                        folded,
                                        text,
                                    ],
                                )
                                .map_err(db)?;
                                let row = tx.last_insert_rowid();
                                for table in ["memories_tri", "memories_word"] {
                                    tx.execute(
                                        &format!(
                                            "INSERT INTO {table}(rowid, folded) VALUES(?1, ?2)"
                                        ),
                                        params![row, folded],
                                    )
                                    .map_err(db)?;
                                }
                                report.revisions_indexed += 1;
                            }
                        }
                        RecordKind::Project => {
                            if let Some((project, text)) =
                                read::<ProjectEntity>(vault, &pin, reference)?
                            {
                                tx.execute(
                                    "UPDATE projects SET known_until = ?1
                                 WHERE project_id = ?2 AND known_until IS NULL",
                                    params![sequence, reference.record_id],
                                )
                                .map_err(db)?;
                                tx.execute(
                                "INSERT INTO projects(project_id, revision, known_from, names, record)
                                 VALUES(?1, ?2, ?3, ?4, ?5)",
                                params![
                                    reference.record_id,
                                    reference.revision.get() as i64,
                                    sequence,
                                    serde_json::to_string(&project.names()).expect("names"),
                                    text,
                                ],
                            )
                            .map_err(db)?;
                            }
                        }
                        RecordKind::Tombstone => {
                            if let Some((tombstone, text)) =
                                read::<Tombstone>(vault, &pin, reference)?
                            {
                                tx.execute(
                                    "INSERT INTO tombstones(delete_id, known_from, record)
                                 VALUES(?1, ?2, ?3)",
                                    params![reference.record_id, sequence, text],
                                )
                                .map_err(db)?;
                                if tombstone.mode == DeleteMode::Purge {
                                    report.rows_purged += purge_rows(&tx, &tombstone)?;
                                    purged = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                tx.execute(
                    "INSERT INTO commits(sequence, commit_id) VALUES(?1, ?2)",
                    params![sequence, commit.commit_id.as_str()],
                )
                .map_err(db)?;
                for (key, value) in [
                    ("commit_id", commit.commit_id.to_string()),
                    ("sequence", commit.sequence.to_string()),
                    ("policy_epoch", commit.policy_epoch.to_string()),
                    ("deletion_epoch", commit.deletion_epoch.to_string()),
                ] {
                    tx.execute(
                        "INSERT INTO meta(key, value) VALUES(?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                        [key, value.as_str()],
                    )
                    .map_err(db)?;
                }
                report.commits_applied += 1;
            }
            tx.commit().map_err(db)?;
        }
        if purged {
            // Deleted FTS terms live on in older segments until they are
            // merged; then free pages are zeroed and the file shrinks.
            self.conn
                .execute_batch(
                    "INSERT INTO memories_tri(memories_tri) VALUES('optimize');
                     INSERT INTO memories_word(memories_word) VALUES('optimize');
                     VACUUM;",
                )
                .map_err(db)?;
        }
        report.reached_head = true;
        Ok(report)
    }
}

/// Parse one revision the head names; purged content reads as absent.
fn read<T: Record>(
    vault: &Vault,
    pin: &CommitPin,
    reference: &RecordRef,
) -> Result<Option<(T, String)>> {
    match vault.read_revision(pin, reference) {
        Ok(bytes) => {
            let record = parse_record(&bytes).map_err(|_| VaultError::corrupt("record"))?;
            let text = String::from_utf8(bytes).map_err(|_| VaultError::corrupt("record"))?;
            Ok(Some((record, text)))
        }
        Err(e) if e.code() == MemoryErrorCode::IntentionallyPurged => Ok(None),
        Err(e) => Err(e),
    }
}

/// Remove every row (and its FTS entries) a purge tombstone names.
fn purge_rows(tx: &rusqlite::Transaction<'_>, tombstone: &Tombstone) -> Result<u64> {
    let mut removed = 0;
    for target in tombstone
        .targets
        .iter()
        .filter(|t| t.record_kind == RecordKind::Memory)
    {
        let rows: Vec<(i64, String)> = {
            let mut statement = tx
                .prepare(
                    "SELECT row, folded FROM memories
                     WHERE memory_id = ?1 AND (?2 IS NULL OR revision = ?2)",
                )
                .map_err(db)?;
            statement
                .query_map(
                    params![target.record_id, target.revision.map(|r| r.get() as i64)],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .map_err(db)?
                .collect::<std::result::Result<_, _>>()
                .map_err(db)?
        };
        for (row, folded) in rows {
            for table in ["memories_tri", "memories_word"] {
                tx.execute(
                    &format!(
                        "INSERT INTO {table}({table}, rowid, folded) VALUES('delete', ?1, ?2)"
                    ),
                    params![row, folded],
                )
                .map_err(db)?;
            }
            tx.execute("DELETE FROM memories WHERE row = ?1", [row])
                .map_err(db)?;
            removed += 1;
        }
    }
    Ok(removed)
}
