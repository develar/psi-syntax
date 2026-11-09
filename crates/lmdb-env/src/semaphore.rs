//! A System V semaphore set of its own for each environment on macOS. LMDB locks an environment there with the set of key
//! `ftok(lock.mdb, 'M')`, and macOS's `ftok` keeps only the low 16 bits of the inode and the low 8 of the device: two
//! lock files of one volume can share a set. The first user of the second then takes the set and resets it, and the
//! last user of either removes it under the other, whose later opens and transactions fail with `EINVAL`. Branch Deck's
//! cache went dark so while thousands of environments of tests came and went.
//!
//! Before LMDB first opens an environment, while no process holds its lock file, a lock file whose key names a set
//! already is made again, with another inode, until its key is free. Opens and closes run under the directory's
//! `open.lock` (`open_env`), so no process opens the environment between this check and LMDB's first open, or while the
//! close of its last user removes its set.

use std::path::Path;

/// The lock files made again at most before the set of the last one is shared, as before.
#[cfg(target_os = "macos")]
const MAX_ATTEMPTS: usize = 8;

/// Give the lock file of the environment in `dir` a key no other set holds, if no process has it open; a failure
/// leaves the lock file as it is.
#[cfg(target_os = "macos")]
pub(crate) fn claim(dir: &Path) {
    let _ = claim_key(&dir.join("lock.mdb"));
}

/// Other platforms lock without System V semaphores: robust mutexes on Linux, named mutexes on Windows.
#[cfg(not(target_os = "macos"))]
pub(crate) fn claim(_dir: &Path) {}

#[cfg(target_os = "macos")]
fn claim_key(path: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::OpenOptionsExt;

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    for _ in 0..MAX_ATTEMPTS {
        // The mode LMDB gives the lock file it makes.
        let file =
            std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).mode(0o600).open(path)?;
        if !try_lock_exclusive(&file)? {
            // Another process has the environment open: this lock file's set is the one its users share.
            return Ok(());
        }
        if !set_exists(key(&c_path)?) {
            // Closing `file` releases the lock before LMDB opens it: a close of another descriptor of the lock file would
            // drop the locks LMDB takes on it in this process.
            return Ok(());
        }
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// The exclusive lock LMDB takes to tell it is the only user (`mdb_env_excl_lock`: the first byte, `F_SETLK`).
#[cfg(target_os = "macos")]
fn try_lock_exclusive(file: &std::fs::File) -> std::io::Result<bool> {
    use std::os::fd::AsRawFd;

    // SAFETY: `flock` is a plain C struct of integers, valid zeroed.
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = libc::F_WRLCK as libc::c_short;
    lock.l_whence = libc::SEEK_SET as libc::c_short;
    lock.l_start = 0;
    lock.l_len = 1;
    // SAFETY: the descriptor of `file` is open for the call, and `lock` outlives it.
    let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &raw const lock) };
    if result == 0 {
        return Ok(true);
    }
    // `EAGAIN` or `EACCES`: another process holds the byte.
    let error = std::io::Error::last_os_error();
    match error.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::PermissionDenied => Ok(false),
        _ => Err(error),
    }
}

/// The key LMDB derives for the lock file's semaphore set.
#[cfg(target_os = "macos")]
fn key(path: &std::ffi::CStr) -> std::io::Result<libc::key_t> {
    // SAFETY: `path` is a NUL-terminated string that outlives the call.
    let key = unsafe { libc::ftok(path.as_ptr(), libc::c_int::from(b'M')) };
    if key == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(key)
}

/// Does a semaphore set of `key` exist?
#[cfg(target_os = "macos")]
fn set_exists(key: libc::key_t) -> bool {
    // SAFETY: `semget` with no flags only looks the set up.
    unsafe { libc::semget(key, 0, 0) != -1 }
}
