//! An LMDB environment that the processes of one host share, for entries a caller can derive again: accio's forward
//! store (decision 2026-11-06-forward-edges-in-lmdb in accio's `docs/decisions/`) and Branch Deck's object cache, whose
//! copies of these rules diverged until they moved here (decision 2026-11-09-lmdb-env-in-psi-syntax). The rules:
//!
//! - **The map size caps the environment.** A write that would fill it, or meets `MDB_MAP_FULL`, clears this store's
//!   tables and is retried once: every entry is derived, so a clear costs its derivation and nothing else. The clear
//!   is two transactions, so the retried batch reuses the freed pages (`Store::clear`).
//! - **One `Env` per directory in a process** (`ENVS`): LMDB must not open an environment twice in one process. The
//!   directory's slot is held across the open and the close.
//! - **Opens and closes run under the directory's `open.lock`**, one process at a time. On macOS LMDB locks an
//!   environment with a System V semaphore set per lock file, which only `mdb_env_close` removes; a close must not
//!   remove the set another process's open is reading.
//! - **On macOS a lock file whose semaphore key names another set is made again** before the first open (`semaphore`).
//! - **The last handle closes the environment** (`Store`'s drop), never an exit: an environment left open at an exit
//!   leaves its semaphore set behind, and enough of them disable every environment of the host.
//! - **A table's version is part of its name** (`forward.v3`). Opening drops the tables of the same name and an older
//!   version, and keeps newer ones: a newer binary that may still run uses them. A retired table, which no binary
//!   writes any more, is dropped in every version.
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

/// The share of the map the live pages may take before a batch clears the tables (`Store::put`): what a batch adds
/// past the live pages, and the freelist's own pages, stay under the map size.
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

/// The environment of a directory in this process, if one is open: its slot is locked across an open and a close.
type Slot = Arc<Mutex<Weak<Env>>>;

static ENVS: OnceLock<Mutex<HashMap<PathBuf, Slot>>> = OnceLock::new();

fn slot_of(dir: &Path) -> Slot {
    let mut envs = ENVS.get_or_init(Default::default).lock().unwrap_or_else(PoisonError::into_inner);
    // A slot no store holds and no thread opens is dropped; one another thread holds (`try_lock` fails) may be opening.
    envs.retain(|_, slot| Arc::strong_count(slot) > 1 || slot.try_lock().map_or(true, |env| env.strong_count() > 0));
    Arc::clone(envs.entry(dir.to_path_buf()).or_default())
}

fn lock(slot: &Slot) -> MutexGuard<'_, Weak<Env>> {
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
    env: Option<Arc<Env>>,
    slot: Slot,
    tables: Vec<RawDatabase>,
}

impl Store {
    /// The environment of `dir` (the one of this process if it is open) with `tables`, each `(name, version)`, created
    /// if missing; the tables of the same names and older versions are dropped, and those of the `retired` names in
    /// every version.
    pub fn open(dir: &Path, config: &Config, tables: &[(&str, u32)], retired: &[&str]) -> Result<Store, String> {
        let (env, slot) =
            open_env(dir, config).map_err(|e| format!("cannot open the store in {}: {e}", dir.display()))?;
        let tables = open_tables(&env, tables, retired)
            .map_err(|e| format!("cannot open the tables of {}: {e}", dir.display()))?;
        Ok(Store { env: Some(env), slot, tables })
    }

    fn env(&self) -> &Env {
        self.env.as_deref().expect("taken only by drop")
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

    /// Write each table's entries, all in one transaction. A batch that would take the live pages past `FULL` of the map
    /// clears this store's tables first (`clear`, two transactions, so the batch reuses the pages the clear freed). A
    /// map full anyway (a batch larger than the map's free part) clears and writes once more; what fails then is lost,
    /// as every entry is derived.
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

    /// The bytes of the pages this store's tables hold (branch, leaf and overflow pages), free pages not counted.
    fn live_bytes(&self) -> Option<usize> {
        let txn = self.env().read_txn().ok()?;
        self.tables.iter().try_fold(0, |sum, table| {
            let s = table.stat(&txn).ok()?;
            Some(sum + (s.branch_pages + s.leaf_pages + s.overflow_pages) * s.page_size as usize)
        })
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

    /// Empty every table of this store, then commit one small write (`CLEAR_MARK`): LMDB hands the pages a
    /// transaction frees to the second transaction after it, never the first, so a batch right after the clear alone
    /// would take new pages from the map's end, and every clear would move the end until it met the map size
    /// (`MDB_MAP_FULL` at batch 90 of `a_full_map_clears_the_tables_and_takes_the_batch` on 4 KiB pages). An empty
    /// commit does not count: LMDB writes no transaction without a dirty page. A dead process's reader would keep the
    /// pages the clear frees from the batch, whichever check found the map full, so its slot goes first.
    fn clear(&self) -> heed::Result<()> {
        let _ = self.env().clear_stale_readers();
        let mut txn = self.env().write_txn()?;
        for table in &self.tables {
            table.clear(&mut txn)?;
        }
        txn.commit()?;
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
        let env = self.env.take();
        let closing =
            env.as_ref().filter(|env| Arc::strong_count(env) == 1).and_then(|env| lock_opens(env.path()).ok());
        drop(env);
        drop(closing);
        drop(slot);
    }
}

fn open_env(dir: &Path, config: &Config) -> Result<(Arc<Env>, Slot), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let dir = dir.canonicalize()?;
    let slot = slot_of(&dir);
    let mut opened = lock(&slot);
    if let Some(env) = opened.upgrade() {
        drop(opened);
        return Ok((env, slot));
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
    let env = Arc::new(match open() {
        // LMDB's race with another process's close: the open waited for the last other user's close, which removed the
        // semaphores the open then read; the next open is the first.
        Err(heed::Error::Io(e)) if e.kind() == std::io::ErrorKind::InvalidInput => open()?,
        result => result?,
    });
    // The slots a killed process left are free again; a failure leaves them, as before.
    let _ = env.clear_stale_readers();
    *opened = Arc::downgrade(&env);
    drop(opened);
    Ok((env, slot))
}

/// The tables `tables` of `env`, in their order, created if missing; the tables of their names and older versions, and
/// of the `retired` names in every version, are dropped.
fn open_tables(env: &Env, tables: &[(&str, u32)], retired: &[&str]) -> heed::Result<Vec<RawDatabase>> {
    let names: Vec<String> = tables.iter().map(|(name, version)| format!("{name}.v{version}")).collect();
    // The common case, every table there and none to drop, needs no write transaction; the handles stay valid only
    // because the read transaction commits (`open_databases_and_commit`), which an abort of a miss does not.
    let existing = env.open_databases_and_commit(env.read_txn()?, |opener, txn| -> heed::Result<_> {
        let stale = stale_names(opener, txn, tables, retired)?;
        let opened: Option<Vec<_>> = names
            .iter()
            .map(|name| opener.open_database::<Bytes, Bytes>(txn, Some(name)))
            .collect::<heed::Result<Vec<_>>>()?
            .into_iter()
            .collect();
        Ok(match opened.filter(|_| stale.is_empty()) {
            Some(opened) => CommitOrAbort::Commit(Opened(opened)),
            None => CommitOrAbort::Abort(()),
        })
    })?;
    if let CommitOrAbort::Commit(opened) = existing {
        return Ok(opened);
    }
    let opened = env
        .create_databases_and_commit(env.write_txn()?, |opener, txn| -> heed::Result<_> {
            for name in stale_names(opener, txn, tables, retired)? {
                if let Some(database) = opener.open_database::<Bytes, Bytes>(txn, Some(&name))? {
                    // SAFETY: the handle serves nothing else; a process of an older version that still uses it gets errors,
                    // which it reads as misses.
                    unsafe { database.remove(txn)? };
                }
            }
            let opened = names
                .iter()
                .map(|name| opener.create_database::<Bytes, Bytes>(txn, Some(name)))
                .collect::<heed::Result<Vec<_>>>()?;
            Ok(CommitOrAbort::<_, Infallible>::Commit(Opened(opened)))
        })?
        .unwrap_commit();
    Ok(opened)
}

/// The tables of the environment with one of `tables`' names and an older version, or a `retired` name, from the
/// unnamed database that lists them.
fn stale_names<'e>(
    opener: &DatabaseOpener<'e, WithoutTls>,
    txn: &RoTxn<'e>,
    tables: &[(&str, u32)],
    retired: &[&str],
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
        if older {
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
