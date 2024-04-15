// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Pallet provides a set of guard functions that are running in background threads
//! and are aborting process if some condition fails.

use crate::{error::Error, Chain, Client};

use async_trait::async_trait;
use sp_version::RuntimeVersion;
use std::{
	fmt::Display,
	time::{Duration, Instant},
};

/// Guards environment.
#[async_trait]
pub trait Environment<C>: Send + Sync + 'static {
	/// Error type.
	type Error: Display + Send + Sync + 'static;

	/// Return current runtime version.
	async fn runtime_version(&mut self) -> Result<RuntimeVersion, Self::Error>;

	/// Return current time.
	fn now(&self) -> Instant {
		Instant::now()
	}

	/// Sleep given amount of time.
	async fn sleep(&mut self, duration: Duration) {
		async_std::task::sleep(duration).await
	}

	/// Abort current process. Called when guard condition check fails.
	async fn abort(&mut self) {
		std::process::abort();
	}
}

/// Abort when runtime spec version is different from specified.
pub fn abort_on_spec_version_change<C: Chain>(
	mut env: impl Environment<C>,
	expected_spec_version: u32,
) {
	async_std::task::spawn(async move {
		log::info!(
			target: "bridge-guard",
			"Starting spec_version guard for {}. Expected spec_version: {}",
			C::NAME,
			expected_spec_version,
		);

		loop {
			let actual_spec_version = env.runtime_version().await;
			match actual_spec_version {
				Ok(version) if version.spec_version == expected_spec_version => (),
				Ok(version) => {
					log::error!(
						target: "bridge-guard",
						"{} runtime spec version has changed from {} to {}. Aborting relay",
						C::NAME,
						expected_spec_version,
						version.spec_version,
					);

					env.abort().await;
				},
				Err(error) => log::warn!(
					target: "bridge-guard",
					"Failed to read {} runtime version: {}. Relay may need to be stopped manually",
					C::NAME,
					error,
				),
			}

			env.sleep(conditions_check_delay::<C>()).await;
		}
	});
}

/// Delay between conditions check.
fn conditions_check_delay<C: Chain>() -> Duration {
	C::AVERAGE_BLOCK_INTERVAL * (10 + rand::random::<u32>() % 10)
}

#[async_trait]
impl<C: Chain> Environment<C> for Client<C> {
	type Error = Error;

	async fn runtime_version(&mut self) -> Result<RuntimeVersion, Self::Error> {
		Client::<C>::runtime_version(self).await
	}
}

#[cfg(test)]
pub(crate) mod tests {
	use super::*;
	use crate::test_chain::TestChain;
	use futures::{
		channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender},
		future::FutureExt,
		stream::StreamExt,
		SinkExt,
	};

	pub struct TestEnvironment {
		pub runtime_version_rx: UnboundedReceiver<RuntimeVersion>,
		pub slept_tx: UnboundedSender<()>,
		pub aborted_tx: UnboundedSender<()>,
	}

	#[async_trait]
	impl Environment<TestChain> for TestEnvironment {
		type Error = Error;

		async fn runtime_version(&mut self) -> Result<RuntimeVersion, Self::Error> {
			Ok(self.runtime_version_rx.next().await.unwrap_or_default())
		}

		async fn sleep(&mut self, _duration: Duration) {
			let _ = self.slept_tx.send(()).await;
		}

		async fn abort(&mut self) {
			let _ = self.aborted_tx.send(()).await;
			// simulate process abort :)
			async_std::task::sleep(Duration::from_secs(60)).await;
		}
	}

	#[test]
	fn aborts_when_spec_version_is_changed() {
		async_std::task::block_on(async {
			let (
				(mut runtime_version_tx, runtime_version_rx),
				(slept_tx, mut slept_rx),
				(aborted_tx, mut aborted_rx),
			) = (unbounded(), unbounded(), unbounded());
			abort_on_spec_version_change(
				TestEnvironment { runtime_version_rx, slept_tx, aborted_tx },
				0,
			);

			// client responds with wrong version
			runtime_version_tx
				.send(RuntimeVersion { spec_version: 42, ..Default::default() })
				.await
				.unwrap();

			// then the `abort` function is called
			aborted_rx.next().await;
			// and we do not reach the `sleep` function call
			assert!(slept_rx.next().now_or_never().is_none());
		});
	}

	#[test]
	fn does_not_aborts_when_spec_version_is_unchanged() {
		async_std::task::block_on(async {
			let (
				(mut runtime_version_tx, runtime_version_rx),
				(slept_tx, mut slept_rx),
				(aborted_tx, mut aborted_rx),
			) = (unbounded(), unbounded(), unbounded());
			abort_on_spec_version_change(
				TestEnvironment { runtime_version_rx, slept_tx, aborted_tx },
				42,
			);

			// client responds with the same version
			runtime_version_tx
				.send(RuntimeVersion { spec_version: 42, ..Default::default() })
				.await
				.unwrap();

			// then the `sleep` function is called
			slept_rx.next().await;
			// and the `abort` function is not called
			assert!(aborted_rx.next().now_or_never().is_none());
		});
	}
}
