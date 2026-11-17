//! An LMDB environment that the processes of one host share, for entries a caller can derive again: accio's forward
//! store (decision 2026-11-06-forward-edges-in-lmdb in accio's `docs/decisions/`) and Branch Deck's object cache, whose
//! copies of these rules diverged until they moved here (decision 2026-11-09-lmdb-env-in-psi-syntax). The rules:
//!
//! - **The map size caps the environment, so a full map clears the environment**, not the tables of the store that
//!   writes: a write that would take the live pages of every table past `FULL` of the map, or meets `MDB_MAP_FULL`,
//!   clears every table and is retried once. Every entry is derived, so a clear costs its derivation and nothing else.
//!   The tables a live store of this process holds are emptied, every other one is dropped: those of another version
//!   or another layout of the record, which another binary wrote, held the pages a clear of the writer's own tables
//!   left, and each batch after it was lost (accio's issue #861). The clear is a transaction per table, in steps where
//!   the map has no room for one (`empty_table`), and one more, so the retried batch reuses the freed pages
//!   (`Store::clear`).
//! - **One `Env` per directory in a process** (`ENVS`): LMDB must not open an environment twice in one process. The
//!   directory's slot is held across the open and the close. The environment knows the tables its live stores hold
//!   (`Shared::held`): a dropped table frees its handle, which LMDB gives the next table opened, so dropping a table a
//!   store of the process holds would point that store at another table.
//! - **Opens and closes run under the directory's `open.lock`**, one process at a time. On macOS LMDB locks an
//!   environment with a System V semaphore set per lock file, which only `mdb_env_close` removes; a close must not
//!   remove the set another process's open is reading.
//! - **On macOS a lock file whose semaphore key names another set is made again** before the first open (`semaphore`).
//! - **The last handle closes the environment** (`Store`'s drop), never an exit: an environment left open at an exit
//!   leaves its semaphore set behind, and enough of them disable every environment of the host.
//! - **A table's version is part of its name** (`forward.v3`). Opening drops the tables of the same name and an older
//!   version, and keeps newer ones: a newer binary that may still run uses them. A retired table, which no binary
//!   writes any more, is dropped in every version. A table a live store of this process holds is never dropped.
//! - **A reader slot is held for a transaction, not for a thread** (`MDB_NOTLS`): a slot bound to a thread stays taken
//!   until the thread exits, so the workers of a few parallel queries took every slot and each later read was a miss.
//! - **The readers of a dead process are cleared** at an open and before a full map's clear (`clear_stale_readers`):
//!   LMDB clears them itself only when a lock's owner dies, and a killed reader's snapshot keeps every page freed after
//!   it from reuse, so each clear would free nothing a batch can take.
//! - **One write transaction per batch** (`Store::put`): a commit syncs the file and takes the writer lock that the other
//!   processes wait on.
//! - **Every failure is a miss**: a store that cannot open, read or write answers nothing and fails no caller.
use heed::types::{Bytes, DecodeIgnore};
use heed::{
    CommitOrAbort, CommitToken, Database, DatabaseOpener, EnvFlags, EnvOpenOptions, OnCommit, RoTxn, WithoutTls,
};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};

mod semaphore;

/// An environment whose read transactions hold a reader slot only while they run (`MDB_NOTLS`).
type Env = heed::Env<WithoutTls>;

/// The share of the map the live pages of the environment may take before a batch clears it (`Store::put`): what a
/// batch adds past the live pages, and the freelist's own pages, stay under the map size, and so do the pages the
/// clear itself writes.
const FULL: f64 = 0.75;

/// The key the transaction after a clear writes and the next batch removes (`Store::clear`): LMDB reuses the pages a
/// transaction frees from the second transaction after it, so the batch after a clear must be the second. A caller
/// never writes this key to the first table.
const CLEAR_MARK: &[u8] = b"\0lmdb-env.clear";

/// The tables an environment may hold at once: those of the binaries of several versions that run side by side.
const MAX_TABLES: u32 = 64;

/// How an environment opens.
#[derive(Clone, Copy, Debug)]
pub struct Config {
    /// The cap of the environment, rounded up to a multiple of the platforms' pages.
    pub map_size: usize,
    /// Sync each commit: off only for tests, which outlive no crash of the system.
    pub sync: bool,
    /// The read transactions of every process that may run at once, a slot each; the first process to open the
    /// environment sizes its lock file.
    pub max_readers: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config { map_size: 2 << 30, sync: true, max_readers: 1024 }
    }
}

/// A table of a store: an index into its tables, in the order `Store::open` was given them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Table(usize);

/// A database committed by the transaction that opened it, usable in any later one (heed's `OnCommit`).
type RawDatabase = Database<'static, Bytes, Bytes>;

/// The tables of one environment while the transaction that opens them runs; its commit makes them `RawDatabase`s.
struct Opened<'e>(Vec<Database<'e, Bytes, Bytes>>);

impl OnCommit for Opened<'_> {
    type Committed = Vec<RawDatabase>;

    fn on_commit(self, token: &CommitToken) -> Self::Committed {
        self.0.into_iter().map(|database| database.on_commit(token)).collect()
    }
}

/// The open environment of a directory in this process and the tables its live stores hold.
struct Shared {
    env: Env,
    /// The table names of the live stores, a name once per store that holds it: a clear drops every other table
    /// (`Store::clear`). Locked across an open's tables and a clear, so a table is never dropped between its open and
    /// its store's entry here.
    held: Mutex<Vec<String>>,
}

impl Shared {
    fn held(&self) -> MutexGuard<'_, Vec<String>> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The environment of a directory in this process, if one is open: its slot is locked across an open and a close.
type Slot = Arc<Mutex<Weak<Shared>>>;

static ENVS: OnceLock<Mutex<HashMap<PathBuf, Slot>>> = OnceLock::new();

fn slot_of(dir: &Path) -> Slot {
    let mut envs = ENVS.get_or_init(Default::default).lock().unwrap_or_else(PoisonError::into_inner);
    // A slot no store holds and no thread opens is dropped; one another thread holds (`try_lock` fails) may be opening.
    envs.retain(|_, slot| Arc::strong_count(slot) > 1 || slot.try_lock().map_or(true, |env| env.strong_count() > 0));
    Arc::clone(envs.entry(dir.to_path_buf()).or_default())
}

fn lock(slot: &Slot) -> MutexGuard<'_, Weak<Shared>> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `open.lock` of the environment's directory, held until dropped: the opens and closes of its processes, one at a time.
fn lock_opens(dir: &Path) -> std::io::Result<std::fs::File> {
    let file = std::fs::OpenOptions::new().write(true).create(true).truncate(false).open(dir.join("open.lock"))?;
    file.lock()?;
    Ok(file)
}

/// An open environment and the tables a caller opened in it; cheap to share behind an `Arc`.
pub struct Store {
    shared: Option<Arc<Shared>>,
    slot: Slot,
    tables: Vec<RawDatabase>,
    /// The names of `tables`, in their order, as `Shared::held` lists them.
    names: Vec<String>,
    /// The unnamed database, whose entries name every table of the environment and hold its record (`live_bytes`).
    main: RawDatabase,
}

impl Store {
    /// The environment of `dir` (the one of this process if it is open) with `tables`, each `(name, version)`, created
    /// if missing; the tables of the same names and older versions are dropped, and those of the `retired` names in
    /// every version.
    pub fn open(dir: &Path, config: &Config, tables: &[(&str, u32)], retired: &[&str]) -> Result<Store, String> {
        let (shared, slot) =
            open_env(dir, config).map_err(|e| format!("cannot open the store in {}: {e}", dir.display()))?;
        let names: Vec<String> = tables.iter().map(|(name, version)| format!("{name}.v{version}")).collect();
        let (opened, main) = {
            let mut held = shared.held();
            let opened = open_tables(&shared.env, &names, tables, retired, &held)
                .map_err(|e| format!("cannot open the tables of {}: {e}", dir.display()))?;
            held.extend(names.iter().cloned());
            opened
        };
        Ok(Store { shared: Some(shared), slot, tables: opened, names, main })
    }

    fn shared(&self) -> &Shared {
        self.shared.as_deref().expect("taken only by drop")
    }

    fn env(&self) -> &Env {
        &self.shared().env
    }

    /// The value of `key` in `table`, or None: a miss, or an error read as one.
    pub fn get(&self, table: Table, key: &[u8]) -> Option<Vec<u8>> {
        self.get_with(table, key, <[u8]>::to_vec)
    }

    /// `read` of the value of `key` in `table` where the map holds it, inside the read transaction: a reader that
    /// decodes the value copies the mapped bytes once, where it needs them, instead of after `get`'s copy. None for a
    /// miss, or an error read as one. The slice lives only as long as the call.
    pub fn get_with<R>(&self, table: Table, key: &[u8], read: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let txn = self.env().read_txn().ok()?;
        self.tables[table.0].get(&txn, key).ok().flatten().map(read)
    }

    /// The values of `keys` in `table`, in their order, read in one transaction.
    pub fn get_all(&self, table: Table, keys: &[&[u8]]) -> Vec<Option<Vec<u8>>> {
        let Ok(txn) = self.env().read_txn() else { return keys.iter().map(|_| None).collect() };
        keys.iter().map(|key| self.tables[table.0].get(&txn, key).ok().flatten().map(<[u8]>::to_vec)).collect()
    }

    /// Whether `table` holds each of `keys`, in their order, read in one transaction; an error reads as no entry.
    pub fn contains_all(&self, table: Table, keys: &[&[u8]]) -> Vec<bool> {
        let Ok(txn) = self.env().read_txn() else { return vec![false; keys.len()] };
        keys.iter().map(|key| self.tables[table.0].get(&txn, key).ok().flatten().is_some()).collect()
    }

    /// Empty `table` alone, in one transaction: an entry format that a generation of the table's own retires (Branch
    /// Deck's `keep_only_version`). The pages it frees are reusable from the second transaction after it, as every
    /// transaction's are; a full map clears every table in two (`put_batches`).
    pub fn clear_table(&self, table: Table) -> Result<(), String> {
        let mut txn = self.env().write_txn().map_err(|e| e.to_string())?;
        self.tables[table.0].clear(&mut txn).map_err(|e| e.to_string())?;
        txn.commit().map_err(|e| e.to_string())
    }

    /// Write `entries` to `table` in one transaction (`put_batches`).
    pub fn put(&self, table: Table, entries: &[(Vec<u8>, Vec<u8>)]) -> Result<(), String> {
        self.put_batches(&[(table, entries)])
    }

    /// Write each table's entries, all in one transaction. A batch that would take the live pages of the environment
    /// past `FULL` of the map clears the environment first (`clear`, whose last transaction comes before the batch, so
    /// the batch reuses the pages the clear freed). A map full anyway (a batch larger than the map's free part) clears
    /// and writes once more; what fails then is lost, as every entry is derived.
    pub fn put_batches(&self, batches: &[(Table, &[(Vec<u8>, Vec<u8>)])]) -> Result<(), String> {
        let batch: usize = batches.iter().flat_map(|(_, entries)| entries.iter()).map(|(k, v)| k.len() + v.len()).sum();
        if self.live_bytes().is_some_and(|live| (live + batch * 2) as f64 > self.env().info().map_size as f64 * FULL) {
            self.clear().map_err(|e| format!("cannot clear a full store: {e}"))?;
        }
        match self.write(batches) {
            Err(heed::Error::Mdb(heed::MdbError::MapFull)) => {
                self.clear().map_err(|e| format!("cannot clear a full store: {e}"))?;
                self.write(batches).map_err(|e| e.to_string())
            }
            result => result.map_err(|e| e.to_string()),
        }
    }

    /// The bytes of the pages every table of the environment holds (branch, leaf and overflow pages) and the unnamed
    /// database's own, free pages not counted: the map caps them all, whichever binary wrote them. A table's record is
    /// its entry in the unnamed database (`TableRecord`), so no table needs a handle of this process.
    fn live_bytes(&self) -> Option<usize> {
        let txn = self.env().read_txn().ok()?;
        let main = self.main.stat(&txn).ok()?;
        let mut pages = main.branch_pages + main.leaf_pages + main.overflow_pages;
        for item in self.main.iter(&txn).ok()? {
            let (_, record) = item.ok()?;
            pages += TableRecord::read(record).map_or(0, |r| r.pages());
        }
        Some(pages * main.page_size as usize)
    }

    fn write(&self, batches: &[(Table, &[(Vec<u8>, Vec<u8>)])]) -> heed::Result<()> {
        let mut txn = self.env().write_txn()?;
        if let Some(first) = self.tables.first() {
            first.delete(&mut txn, CLEAR_MARK)?;
        }
        for (table, entries) in batches {
            for (key, value) in *entries {
                self.tables[table.0].put(&mut txn, key, value)?;
            }
        }
        txn.commit()
    }

    /// Clear the environment: empty every table a live store of this process holds (this store's among them), whose
    /// handles stay valid, and drop every other table, which frees its pages and its name; then commit one small write
    /// (`CLEAR_MARK`). The map caps the environment, so a clear of this store's tables alone left the pages of the
    /// tables of other versions and layouts, and the batch after it met the full map again (accio's issue #861). The
    /// tables are opened one at a time and a dropped one frees its handle, so a clear drops more tables than a process
    /// may hold open (`MAX_TABLES`).
    ///
    /// Each table is cleared in a transaction of its own (`empty_table`): a transaction lists the pages it frees in one
    /// record, on contiguous pages, and the clear of every table of a 2 GiB map filled to its end at once (115 048
    /// pages, a record of 57 pages of 16 KiB) found no such run among its 16 006 scattered free pages and failed itself
    /// with `MDB_MAP_FULL`. What a failure leaves cleared stays so: every entry is derived.
    ///
    /// LMDB hands the pages a transaction frees to the second transaction after it, never the first, so a batch right
    /// after the clear alone would take new pages from the map's end, and every clear would move the end until it met
    /// the map size (`MDB_MAP_FULL` at batch 90 of `a_full_map_clears_the_tables_and_takes_the_batch` on 4 KiB pages).
    /// An empty commit does not count: LMDB writes no transaction without a dirty page. A dead process's reader would
    /// keep the pages the clear frees from the batch, whichever check found the map full, so its slot goes first.
    fn clear(&self) -> heed::Result<()> {
        let shared = self.shared();
        let _ = shared.env.clear_stale_readers();
        let held = shared.held();
        let names = {
            let txn = shared.env.read_txn()?;
            table_names(&self.main, &txn)?
        };
        for name in names {
            // A table no store of this process holds is dropped (`held`, locked across the clear).
            empty_table(&shared.env, &name, !held.contains(&name))?;
        }
        drop(held);
        if let Some(first) = self.tables.first() {
            let mut txn = self.env().write_txn()?;
            first.put(&mut txn, CLEAR_MARK, b"")?;
            txn.commit()?;
        }
        Ok(())
    }

    /// The entries of `table`: what its key-value pairs hold, for tests and diagnostics.
    pub fn len(&self, table: Table) -> u64 {
        let Ok(txn) = self.env().read_txn() else { return 0 };
        self.tables[table.0].len(&txn).unwrap_or(0)
    }

    /// Does `table` hold no entry?
    pub fn is_empty(&self, table: Table) -> bool {
        self.len(table) == 0
    }

    /// The table opened as the `index`th of `Store::open`'s list.
    pub fn table(&self, index: usize) -> Option<Table> {
        (index < self.tables.len()).then_some(Table(index))
    }
}

impl Drop for Store {
    /// The last handle of the process closes the environment (`mdb_env_close`, which removes its semaphores when no
    /// other process has it open), with its slot locked, so an open of the same directory waits for it, and under
    /// `open.lock`, so another process's open does not read the semaphores the close removes.
    fn drop(&mut self) {
        let slot = lock(&self.slot);
        let shared = self.shared.take();
        if let Some(shared) = &shared {
            let mut held = shared.held();
            for name in &self.names {
                if let Some(at) = held.iter().position(|h| h == name) {
                    held.swap_remove(at);
                }
            }
        }
        let closing = shared
            .as_ref()
            .filter(|shared| Arc::strong_count(shared) == 1)
            .and_then(|shared| lock_opens(shared.env.path()).ok());
        drop(shared);
        drop(closing);
        drop(slot);
    }
}

fn open_env(dir: &Path, config: &Config) -> Result<(Arc<Shared>, Slot), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let dir = dir.canonicalize()?;
    let slot = slot_of(&dir);
    let mut opened = lock(&slot);
    if let Some(shared) = opened.upgrade() {
        drop(opened);
        return Ok((shared, slot));
    }
    // A multiple of the 4 KiB and 16 KiB pages of the platforms.
    let page = 16 * 1024;
    let map_size = config.map_size.div_ceil(page).max(1) * page;
    let _opening = lock_opens(&dir)?;
    let open = || {
        semaphore::claim(&dir);
        let mut options = EnvOpenOptions::new().read_txn_without_tls();
        options.map_size(map_size).max_dbs(MAX_TABLES).max_readers(config.max_readers);
        if !config.sync {
            // SAFETY: the flags are set before the open. Without `MDB_WRITEMAP` LMDB writes its pages with `write`, so a
            // crash of the process loses no commit; only a crash of the system may lose the last ones, which only a
            // test, outliving no such crash, accepts.
            unsafe {
                options.flags(EnvFlags::NO_SYNC | EnvFlags::NO_META_SYNC);
            }
        }
        // SAFETY: the slot of this directory in `ENVS` holds the process's only `Env` of it, and only LMDB writes its
        // files.
        unsafe { options.open(&dir) }
    };
    let env = match open() {
        // LMDB's race with another process's close: the open waited for the last other user's close, which removed the
        // semaphores the open then read; the next open is the first.
        Err(heed::Error::Io(e)) if e.kind() == std::io::ErrorKind::InvalidInput => open()?,
        result => result?,
    };
    // The slots a killed process left are free again; a failure leaves them, as before.
    let _ = env.clear_stale_readers();
    let shared = Arc::new(Shared { env, held: Mutex::default() });
    *opened = Arc::downgrade(&shared);
    drop(opened);
    Ok((shared, slot))
}

/// The tables `tables` of `env` (named `names`), in their order, created if missing, and the unnamed database; the
/// tables of their names and older versions, and of the `retired` names in every version, are dropped, but for those a
/// live store of this process holds (`held`).
fn open_tables(
    env: &Env,
    names: &[String],
    tables: &[(&str, u32)],
    retired: &[&str],
    held: &[String],
) -> heed::Result<(Vec<RawDatabase>, RawDatabase)> {
    // The common case, every table there and none to drop, needs no write transaction; the handles stay valid only
    // because the read transaction commits (`open_databases_and_commit`), which an abort of a miss does not.
    let existing = env.open_databases_and_commit(env.read_txn()?, |opener, txn| -> heed::Result<_> {
        let stale = stale_names(opener, txn, tables, retired, held)?;
        let main = unnamed(opener, txn)?;
        let opened: Option<Vec<_>> = names
            .iter()
            .map(|name| opener.open_database::<Bytes, Bytes>(txn, Some(name)))
            .collect::<heed::Result<Vec<_>>>()?
            .into_iter()
            .collect();
        Ok(match opened.filter(|_| stale.is_empty()) {
            Some(opened) => CommitOrAbort::Commit((Opened(opened), main)),
            None => CommitOrAbort::Abort(()),
        })
    })?;
    if let CommitOrAbort::Commit(opened) = existing {
        return Ok(opened);
    }
    let opened = env
        .create_databases_and_commit(env.write_txn()?, |opener, txn| -> heed::Result<_> {
            for name in stale_names(opener, txn, tables, retired, held)? {
                if let Some(database) = opener.open_database::<Bytes, Bytes>(txn, Some(&name))? {
                    // SAFETY: no store of this process holds the table (`held`), so the handle serves nothing else; a
                    // process of an older version that still uses it gets errors, which it reads as misses.
                    unsafe { database.remove(txn)? };
                }
            }
            let opened = names
                .iter()
                .map(|name| opener.create_database::<Bytes, Bytes>(txn, Some(name)))
                .collect::<heed::Result<Vec<_>>>()?;
            Ok(CommitOrAbort::<_, Infallible>::Commit((Opened(opened), unnamed(opener, txn)?)))
        })?
        .unwrap_commit();
    Ok(opened)
}

/// The entries a step of `empty_table` deletes first; a step that meets the full map halves it, one that commits
/// doubles it.
const CLEAR_STEP: usize = 256;

/// Empty the table `name` of `env`, and drop it when `drop_it`. One transaction drops or empties the whole table, which
/// lists every page it frees in one record on contiguous pages; where the map has no such run (`MDB_MAP_FULL`: a map
/// filled to its end, its free pages scattered, on 4 KiB pages even one table's record of tens of pages), the entries
/// go in steps, each a transaction whose record is as small as its step, halved after a step that meets the full map
/// and doubled after one that commits, and the empty table goes last.
fn empty_table(env: &Env, name: &str, drop_it: bool) -> heed::Result<()> {
    match empty_whole(env, name, drop_it) {
        Err(heed::Error::Mdb(heed::MdbError::MapFull)) => empty_in_steps(env, name, drop_it),
        done => done,
    }
}

/// `empty_table`'s one transaction: the whole table dropped or emptied.
fn empty_whole(env: &Env, name: &str, drop_it: bool) -> heed::Result<()> {
    let Some(table) = open_existing(env, name)? else { return Ok(()) };
    let mut txn = env.write_txn()?;
    if drop_it {
        // SAFETY: the caller drops only a table no store of this process holds, so its handle serves nothing else; a
        // process of another version or layout that uses it gets errors, which it reads as misses.
        unsafe { table.remove(&mut txn)? };
    } else {
        table.clear(&mut txn)?;
    }
    txn.commit()
}

/// `empty_table`'s steps: the entries in transactions of `CLEAR_STEP` and more or fewer, then the empty table.
fn empty_in_steps(env: &Env, name: &str, drop_it: bool) -> heed::Result<()> {
    // A drop that failed closed its handle (`mdb_drop` closes it before the commit), so the table opens again.
    let Some(table) = open_existing(env, name)? else { return Ok(()) };
    let mut step = CLEAR_STEP;
    loop {
        let deleted = (|| -> heed::Result<usize> {
            let mut txn = env.write_txn()?;
            let keys = table
                .iter(&txn)?
                .take(step)
                .map(|item| item.map(|(key, _)| key.to_vec()))
                .collect::<heed::Result<Vec<_>>>()?;
            for key in &keys {
                table.delete(&mut txn, key)?;
            }
            txn.commit()?;
            Ok(keys.len())
        })();
        match deleted {
            Ok(0) => break,
            Ok(_) => step = step.saturating_mul(2),
            Err(heed::Error::Mdb(heed::MdbError::MapFull)) if step > 1 => step /= 2,
            Err(e) => return Err(e),
        }
    }
    empty_whole(env, name, drop_it)
}

/// The table `name` of `env`, None when the environment has none of that name; its handle outlives the transaction
/// that opens it.
fn open_existing(env: &Env, name: &str) -> heed::Result<Option<RawDatabase>> {
    let opened = env.open_databases_and_commit(env.read_txn()?, |opener, txn| -> heed::Result<_> {
        Ok(match opener.open_database::<Bytes, Bytes>(txn, Some(name))? {
            Some(table) => CommitOrAbort::Commit(table),
            None => CommitOrAbort::Abort(()),
        })
    })?;
    Ok(match opened {
        CommitOrAbort::Commit(table) => Some(table),
        CommitOrAbort::Abort(()) => None,
    })
}

/// The unnamed database, which every environment has: its keys name the tables, its values hold their records.
fn unnamed<'e>(opener: &DatabaseOpener<'e, WithoutTls>, txn: &RoTxn<'e>) -> heed::Result<Database<'e, Bytes, Bytes>> {
    opener.open_database::<Bytes, Bytes>(txn, None)?.ok_or(heed::Error::Mdb(heed::MdbError::NotFound))
}

/// The names of every table of the environment, from the unnamed database that lists them; a name that is not UTF-8,
/// which no `Store::open` writes, is left out.
fn table_names(main: &RawDatabase, txn: &RoTxn) -> heed::Result<Vec<String>> {
    let mut names = Vec::new();
    for item in main.iter(txn)? {
        let (name, _) = item?;
        if let Ok(name) = std::str::from_utf8(name) {
            names.push(name.to_string());
        }
    }
    Ok(names)
}

/// A table's record, its value in the unnamed database: LMDB's `MDB_db` (`mdb.c`), `md_pad` (`u32`), `md_flags` and
/// `md_depth` (`u16` each), then `md_branch_pages`, `md_leaf_pages`, `md_overflow_pages`, `md_entries` and `md_root`,
/// each a `size_t` in the platform's order (no `MDB_VL32`, a 32-bit build's option). Read without a handle of the
/// table, so the live pages of every table are measured without opening one (`Store::live_bytes`);
/// `a_tables_record_in_the_unnamed_database_is_its_stat` checks the layout against `mdb_stat`.
struct TableRecord {
    branch_pages: usize,
    leaf_pages: usize,
    overflow_pages: usize,
}

impl TableRecord {
    const WORD: usize = size_of::<usize>();

    fn read(bytes: &[u8]) -> Option<TableRecord> {
        if bytes.len() != 8 + 5 * Self::WORD {
            return None;
        }
        let word = |i: usize| -> Option<usize> {
            let at = 8 + i * Self::WORD;
            Some(usize::from_ne_bytes(bytes.get(at..at + Self::WORD)?.try_into().ok()?))
        };
        Some(TableRecord { branch_pages: word(0)?, leaf_pages: word(1)?, overflow_pages: word(2)? })
    }

    fn pages(&self) -> usize {
        self.branch_pages + self.leaf_pages + self.overflow_pages
    }
}

/// The tables of the environment with one of `tables`' names and an older version, or a `retired` name, from the
/// unnamed database that lists them; a table a live store of this process holds (`held`) is not stale.
fn stale_names<'e>(
    opener: &DatabaseOpener<'e, WithoutTls>,
    txn: &RoTxn<'e>,
    tables: &[(&str, u32)],
    retired: &[&str],
    held: &[String],
) -> heed::Result<Vec<String>> {
    let Some(main) = opener.open_database::<Bytes, DecodeIgnore>(txn, None)? else { return Ok(Vec::new()) };
    let mut stale = Vec::new();
    for item in main.iter(txn)? {
        let (name, ()) = item?;
        let Ok(name) = std::str::from_utf8(name) else { continue };
        let older = name.rsplit_once(".v").is_some_and(|(base, version)| {
            retired.contains(&base)
                || tables.iter().any(|&(table, current)| {
                    table == base && version.parse::<u32>().is_ok_and(|version| version < current)
                })
        });
        if older && !held.iter().any(|h| h == name) {
            stale.push(name.to_string());
        }
    }
    Ok(stale)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A store is shared by the threads of a query behind an `Arc`.
    #[test]
    fn a_store_is_shared_between_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<Store>();
    }

    fn scratch(name: &str) -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("lmdb-env-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    const TEST: Config = Config { map_size: 1 << 20, sync: false, max_readers: 126 };

    #[test]
    fn entries_outlive_the_store_that_wrote_them() {
        let dir = scratch("reopen");
        {
            let store = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
            let t = store.table(0).unwrap();
            store.put(t, &[(b"a".to_vec(), b"1".to_vec()), (b"b".to_vec(), b"2".to_vec())]).unwrap();
            assert_eq!(store.get(t, b"a").as_deref(), Some(&b"1"[..]));
            assert_eq!(store.get_all(t, &[b"b", b"c"]), [Some(b"2".to_vec()), None]);
        }
        let store = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
        assert_eq!(store.get(store.table(0).unwrap(), b"b").as_deref(), Some(&b"2"[..]));
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_stores_of_one_directory_share_its_environment() {
        let dir = scratch("share");
        let one = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
        let two = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
        one.put(one.table(0).unwrap(), &[(b"k".to_vec(), b"v".to_vec())]).unwrap();
        assert_eq!(two.get(two.table(0).unwrap(), b"k").as_deref(), Some(&b"v"[..]));
        drop(one);
        // The environment stays open for the second handle.
        assert_eq!(two.get(two.table(0).unwrap(), b"k").as_deref(), Some(&b"v"[..]));
        drop(two);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_transaction_writes_every_table_of_a_batch() {
        let dir = scratch("batches");
        let store = Store::open(&dir, &TEST, &[("forward", 1), ("callers", 1)], &[]).unwrap();
        let (a, b) = (store.table(0).unwrap(), store.table(1).unwrap());
        store
            .put_batches(&[(a, &[(b"k".to_vec(), b"edges".to_vec())]), (b, &[(b"k".to_vec(), b"callers".to_vec())])])
            .unwrap();
        assert_eq!(store.get(a, b"k").as_deref(), Some(&b"edges"[..]));
        assert_eq!(store.get(b, b"k").as_deref(), Some(&b"callers"[..]), "each table holds its own entry of one key");
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_newer_version_drops_the_older_table_and_keeps_its_neighbours() {
        let dir = scratch("versions");
        {
            let store = Store::open(&dir, &TEST, &[("forward", 1), ("other", 1)], &[]).unwrap();
            store.put(store.table(0).unwrap(), &[(b"k".to_vec(), b"old".to_vec())]).unwrap();
            store.put(store.table(1).unwrap(), &[(b"k".to_vec(), b"kept".to_vec())]).unwrap();
        }
        let store = Store::open(&dir, &TEST, &[("forward", 2), ("other", 1)], &[]).unwrap();
        assert_eq!(store.get(store.table(0).unwrap(), b"k"), None, "the new version starts empty");
        assert_eq!(store.get(store.table(1).unwrap(), b"k").as_deref(), Some(&b"kept"[..]));
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Values inline in a page (4 KiB) and past any page (20 KiB, overflow pages on every platform's page size): the
    /// pages a clear frees are reusable from the second transaction after it, so a batch right after a clear in a full
    /// map failed on Linux's 4 KiB pages; filling the map again and again, every batch is written.
    #[test]
    fn a_full_map_clears_the_tables_and_takes_the_batch() {
        for (name, size) in [("full", 4096), ("full-overflow", 20 * 1024)] {
            let dir = scratch(name);
            let store = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
            let t = store.table(0).unwrap();
            let value = vec![7u8; size];
            for i in 0..400u32 {
                store
                    .put(t, &[(i.to_be_bytes().to_vec(), value.clone())])
                    .unwrap_or_else(|e| panic!("{name}: batch {i}: {e}"));
                assert!(store.get(t, &i.to_be_bytes()).is_some(), "{name}: batch {i} is written");
            }
            assert!(store.len(t) < 400, "{name}: a clear dropped the earlier entries: {}", store.len(t));
            drop(store);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_retired_table_is_dropped_in_every_version_and_its_neighbours_kept() {
        let dir = scratch("retired");
        {
            let store = Store::open(&dir, &TEST, &[("patch", 3), ("diff", 1)], &[]).unwrap();
            store.put(store.table(0).unwrap(), &[(b"k".to_vec(), b"old".to_vec())]).unwrap();
            store.put(store.table(1).unwrap(), &[(b"k".to_vec(), b"kept".to_vec())]).unwrap();
        }
        let store = Store::open(&dir, &TEST, &[("diff", 1)], &["patch"]).unwrap();
        assert_eq!(store.get(store.table(0).unwrap(), b"k").as_deref(), Some(&b"kept"[..]));
        drop(store);
        let store = Store::open(&dir, &TEST, &[("patch", 3)], &[]).unwrap();
        assert_eq!(
            store.get(store.table(0).unwrap(), b"k"),
            None,
            "the retired table is gone, not kept for its version"
        );
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Writes `size`-byte values to `table`, a transaction each and past the store's own check, until the live pages
    /// of the environment take `share` of the map, the map is full or `max` values are written: the environment that
    /// binaries of other layouts or versions left. Returns how many it wrote.
    fn fill(store: &Store, table: Table, size: usize, share: f64, max: u32) -> u32 {
        let value = vec![7u8; size];
        let map = store.env().info().map_size as f64;
        for i in 0..max {
            if store.live_bytes().unwrap() as f64 >= map * share {
                return i;
            }
            let mut txn = store.env().write_txn().unwrap();
            match store.tables[table.0].put(&mut txn, &i.to_be_bytes(), &value).and_then(|()| txn.commit()) {
                Ok(()) => {}
                Err(heed::Error::Mdb(heed::MdbError::MapFull)) => return i,
                Err(e) => panic!("write {i}: {e}"),
            }
        }
        max
    }

    /// The names of every table of the store's environment.
    fn names_in(store: &Store) -> Vec<String> {
        let txn = store.env().read_txn().unwrap();
        table_names(&store.main, &txn).unwrap()
    }

    /// Writes 100 batches of 32 values of 4 KiB to `table`, each read back: a batch larger than the part of the map
    /// that the other tables leave, as a query's misses are (16 MiB in accio's blob store).
    fn takes_every_batch(store: &Store, table: Table) {
        let value = vec![9u8; 4096];
        for i in 0..100u32 {
            let batch: Vec<_> = (i * 32..(i + 1) * 32).map(|k| (k.to_be_bytes().to_vec(), value.clone())).collect();
            store.put(table, &batch).unwrap_or_else(|e| panic!("batch {i}: {e}"));
            assert!(batch.iter().all(|(k, _)| store.get(table, k).is_some()), "batch {i} is written");
        }
    }

    /// `takes_every_batch` of the store's first table, which is then the environment's only table; `dir` goes.
    fn takes_every_batch_alone(store: Store, dir: &Path) {
        takes_every_batch(&store, store.table(0).unwrap());
        assert_eq!(names_in(&store), store.names[..1], "every other table is dropped");
        drop(store);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The map caps the environment, not a store's tables: a map the tables of another layout filled, which a clear of
    /// the writer's own tables left as they were, lost every batch after it (accio's issue #861, a blob store of four
    /// layouts).
    #[test]
    fn a_full_map_drops_the_tables_of_another_layout() {
        let dir = scratch("other-layout");
        {
            let other = Store::open(&dir, &TEST, &[("callees-kotlin-aaaa", 1)], &[]).unwrap();
            assert!(fill(&other, other.table(0).unwrap(), 4096, 0.95, u32::MAX) > 0, "the other layout fills the map");
        }
        takes_every_batch_alone(Store::open(&dir, &TEST, &[("callees-kotlin-bbbb", 1)], &[]).unwrap(), &dir);
    }

    /// A transaction lists the pages it frees in one record, on contiguous pages: a clear of every table at once in a
    /// map filled to its end found no such run among the scattered free pages, failed itself with `MDB_MAP_FULL`, and
    /// every batch after it was lost (accio's issue #861: 115 048 pages, a record of 57 pages, in a map of 131 072).
    /// Each table cleared in a transaction of its own lists only its own pages.
    #[test]
    fn a_map_filled_to_its_end_by_many_tables_is_cleared() {
        let dir = scratch("fragmented");
        let config = Config { map_size: 64 << 20, ..TEST };
        {
            let names: Vec<String> = (0..8).map(|i| format!("layout-{i}")).collect();
            let tables: Vec<(&str, u32)> = names.iter().map(|n| (n.as_str(), 1)).collect();
            let other = Store::open(&dir, &config, &tables, &[]).unwrap();
            let value = vec![7u8; 4096];
            'fill: for i in 0u32.. {
                for t in &other.tables {
                    let mut txn = other.env().write_txn().unwrap();
                    match t.put(&mut txn, &i.to_be_bytes(), &value).and_then(|()| txn.commit()) {
                        Ok(()) => {}
                        Err(heed::Error::Mdb(heed::MdbError::MapFull)) => break 'fill,
                        Err(e) => panic!("{e}"),
                    }
                }
            }
        }
        takes_every_batch_alone(Store::open(&dir, &config, &[("current", 1)], &[]).unwrap(), &dir);
    }

    /// The steps of a clear whose whole drop met the full map: every entry goes, the table goes when no store of the
    /// process holds it, and a holder's handle stays valid when it does. A map filled to its end reaches them on 4 KiB
    /// pages (`a_map_filled_to_its_end_by_many_tables_is_cleared` on Linux); here they run directly, on every platform.
    #[test]
    fn a_table_emptied_in_steps_loses_every_entry_and_its_holder_keeps_its_handle() {
        let dir = scratch("steps");
        let holder = Store::open(&dir, &TEST, &[("held", 1)], &[]).unwrap();
        let h = holder.table(0).unwrap();
        {
            let other = Store::open(&dir, &TEST, &[("other", 1)], &[]).unwrap();
            assert_eq!(fill(&other, other.table(0).unwrap(), 100, 0.5, 1000), 1000);
        }
        assert_eq!(fill(&holder, h, 100, 0.7, 1000), 1000);
        empty_in_steps(holder.env(), "other.v1", true).unwrap();
        empty_in_steps(holder.env(), "held.v1", false).unwrap();
        assert_eq!(names_in(&holder), ["held.v1"], "the table no store holds is dropped");
        assert!(holder.is_empty(h), "the held table lost every entry");
        holder.put(h, &[(b"after".to_vec(), b"kept".to_vec())]).unwrap();
        assert_eq!(holder.get(h, b"after").as_deref(), Some(&b"kept"[..]));
        drop(holder);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A clear opens the tables one at a time and a dropped one frees its handle, so it drops more tables than a
    /// process may hold open.
    #[test]
    fn more_stale_tables_than_handle_slots_are_dropped_by_one_clear() {
        let dir = scratch("many-tables");
        // Room for a leaf page of every table on 16 KiB pages.
        let config = Config { map_size: 8 << 20, ..TEST };
        for i in 0..MAX_TABLES + 16 {
            // Each store is the environment's last handle, so its close frees the handles of the next.
            let other = Store::open(&dir, &config, &[(&format!("layout-{i}"), 1)], &[]).unwrap();
            fill(&other, other.table(0).unwrap(), 4096, 0.9, 16);
        }
        let store = Store::open(&dir, &config, &[("current", 1)], &[]).unwrap();
        let map = store.env().info().map_size as f64;
        assert!(store.live_bytes().unwrap() as f64 > map * FULL, "the other tables fill the map");
        takes_every_batch_alone(store, &dir);
    }

    /// A table a live store of the process holds is emptied, not dropped: a dropped table frees its handle, which the
    /// next table opened takes, so the store holding it would read and write that table.
    #[test]
    fn a_full_map_empties_a_live_neighbours_table_and_its_handle_still_works() {
        let dir = scratch("live-neighbour");
        let neighbour = Store::open(&dir, &TEST, &[("neighbour", 1)], &[]).unwrap();
        let n = neighbour.table(0).unwrap();
        neighbour.put(n, &[(b"k".to_vec(), b"before".to_vec())]).unwrap();
        let store = Store::open(&dir, &TEST, &[("writer", 1)], &[]).unwrap();
        let w = store.table(0).unwrap();
        assert!(fill(&store, w, 4096, 0.95, u32::MAX) > 0);
        store.put(w, &[(b"batch".to_vec(), b"written".to_vec())]).unwrap();
        assert_eq!(neighbour.get(n, b"k"), None, "the clear emptied the neighbour's table");
        neighbour.put(n, &[(b"after".to_vec(), b"kept".to_vec())]).unwrap();
        assert_eq!(neighbour.get(n, b"after").as_deref(), Some(&b"kept"[..]));
        assert_eq!(store.get(w, b"after"), None, "the neighbour wrote its own table");
        assert_eq!(store.get(w, b"batch").as_deref(), Some(&b"written"[..]));
        let mut names = names_in(&store);
        names.sort();
        assert_eq!(names, ["neighbour.v1", "writer.v1"]);
        drop((neighbour, store));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `live_bytes` reads each table's record from the unnamed database by LMDB's layout of `MDB_db`; the record must
    /// say what `mdb_stat` of the open table says, for inline values and for values past any page.
    #[test]
    fn a_tables_record_in_the_unnamed_database_is_its_stat() {
        let dir = scratch("records");
        let store = Store::open(&dir, &TEST, &[("inline", 1), ("overflow", 1), ("empty", 1)], &[]).unwrap();
        fill(&store, store.table(0).unwrap(), 100, 0.5, 600);
        fill(&store, store.table(1).unwrap(), 20 * 1024, 0.7, 8);
        let txn = store.env().read_txn().unwrap();
        for (i, name) in store.names.iter().enumerate() {
            let stat = store.tables[i].stat(&txn).unwrap();
            let bytes = store.main.get(&txn, name.as_bytes()).unwrap().unwrap();
            let record = TableRecord::read(bytes).unwrap_or_else(|| panic!("{name}: {} bytes", bytes.len()));
            let pages = (record.branch_pages, record.leaf_pages, record.overflow_pages);
            assert_eq!(pages, (stat.branch_pages, stat.leaf_pages, stat.overflow_pages), "{name}");
        }
        assert!(store.tables[1].stat(&txn).unwrap().overflow_pages > 0, "the values past a page overflow");
        assert!(store.tables[0].stat(&txn).unwrap().branch_pages > 0, "the inline table is deeper than a leaf");
        drop(txn);
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cleared_table_leaves_the_others_and_contains_says_which_keys_are_there() {
        let dir = scratch("clear-table");
        let store = Store::open(&dir, &TEST, &[("forward", 1), ("callers", 1)], &[]).unwrap();
        let (a, b) = (store.table(0).unwrap(), store.table(1).unwrap());
        store.put_batches(&[(a, &[(b"k".to_vec(), b"1".to_vec())]), (b, &[(b"k".to_vec(), b"2".to_vec())])]).unwrap();
        assert_eq!(store.contains_all(a, &[b"k", b"other"]), [true, false]);
        store.clear_table(a).unwrap();
        assert_eq!(store.contains_all(a, &[b"k"]), [false]);
        assert_eq!(store.get(b, b"k").as_deref(), Some(&b"2"[..]), "the other table keeps its entry");
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A slot bound to a thread (LMDB's default) stays taken while the thread lives: more live threads than slots, each
    /// done with its read, left the later ones `MDB_READERS_FULL`, a miss.
    #[test]
    fn more_live_threads_than_reader_slots_all_read() {
        let dir = scratch("readers");
        let config = Config { max_readers: 4, ..TEST };
        let store = Store::open(&dir, &config, &[("forward", 1)], &[]).unwrap();
        let t = store.table(0).unwrap();
        store.put(t, &[(b"k".to_vec(), b"v".to_vec())]).unwrap();
        let read = std::sync::Barrier::new(16);
        let hits = std::thread::scope(|s| {
            let readers: Vec<_> = (0..16)
                .map(|_| {
                    s.spawn(|| {
                        let hit = store.get(t, b"k").is_some();
                        // Every thread stays alive until all of them have read.
                        read.wait();
                        hit
                    })
                })
                .collect();
            readers.into_iter().map(|r| r.join().unwrap()).filter(|&hit| hit).count()
        });
        assert_eq!(hits, 16, "every thread reads the entry");
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The directory a child process of `a_killed_readers_snapshot_pins_no_page` holds a read transaction in.
    const CHILD_DIR: &str = "LMDB_ENV_TEST_READER_DIR";

    /// What the child prints to its stderr once its transaction runs.
    const READY: &str = "lmdb-env: the child reads";

    /// The child of `a_killed_readers_snapshot_pins_no_page`: opens the store, starts a read transaction, says `READY`
    /// and waits to be killed. Outside that test it does nothing.
    #[test]
    fn child_reader_holding_a_transaction() {
        let Some(dir) = std::env::var_os(CHILD_DIR) else { return };
        let store = Store::open(Path::new(&dir), &TEST, &[("forward", 1)], &[]).unwrap();
        let _txn = store.env().read_txn().unwrap();
        eprintln!("{READY}");
        std::thread::sleep(std::time::Duration::from_secs(600));
    }

    /// A process killed in a read transaction leaves its slot and its snapshot: every page freed after it stays
    /// unusable, so a full map's clear frees nothing the batch can take and each batch after it is lost.
    ///
    /// Unix only: LMDB tells a dead reader by its process id (`OpenProcess` on Windows, a lock on Unix). Windows keeps
    /// the id while a handle to the process is open and hands a freed id to the next process at once, so under a test
    /// runner spawning processes the killed child's id is live again before the clear and its slot stays.
    #[test]
    #[cfg(unix)]
    fn a_killed_readers_snapshot_pins_no_page() {
        use std::io::BufRead;
        let dir = scratch("killed");
        let store = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
        let t = store.table(0).unwrap();
        let value = vec![7u8; 4096];
        store.put(t, &[(0u32.to_be_bytes().to_vec(), value.clone())]).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["tests::child_reader_holding_a_transaction", "--exact", "--nocapture", "--test-threads", "1"])
            .env(CHILD_DIR, &dir)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let ready = std::io::BufReader::new(child.stderr.take().unwrap())
            .lines()
            .map_while(Result::ok)
            .any(|line| line == READY);
        assert!(ready, "the child holds a read transaction");
        child.kill().unwrap();
        child.wait().unwrap();
        for i in 1..400u32 {
            store.put(t, &[(i.to_be_bytes().to_vec(), value.clone())]).unwrap_or_else(|e| panic!("batch {i}: {e}"));
        }
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Is the environment of `dir` open in this process (`ENVS`)?
    fn is_open(dir: &Path) -> bool {
        let dir = dir.canonicalize().unwrap();
        let envs = ENVS.get_or_init(Default::default).lock().unwrap();
        envs.get(&dir).is_some_and(|slot| slot.try_lock().is_ok_and(|env| env.strong_count() > 0))
    }

    /// Is a thread of this process opening or closing the environment of `dir` (its slot is held)?
    fn is_opening(dir: &Path) -> bool {
        let dir = dir.canonicalize().unwrap();
        let envs = ENVS.get_or_init(Default::default).lock().unwrap();
        envs.get(&dir).is_some_and(|slot| slot.try_lock().is_err())
    }

    /// An environment kept open to the end of the process left its System V semaphore set behind on macOS, one per
    /// temporary directory of a test, until the system had none left (Branch Deck's object cache).
    #[test]
    fn the_last_handle_closes_the_environment() {
        let dir = scratch("last-handle");
        let one = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
        let two = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
        one.put(one.table(0).unwrap(), &[(b"k".to_vec(), b"v".to_vec())]).unwrap();
        drop(one);
        assert!(is_open(&dir), "the other handle keeps it open");
        drop(two);
        assert!(!is_open(&dir));
        // Closed by LMDB too, not only gone from `ENVS`.
        assert!(heed::env_closing_event(dir.canonicalize().unwrap()).is_none(), "heed still has it open");
        let again = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
        assert_eq!(again.get(again.table(0).unwrap(), b"k").as_deref(), Some(&b"v"[..]));
        drop(again);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The ids of the System V semaphore sets of the system, by key (`ipcs -s`).
    #[cfg(target_os = "macos")]
    fn semaphore_sets() -> HashMap<u32, String> {
        let output = std::process::Command::new("ipcs").arg("-s").output().unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter_map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                let key = u32::from_str_radix(fields.get(2)?.strip_prefix("0x")?, 16).ok()?;
                (fields.first() == Some(&"s")).then(|| (key, fields[1].to_string()))
            })
            .collect()
    }

    /// A closed environment gives back its semaphore set: LMDB makes one per lock file on macOS, keyed by
    /// `ftok(lock.mdb, 'M')`, and only the close of its last user removes it. The test follows its own set by id, not
    /// the system's count, which other processes change meanwhile.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_closed_environment_gives_back_its_semaphore_set() {
        use std::os::unix::fs::MetadataExt;
        for _ in 0..3 {
            let dir = scratch("semaphores");
            let store = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
            let metadata = std::fs::metadata(dir.join("lock.mdb")).unwrap();
            // macOS's `ftok`: the project id, the low byte of the device and the low 16 bits of the inode.
            let key = u32::from(b'M') << 24
                | u32::try_from(metadata.dev() & 0xff).unwrap() << 16
                | u32::try_from(metadata.ino() & 0xffff).unwrap();
            let id = semaphore_sets().remove(&key).unwrap_or_else(|| panic!("no semaphore set of key {key:#x}"));
            drop(store);
            assert!(!semaphore_sets().values().any(|set| *set == id), "the set {id} outlives its environment");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// The inode of `path`; None where files have none (Windows), whose lock files are never made again.
    fn inode(path: &Path) -> Option<u64> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            std::fs::metadata(path).ok().map(|metadata| metadata.ino())
        }
        #[cfg(not(unix))]
        {
            std::fs::metadata(path).ok().map(|_| 0)
        }
    }

    /// macOS keys the semaphore set of a lock file by the low 16 bits of its inode: a lock file whose key another set
    /// holds is made again before LMDB first opens it, so this environment's close leaves the other set alone.
    #[test]
    fn a_lock_file_whose_semaphore_key_another_set_holds_is_made_again() {
        let dir = scratch("semaphore-key");
        // The lock file exists, its set removed by the close.
        drop(Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap());
        let lock = dir.canonicalize().unwrap().join("lock.mdb");
        let before = inode(&lock).expect("the lock file");
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::ffi::OsStrExt;
            let c_path = std::ffi::CString::new(lock.as_os_str().as_bytes()).unwrap();
            let key = semaphore::key(&c_path).unwrap();
            // SAFETY: no pointer is passed; the set of another environment, made and removed by the test.
            let other = unsafe { libc::semget(key, 2, libc::IPC_CREAT | 0o600) };
            assert!(other >= 0, "{}", std::io::Error::last_os_error());
            let store = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
            store.put(store.table(0).unwrap(), &[(b"k".to_vec(), b"v".to_vec())]).unwrap();
            assert_ne!(inode(&lock), Some(before), "the lock file was made again");
            drop(store);
            // SAFETY: a zeroed `semid_ds` is a valid buffer, and no other pointer is passed.
            unsafe {
                let mut buffer: libc::semid_ds = std::mem::zeroed();
                assert_eq!(
                    libc::semctl(other, 0, libc::IPC_STAT, &raw mut buffer),
                    0,
                    "the close removed the set of the other environment"
                );
                libc::semctl(other, 0, libc::IPC_RMID);
            }
        }
        // Without System V semaphores (robust mutexes on Linux, named mutexes on Windows) the lock file stays.
        #[cfg(not(target_os = "macos"))]
        {
            let store = Store::open(&dir, &TEST, &[("forward", 1)], &[]).unwrap();
            store.put(store.table(0).unwrap(), &[(b"k".to_vec(), b"v".to_vec())]).unwrap();
            assert_eq!(inode(&lock), Some(before));
            drop(store);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The default config syncs each commit (no `MDB_NOSYNC`, no `MDB_NOMETASYNC`): without it LMDB promises no
    /// integrity after a power loss unless the file system keeps the order of writes, and stores no checksum, so a
    /// wrong but readable entry could be read back. A test's config commits without the drive's flush.
    #[test]
    fn the_default_config_syncs_every_commit_and_a_tests_does_not() {
        let unsynced = (EnvFlags::NO_SYNC | EnvFlags::NO_META_SYNC).bits();
        let dir = scratch("synced");
        let store = Store::open(&dir, &Config::default(), &[("forward", 1)], &[]).unwrap();
        assert_eq!(store.env().get_flags().unwrap() & unsynced, 0);
        drop(store);
        let dir_test = scratch("unsynced");
        let store = Store::open(&dir_test, &TEST, &[("forward", 1)], &[]).unwrap();
        assert_eq!(store.env().get_flags().unwrap() & unsynced, unsynced);
        drop(store);
        for dir in [dir, dir_test] {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    /// An open that waits for `open.lock` (another process opens or closes the environment) holds only the slot of its
    /// directory: the environment of another directory opens meanwhile, and the waiting one opens once the lock is free.
    #[test]
    fn an_environment_opens_while_the_open_of_another_waits_for_its_open_lock() {
        let (waiting, other) = (scratch("waiting"), scratch("other"));
        std::fs::create_dir_all(&waiting).unwrap();
        let held = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(waiting.join("open.lock"))
            .unwrap();
        held.lock().unwrap();
        std::thread::scope(|scope| {
            let opening = scope.spawn(|| Store::open(&waiting, &TEST, &[("forward", 1)], &[]).map(drop));
            while !is_opening(&waiting) {
                assert!(!opening.is_finished(), "the open waits for the lock of the other process");
                std::thread::yield_now();
            }
            let opened = Store::open(&other, &TEST, &[("forward", 1)], &[]).unwrap();
            assert!(is_open(&other));
            drop(opened);
            held.unlock().unwrap();
            opening.join().unwrap().unwrap();
        });
        for dir in [waiting, other] {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn a_directory_that_cannot_hold_a_store_is_an_error_not_a_panic() {
        let dir = scratch("blocked");
        std::fs::write(&dir, b"a file where the directory should be").unwrap();
        assert!(Store::open(&dir, &TEST, &[("forward", 1)], &[]).is_err());
        let _ = std::fs::remove_file(&dir);
    }
}
