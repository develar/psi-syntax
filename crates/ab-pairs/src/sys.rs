//! The operating system as a measurement sees it: a child process timed by its own resource usage, the load average,
//! the processes busy right now. Every OS difference of a timed run and of the host's state is here; the machine a
//! result describes and the commands a run starts are the caller's.
use anyhow::{Context, Result, bail};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// One finished child: wall time from spawn to exit, CPU time (user + system, its waited-for children included) and
/// peak resident memory from the kernel's record of that one process, and the instructions retired and cycles of its
/// run where the hardware counters are readable (`counters`: what they cover differs by platform), `None` elsewhere.
/// `foreign_cpu` is the CPU time the rest of the host took meanwhile (`host_busy` over the run less the child's
/// CPU), to `HOST_TICK` of each core: what a neighbour's burst cost the run's wall.
#[derive(Debug, Clone, Default)]
pub struct Run {
    pub wall: Duration,
    pub cpu: Option<Duration>,
    pub max_rss_bytes: Option<u64>,
    pub instructions: Option<u64>,
    pub cycles: Option<u64>,
    pub foreign_cpu: Option<Duration>,
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Run {
    pub fn wall_ms(&self) -> f64 {
        self.wall.as_secs_f64() * 1000.0
    }
    pub fn cpu_ms(&self) -> f64 {
        self.cpu.map_or(0.0, |c| c.as_secs_f64() * 1000.0)
    }
    pub fn foreign_cpu_ms(&self) -> Option<f64> {
        self.foreign_cpu.map(|c| c.as_secs_f64() * 1000.0)
    }
    /// stdout then stderr, as text: what the output parsers read.
    pub fn text(&self) -> String {
        let mut s = String::from_utf8_lossy(&self.stdout).into_owned();
        s.push_str(&String::from_utf8_lossy(&self.stderr));
        s
    }
}

/// Run `cmd` to its end with captured output and time it. A non-zero exit is an error carrying the output: a fast
/// failure is not a fast answer.
pub fn run_timed(cmd: &mut Command) -> Result<Run> {
    let run = run_measured(cmd)?;
    if run.code != 0 {
        bail!("{cmd:?} exited {}:\n{}", run.code, run.text());
    }
    Ok(run)
}

/// `run_timed` without the exit check.
pub fn run_measured(cmd: &mut Command) -> Result<Run> {
    let counting = counters::Counting::arm(cmd);
    let busy = host_busy();
    let mut p = Piped::spawn(cmd)?;
    let counters = counting.started();
    let (code, cpu, max_rss_bytes, counted) = wait_with_usage(&mut p.child, counters)?;
    let foreign_cpu =
        busy.zip(host_busy()).zip(cpu).map(|((before, after), cpu)| after.since(before).saturating_sub(cpu));
    let (instructions, cycles) = counted.map_or((None, None), |(i, c)| (Some(i), Some(c)));
    Ok(Run { cpu, max_rss_bytes, instructions, cycles, foreign_cpu, ..p.finish(code) })
}

/// The clock of the host's CPU accounting: each core's busy time counts in ticks of 10 ms (`CLK_TCK`, Mach's
/// `cpu_ticks`), so a difference of two readings is off by up to a tick per core.
pub const HOST_TICK: Duration = Duration::from_millis(10);

/// The busy time of every core of the host since boot (user, nice and system), in `HOST_TICK`s: the sum over the
/// processors of `host_processor_info`'s `PROCESSOR_CPU_LOAD_INFO` on macOS (not `host_statistics`'
/// `HOST_CPU_LOAD_INFO`, which XNU rate-limits: it repeats a cached reading for about 0.75 s, then jumps), the `cpu`
/// line of `/proc/stat` on Linux (user, nice, system, irq, softirq, steal; not idle or iowait); `None` elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Busy(u64);

impl Busy {
    /// The busy time of the host between `earlier` and this reading (none across a core's 32-bit wrap on macOS, after
    /// some 16 months of uptime).
    pub fn since(self, earlier: Busy) -> Duration {
        HOST_TICK.saturating_mul(u32::try_from(self.0.saturating_sub(earlier.0)).unwrap_or(u32::MAX))
    }
}

#[cfg(target_os = "macos")]
#[allow(deprecated)] // libc points Mach's calls to the mach2 crate; the tools take no dependency for two of them
pub fn host_busy() -> Option<Busy> {
    // One send right of the host port for the process: each `mach_host_self` adds a reference to it.
    static HOST: std::sync::OnceLock<libc::mach_port_t> = std::sync::OnceLock::new();
    // SAFETY: mach_host_self has no preconditions.
    let host = *HOST.get_or_init(|| unsafe { libc::mach_host_self() });
    let (mut cpus, mut info, mut count): (libc::natural_t, libc::processor_info_array_t, libc::mach_msg_type_number_t) =
        (0, std::ptr::null_mut(), 0);
    // SAFETY: valid out-pointers; on success the kernel maps `count` integers at `info`, four ticks per processor.
    if unsafe { libc::host_processor_info(host, libc::PROCESSOR_CPU_LOAD_INFO, &mut cpus, &mut info, &mut count) }
        != libc::KERN_SUCCESS
    {
        return None;
    }
    // SAFETY: `count` integers at `info`, as the call returned them.
    let ticks = unsafe { std::slice::from_raw_parts(info, count as usize) };
    let busy = ticks
        .as_chunks::<{ libc::CPU_STATE_MAX as usize }>()
        .0
        .iter()
        .map(|t| {
            [libc::CPU_STATE_USER, libc::CPU_STATE_SYSTEM, libc::CPU_STATE_NICE]
                .iter()
                .map(|&s| t[s as usize] as u32 as u64)
                .sum::<u64>()
        })
        .sum();
    // SAFETY: the region the call mapped into this task, unmapped once; `ticks` is not used after it.
    unsafe {
        libc::vm_deallocate(
            libc::mach_task_self(),
            info as libc::vm_address_t,
            count as libc::vm_size_t * std::mem::size_of::<libc::integer_t>(),
        )
    };
    Some(Busy(busy))
}

#[cfg(target_os = "linux")]
pub fn host_busy() -> Option<Busy> {
    proc_stat_busy(&std::fs::read_to_string("/proc/stat").ok()?)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn host_busy() -> Option<Busy> {
    None
}

/// The busy ticks of `/proc/stat`'s first line: `cpu user nice system idle iowait irq softirq steal …`.
#[cfg(any(target_os = "linux", test))]
fn proc_stat_busy(stat: &str) -> Option<Busy> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line.split_whitespace().skip(1).map(|x| x.parse().ok()).collect::<Option<_>>()?;
    (v.len() >= 8).then(|| Busy(v[0] + v[1] + v[2] + v[5] + v[6] + v[7]))
}

/// Whether this host counts a child's instructions (`Run::instructions`): a trivial child's run, once per process.
pub fn counts_instructions() -> bool {
    static COUNTS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *COUNTS.get_or_init(|| run_measured(Command::new("sh").args(["-c", ":"])).is_ok_and(|r| r.instructions.is_some()))
}

/// Run an untimed child (a suite's query) to its end with captured output, killed after `timeout`: a hung answer
/// fails the suite instead of stalling it. Ctrl+C ends the wait too.
pub fn run_within(cmd: &mut Command, timeout: Duration) -> Result<Run> {
    let mut p = Piped::spawn(cmd)?;
    let status = loop {
        if let Some(status) = p.child.try_wait()? {
            break status;
        }
        if p.start.elapsed() > timeout || INTERRUPTED.load(Ordering::SeqCst) {
            let _ = p.child.kill();
            let _ = p.child.wait();
            check_interrupt()?;
            return Err(TimedOut { what: format!("{cmd:?}"), seconds: timeout.as_secs_f64() }.into());
        }
        std::thread::sleep(Duration::from_millis(2));
    };
    Ok(p.finish(status.code().unwrap_or(-1)))
}

/// A child `run_within` killed at its timeout: a caller that reports it as a cell (`>300`) tells it from other errors.
#[derive(Debug)]
pub struct TimedOut {
    pub what: String,
    pub seconds: f64,
}

impl std::fmt::Display for TimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} did not finish in {} s", self.what, self.seconds)
    }
}

impl std::error::Error for TimedOut {}

type Pipe = std::thread::JoinHandle<Vec<u8>>;

/// A started child whose pipes drain while it runs: a full pipe would stall it (and a timed stall would be measured).
struct Piped {
    start: Instant,
    child: Child,
    out: Pipe,
    err: Pipe,
}

impl Piped {
    fn spawn(cmd: &mut Command) -> Result<Piped> {
        fn reader(mut r: impl Read + Send + 'static) -> Pipe {
            std::thread::spawn(move || {
                let mut b = Vec::new();
                let _ = r.read_to_end(&mut b);
                b
            })
        }
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let start = Instant::now();
        let mut child = cmd.spawn().with_context(|| format!("cannot start {cmd:?}"))?;
        let (out, err) =
            (reader(child.stdout.take().expect("piped stdout")), reader(child.stderr.take().expect("piped stderr")));
        Ok(Piped { start, child, out, err })
    }

    /// The run of the exited child: wall time from spawn to now, its output.
    fn finish(self, code: i32) -> Run {
        let wall = self.start.elapsed();
        Run {
            wall,
            code,
            stdout: self.out.join().unwrap_or_default(),
            stderr: self.err.join().unwrap_or_default(),
            ..Run::default()
        }
    }
}

/// stdout of an untimed helper command (git, ps, sysctl); `None` when it cannot start or fails.
pub fn output(cmd: &mut Command) -> Option<String> {
    let o = cmd.stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// The exit code, CPU time, peak RSS and (instructions, cycles) of the exited child. The counters are read before
/// the child is reaped: until `wait4` its pid is a zombie of ours that no other process can take.
#[cfg(unix)]
fn wait_with_usage(
    child: &mut Child,
    counters: counters::Started,
) -> Result<(i32, Option<Duration>, Option<u64>, Option<(u64, u64)>)> {
    let pid = child.id() as libc::pid_t;
    let counted = counters.read(pid);
    let mut status: libc::c_int = 0;
    // SAFETY: a zeroed rusage is a valid value; wait4 writes it for the child it reaps.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    loop {
        // SAFETY: pid is our own unreaped child; status and usage are valid out-pointers.
        let r = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
        if r == pid {
            break;
        }
        let e = std::io::Error::last_os_error();
        if e.kind() != std::io::ErrorKind::Interrupted {
            return Err(e).context("wait4");
        }
    }
    let time = |t: libc::timeval| Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64);
    let cpu = time(usage.ru_utime) + time(usage.ru_stime);
    // Bytes on macOS, KiB on Linux.
    let rss = usage.ru_maxrss as u64 * if cfg!(target_os = "macos") { 1 } else { 1024 };
    let code = if libc::WIFEXITED(status) { libc::WEXITSTATUS(status) } else { 128 + libc::WTERMSIG(status) };
    Ok((code, Some(cpu), Some(rss), counted))
}

#[cfg(not(unix))]
fn wait_with_usage(
    child: &mut Child,
    _: counters::Started,
) -> Result<(i32, Option<Duration>, Option<u64>, Option<(u64, u64)>)> {
    let status = child.wait()?;
    Ok((status.code().unwrap_or(-1), None, None, None))
}

/// The hardware counters of one timed child: instructions retired and cycles. The two platforms count different
/// things, so compare counts of one platform only:
/// - macOS: the process alone, user and kernel together (XNU's `recount_usage_instructions` sums every level), so
///   syscalls and page faults (an mmapped shard's first touch) count; its children do not.
/// - Linux: user space alone (`exclude_kernel`), the process and its children (`inherit`).
///
/// macOS keeps the `rusage_info` of an exited process until it is reaped: `waitid(WNOWAIT)` waits for the exit
/// without reaping, `proc_pid_rusage(RUSAGE_INFO_V4)` reads `ri_instructions` and `ri_cycles` of the zombie (no root
/// needed), then `wait4` reaps it. They are the process's own, not its children's (`ri_child_*` has times only).
///
/// Linux counts with `perf_event_open`: the forked child, before its exec, opens an instructions and a cycles counter
/// on itself (`exclude_kernel`, `exclude_hv`, `inherit` into its threads and children, `enable_on_exec`, so nothing
/// before the exec counts) and passes both descriptors to the parent over a socket pair (`SCM_RIGHTS`). Nothing waits
/// across the fork: `spawn` returns only after the exec, by which time the message is queued, so the parent reads it
/// without blocking; a child that could not open them (seccomp in Docker, `perf_event_paranoid`, no PMU in a VM) sends
/// nothing and the run has `None`. A count the kernel multiplexed (running shorter than enabled) is `None` too.
mod counters {
    #[cfg(target_os = "linux")]
    pub use linux::{Counting, Started};

    #[cfg(not(target_os = "linux"))]
    pub struct Counting;
    #[cfg(not(target_os = "linux"))]
    pub struct Started;

    #[cfg(not(target_os = "linux"))]
    impl Counting {
        pub fn arm(_: &mut std::process::Command) -> Counting {
            Counting
        }
        pub fn started(self) -> Started {
            Started
        }
    }

    /// Waits for `pid`, our unreaped child, to exit, leaving it a zombie for `wait4`; false on an error.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn exited(pid: libc::pid_t) -> bool {
        loop {
            // SAFETY: a zeroed siginfo_t is valid; waitid fills it for our own child and, with WNOWAIT, leaves it
            // unreaped.
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            if unsafe { libc::waitid(libc::P_PID, pid as libc::id_t, &mut info, libc::WEXITED | libc::WNOWAIT) } == 0 {
                return true;
            }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                return false;
            }
        }
    }

    #[cfg(target_os = "macos")]
    impl Started {
        /// Waits for `pid`, our unreaped child, to exit and reads its counters; it stays a zombie for `wait4`.
        pub fn read(self, pid: libc::pid_t) -> Option<(u64, u64)> {
            if !exited(pid) {
                return None;
            }
            // SAFETY: a zeroed rusage_info_v4 is valid; proc_pid_rusage writes one of its flavor into it.
            let mut ri: libc::rusage_info_v4 = unsafe { std::mem::zeroed() };
            let r = unsafe {
                libc::proc_pid_rusage(pid, libc::RUSAGE_INFO_V4, (&mut ri as *mut libc::rusage_info_v4).cast())
            };
            (r == 0 && ri.ri_instructions > 0).then_some((ri.ri_instructions, ri.ri_cycles))
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    impl Started {
        pub fn read(self, _: i32) -> Option<(u64, u64)> {
            None
        }
    }

    #[cfg(target_os = "linux")]
    mod linux {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        use std::os::unix::process::CommandExt;

        /// `perf_event_attr` up to `PERF_ATTR_SIZE_VER5` (112 bytes), the fields set here named.
        #[repr(C)]
        struct Attr {
            kind: u32,
            size: u32,
            config: u64,
            sample_period: u64,
            sample_type: u64,
            read_format: u64,
            flags: u64,
            rest: [u64; 8],
        }

        const PERF_TYPE_HARDWARE: u32 = 0;
        /// `PERF_COUNT_HW_INSTRUCTIONS`, `PERF_COUNT_HW_CPU_CYCLES`.
        const EVENTS: [u64; 2] = [1, 0];
        /// `PERF_FORMAT_TOTAL_TIME_ENABLED | PERF_FORMAT_TOTAL_TIME_RUNNING`: a read is (value, enabled, running).
        const READ_FORMAT: u64 = 1 | 2;
        /// disabled, inherit, exclude_kernel, exclude_hv, enable_on_exec.
        const FLAGS: u64 = 1 | 1 << 1 | 1 << 5 | 1 << 6 | 1 << 12;
        const PERF_FLAG_FD_CLOEXEC: libc::c_ulong = 8;
        /// Room for the control message of two descriptors.
        const CMSG_BYTES: usize = 64;

        /// A socket pair whose child end the command's pre-exec hook sends the counters over.
        pub struct Counting(Option<OwnedFd>);
        /// The counters of the started child, if it sent them.
        pub struct Started(Option<[OwnedFd; 2]>);

        impl Counting {
            /// Hooks `cmd` to open the counters in its child. The child end lives in the hook (closed on exec in the
            /// child, `SOCK_CLOEXEC`), so a reused command never sends to a descriptor number closed since.
            pub fn arm(cmd: &mut std::process::Command) -> Counting {
                let mut fds = [0; 2];
                // SAFETY: two slots for the pair.
                if unsafe {
                    libc::socketpair(libc::AF_UNIX, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0, fds.as_mut_ptr())
                } != 0
                {
                    return Counting(None);
                }
                // SAFETY: socketpair just opened both; each is owned once.
                let (parent, child) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
                // SAFETY: the hook runs between fork and exec and only makes system calls on stack memory: no
                // allocation, no lock.
                unsafe {
                    cmd.pre_exec(move || {
                        send_counters(child.as_raw_fd());
                        Ok(())
                    });
                }
                Counting(Some(parent))
            }

            /// After `spawn` returned (the child has exec'd): the descriptors it sent, without waiting.
            pub fn started(self) -> Started {
                Started(self.0.and_then(|s| receive(s.as_raw_fd())))
            }
        }

        impl Started {
            /// The totals once `pid` has exited (its inherited counts are folded in at exit); the caller reaps it.
            pub fn read(self, pid: libc::pid_t) -> Option<(u64, u64)> {
                let fds = self.0?;
                if !super::exited(pid) {
                    return None;
                }
                let read = |fd: &OwnedFd| {
                    let mut v = [0u64; 3];
                    // SAFETY: room for (value, enabled, running).
                    let n = unsafe { libc::read(fd.as_raw_fd(), v.as_mut_ptr().cast(), 24) };
                    (n == 24 && v[1] > 0 && v[2] == v[1]).then_some(v[0])
                };
                Some((read(&fds[0])?, read(&fds[1])?)).filter(|&(i, _)| i > 0)
            }
        }

        /// In the forked child: both counters on itself, sent together or not at all, then closed here (the
        /// parent's copies keep them).
        fn send_counters(sock: libc::c_int) {
            let mut fds = [-1; 2];
            for (fd, config) in fds.iter_mut().zip(EVENTS) {
                let attr = Attr {
                    kind: PERF_TYPE_HARDWARE,
                    size: std::mem::size_of::<Attr>() as u32,
                    config,
                    sample_period: 0,
                    sample_type: 0,
                    read_format: READ_FORMAT,
                    flags: FLAGS,
                    rest: [0; 8],
                };
                // SAFETY: a valid attr; pid 0 and cpu -1 count this process on any CPU.
                *fd = unsafe {
                    libc::syscall(libc::SYS_perf_event_open, &attr as *const Attr, 0, -1, -1, PERF_FLAG_FD_CLOEXEC)
                } as libc::c_int;
            }
            if fds.iter().all(|&fd| fd >= 0) {
                let mut env = Envelope::default();
                // SAFETY: the header points into `env`, which outlives sendmsg; the control message of two ints fits
                // its space.
                unsafe {
                    let msg = env.header(libc::CMSG_SPACE(8) as usize);
                    let c = libc::CMSG_FIRSTHDR(&msg);
                    (*c).cmsg_level = libc::SOL_SOCKET;
                    (*c).cmsg_type = libc::SCM_RIGHTS;
                    (*c).cmsg_len = libc::CMSG_LEN(8) as _;
                    std::ptr::copy_nonoverlapping(fds.as_ptr().cast::<u8>(), libc::CMSG_DATA(c), 8);
                    libc::sendmsg(sock, &msg, libc::MSG_DONTWAIT);
                }
            }
            for fd in fds.into_iter().filter(|&fd| fd >= 0) {
                // SAFETY: opened above, closed once.
                unsafe { libc::close(fd) };
            }
        }

        /// The buffers of a one-byte message with room for two descriptors, on the stack of who sends or receives it.
        #[derive(Default)]
        struct Envelope {
            byte: [u8; 1],
            space: [u64; CMSG_BYTES / 8],
            iov: Option<libc::iovec>,
        }

        impl Envelope {
            /// A header over this envelope's byte and `control` bytes of its space; valid while the envelope is
            /// neither moved nor dropped.
            fn header(&mut self, control: usize) -> libc::msghdr {
                let iov = self.iov.insert(libc::iovec { iov_base: self.byte.as_mut_ptr().cast(), iov_len: 1 });
                // SAFETY: a zeroed msghdr is valid: no name, no buffers until set here.
                let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
                msg.msg_iov = iov;
                msg.msg_iovlen = 1;
                msg.msg_control = self.space.as_mut_ptr().cast();
                msg.msg_controllen = control as _;
                msg
            }
        }

        /// The two descriptors queued on `sock`, if any.
        fn receive(sock: libc::c_int) -> Option<[OwnedFd; 2]> {
            let mut env = Envelope::default();
            // SAFETY: the header points into `env`, which outlives recvmsg; the kernel installs the descriptors
            // (close-on-exec) and they are owned once below.
            unsafe {
                let mut msg = env.header(CMSG_BYTES);
                if libc::recvmsg(sock, &mut msg, libc::MSG_DONTWAIT | libc::MSG_CMSG_CLOEXEC) < 1 {
                    return None;
                }
                let c = libc::CMSG_FIRSTHDR(&msg);
                if c.is_null()
                    || (*c).cmsg_level != libc::SOL_SOCKET
                    || (*c).cmsg_type != libc::SCM_RIGHTS
                    || (*c).cmsg_len as usize != libc::CMSG_LEN(8) as usize
                {
                    return None;
                }
                let mut fds = [0 as libc::c_int; 2];
                std::ptr::copy_nonoverlapping(libc::CMSG_DATA(c), fds.as_mut_ptr().cast::<u8>(), 8);
                Some([OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])])
            }
        }
    }
}

/// The 1-minute load average (`None` where the OS has none).
pub fn load_average() -> Option<f64> {
    #[cfg(unix)]
    {
        let mut l = [0f64; 3];
        // SAFETY: three slots for three averages.
        (unsafe { libc::getloadavg(l.as_mut_ptr(), 3) } >= 1).then_some(l[0])
    }
    #[cfg(not(unix))]
    None
}

/// Logical CPUs online, as Python's `os.cpu_count()` counts them (not the cgroup quota): the `host` key must agree
/// with the results filed by the Python suites.
pub fn cpu_count() -> usize {
    #[cfg(unix)]
    {
        // SAFETY: sysconf has no preconditions.
        let n = unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) };
        if n > 0 {
            return n as usize;
        }
    }
    std::thread::available_parallelism().map_or(1, |n| n.get())
}

/// Is a process with this id alive (ours or another user's)?
pub fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: signal 0 checks existence and permission only.
        unsafe {
            libc::kill(pid as libc::pid_t, 0) == 0
                || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        false
    }
}

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

/// Ctrl+C no longer ends this process at once: the children die of it, their runs fail, and every scene's restore
/// (a `Drop`) runs on the way out. A handler, not `SIG_IGN`: an ignored signal would stay ignored in the children.
pub fn catch_interrupt() {
    #[cfg(unix)]
    {
        extern "C" fn on_interrupt(_: libc::c_int) {
            INTERRUPTED.store(true, Ordering::SeqCst);
        }
        // SAFETY: the handler only stores an atomic.
        unsafe { libc::signal(libc::SIGINT, on_interrupt as *const () as libc::sighandler_t) };
    }
}

/// Fails once Ctrl+C was pressed: the waits of a scene check it between their sleeps.
pub fn check_interrupt() -> Result<()> {
    if INTERRUPTED.load(Ordering::SeqCst) {
        bail!("interrupted");
    }
    Ok(())
}

/// Sleep, ending early with an error on Ctrl+C.
pub fn sleep(d: Duration) -> Result<()> {
    let end = Instant::now() + d;
    while Instant::now() < end {
        check_interrupt()?;
        std::thread::sleep((end - Instant::now()).min(Duration::from_millis(100)));
    }
    check_interrupt()
}

/// One row of `ps`.
#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    pub cpu_percent: f64,
    pub command: String,
}

/// Every process with its parent and its share of a core right now (`ps`; empty where there is none).
pub fn processes() -> Vec<Process> {
    let Some(out) = output(Command::new("ps").args(["-axo", "pid=,ppid=,%cpu=,comm="])) else { return Vec::new() };
    parse_ps(&out)
}

fn parse_ps(out: &str) -> Vec<Process> {
    out.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?.parse().ok()?;
            let ppid = parts.next()?.parse().ok()?;
            let cpu_percent = parts.next()?.replace(',', ".").parse().ok()?;
            let command = parts.collect::<Vec<_>>().join(" ");
            Some(Process { pid, ppid, cpu_percent, command })
        })
        .collect()
}

/// Processes at or above `min_cpu` percent of a core, this process, its ancestors (the agent's shell, cargo) and its
/// descendants left out.
pub fn busy_processes(all: &[Process], me: u32, min_cpu: f64) -> Vec<String> {
    let parent = |pid: u32| all.iter().find(|p| p.pid == pid).map(|p| p.ppid);
    let mut mine = vec![me];
    let mut at = me;
    while let Some(p) = parent(at).filter(|&p| p > 1 && !mine.contains(&p)) {
        mine.push(p);
        at = p;
    }
    let descends = |mut pid: u32| {
        for _ in 0..64 {
            if pid == me {
                return true;
            }
            match parent(pid) {
                Some(p) if p > 1 && p != pid => pid = p,
                _ => return false,
            }
        }
        false
    };
    all.iter()
        .filter(|p| p.cpu_percent >= min_cpu && !mine.contains(&p.pid) && !descends(p.pid))
        .map(|p| format!("{}% pid {} {}", p.cpu_percent, p.pid, p.command))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(program: &str) -> Command {
        Command::new(program)
    }

    #[test]
    fn a_nonzero_exit_is_not_a_fast_measurement() {
        let err = run_timed(command("sh").args(["-c", "echo out; exit 7"])).unwrap_err().to_string();
        assert!(err.contains("exited 7") && err.contains("out"), "{err}");
    }

    #[test]
    fn a_child_reports_its_own_cpu_time() {
        let run = run_timed(command("sh").args(["-c", "i=0; while [ $i -lt 200000 ]; do i=$((i+1)); done; echo done"]))
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "done");
        if cfg!(unix) {
            assert!(run.cpu.unwrap() > Duration::ZERO && run.max_rss_bytes.unwrap() > 0);
        }
    }

    /// A shell loop of 10⁵ iterations retires about ten times the instructions of one of 10⁴ (each over the shell's
    /// start, a loop of none), the same within 1 % run to run; instructions and cycles come together or not at all,
    /// and macOS always has them.
    #[test]
    fn a_child_reports_its_instructions_and_cycles() {
        let counted = |n: u32| {
            let run = run_timed(command("sh").args(["-c", &format!("i=0; while [ $i -lt {n} ]; do i=$((i+1)); done")]))
                .unwrap();
            assert_eq!(run.instructions.is_some(), run.cycles.is_some(), "{run:?}");
            run.instructions.map(|i| (i as f64, run.cycles.unwrap()))
        };
        let Some((start, _)) = counted(0) else {
            if cfg!(target_os = "macos") {
                panic!("macOS has the counters");
            }
            return;
        };
        let small: Vec<f64> = (0..3).map(|_| counted(10_000).unwrap().0 - start).collect();
        let large: Vec<f64> = (0..3).map(|_| counted(100_000).unwrap().0 - start).collect();
        let ratio = crate::stats::median(&large) / crate::stats::median(&small);
        assert!((9.0..11.0).contains(&ratio), "{ratio}: {small:?} {large:?}");
        let spread = |v: &[f64]| {
            (v.iter().copied().fold(f64::MIN, f64::max) - v.iter().copied().fold(f64::MAX, f64::min))
                / crate::stats::median(v)
        };
        assert!(spread(&large) < 0.01, "{large:?}");
        assert!(counted(100_000).unwrap().1 > 0);
    }

    /// The host's busy time counts the child (a CPU-bound child's run is at least its own CPU, to a tick per core) and
    /// the rest of the host as foreign: a thread of this process spinning through a sleeping child's run is in its
    /// `foreign_cpu`, as much as the thread got of a core, to a tick per core. A loaded host only adds to both, so the
    /// bounds are lower ones.
    #[test]
    fn a_run_sees_the_host_busy_beside_it() {
        if host_busy().is_none() {
            if cfg!(any(target_os = "macos", target_os = "linux")) {
                panic!("macOS and Linux account the host's CPU");
            }
            return;
        }
        let slack = HOST_TICK * cpu_count() as u32;
        let before = host_busy().unwrap();
        let run = run_timed(command("sh").args(["-c", "i=0; while [ $i -lt 300000 ]; do i=$((i+1)); done"])).unwrap();
        let busy = host_busy().unwrap().since(before);
        assert!(busy + slack >= run.cpu.unwrap(), "the host's {busy:?} holds the child's {:?}", run.cpu);
        assert!(
            busy <= (run.wall + Duration::from_millis(5)) * cpu_count() as u32 + slack,
            "the host's {busy:?} in {:?}",
            run.wall
        );
        assert!(run.foreign_cpu.is_some(), "{run:?}");
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let spinning = stop.clone();
        let spinner = std::thread::spawn(move || {
            let mut x = 0u64;
            while !spinning.load(Ordering::Relaxed) {
                x = std::hint::black_box(x.wrapping_add(1));
            }
            // SAFETY: a zeroed timespec is valid; the call fills it for this thread.
            let mut t: libc::timespec = unsafe { std::mem::zeroed() };
            unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
            Duration::new(t.tv_sec as u64, t.tv_nsec as u32)
        });
        std::thread::sleep(Duration::from_millis(20));
        let run = run_timed(command("sh").args(["-c", "sleep 0.5"])).unwrap();
        stop.store(true, Ordering::Relaxed);
        let spun = spinner.join().unwrap();
        let foreign = run.foreign_cpu.unwrap();
        // The spinner ran 20 ms before the child and a moment after it.
        let inside = spun.saturating_sub(Duration::from_millis(60));
        assert!(foreign + slack >= inside, "foreign {foreign:?} against the spinner's {spun:?} (slack {slack:?})");
        // No reading lags and jumps: the host has no more CPU in a run than its cores over its wall.
        assert!(foreign <= run.wall * cpu_count() as u32 + slack, "foreign {foreign:?} in {:?}", run.wall);
        assert!(run.foreign_cpu_ms().unwrap() >= 0.0);
    }

    #[test]
    fn proc_stat_counts_busy_ticks_and_not_idle() {
        let stat = "cpu  100 5 50 9000 70 3 2 1 0 0\ncpu0 50 2 25 4500 35 1 1 0 0 0\n";
        assert_eq!(proc_stat_busy(stat), Some(Busy(161)));
        assert_eq!(proc_stat_busy("cpu  1 2 3"), None);
        assert_eq!(Busy(161).since(Busy(100)), Duration::from_millis(610));
        assert_eq!(Busy(5).since(Busy(100)), Duration::ZERO, "a wrapped core is no busy time");
    }

    #[test]
    fn busy_leaves_out_this_process_its_ancestors_and_its_children() {
        let ps = parse_ps(
            "  1 0 0.0 launchd\n 10 1 90.0 zsh\n 20 10 99.0 cargo\n 30 20 0,5 accio-bench\n 40 30 150.0 accio\n 50 1 60.0 cc1\n 60 1 10.0 idle\n",
        );
        assert_eq!(ps[3].cpu_percent, 0.5);
        assert_eq!(busy_processes(&ps, 30, 50.0), vec!["60% pid 50 cc1".to_string()]);
    }
}
