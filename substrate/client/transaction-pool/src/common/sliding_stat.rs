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

//! Logging helper. Sliding window statistics with retention-based pruning.
//!
//! `SlidingStats<T>` tracks timestamped values and computes statistical summaries
//! (min, max, average, percentiles, count) over a rolling time window.
//!
//! Old entries are automatically pruned based on a configurable retention `Duration`.
//! Values can be logged periodically using `insert_with_log` or the `insert_and_log_throttled!`
//! macro.

use std::{
	collections::{BTreeSet, HashMap, VecDeque},
	fmt::Display,
	sync::Arc,
	time::{Duration, Instant},
};
use tokio::sync::RwLock;

mod sealed {
	pub trait HasDefaultStatFormatter {}
}

impl sealed::HasDefaultStatFormatter for u32 {}
impl sealed::HasDefaultStatFormatter for i64 {}

pub trait StatFormatter {
	fn format_stat(value: f64) -> String;
}

impl<T> StatFormatter for T
where
	T: Display + sealed::HasDefaultStatFormatter,
{
	fn format_stat(value: f64) -> String {
		format!("{value:.2}")
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatDuration(pub std::time::Duration);

impl Into<f64> for StatDuration {
	fn into(self) -> f64 {
		self.0.as_secs_f64()
	}
}

impl Into<StatDuration> for Duration {
	fn into(self) -> StatDuration {
		StatDuration(self)
	}
}

impl std::fmt::Display for StatDuration {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}", self.0)
	}
}

impl StatFormatter for StatDuration {
	fn format_stat(value: f64) -> String {
		format!("{:?}", Duration::from_secs_f64(value))
	}
}

/// Sliding window statistics collector.
///
/// `SlidingStats<T>` maintains a rolling buffer of values with timestamps,
/// automatically pruning values older than the configured `retention` period.
/// It provides percentile queries (e.g., p50, p95), min/max, average, and count.
pub struct SlidingStats<T> {
	inner: Arc<RwLock<Inner<T>>>,
}

/// Sync version of `SlidingStats`
pub struct SyncSlidingStats<T> {
	inner: Arc<parking_lot::RwLock<Inner<T>>>,
}

/// A type alias for `SlidingStats` specialized for durations with human-readable formatting.
///
/// Wraps `std::time::Duration` values using `StatDuration`, allowing for statistical summaries
/// (e.g. p50, p95, average) to be displayed in units like nanoseconds, milliseconds, or seconds.
pub type DurationSlidingStats = SlidingStats<StatDuration>;

/// Sync version of `DurationSlidingStats`
pub type SyncDurationSlidingStats = SyncSlidingStats<StatDuration>;

/// Internal state of the statistics buffer.
pub struct Inner<T> {
	/// How long to retain items after insertion.
	retention: Duration,

	/// Counter to assign unique ids to each entry.
	next_id: usize,

	/// Maps id to actual value + timestamp.
	entries: HashMap<usize, Entry<T>>,

	/// Queue of IDs in insertion order for expiration.
	by_time: VecDeque<usize>,

	/// Set of values with ids, ordered by value.
	by_value: BTreeSet<(T, usize)>,

	/// The time stamp of most recent insertion with log.
	///
	/// Used to throttle debug messages.
	last_log: Option<Instant>,
}

impl<T> Default for Inner<T> {
	fn default() -> Self {
		Self {
			retention: Default::default(),
			next_id: Default::default(),
			entries: Default::default(),
			by_time: Default::default(),
			by_value: Default::default(),
			last_log: None,
		}
	}
}

impl<T> Display for Inner<T>
where
	T: Ord + Copy + Into<f64> + std::fmt::Display + StatFormatter,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let mut parts = Vec::new();

		parts.push(format!("count={}", self.count()));
		if let Some(min) = self.min() {
			parts.push(format!("min={}", min));
		}
		if let Some(max) = self.max() {
			parts.push(format!("max={}", max));
		}
		if let Some(avg) = self.avg() {
			parts.push(format!("avg={}", <T as StatFormatter>::format_stat(avg)));
		}

		for p in [50, 90, 95, 99] {
			let val = self.percentile(p);
			if val.is_finite() {
				parts.push(format!("p{}={}", p, <T as StatFormatter>::format_stat(val)));
			}
		}
		parts.push(format!("span={:?}", self.retention));
		write!(f, "{}", parts.join(", "))
	}
}

/// A value inserted into the buffer, along with its insertion time.
#[derive(Clone, Copy)]
struct Entry<T> {
	timestamp: Instant,
	value: T,
}

impl<T> SlidingStats<T>
where
	T: Ord + Copy,
{
	/// Creates a new `SlidingStats` with the given retention duration.
	pub fn new(retention: Duration) -> Self {
		Self { inner: Arc::new(RwLock::new(Inner { retention, ..Default::default() })) }
	}

	/// Inserts a value into the buffer, timestamped with `Instant::now()`.
	///
	/// May trigger pruning of old items.
	#[cfg(test)]
	pub async fn insert(&self, value: T) {
		self.inner.write().await.insert(value)
	}

	/// Inserts a value into the buffer with provided timestamp.
	///
	/// May trigger pruning of old items.
	#[cfg(test)]
	pub async fn insert_using_timestamp(&self, value: T, now: Instant) {
		self.inner.write().await.insert_using_timestamp(value, now)
	}

	#[cfg(test)]
	pub async fn len(&self) -> usize {
		self.inner.read().await.len()
	}

	/// Grants temporary read-only access to the locked inner structure,
	/// passing it into the provided closure.
	///
	/// Intended to dump stats and prune inner based on current timestamp.
	#[cfg(test)]
	pub async fn with_inner<R>(&self, f: impl FnOnce(&mut Inner<T>) -> R) -> R {
		let mut guard = self.inner.write().await;
		f(&mut *guard)
	}
}

impl<T> SyncSlidingStats<T>
where
	T: Ord + Copy,
{
	/// Creates a new `SlidingStats` with the given retention duration.
	pub fn new(retention: Duration) -> Self {
		Self {
			inner: Arc::new(parking_lot::RwLock::new(Inner { retention, ..Default::default() })),
		}
	}
}

impl<T> SlidingStats<T>
where
	T: Ord + Copy + Into<f64> + std::fmt::Display + StatFormatter,
{
	/// Inserts a value and optionally returns a formatted log string of the current stats.
	///
	/// If enough time has passed since the last log (determined by `log_interval` or retention),
	/// this method returns `Some(log_string)`, otherwise it returns `None`.
	///
	/// This method performs:
	/// - Automatic pruning of expired entries
	/// - Throttling via `last_log` timestamp
	///
	///  Note: The newly inserted value may not  be included in the returned summary.
	pub async fn insert_with_log(
		&self,
		value: T,
		log_interval: Option<Duration>,
		now: Instant,
	) -> Option<String> {
		let mut inner = self.inner.write().await;
		inner.insert_with_log(value, log_interval, now)
	}
}

impl<T> SyncSlidingStats<T>
where
	T: Ord + Copy + Into<f64> + std::fmt::Display + StatFormatter,
{
	pub fn insert_with_log(
		&self,
		value: T,
		log_interval: Option<Duration>,
		now: Instant,
	) -> Option<String> {
		let mut inner = self.inner.write();
		inner.insert_with_log(value, log_interval, now)
	}
}

impl<T> Inner<T>
where
	T: Ord + Copy,
{
	#[cfg(test)]
	fn insert(&mut self, value: T) {
		self.insert_using_timestamp(value, Instant::now())
	}

	/// Refer to [`SlidingStats::insert_using_timestamp`]
	fn insert_using_timestamp(&mut self, value: T, now: Instant) {
		let id = self.next_id;
		self.next_id += 1;

		let entry = Entry { timestamp: now, value };

		self.entries.insert(id, entry);
		self.by_time.push_back(id);
		self.by_value.insert((value, id));

		self.prune(now);
	}

	/// Returns the minimum value in the current window.
	pub fn min(&self) -> Option<T> {
		self.by_value.first().map(|(v, _)| *v)
	}

	/// Returns the maximum value in the current window.
	pub fn max(&self) -> Option<T> {
		self.by_value.last().map(|(v, _)| *v)
	}

	/// Returns the number of items currently retained.
	pub fn count(&self) -> usize {
		self.len()
	}

	/// Explicitly prunes expired items from the buffer.
	///
	/// This is also called automatically during insertions.
	pub fn prune(&mut self, now: Instant) {
		let cutoff = now - self.retention;

		while let Some(&oldest_id) = self.by_time.front() {
			let expired = match self.entries.get(&oldest_id) {
				Some(entry) => entry.timestamp < cutoff,
				None => {
					debug_assert!(false);
					true
				},
			};

			if !expired {
				break;
			}

			if let Some(entry) = self.entries.remove(&oldest_id) {
				self.by_value.remove(&(entry.value, oldest_id));
			} else {
				debug_assert!(false);
			}
			self.by_time.pop_front();
		}
	}

	pub fn len(&self) -> usize {
		debug_assert_eq!(self.entries.len(), self.by_time.len());
		debug_assert_eq!(self.entries.len(), self.by_value.len());
		self.entries.len()
	}
}

impl<T> Inner<T>
where
	T: Ord + Copy + Into<f64>,
{
	/// Returns the average (mean) of values in the current window.
	pub fn avg(&self) -> Option<f64> {
		let len = self.len();
		if len == 0 {
			None
		} else {
			Some(self.entries.values().map(|e| e.value.into()).sum::<f64>() / len as f64)
		}
	}

	/// Returns the value at the given percentile (e.g., 0.5 for p50).
	///
	/// Returns `None` if the buffer is empty.
	// note: copied from: https://docs.rs/statrs/0.18.0/src/statrs/statistics/slice_statistics.rs.html#164-182
	pub fn percentile(&self, percentile: usize) -> f64 {
		if self.len() == 0 || percentile > 100 {
			return f64::NAN;
		}

		let tau = percentile as f64 / 100.0;
		let len = self.len();

		let h = (len as f64 + 1.0 / 3.0) * tau + 1.0 / 3.0;
		let hf = h as i64;

		if hf <= 0 || percentile == 0 {
			return self.min().map(|v| v.into()).unwrap_or(f64::NAN);
		}

		if hf >= len as i64 || percentile == 100 {
			return self.max().map(|v| v.into()).unwrap_or(f64::NAN);
		}

		let mut iter = self.by_value.iter().map(|(v, _)| (*v).into());

		let a = iter.nth((hf as usize).saturating_sub(1)).unwrap_or(f64::NAN);
		let b = iter.next().unwrap_or(f64::NAN);

		a + (h - hf as f64) * (b - a)
	}
}

impl<T> Inner<T>
where
	T: Ord + Copy + Into<f64> + std::fmt::Display + StatFormatter,
{
	/// Refer to [`SlidingStats::insert_with_log`]
	pub fn insert_with_log(
		&mut self,
		value: T,
		log_interval: Option<Duration>,
		now: Instant,
	) -> Option<String> {
		let Some(last_log) = self.last_log else {
			self.last_log = Some(now);
			self.insert_using_timestamp(value, now);
			return None;
		};

		let log_interval = log_interval.unwrap_or(self.retention);
		let should_log = now.duration_since(last_log) >= log_interval;
		let result = should_log.then(|| {
			self.last_log = Some(now);
			format!("{self}")
		});
		self.insert_using_timestamp(value, now);
		result
	}
}

impl<T> Clone for SlidingStats<T> {
	fn clone(&self) -> Self {
		Self { inner: Arc::clone(&self.inner) }
	}
}

impl<T> Clone for SyncSlidingStats<T> {
	fn clone(&self) -> Self {
		Self { inner: Arc::clone(&self.inner) }
	}
}

/// Inserts a value into a `SlidingStats` and conditionally logs the current stats using `tracing`.
///
/// This macro inserts the given `$value` into the `$stats` collector only if tracing is enabled
/// for the given `$target` and `$level`. The log will be emiited only if enough time has passed
/// since the last logged output (as tracked by the internal last_log timestamp).
///
/// The macro respects throttling: stats will not be logged more frequently than either the
/// explicitly provided `log_interval` or the stats' retention period (if no interval is given).
///
/// Note that:
/// - Logging is skipped unless `tracing::enabled!` returns true for the target and level.
/// - All entries older than the retention period will be logged and pruned,
/// - The newly inserted value may not be included in the logged statistics output (it is inserted
///   *after* the log decision).
#[macro_export]
macro_rules! insert_and_log_throttled {
    (
        $level:expr,
        target: $target:expr,
        log_interval: $log_interval:expr,
        prefix: $prefix:expr,
        $stats:expr,
        $value:expr
    ) => {{
        if tracing::enabled!(target: $target, $level) {
            let now = Instant::now();
            if let Some(msg) = $stats.insert_with_log($value, Some($log_interval), now).await {
                tracing::event!(target: $target, $level, "{}: {}", $prefix, msg);
            }
        }
    }};

    (
        $level:expr,
        target: $target:expr,
        prefix: $prefix:expr,
        $stats:expr,
        $value:expr
    ) => {{
        if tracing::enabled!(target: $target, $level) {
            let now = std::time::Instant::now();
            if let Some(msg) = $stats.insert_with_log($value, None, now).await {
                tracing::event!(target: $target, $level, "{}: {}", $prefix, msg);
            }
        }
    }};
}

/// Sync version of `insert_and_log_throttled`
#[macro_export]
macro_rules! insert_and_log_throttled_sync {
    (
        $level:expr,
        target: $target:literal,
        prefix: $prefix:expr,
        $stats:expr,
        $value:expr
    ) => {{
        if tracing::enabled!(target: $target, $level) {
            let now = std::time::Instant::now();
            if let Some(msg) = $stats.insert_with_log($value, None, now){
                tracing::event!(target: $target, $level, "{}: {}", $prefix, msg);
            }
        }
    }};
}

#[cfg(test)]
mod test {
	use super::*;
	use std::time::{Duration, Instant};

	#[tokio::test]
	async fn retention_prunes_old_items() {
		let stats = SlidingStats::<u64>::new(Duration::from_secs(10));

		let base = Instant::now();
		for i in 0..5 {
			stats.insert_using_timestamp(i * 10, base + Duration::from_secs(i * 5)).await;
		}
		assert_eq!(stats.len().await, 3);

		stats.insert_using_timestamp(999, base + Duration::from_secs(26)).await;

		assert_eq!(stats.len().await, 2);
	}

	#[tokio::test]
	async fn retention_prunes_old_items2() {
		let stats = SlidingStats::<u64>::new(Duration::from_secs(10));

		let base = Instant::now();
		for i in 0..100 {
			stats.insert_using_timestamp(i * 10, base + Duration::from_secs(5)).await;
		}
		assert_eq!(stats.len().await, 100);

		stats.insert_using_timestamp(999, base + Duration::from_secs(16)).await;

		let len = stats.len().await;
		assert_eq!(len, 1);
	}

	#[tokio::test]
	async fn insert_with_log_message_contains_all_old_items() {
		let stats = SlidingStats::<u32>::new(Duration::from_secs(100));

		let base = Instant::now();
		for _ in 0..10 {
			stats.insert_with_log(1, None, base + Duration::from_secs(5)).await;
		}
		assert_eq!(stats.len().await, 10);

		let output = stats.insert_with_log(1, None, base + Duration::from_secs(200)).await.unwrap();
		assert!(output.contains("count=10"));

		let len = stats.len().await;
		assert_eq!(len, 1);
	}

	#[tokio::test]
	async fn insert_with_log_message_prunes_all_old_items() {
		let stats = SlidingStats::<u32>::new(Duration::from_secs(25));

		let base = Instant::now();
		for i in 0..10 {
			stats.insert_with_log(1, None, base + Duration::from_secs(i * 5)).await;
		}
		assert_eq!(stats.len().await, 6);

		let output = stats.insert_with_log(1, None, base + Duration::from_secs(200)).await.unwrap();
		assert!(output.contains("count=6"));

		let len = stats.len().await;
		assert_eq!(len, 1);
	}

	#[tokio::test]
	async fn test_avg_min_max() {
		let stats = SlidingStats::<u32>::new(Duration::from_secs(100));
		let base = Instant::now();

		stats.insert_using_timestamp(10, base).await;
		stats.insert_using_timestamp(20, base + Duration::from_secs(1)).await;
		stats.insert_using_timestamp(30, base + Duration::from_secs(2)).await;

		stats
			.with_inner(|inner| {
				assert_eq!(inner.count(), 3);
				assert_eq!(inner.avg(), Some(20.0));
				assert_eq!(inner.min(), Some(10));
				assert_eq!(inner.max(), Some(30));
			})
			.await;
	}

	#[tokio::test]
	async fn duration_format() {
		let stats = SlidingStats::<StatDuration>::new(Duration::from_secs(100));
		stats.insert(Duration::from_nanos(100).into()).await;
		let output = stats.with_inner(|i| format!("{i}")).await;
		assert!(output.contains("max=100ns"));

		let stats = SlidingStats::<StatDuration>::new(Duration::from_secs(100));
		stats.insert(Duration::from_micros(100).into()).await;
		let output = stats.with_inner(|i| format!("{i}")).await;
		assert!(output.contains("max=100µs"));

		let stats = SlidingStats::<StatDuration>::new(Duration::from_secs(100));
		stats.insert(Duration::from_millis(100).into()).await;
		let output = stats.with_inner(|i| format!("{i}")).await;
		assert!(output.contains("max=100ms"));

		let stats = SlidingStats::<StatDuration>::new(Duration::from_secs(100));
		stats.insert(Duration::from_secs(100).into()).await;
		let output = stats.with_inner(|i| format!("{i}")).await;
		assert!(output.contains("max=100s"));

		let stats = SlidingStats::<StatDuration>::new(Duration::from_secs(100));
		stats.insert(Duration::from_nanos(100).into()).await;
		stats.insert(Duration::from_micros(100).into()).await;
		stats.insert(Duration::from_millis(100).into()).await;
		stats.insert(Duration::from_secs(100).into()).await;
		let output = stats.with_inner(|i| format!("{i}")).await;
		println!("{output}");
		assert_eq!(output, "count=4, min=100ns, max=100s, avg=25.025025025s, p50=50.05ms, p90=100s, p95=100s, p99=100s, span=100s");
	}
}
