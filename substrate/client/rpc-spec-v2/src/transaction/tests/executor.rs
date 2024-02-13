// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use sp_core::{testing::TaskExecutor, traits::SpawnNamed};
use tokio::sync::mpsc;

/// Wrap the `TaskExecutor` to know when the broadcast future is dropped.
#[derive(Clone)]
pub struct TaskExecutorBroadcast {
	executor: TaskExecutor,
	sender: mpsc::UnboundedSender<()>,
}

/// The channel that receives events when the broadcast futures are dropped.
pub type TaskExecutorRecv = mpsc::UnboundedReceiver<()>;

impl TaskExecutorBroadcast {
	/// Construct a new `TaskExecutorBroadcast` and a receiver to know when the broadcast futures
	/// are dropped.
	pub fn new() -> (Self, TaskExecutorRecv) {
		let (sender, recv) = mpsc::unbounded_channel();

		(Self { executor: TaskExecutor::new(), sender }, recv)
	}
}

impl SpawnNamed for TaskExecutorBroadcast {
	fn spawn(
		&self,
		name: &'static str,
		group: Option<&'static str>,
		future: futures::future::BoxFuture<'static, ()>,
	) {
		let sender = self.sender.clone();
		let future = Box::pin(async move {
			future.await;
			let _ = sender.send(());
		});

		self.executor.spawn(name, group, future)
	}

	fn spawn_blocking(
		&self,
		name: &'static str,
		group: Option<&'static str>,
		future: futures::future::BoxFuture<'static, ()>,
	) {
		let sender = self.sender.clone();
		let future = Box::pin(async move {
			future.await;
			let _ = sender.send(());
		});

		self.executor.spawn_blocking(name, group, future)
	}
}
