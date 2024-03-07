use std::{
	sync::Mutex,
	time::{Duration, SystemTimeError},
};

use serde::Serialize;
use wasm_timer::{SystemTime, UNIX_EPOCH};

use crate::{telemetry, TelemetryHandle, SUBSTRATE_INFO};

/// Maximum amount of intervals that we will keep in our storage.
pub const MAXIMUM_INTERVALS_LENGTH: usize = 50;
/// Maximum amount of block requests info that we will keep in our storage.
pub const MAXIMUM_BLOCK_REQUESTS_LENGTH: usize = 15;

///
#[repr(u8)]
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum IntervalKind {
	/// Tells us how long it took us to produce a block. Basically it's all about
	/// taking transactions from the mem pool and executing them.
	Proposal = 0,
	/// Tells us how long it took to get a block from someone.
	Sync = 1,
	/// Tells us how long it took to import a block.
	/// Import is measured for the node that produced the block as well as for the
	/// node that requested that block.
	Import = 2,
}

/// Interval information bundled together with block information.
#[derive(Serialize, Clone)]
pub struct IntervalWithBlockInformation {
	///
	pub kind: IntervalKind,
	///
	pub block_number: u64,
	///
	pub block_hash: String,
	///
	pub start_timestamp: u64,
	///
	pub end_timestamp: u64,
}

///
#[derive(Serialize)]
pub struct BlockRequestsDetail {
	///
	pub current_queue_size: u32,
	///
	pub requests_handled: u32,
	///
	pub time_frame: u64,
}

///
#[derive(Default)]
pub struct BlockMetrics {
	///
	intervals: Vec<IntervalWithBlockInformation>,
	///
	partial_intervals: Vec<IntervalWithBlockInformation>,
	///
	block_requests: Vec<BlockRequestsDetail>,
}

impl BlockMetrics {
	///
	pub const fn new() -> Self {
		Self { intervals: Vec::new(), partial_intervals: Vec::new(), block_requests: Vec::new() }
	}
}

static BLOCK_METRICS: Mutex<BlockMetrics> = Mutex::new(BlockMetrics::new());

impl BlockMetrics {
	///
	pub fn observe_interval(value: IntervalWithBlockInformation) {
		let Ok(mut lock) = BLOCK_METRICS.lock() else {
			return;
		};

		if lock.partial_intervals.len() >= MAXIMUM_INTERVALS_LENGTH {
			lock.partial_intervals.remove(0);
		}

		lock.intervals.push(value);
	}

	///
	pub fn observe_interval_partial(
		kind: IntervalKind,
		block_number: u64,
		block_hash: String,
		timestamp: u64,
		is_start: bool,
	) {
		let Ok(mut lock) = BLOCK_METRICS.lock() else {
			return;
		};

		if is_start {
			if lock.partial_intervals.len() >= MAXIMUM_INTERVALS_LENGTH {
				lock.partial_intervals.remove(0);
			}

			let value = IntervalWithBlockInformation {
				kind,
				block_number,
				block_hash,
				start_timestamp: timestamp,
				end_timestamp: 0,
			};

			lock.partial_intervals.push(value);
			return;
		}

		let existing_entry_pos = lock.partial_intervals.iter_mut().position(|v| {
			v.block_hash == block_hash && v.block_number == block_number && v.kind == kind
		});

		let Some(pos) = existing_entry_pos else {
			return;
		};

		let mut entry = lock.partial_intervals.remove(pos);
		entry.end_timestamp = timestamp;

		if lock.partial_intervals.len() >= MAXIMUM_INTERVALS_LENGTH {
			lock.partial_intervals.remove(0);
		}

		lock.intervals.push(entry);
	}

	///
	pub fn observe_block_request(value: BlockRequestsDetail) {
		let Ok(mut lock) = BLOCK_METRICS.lock() else {
			return;
		};

		if lock.block_requests.len() >= MAXIMUM_BLOCK_REQUESTS_LENGTH {
			lock.block_requests.remove(0);
		}

		lock.block_requests.push(value);
	}

	///
	pub fn take_metrics() -> Option<BlockMetrics> {
		let Ok(mut lock) = BLOCK_METRICS.lock() else {
			return None;
		};

		let metrics = std::mem::take(&mut *lock);
		Some(metrics)
	}

	///
	pub fn get_current_timestamp_in_ms_or_default() -> u64 {
		Self::get_current_timestamp_in_ms().map(|v| v as u64).unwrap_or(0u64)
	}

	fn get_current_timestamp_in_ms() -> Result<u128, SystemTimeError> {
		let start = SystemTime::now();
		start.duration_since(UNIX_EPOCH).map(|f| f.as_millis())
	}
}

/// This will be send to the telemetry
mod external {
	use super::*;

	#[derive(Debug, Serialize, Clone)]
	pub struct IntervalFromNode {
		///
		pub kind: IntervalKind,
		///
		pub start_timestamp: u64,
		///
		pub end_timestamp: u64,
	}

	#[derive(Debug, Default, Serialize, Clone)]
	pub struct BlockIntervalFromNode {
		///
		pub block_number: u64,
		///
		pub block_hash: String,
		///
		pub intervals: Vec<IntervalFromNode>,
	}

	pub fn prepare_data(
		mut value: Vec<IntervalWithBlockInformation>,
	) -> Vec<BlockIntervalFromNode> {
		let mut output = Vec::with_capacity(value.len() / 2);
		value.sort_by(|l, r| {
			if l.block_number == r.block_number {
				l.block_hash.cmp(&r.block_hash)
			} else {
				l.block_number.cmp(&r.block_number)
			}
		});

		let mut block = BlockIntervalFromNode::default();
		for v in value {
			let interval = IntervalFromNode {
				kind: v.kind,
				start_timestamp: v.start_timestamp,
				end_timestamp: v.end_timestamp,
			};

			if (v.block_number != block.block_number || v.block_hash != block.block_hash)
				&& block.block_number != u64::default()
			{
				output.push(std::mem::take(&mut block));
			}

			block.block_number = v.block_number;
			block.block_hash = v.block_hash;
			block.intervals.push(interval);
		}

		if block.block_number != u64::default() {
			output.push(block);
		}

		output
	}
}

///
pub struct CustomTelemetryWorker {
	///
	pub handle: Option<TelemetryHandle>,
}

impl CustomTelemetryWorker {
	///
	pub async fn run(self) {
		const SLEEP_DURATION: Duration = Duration::from_millis(250);
		const MAX_SLEEP_DURATION: u128 = 60_000;

		let mut start = std::time::Instant::now();
		loop {
			if start.elapsed().as_millis() >= MAX_SLEEP_DURATION {
				let metrics = BlockMetrics::take_metrics().unwrap_or_default();
				let block_intervals = external::prepare_data(metrics.intervals);

				telemetry!(
					self.handle;
					SUBSTRATE_INFO;
					"block.metrics";
					"block_intervals" => block_intervals,
					"block_requests" => metrics.block_requests,
				);

				start = std::time::Instant::now();
			}

			tokio::time::sleep(SLEEP_DURATION).await;
		}
	}
}
