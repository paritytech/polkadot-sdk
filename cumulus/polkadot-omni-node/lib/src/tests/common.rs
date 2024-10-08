// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(unix)]

use assert_cmd::cargo::cargo_bin;
use nix::{
	sys::signal::{kill, Signal},
	unistd::Pid,
};
use std::{
	io::{BufRead, BufReader, Read},
	ops::{Deref, DerefMut},
	path::Path,
	process::{self, Child, Command, ExitStatus},
};
use tokio::time::{sleep, Duration};

/// Wait for the given `child` the given number of `secs`.
///
/// Returns the `Some(exit status)` or `None` if the process did not finish in the given time.
pub fn wait_for(child: &mut Child, secs: u64) -> Result<ExitStatus, ()> {
	let result = wait_timeout::ChildExt::wait_timeout(child, Duration::from_secs(5.min(secs)))
		.map_err(|_| ())?;
	if let Some(exit_status) = result {
		Ok(exit_status)
	} else {
		if secs > 5 {
			eprintln!("Child process taking over 5 seconds to exit gracefully");
			let result = wait_timeout::ChildExt::wait_timeout(child, Duration::from_secs(secs - 5))
				.map_err(|_| ())?;
			if let Some(exit_status) = result {
				return Ok(exit_status)
			}
		}
		eprintln!("Took too long to exit (> {} seconds). Killing...", secs);
		let _ = child.kill();
		child.wait().unwrap();
		Err(())
	}
}

/// Run the node for a while (till the RPC is up + 30 secs)
/// TODO: needs to be revisited to hit the RPC
pub async fn run_node_for_a_while(base_path: &Path, args: &[&str], signal: Signal) {
	let mut cmd = Command::new(cargo_bin("polkadot-parachain"))
		.stdout(process::Stdio::piped())
		.stderr(process::Stdio::piped())
		.arg("-d")
		.arg(base_path)
		.args(args)
		.spawn()
		.unwrap();

	let stderr = cmd.stderr.take().unwrap();

	let mut child = KillChildOnDrop(cmd);
	// TODO: use this instead of the timeout going forward?
	let (_, _) = find_ws_url_from_output(stderr);

	// TODO: Revisit this to find a better approach for collators
	sleep(Duration::from_secs(120)).await;

	assert!(child.try_wait().unwrap().is_none(), "the process should still be running");

	// Stop the process
	kill(Pid::from_raw(child.id().try_into().unwrap()), signal).unwrap();
	assert!(wait_for(&mut child, 40).map(|x| x.success()).unwrap());
}

pub struct KillChildOnDrop(pub Child);

impl Drop for KillChildOnDrop {
	fn drop(&mut self) {
		let _ = self.0.kill();
	}
}

impl Deref for KillChildOnDrop {
	type Target = Child;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl DerefMut for KillChildOnDrop {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

/// Read the RPC server address from the output.
///
/// This is hack to get the actual bound sockaddr because
/// substrate assigns a random port if the specified port was already bound.
pub fn find_ws_url_from_output(read: impl Read + Send) -> (String, String) {
	let mut data = String::new();

	let ws_url = BufReader::new(read)
		.lines()
		.find_map(|line| {
			let line =
				line.expect("failed to obtain next line from stdout for WS address discovery");

			data.push_str(&line);
			data.push('\n');

			// does the line contain our port (we expect this specific output from substrate).
			let sock_addr = match line.split_once("Running JSON-RPC server: addr=") {
				None => return None,
				Some((_, after)) => after.split_once(',').unwrap().0,
			};

			Some(format!("ws://{}", sock_addr))
		})
		.unwrap_or_else(|| {
			eprintln!("Output:\n{}", data);
			panic!("We should get an address")
		});

	(ws_url, data)
}
