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

//! Common logic for implementation of worker processes.

use crate::LOG_TARGET;
use futures::FutureExt as _;
use futures_timer::Delay;
use pin_project::pin_project;
use polkadot_node_core_pvf_common::SecurityStatus;
use rand::Rng;
use std::{
	fmt, mem,
	path::{Path, PathBuf},
	pin::Pin,
	task::{Context, Poll},
	time::Duration,
};
use tokio::{
	io::{self, AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _, ReadBuf},
	net::{UnixListener, UnixStream},
	process,
};

/// A multiple of the job timeout (in CPU time) for which we are willing to wait on the host (in
/// wall clock time). This is lenient because CPU time may go slower than wall clock time.
pub const JOB_TIMEOUT_WALL_CLOCK_FACTOR: u32 = 4;

/// This is publicly exposed only for integration tests.
///
/// # Parameters
///
/// - `debug_id`: An identifier for the process (e.g. "execute" or "prepare").
///
/// - `program_path`: The path to the program.
///
/// - `cache_path`: The path to the artifact cache.
///
/// - `extra_args`: Optional extra CLI arguments to the program. NOTE: Should only contain data
///   required before the handshake, like node/worker versions for the version check. Other data
///   should go through the handshake.
///
/// - `spawn_timeout`: The amount of time to wait for the child process to spawn.
///
/// - `security_status`: contains the detected status of security features.
#[doc(hidden)]
pub async fn spawn_with_program_path(
	debug_id: &'static str,
	program_path: impl Into<PathBuf>,
	cache_path: &Path,
	extra_args: &[&str],
	spawn_timeout: Duration,
	security_status: SecurityStatus,
) -> Result<(IdleWorker, WorkerHandle), SpawnErr> {
	let program_path = program_path.into();
	let worker_dir = WorkerDir::new(debug_id, cache_path).await?;
	let extra_args: Vec<String> = extra_args.iter().map(|arg| arg.to_string()).collect();

	with_transient_socket_path(debug_id, |socket_path| {
		let socket_path = socket_path.to_owned();

		async move {
			let listener = UnixListener::bind(&socket_path).map_err(|err| {
				gum::warn!(
					target: LOG_TARGET,
					%debug_id,
					?program_path,
					?extra_args,
					?worker_dir,
					?socket_path,
					"cannot bind unix socket: {:?}",
					err,
				);
				SpawnErr::Bind
			})?;

			let handle = WorkerHandle::spawn(
				&program_path,
				&extra_args,
				&socket_path,
				&worker_dir.path,
				security_status,
			)
			.map_err(|err| {
				gum::warn!(
					target: LOG_TARGET,
					%debug_id,
					?program_path,
					?extra_args,
					?worker_dir.path,
					?socket_path,
					"cannot spawn a worker: {:?}",
					err,
				);
				SpawnErr::ProcessSpawn
			})?;

			let worker_dir_path = worker_dir.path.clone();
			futures::select! {
				accept_result = listener.accept().fuse() => {
					let (stream, _) = accept_result.map_err(|err| {
						gum::warn!(
							target: LOG_TARGET,
							%debug_id,
							?program_path,
							?extra_args,
							?worker_dir_path,
							?socket_path,
							"cannot accept a worker: {:?}",
							err,
						);
						SpawnErr::Accept
					})?;
					Ok((IdleWorker { stream, pid: handle.id(), worker_dir }, handle))
				}
				_ = Delay::new(spawn_timeout).fuse() => {
					gum::warn!(
						target: LOG_TARGET,
						%debug_id,
						?program_path,
						?extra_args,
						?worker_dir_path,
						?socket_path,
						?spawn_timeout,
						"spawning and connecting to socket timed out",
					);
					Err(SpawnErr::AcceptTimeout)
				}
			}
		}
	})
	.await
}

async fn with_transient_socket_path<T, F, Fut>(debug_id: &'static str, f: F) -> Result<T, SpawnErr>
where
	F: FnOnce(&Path) -> Fut,
	Fut: futures::Future<Output = Result<T, SpawnErr>> + 'static,
{
	let socket_path = tmppath(&format!("pvf-host-{}", debug_id))
		.await
		.map_err(|_| SpawnErr::TmpPath)?;
	let result = f(&socket_path).await;

	// Best effort to remove the socket file. Under normal circumstances the socket will be removed
	// by the worker. We make sure that it is removed here, just in case a failed rendezvous.
	let _ = tokio::fs::remove_file(socket_path).await;

	result
}

/// Returns a path under the given `dir`. The path name will start with the given prefix.
///
/// There is only a certain number of retries. If exceeded this function will give up and return an
/// error.
pub async fn tmppath_in(prefix: &str, dir: &Path) -> io::Result<PathBuf> {
	fn make_tmppath(prefix: &str, dir: &Path) -> PathBuf {
		use rand::distributions::Alphanumeric;

		const DESCRIMINATOR_LEN: usize = 10;

		let mut buf = Vec::with_capacity(prefix.len() + DESCRIMINATOR_LEN);
		buf.extend(prefix.as_bytes());
		buf.extend(rand::thread_rng().sample_iter(&Alphanumeric).take(DESCRIMINATOR_LEN));

		let s = std::str::from_utf8(&buf)
			.expect("the string is collected from a valid utf-8 sequence; qed");

		let mut path = dir.to_owned();
		path.push(s);
		path
	}

	const NUM_RETRIES: usize = 50;

	for _ in 0..NUM_RETRIES {
		let tmp_path = make_tmppath(prefix, dir);
		if !tmp_path.exists() {
			return Ok(tmp_path)
		}
	}

	Err(io::Error::new(io::ErrorKind::Other, "failed to create a temporary path"))
}

/// The same as [`tmppath_in`], but uses [`std::env::temp_dir`] as the directory.
pub async fn tmppath(prefix: &str) -> io::Result<PathBuf> {
	let temp_dir = PathBuf::from(std::env::temp_dir());
	tmppath_in(prefix, &temp_dir).await
}

/// A struct that represents an idle worker.
///
/// This struct is supposed to be used as a token that is passed by move into a subroutine that
/// initiates a job. If the worker dies on the duty, then the token is not returned.
#[derive(Debug)]
pub struct IdleWorker {
	/// The stream to which the child process is connected.
	pub stream: UnixStream,

	/// The identifier of this process. Used to reset the niceness.
	pub pid: u32,

	/// The temporary per-worker path. We clean up the worker dir between jobs and delete it when
	/// the worker dies.
	pub worker_dir: WorkerDir,
}

/// An error happened during spawning a worker process.
#[derive(Clone, Debug)]
pub enum SpawnErr {
	/// Cannot obtain a temporary path location.
	TmpPath,
	/// An FS error occurred.
	Fs(String),
	/// Cannot bind the socket to the given path.
	Bind,
	/// An error happened during accepting a connection to the socket.
	Accept,
	/// An error happened during spawning the process.
	ProcessSpawn,
	/// The deadline allotted for the worker spawning and connecting to the socket has elapsed.
	AcceptTimeout,
	/// Failed to send handshake after successful spawning was signaled
	Handshake,
}

/// This is a representation of a potentially running worker. Drop it and the process will be
/// killed.
///
/// A worker's handle is also a future that resolves when it's detected that the worker's process
/// has been terminated. Since the worker is running in another process it is obviously not
/// necessary to poll this future to make the worker run, it's only for termination detection.
///
/// This future relies on the fact that a child process's stdout `fd` is closed upon it's
/// termination.
#[pin_project]
pub struct WorkerHandle {
	child: process::Child,
	child_id: u32,
	#[pin]
	stdout: process::ChildStdout,
	program: PathBuf,
	drop_box: Box<[u8]>,
}

impl WorkerHandle {
	fn spawn(
		program: impl AsRef<Path>,
		extra_args: &[String],
		socket_path: impl AsRef<Path>,
		worker_dir_path: impl AsRef<Path>,
		security_status: SecurityStatus,
	) -> io::Result<Self> {
		let security_args = {
			let mut args = vec![];
			if security_status.can_enable_landlock {
				args.push("--can-enable-landlock".to_string());
			}
			if security_status.can_unshare_user_namespace_and_change_root {
				args.push("--can-unshare-user-namespace-and-change-root".to_string());
			}
			args
		};

		// Clear all env vars from the spawned process.
		let mut command = process::Command::new(program.as_ref());
		command.env_clear();
		// Add back any env vars we want to keep.
		if let Ok(value) = std::env::var("RUST_LOG") {
			command.env("RUST_LOG", value);
		}

		let mut child = command
			.args(extra_args)
			.arg("--socket-path")
			.arg(socket_path.as_ref().as_os_str())
			.arg("--worker-dir-path")
			.arg(worker_dir_path.as_ref().as_os_str())
			.args(&security_args)
			.stdout(std::process::Stdio::piped())
			.kill_on_drop(true)
			.spawn()?;

		let child_id = child
			.id()
			.ok_or(io::Error::new(io::ErrorKind::Other, "could not get id of spawned process"))?;
		let stdout = child
			.stdout
			.take()
			.expect("the process spawned with piped stdout should have the stdout handle");

		Ok(WorkerHandle {
			child,
			child_id,
			stdout,
			program: program.as_ref().to_path_buf(),
			// We don't expect the bytes to be ever read. But in case we do, we should not use a
			// buffer of a small size, because otherwise if the child process does return any data
			// we will end up issuing a syscall for each byte. We also prefer not to do allocate
			// that on the stack, since each poll the buffer will be allocated and initialized (and
			// that's due `poll_read` takes &mut [u8] and there are no guarantees that a `poll_read`
			// won't ever read from there even though that's unlikely).
			//
			// OTOH, we also don't want to be super smart here and we could just afford to allocate
			// a buffer for that here.
			drop_box: vec![0; 8192].into_boxed_slice(),
		})
	}

	/// Returns the process id of this worker.
	pub fn id(&self) -> u32 {
		self.child_id
	}
}

impl futures::Future for WorkerHandle {
	type Output = ();

	fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		let me = self.project();
		// Create a `ReadBuf` here instead of storing it in `WorkerHandle` to avoid a lifetime
		// parameter on `WorkerHandle`. Creating the `ReadBuf` is fairly cheap.
		let mut read_buf = ReadBuf::new(&mut *me.drop_box);
		match futures::ready!(AsyncRead::poll_read(me.stdout, cx, &mut read_buf)) {
			Ok(()) => {
				if read_buf.filled().len() > 0 {
					// weird, we've read something. Pretend that never happened and reschedule
					// ourselves.
					cx.waker().wake_by_ref();
					Poll::Pending
				} else {
					// Nothing read means `EOF` means the child was terminated. Resolve.
					Poll::Ready(())
				}
			},
			Err(err) => {
				// The implementation is guaranteed to not to return `WouldBlock` and Interrupted.
				// This leaves us with legit errors which we suppose were due to termination.

				// Log the status code.
				gum::debug!(
					target: LOG_TARGET,
					worker_pid = %me.child_id,
					status_code = ?me.child.try_wait().ok().flatten().map(|c| c.to_string()),
					"pvf worker ({}): {:?}",
					me.program.display(),
					err,
				);
				Poll::Ready(())
			},
		}
	}
}

impl fmt::Debug for WorkerHandle {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "WorkerHandle(pid={})", self.id())
	}
}

/// Write some data prefixed by its length into `w`.
pub async fn framed_send(w: &mut (impl AsyncWrite + Unpin), buf: &[u8]) -> io::Result<()> {
	let len_buf = buf.len().to_le_bytes();
	w.write_all(&len_buf).await?;
	w.write_all(buf).await?;
	Ok(())
}

/// Read some data prefixed by its length from `r`.
pub async fn framed_recv(r: &mut (impl AsyncRead + Unpin)) -> io::Result<Vec<u8>> {
	let mut len_buf = [0u8; mem::size_of::<usize>()];
	r.read_exact(&mut len_buf).await?;
	let len = usize::from_le_bytes(len_buf);
	let mut buf = vec![0; len];
	r.read_exact(&mut buf).await?;
	Ok(buf)
}

/// A temporary worker dir that contains only files needed by the worker. The worker will change its
/// root (the `/` directory) to this directory; it should have access to no other paths on its
/// filesystem.
///
/// NOTE: This struct cleans up its associated directory when it is dropped. Therefore it should not
/// implement `Clone`.
///
/// # File structure
///
/// The overall file structure for the PVF system is as follows. The `worker-dir-X`s are managed by
/// this struct.
///
/// ```nocompile
/// + /<cache_path>/
///   - artifact-1
///   - artifact-2
///   - [...]
///   - worker-dir-1/  (new `/` for worker-1)
///     + socket                            (created by host)
///     + tmp-artifact                      (created by host) (prepare-only)
///     + artifact     (link -> artifact-1) (created by host) (execute-only)
///   - worker-dir-2/  (new `/` for worker-2)
///     + [...]
/// ```
#[derive(Debug)]
pub struct WorkerDir {
	pub path: PathBuf,
}

impl WorkerDir {
	/// Creates a new, empty worker dir with a random name in the given cache dir.
	pub async fn new(debug_id: &'static str, cache_dir: &Path) -> Result<Self, SpawnErr> {
		let prefix = format!("worker-dir-{}-", debug_id);
		let path = tmppath_in(&prefix, cache_dir).await.map_err(|_| SpawnErr::TmpPath)?;
		tokio::fs::create_dir(&path)
			.await
			.map_err(|err| SpawnErr::Fs(err.to_string()))?;
		Ok(Self { path })
	}
}

// Try to clean up the temporary worker dir at the end of the worker's lifetime. It should be wiped
// on startup, but we make a best effort not to leave it around.
impl Drop for WorkerDir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.path);
	}
}

// Not async since Rust has trouble with async recursion. There should be few files here anyway.
//
// TODO: A lingering malicious job can still access future files in this dir. See
// <https://github.com/paritytech/polkadot-sdk/issues/574> for how to fully secure this.
/// Clear the temporary worker dir without deleting it. Not deleting is important because the worker
/// has mounted its own separate filesystem here.
///
/// Should be called right after a job has finished. We don't want jobs to have access to
/// artifacts from previous jobs.
pub fn clear_worker_dir_path(worker_dir_path: &Path) -> io::Result<()> {
	fn remove_dir_contents(path: &Path) -> io::Result<()> {
		for entry in std::fs::read_dir(&path)? {
			let entry = entry?;
			let path = entry.path();

			if entry.file_type()?.is_dir() {
				remove_dir_contents(&path)?;
				std::fs::remove_dir(path)?;
			} else {
				std::fs::remove_file(path)?;
			}
		}
		Ok(())
	}

	// Note the worker dir may not exist anymore because of the worker dying and being cleaned up.
	match remove_dir_contents(worker_dir_path) {
		Err(err) if matches!(err.kind(), io::ErrorKind::NotFound) => Ok(()),
		result => result,
	}
}
