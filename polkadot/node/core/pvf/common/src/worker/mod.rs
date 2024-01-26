// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Functionality common to both prepare and execute workers.

pub mod security;

use crate::{framed_recv_blocking, SecurityStatus, WorkerHandshake, LOG_TARGET};
use cpu_time::ProcessTime;
use futures::never::Never;
use parity_scale_codec::Decode;
use std::{
	any::Any,
	fmt::{self},
	fs::File,
	io::{self, Read, Write},
	os::{
		fd::{AsRawFd, FromRawFd, RawFd},
		unix::net::UnixStream,
	},
	path::PathBuf,
	sync::mpsc::{Receiver, RecvTimeoutError},
	time::Duration,
};

/// Use this macro to declare a `fn main() {}` that will create an executable that can be used for
/// spawning the desired worker.
#[macro_export]
macro_rules! decl_worker_main {
	($expected_command:expr, $entrypoint:expr, $worker_version:expr, $worker_version_hash:expr $(,)*) => {
		fn get_full_version() -> String {
			format!("{}-{}", $worker_version, $worker_version_hash)
		}

		fn print_help(expected_command: &str) {
			println!("{} {}", expected_command, $worker_version);
			println!("commit: {}", $worker_version_hash);
			println!();
			println!("PVF worker that is called by polkadot.");
		}

		fn main() {
			#[cfg(target_os = "linux")]
			use $crate::worker::security;

			$crate::sp_tracing::try_init_simple();

			let worker_pid = std::process::id();

			let args = std::env::args().collect::<Vec<_>>();
			if args.len() == 1 {
				print_help($expected_command);
				return
			}

			match args[1].as_ref() {
				"--help" | "-h" => {
					print_help($expected_command);
					return
				},
				"--version" | "-v" => {
					println!("{}", $worker_version);
					return
				},
				// Useful for debugging. --version is used for version checks.
				"--full-version" => {
					println!("{}", get_full_version());
					return
				},

				"--check-can-enable-landlock" => {
					#[cfg(target_os = "linux")]
					let status = if let Err(err) = security::landlock::check_can_fully_enable() {
						// Write the error to stderr, log it on the host-side.
						eprintln!("{}", err);
						-1
					} else {
						0
					};
					#[cfg(not(target_os = "linux"))]
					let status = -1;
					std::process::exit(status)
				},
				"--check-can-enable-seccomp" => {
					#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
					let status = if let Err(err) = security::seccomp::check_can_fully_enable() {
						// Write the error to stderr, log it on the host-side.
						eprintln!("{}", err);
						-1
					} else {
						0
					};
					#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
					let status = -1;
					std::process::exit(status)
				},
				"--check-can-unshare-user-namespace-and-change-root" => {
					#[cfg(target_os = "linux")]
					let cache_path_tempdir = std::path::Path::new(&args[2]);
					#[cfg(target_os = "linux")]
					let status = if let Err(err) =
						security::change_root::check_can_fully_enable(&cache_path_tempdir)
					{
						// Write the error to stderr, log it on the host-side.
						eprintln!("{}", err);
						-1
					} else {
						0
					};
					#[cfg(not(target_os = "linux"))]
					let status = -1;
					std::process::exit(status)
				},
				"--check-can-do-secure-clone" => {
					#[cfg(target_os = "linux")]
					// SAFETY: new process is spawned within a single threaded process. This
					// invariant is enforced by tests.
					let status = if let Err(err) = unsafe { security::clone::check_can_fully_clone() } {
						// Write the error to stderr, log it on the host-side.
						eprintln!("{}", err);
						-1
					} else {
						0
					};
					#[cfg(not(target_os = "linux"))]
					let status = -1;
					std::process::exit(status)
				},

				"test-sleep" => {
					std::thread::sleep(std::time::Duration::from_secs(5));
					return
				},

				subcommand => {
					// Must be passed for compatibility with the single-binary test workers.
					if subcommand != $expected_command {
						panic!(
							"trying to run {} binary with the {} subcommand",
							$expected_command, subcommand
						)
					}
				},
			}

			let mut socket_path = None;
			let mut worker_dir_path = None;
			let mut node_version = None;

			let mut i = 2;
			while i < args.len() {
				match args[i].as_ref() {
					"--socket-path" => {
						socket_path = Some(args[i + 1].as_str());
						i += 1
					},
					"--worker-dir-path" => {
						worker_dir_path = Some(args[i + 1].as_str());
						i += 1
					},
					"--node-impl-version" => {
						node_version = Some(args[i + 1].as_str());
						i += 1
					},
					arg => panic!("Unexpected argument found: {}", arg),
				}
				i += 1;
			}
			let socket_path = socket_path.expect("the --socket-path argument is required");
			let worker_dir_path =
				worker_dir_path.expect("the --worker-dir-path argument is required");

			let socket_path = std::path::Path::new(socket_path).to_owned();
			let worker_dir_path = std::path::Path::new(worker_dir_path).to_owned();

			$entrypoint(socket_path, worker_dir_path, node_version, Some($worker_version));
		}
	};
}

//taken from the os_pipe crate. Copied here to reduce one dependency and
// because its type-safe abstractions do not play well with nix's clone
#[cfg(not(target_os = "macos"))]
pub fn pipe2_cloexec() -> io::Result<(libc::c_int, libc::c_int)> {
	let mut fds: [libc::c_int; 2] = [0; 2];
	let res = unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) };
	if res != 0 {
		return Err(io::Error::last_os_error())
	}
	Ok((fds[0], fds[1]))
}

#[cfg(target_os = "macos")]
pub fn pipe2_cloexec() -> io::Result<(libc::c_int, libc::c_int)> {
	let mut fds: [libc::c_int; 2] = [0; 2];
	let res = unsafe { libc::pipe(fds.as_mut_ptr()) };
	if res != 0 {
		return Err(io::Error::last_os_error())
	}
	let res = unsafe { libc::fcntl(fds[0], libc::F_SETFD, libc::FD_CLOEXEC) };
	if res != 0 {
		return Err(io::Error::last_os_error())
	}
	let res = unsafe { libc::fcntl(fds[1], libc::F_SETFD, libc::FD_CLOEXEC) };
	if res != 0 {
		return Err(io::Error::last_os_error())
	}
	Ok((fds[0], fds[1]))
}

/// A wrapper around a file descriptor used to encapsulate and restrict
/// functionality for pipe operations.
pub struct PipeFd {
	file: File,
}

impl AsRawFd for PipeFd {
	/// Returns the raw file descriptor associated with this `PipeFd`
	fn as_raw_fd(&self) -> RawFd {
		self.file.as_raw_fd()
	}
}

impl FromRawFd for PipeFd {
	/// Creates a new `PipeFd` instance from a raw file descriptor.
	///
	/// # Safety
	///
	/// The fd passed in must be an owned file descriptor; in particular, it must be open.
	unsafe fn from_raw_fd(fd: RawFd) -> Self {
		PipeFd { file: File::from_raw_fd(fd) }
	}
}

impl Read for PipeFd {
	fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		self.file.read(buf)
	}

	fn read_to_end(&mut self, buf: &mut Vec<u8>) -> io::Result<usize> {
		self.file.read_to_end(buf)
	}
}

impl Write for PipeFd {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		self.file.write(buf)
	}

	fn flush(&mut self) -> io::Result<()> {
		self.file.flush()
	}

	fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
		self.file.write_all(buf)
	}
}

/// Some allowed overhead that we account for in the "CPU time monitor" thread's sleeps, on the
/// child process.
pub const JOB_TIMEOUT_OVERHEAD: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy)]
pub enum WorkerKind {
	Prepare,
	Execute,
	CheckPivotRoot,
}

impl fmt::Display for WorkerKind {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Prepare => write!(f, "prepare"),
			Self::Execute => write!(f, "execute"),
			Self::CheckPivotRoot => write!(f, "check pivot root"),
		}
	}
}

#[derive(Debug)]
pub struct WorkerInfo {
	pub pid: u32,
	pub kind: WorkerKind,
	pub version: Option<String>,
	pub worker_dir_path: PathBuf,
}

// NOTE: The worker version must be passed in so that we accurately get the version of the worker,
// and not the version that this crate was compiled with.
//
// NOTE: This must not spawn any threads due to safety requirements in `event_loop` and to avoid
// errors in [`security::change_root::try_restrict`].
//
/// Initializes the worker process, then runs the given event loop, which spawns a new job process
/// to securely handle each incoming request.
pub fn run_worker<F>(
	worker_kind: WorkerKind,
	socket_path: PathBuf,
	worker_dir_path: PathBuf,
	node_version: Option<&str>,
	worker_version: Option<&str>,
	mut event_loop: F,
) where
	F: FnMut(UnixStream, &WorkerInfo, SecurityStatus) -> io::Result<Never>,
{
	#[cfg_attr(not(target_os = "linux"), allow(unused_mut))]
	let mut worker_info = WorkerInfo {
		pid: std::process::id(),
		kind: worker_kind,
		version: worker_version.map(|v| v.to_string()),
		worker_dir_path,
	};
	gum::debug!(
		target: LOG_TARGET,
		?worker_info,
		?socket_path,
		"starting pvf worker ({})",
		worker_info.kind
	);

	// Check for a mismatch between the node and worker versions.
	if let (Some(node_version), Some(worker_version)) = (node_version, &worker_info.version) {
		if node_version != worker_version {
			gum::error!(
				target: LOG_TARGET,
				?worker_info,
				%node_version,
				"Node and worker version mismatch, node needs restarting, forcing shutdown",
			);
			kill_parent_node_in_emergency();
			worker_shutdown(worker_info, "Version mismatch");
		}
	}

	// Make sure that we can read the worker dir path, and log its contents.
	let entries: io::Result<Vec<_>> = std::fs::read_dir(&worker_info.worker_dir_path)
		.and_then(|d| d.map(|res| res.map(|e| e.file_name())).collect());
	match entries {
		Ok(entries) =>
			gum::trace!(target: LOG_TARGET, ?worker_info, "content of worker dir: {:?}", entries),
		Err(err) => {
			let err = format!("Could not read worker dir: {}", err.to_string());
			worker_shutdown_error(worker_info, &err);
		},
	}

	// Connect to the socket.
	let stream = || -> io::Result<UnixStream> {
		let stream = UnixStream::connect(&socket_path)?;
		let _ = std::fs::remove_file(&socket_path);
		Ok(stream)
	}();
	let mut stream = match stream {
		Ok(ok) => ok,
		Err(err) => worker_shutdown_error(worker_info, &err.to_string()),
	};

	let WorkerHandshake { security_status } = match recv_worker_handshake(&mut stream) {
		Ok(ok) => ok,
		Err(err) => worker_shutdown_error(worker_info, &err.to_string()),
	};

	// Enable some security features.
	{
		gum::trace!(target: LOG_TARGET, ?security_status, "Enabling security features");

		// First, make sure env vars were cleared, to match the environment we perform the checks
		// within. (In theory, running checks with different env vars could result in different
		// outcomes of the checks.)
		if !security::check_env_vars_were_cleared(&worker_info) {
			let err = "not all env vars were cleared when spawning the process";
			gum::error!(
				target: LOG_TARGET,
				?worker_info,
				"{}",
				err
			);
			if security_status.secure_validator_mode {
				worker_shutdown(worker_info, err);
			}
		}

		// Call based on whether we can change root. Error out if it should work but fails.
		//
		// NOTE: This should not be called in a multi-threaded context (i.e. inside the tokio
		// runtime). `unshare(2)`:
		//
		//       > CLONE_NEWUSER requires that the calling process is not threaded.
		#[cfg(target_os = "linux")]
		if security_status.can_unshare_user_namespace_and_change_root {
			if let Err(err) = security::change_root::enable_for_worker(&worker_info) {
				// The filesystem may be in an inconsistent state, always bail out.
				let err = format!("Could not change root to be the worker cache path: {}", err);
				worker_shutdown_error(worker_info, &err);
			}
			worker_info.worker_dir_path = std::path::Path::new("/").to_owned();
		}

		#[cfg(target_os = "linux")]
		if security_status.can_enable_landlock {
			if let Err(err) = security::landlock::enable_for_worker(&worker_info) {
				// We previously were able to enable, so this should never happen. Shutdown if
				// running in secure mode.
				let err = format!("could not fully enable landlock: {:?}", err);
				gum::error!(
					target: LOG_TARGET,
					?worker_info,
					"{}. This should not happen, please report an issue",
					err
				);
				if security_status.secure_validator_mode {
					worker_shutdown(worker_info, &err);
				}
			}
		}

		// TODO: We can enable the seccomp networking blacklist on aarch64 as well, but we need a CI
		//       job to catch regressions. See issue ci_cd/issues/609.
		#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
		if security_status.can_enable_seccomp {
			if let Err(err) = security::seccomp::enable_for_worker(&worker_info) {
				// We previously were able to enable, so this should never happen. Shutdown if
				// running in secure mode.
				let err = format!("could not fully enable seccomp: {:?}", err);
				gum::error!(
					target: LOG_TARGET,
					?worker_info,
					"{}. This should not happen, please report an issue",
					err
				);
				if security_status.secure_validator_mode {
					worker_shutdown(worker_info, &err);
				}
			}
		}
	}

	// Run the main worker loop.
	let err = event_loop(stream, &worker_info, security_status)
		// It's never `Ok` because it's `Ok(Never)`.
		.unwrap_err();

	worker_shutdown(worker_info, &err.to_string());
}

/// Provide a consistent message on unexpected worker shutdown.
fn worker_shutdown(worker_info: WorkerInfo, err: &str) -> ! {
	gum::warn!(target: LOG_TARGET, ?worker_info, "quitting pvf worker ({}): {}", worker_info.kind, err);
	std::process::exit(1);
}

/// Provide a consistent error on unexpected worker shutdown.
fn worker_shutdown_error(worker_info: WorkerInfo, err: &str) -> ! {
	gum::error!(target: LOG_TARGET, ?worker_info, "quitting pvf worker ({}): {}", worker_info.kind, err);
	std::process::exit(1);
}

/// Loop that runs in the CPU time monitor thread on prepare and execute jobs. Continuously wakes up
/// and then either blocks for the remaining CPU time, or returns if we exceed the CPU timeout.
///
/// Returning `Some` indicates that we should send a `TimedOut` error to the host. Will return
/// `None` if the other thread finishes first, without us timing out.
///
/// NOTE: Sending a `TimedOut` error to the host will cause the worker, whether preparation or
/// execution, to be killed by the host. We do not kill the process here because it would interfere
/// with the proper handling of this error.
pub fn cpu_time_monitor_loop(
	cpu_time_start: ProcessTime,
	timeout: Duration,
	finished_rx: Receiver<()>,
) -> Option<Duration> {
	loop {
		let cpu_time_elapsed = cpu_time_start.elapsed();

		// Treat the timeout as CPU time, which is less subject to variance due to load.
		if cpu_time_elapsed <= timeout {
			// Sleep for the remaining CPU time, plus a bit to account for overhead. (And we don't
			// want to wake up too often -- so, since we just want to halt the worker thread if it
			// stalled, we can sleep longer than necessary.) Note that the sleep is wall clock time.
			// The CPU clock may be slower than the wall clock.
			let sleep_interval = timeout.saturating_sub(cpu_time_elapsed) + JOB_TIMEOUT_OVERHEAD;
			match finished_rx.recv_timeout(sleep_interval) {
				// Received finish signal.
				Ok(()) => return None,
				// Timed out, restart loop.
				Err(RecvTimeoutError::Timeout) => continue,
				Err(RecvTimeoutError::Disconnected) => return None,
			}
		}

		return Some(cpu_time_elapsed)
	}
}

/// Attempt to convert an opaque panic payload to a string.
///
/// This is a best effort, and is not guaranteed to provide the most accurate value.
pub fn stringify_panic_payload(payload: Box<dyn Any + Send + 'static>) -> String {
	match payload.downcast::<&'static str>() {
		Ok(msg) => msg.to_string(),
		Err(payload) => match payload.downcast::<String>() {
			Ok(msg) => *msg,
			// At least we tried...
			Err(_) => "unknown panic payload".to_string(),
		},
	}
}

/// In case of node and worker version mismatch (as a result of in-place upgrade), send `SIGTERM`
/// to the node to tear it down and prevent it from raising disputes on valid candidates. Node
/// restart should be handled by the node owner. As node exits, Unix sockets opened to workers
/// get closed by the OS and other workers receive error on socket read and also exit. Preparation
/// jobs are written to the temporary files that are renamed to real artifacts on the node side, so
/// no leftover artifacts are possible.
fn kill_parent_node_in_emergency() {
	unsafe {
		// SAFETY: `getpid()` never fails but may return "no-parent" (0) or "parent-init" (1) in
		// some corner cases, which is checked. `kill()` never fails.
		let ppid = libc::getppid();
		if ppid > 1 {
			libc::kill(ppid, libc::SIGTERM);
		}
	}
}

/// Receives a handshake with information for the worker.
fn recv_worker_handshake(stream: &mut UnixStream) -> io::Result<WorkerHandshake> {
	let worker_handshake = framed_recv_blocking(stream)?;
	let worker_handshake = WorkerHandshake::decode(&mut &worker_handshake[..]).map_err(|e| {
		io::Error::new(
			io::ErrorKind::Other,
			format!("recv_worker_handshake: failed to decode WorkerHandshake: {}", e),
		)
	})?;
	Ok(worker_handshake)
}

/// Functionality related to threads spawned by the workers.
///
/// The motivation for this module is to coordinate worker threads without using async Rust.
pub mod thread {
	use std::{
		io, panic,
		sync::{Arc, Condvar, Mutex},
		thread,
		time::Duration,
	};

	/// Contains the outcome of waiting on threads, or `Pending` if none are ready.
	#[derive(Debug, Clone, Copy)]
	pub enum WaitOutcome {
		Finished,
		TimedOut,
		Pending,
	}

	impl WaitOutcome {
		pub fn is_pending(&self) -> bool {
			matches!(self, Self::Pending)
		}
	}

	/// Helper type.
	pub type Cond = Arc<(Mutex<WaitOutcome>, Condvar)>;

	/// Gets a condvar initialized to `Pending`.
	pub fn get_condvar() -> Cond {
		Arc::new((Mutex::new(WaitOutcome::Pending), Condvar::new()))
	}

	/// Runs a worker thread. Will run the requested function, and afterwards notify the threads
	/// waiting on the condvar. Catches panics during execution and resumes the panics after
	/// triggering the condvar, so that the waiting thread is notified on panics.
	///
	/// # Returns
	///
	/// Returns the thread's join handle. Calling `.join()` on it returns the result of executing
	/// `f()`, as well as whether we were able to enable sandboxing.
	pub fn spawn_worker_thread<F, R>(
		name: &str,
		f: F,
		cond: Cond,
		outcome: WaitOutcome,
	) -> io::Result<thread::JoinHandle<R>>
	where
		F: FnOnce() -> R,
		F: Send + 'static + panic::UnwindSafe,
		R: Send + 'static,
	{
		thread::Builder::new()
			.name(name.into())
			.spawn(move || cond_notify_on_done(f, cond, outcome))
	}

	/// Runs a worker thread with the given stack size. See [`spawn_worker_thread`].
	pub fn spawn_worker_thread_with_stack_size<F, R>(
		name: &str,
		f: F,
		cond: Cond,
		outcome: WaitOutcome,
		stack_size: usize,
	) -> io::Result<thread::JoinHandle<R>>
	where
		F: FnOnce() -> R,
		F: Send + 'static + panic::UnwindSafe,
		R: Send + 'static,
	{
		thread::Builder::new()
			.name(name.into())
			.stack_size(stack_size)
			.spawn(move || cond_notify_on_done(f, cond, outcome))
	}

	/// Runs a function, afterwards notifying the threads waiting on the condvar. Catches panics and
	/// resumes them after triggering the condvar, so that the waiting thread is notified on panics.
	fn cond_notify_on_done<F, R>(f: F, cond: Cond, outcome: WaitOutcome) -> R
	where
		F: FnOnce() -> R,
		F: panic::UnwindSafe,
	{
		let result = panic::catch_unwind(|| f());
		cond_notify_all(cond, outcome);
		match result {
			Ok(inner) => return inner,
			Err(err) => panic::resume_unwind(err),
		}
	}

	/// Helper function to notify all threads waiting on this condvar.
	fn cond_notify_all(cond: Cond, outcome: WaitOutcome) {
		let (lock, cvar) = &*cond;
		let mut flag = lock.lock().unwrap();
		if !flag.is_pending() {
			// Someone else already triggered the condvar.
			return
		}
		*flag = outcome;
		cvar.notify_all();
	}

	/// Block the thread while it waits on the condvar.
	pub fn wait_for_threads(cond: Cond) -> WaitOutcome {
		let (lock, cvar) = &*cond;
		let guard = cvar.wait_while(lock.lock().unwrap(), |flag| flag.is_pending()).unwrap();
		*guard
	}

	/// Block the thread while it waits on the condvar or on a timeout. If the timeout is hit,
	/// returns `None`.
	#[cfg_attr(not(any(target_os = "linux", feature = "jemalloc-allocator")), allow(dead_code))]
	pub fn wait_for_threads_with_timeout(cond: &Cond, dur: Duration) -> Option<WaitOutcome> {
		let (lock, cvar) = &**cond;
		let result = cvar
			.wait_timeout_while(lock.lock().unwrap(), dur, |flag| flag.is_pending())
			.unwrap();
		if result.1.timed_out() {
			None
		} else {
			Some(*result.0)
		}
	}

	#[cfg(test)]
	mod tests {
		use super::*;
		use assert_matches::assert_matches;

		#[test]
		fn get_condvar_should_be_pending() {
			let condvar = get_condvar();
			let outcome = *condvar.0.lock().unwrap();
			assert!(outcome.is_pending());
		}

		#[test]
		fn wait_for_threads_with_timeout_return_none_on_time_out() {
			let condvar = Arc::new((Mutex::new(WaitOutcome::Pending), Condvar::new()));
			let outcome = wait_for_threads_with_timeout(&condvar, Duration::from_millis(100));
			assert!(outcome.is_none());
		}

		#[test]
		fn wait_for_threads_with_timeout_returns_outcome() {
			let condvar = Arc::new((Mutex::new(WaitOutcome::Pending), Condvar::new()));
			let condvar2 = condvar.clone();
			cond_notify_all(condvar2, WaitOutcome::Finished);
			let outcome = wait_for_threads_with_timeout(&condvar, Duration::from_secs(2));
			assert_matches!(outcome.unwrap(), WaitOutcome::Finished);
		}

		#[test]
		fn spawn_worker_thread_should_notify_on_done() {
			let condvar = Arc::new((Mutex::new(WaitOutcome::Pending), Condvar::new()));
			let response =
				spawn_worker_thread("thread", || 2, condvar.clone(), WaitOutcome::TimedOut);
			let (lock, _) = &*condvar;
			let r = response.unwrap().join().unwrap();
			assert_eq!(r, 2);
			assert_matches!(*lock.lock().unwrap(), WaitOutcome::TimedOut);
		}

		#[test]
		fn spawn_worker_should_not_change_finished_outcome() {
			let condvar = Arc::new((Mutex::new(WaitOutcome::Finished), Condvar::new()));
			let response =
				spawn_worker_thread("thread", move || 2, condvar.clone(), WaitOutcome::TimedOut);

			let r = response.unwrap().join().unwrap();
			assert_eq!(r, 2);
			assert_matches!(*condvar.0.lock().unwrap(), WaitOutcome::Finished);
		}

		#[test]
		fn cond_notify_on_done_should_update_wait_outcome_when_panic() {
			let condvar = Arc::new((Mutex::new(WaitOutcome::Pending), Condvar::new()));
			let err = panic::catch_unwind(panic::AssertUnwindSafe(|| {
				cond_notify_on_done(|| panic!("test"), condvar.clone(), WaitOutcome::Finished)
			}));

			assert_matches!(*condvar.0.lock().unwrap(), WaitOutcome::Finished);
			assert!(err.is_err());
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::mpsc::channel;

	#[test]
	fn cpu_time_monitor_loop_should_return_time_elapsed() {
		let cpu_time_start = ProcessTime::now();
		let timeout = Duration::from_secs(0);
		let (_tx, rx) = channel();
		let result = cpu_time_monitor_loop(cpu_time_start, timeout, rx);
		assert_ne!(result, None);
	}

	#[test]
	fn cpu_time_monitor_loop_should_return_none() {
		let cpu_time_start = ProcessTime::now();
		let timeout = Duration::from_secs(10);
		let (tx, rx) = channel();
		tx.send(()).unwrap();
		let result = cpu_time_monitor_loop(cpu_time_start, timeout, rx);
		assert_eq!(result, None);
	}
}
