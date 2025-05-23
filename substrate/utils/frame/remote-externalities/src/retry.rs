// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// std
use std::{fmt::Debug, time::Duration};
// crates.io
use anyhow::Result;
use futures::Future;
use log::*;
use tokio_retry::{strategy::ExponentialBackoff, Retry};
// self
use crate::LOG_TARGET;

/// Default retry configuration values.
#[derive(Clone)]
pub struct RetryConfig {
	/// Maximum number of retries to attempt.
	pub max_retries: usize,
	/// Initial delay between retries.
	pub initial_delay: Duration,
	/// Maximum delay between retries.
	pub max_delay: Duration,
}
impl RetryConfig {
	/// Default maximum number of retries to attempt.
	pub const DEFAULT_MAX_RETRIES: usize = 12;
	/// Default initial delay between retries.
	pub const DEFAULT_INITIAL_DELAY: Duration = Duration::from_secs(3);
	/// Default maximum delay between retries.
	pub const DEFAULT_MAX_DELAY: Duration = Duration::from_secs(60);
}
impl Default for RetryConfig {
	fn default() -> Self {
		Self {
			max_retries: Self::DEFAULT_MAX_RETRIES,
			initial_delay: Self::DEFAULT_INITIAL_DELAY,
			max_delay: Self::DEFAULT_MAX_DELAY,
		}
	}
}

/// Execute a future with automatic retry logic.
///
/// Will retry the operation according to the provided retry configuration.
///
/// Returns the result of the operation if successful, or the last error encountered.
pub async fn with_retry<'a, F, Fut, T, E>(
	config: RetryConfig,
	mut op: F,
	op_name: &'a str,
) -> Result<T>
where
	F: FnMut() -> Fut + Send,
	Fut: Future<Output = Result<T, E>> + Send,
	T: Send,
	E: 'static + Send + Debug,
{
	let retry_strategy = ExponentialBackoff::from_millis(config.initial_delay.as_millis() as u64)
		.max_delay(config.max_delay)
		.take(config.max_retries);

	Retry::spawn(retry_strategy, || {
		let fut = op();

		async move {
			fut.await.map_err(|e| {
				debug!(
					target: LOG_TARGET,
					"{op_name} failed with error: {e:?}. Retrying...",
				);

				e
			})
		}
	})
	.await
	.map_err(|e| anyhow::anyhow!("{op_name} failed due to: {e:?}"))
}

#[cfg(test)]
mod tests {
	// std
	use std::sync::{
		atomic::{AtomicUsize, Ordering},
		Arc,
	};
	// self
	use super::*;

	#[tokio::test]
	async fn test_with_retry_success_first_try() {
		let result = with_retry(
			RetryConfig {
				max_retries: 3,
				initial_delay: Duration::from_millis(1),
				max_delay: Duration::from_millis(10),
			},
			move || async { Ok::<_, ()>(42) },
			"test_success",
		)
		.await;

		assert_eq!(result.unwrap(), 42);
	}

	#[tokio::test]
	async fn test_with_retry_success_after_retries() {
		let counter = Arc::new(AtomicUsize::new(0));
		let counter_clone = counter.clone();
		let res = with_retry(
			RetryConfig {
				max_retries: 3,
				initial_delay: Duration::from_millis(1),
				max_delay: Duration::from_millis(10),
			},
			move || {
				let counter = counter_clone.clone();

				async move {
					let count = counter.fetch_add(1, Ordering::SeqCst);

					if count < 2 {
						Err(())
					} else {
						Ok(count)
					}
				}
			},
			"test_retry_success",
		)
		.await;

		assert_eq!(res.unwrap(), 2);
		assert_eq!(counter.load(Ordering::SeqCst), 3);
	}

	#[tokio::test]
	async fn test_with_retry_max_retries_exceeded() {
		let counter = Arc::new(AtomicUsize::new(0));
		let counter_clone = counter.clone();
		let res = with_retry(
			RetryConfig {
				max_retries: 2,
				initial_delay: Duration::from_millis(1),
				max_delay: Duration::from_millis(10),
			},
			move || {
				let counter = counter_clone.clone();
				
				async move {
					let count = counter.fetch_add(1, Ordering::SeqCst);
					
					if count < 5 {
						Err(())
					} else {
						Ok(count)
					}
				}
			},
			"test_max_retries",
		)
		.await;

		assert!(res.is_err());
		// Original + 2 retries = 3 attempts
		assert_eq!(counter.load(Ordering::SeqCst), 3);
	}

	#[tokio::test]
	async fn test_with_default_retry() {
		let counter = Arc::new(AtomicUsize::new(0));
		let counter_clone = counter.clone();
		let res = with_retry(
			RetryConfig::default(),
			move || {
				let counter = counter_clone.clone();
				
				async move {
					let count = counter.fetch_add(1, Ordering::SeqCst);
					
					if count < 2 {
						Err(())
					} else {
						Ok(count)
					}
				}
			},
			"test_default_retry",
		)
		.await;

		assert_eq!(res.unwrap(), 2);
	}

	#[tokio::test]
	async fn test_multiple_retry_configs() {
		// Test with shorter initial delay.
		let res1 = with_retry(
			RetryConfig {
				max_retries: 1,
				initial_delay: Duration::from_millis(1),
				max_delay: Duration::from_millis(10),
			},
			move || async { Ok::<_, ()>(1) },
			"test_short_interval",
		)
		.await;
		// Test with longer initial delay.
		let res2 = with_retry(
			RetryConfig {
				max_retries: 1,
				initial_delay: Duration::from_millis(5),
				max_delay: Duration::from_millis(50),
			},
			move || async { Ok::<_, ()>(2) },
			"test_long_interval",
		)
		.await;

		assert_eq!(res1.unwrap(), 1);
		assert_eq!(res2.unwrap(), 2);
	}
}
