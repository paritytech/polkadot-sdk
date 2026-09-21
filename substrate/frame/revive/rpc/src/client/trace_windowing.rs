// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Collecting one execution trace over several runtime API calls.
//!
//! The runtime cannot build an arbitrarily long trace in one call, so it is asked for a window at
//! a time. The first window is sized from a prior, every window after from what the last one cost.
//!
//! The sizing is tuned against 322,086 transaction traces from 1,224 Ethereum mainnet blocks:
//! 102 blocks in each of twelve monthly segments, covering the year to April 2026,
//! balanced across gas-used terciles so light and heavy blocks weigh the same.
//!
//! Mainnet gives a diverse and representative workload, and EVM traces are the reference because
//! they run longer than PVM ones: a step per opcode, where PVM records one per host call.

use crate::client::ClientError;
use codec::Encode;
use futures::future::BoxFuture;
use pallet_revive::evm::RUNTIME_STEP_BYTES;
use pallet_revive_types::runtime_api::*;
use sp_core::MAX_POSSIBLE_ALLOCATION;

const LOG_TARGET: &str = "eth-rpc::trace-windowing";

/// What a step is assumed to cost before one has been measured: a little above the 449 byte
/// sampled mean. Only the first window is sized from it.
const PRIOR_STEP_BYTES: u64 = 512;

/// Bytes of a response kept for everything but the steps: 256 KiB for `return_value`, capped at
/// 128 KiB for either VM and doubled as hex, and a kilobyte for the trace's other fields and the
/// JSON-RPC wrapper.
const RESPONSE_RESERVE_BYTES: u64 = 257 * 1024;

/// Failures one window pays for before giving up. The eighth gives up rather than halving, so a
/// window narrows from `STEP_CEILING` to 2,048 steps, under the 3,833 the most expensive sampled
/// trace needed. What a walk pays in total is bounded by the ceiling instead, which never rises.
const MAX_CONSECUTIVE_FAILURES: u32 = 8;

/// Headroom on the window size, which is computed from what the previous window's steps cost.
/// 80% tolerates steps 1.25x greater than that estimate. For sampled traces 90% of consecutive
/// windows at the default size came in under 1.20x.
const WINDOW_BUDGET_SHARE: u64 = 80;

/// Steps one window may ask for, whatever they cost: the largest power of two whose entries fit
/// `MAX_POSSIBLE_ALLOCATION`, the runtime's `Vec<ExecutionStep>` growing by doubling.
const STEP_CEILING: u64 = 1 << (MAX_POSSIBLE_ALLOCATION as u64 / RUNTIME_STEP_BYTES as u64).ilog2();

/// Bytes one window may occupy once encoded. The encode buffer doubles, so half of
/// `MAX_POSSIBLE_ALLOCATION` is safe for any number of steps.
const ENCODED_CEILING: u64 = MAX_POSSIBLE_ALLOCATION as u64 / 2;

fn window_budget_bytes(max_response_size: u32) -> u64 {
	ENCODED_CEILING.min((max_response_size as u64) / 2) * WINDOW_BUDGET_SHARE / 100
}

fn window_steps(bytes_per_step: u64, max_response_size: u32) -> u64 {
	(window_budget_bytes(max_response_size) / bytes_per_step.max(1)).clamp(1, STEP_CEILING)
}

fn response_budget_bytes(max_response_size: u32) -> u64 {
	(max_response_size as u64).saturating_sub(RESPONSE_RESERVE_BYTES)
}

fn steps_still_fitting(collected_json_bytes: u64, json_step_bytes: u64, max: u32) -> u64 {
	response_budget_bytes(max).saturating_sub(collected_json_bytes) / json_step_bytes.max(1)
}

/// Both figures from one render, which is the expensive part.
fn measured_json_bytes(steps: &[ExecutionStepV1]) -> Result<Option<(u64, u64)>, ClientError> {
	let count = steps.len() as u64;
	if count == 0 {
		return Ok(None);
	}
	let total = json_bytes(steps)?;

	Ok(Some((total, total / count)))
}

fn json_bytes(steps: &[ExecutionStepV1]) -> Result<u64, ClientError> {
	serde_json::to_vec(steps)
		.map(|json| json.len() as u64)
		.map_err(|_| ClientError::TraceRenderFailed)
}

fn measured_step_bytes(steps: &[ExecutionStepV1]) -> Option<u64> {
	let count = steps.len() as u64;

	(count > 0).then(|| steps.encoded_size() as u64 / count)
}

/// Walk the windows after the first, extending `collected` with their steps.
///
/// `collected` is the first window, already fetched, because `trace_tx` and `trace_call` answer
/// it with different types and only they can say whether an absent trace is an answer or a
/// failure.
pub(super) async fn extend_with_remaining_windows<'a, F>(
	collected: &mut ExecutionTraceV1,
	walk: TraceWalk,
	max_response_size: u32,
	first: TraceWindow,
	mut narrowing: Narrowing,
	mut fetch: F,
) -> Result<(), ClientError>
where
	F: FnMut(TraceWindow) -> BoxFuture<'a, Result<ExecutionTraceV1, ClientError>>,
{
	let mut returned = collected.struct_logs.len() as u64;
	let mut limit = first.limit;
	let mut step_bytes = measured_step_bytes(&collected.struct_logs);
	// Nothing to measure means the first window was the whole trace.
	let Some((mut collected_json_bytes, mut json_step_bytes)) =
		measured_json_bytes(&collected.struct_logs)?
	else {
		return Ok(());
	};
	// A window holding fewer steps than it asked for is the end of the execution.
	while returned >= limit {
		let steps = collected.struct_logs.len() as u64;
		let Some(remaining) = walk.remaining(steps) else { break };

		let fits_response =
			steps_still_fitting(collected_json_bytes, json_step_bytes, max_response_size);
		let sized = step_bytes.map_or(limit, |b| window_steps(b, max_response_size));
		// One more than fits, so a window that comes back short proves the execution ended rather
		// than the response filling.
		let asked_for = fits_response.saturating_add(1);
		let mut window = TraceWindow {
			step_offset: steps,
			limit: remaining.min(sized).min(asked_for).min(narrowing.ceiling),
		};

		let trace = fetch_narrowing(&mut window, &mut narrowing, &mut fetch).await?;
		limit = window.limit;

		step_bytes = measured_step_bytes(&trace.struct_logs).or(step_bytes);
		let window_json_bytes = match measured_json_bytes(&trace.struct_logs)? {
			Some((total, per_step)) => {
				json_step_bytes = per_step;
				total
			},
			None => 0,
		};

		let budget = response_budget_bytes(max_response_size);
		let room = budget.saturating_sub(collected_json_bytes);
		collected_json_bytes = collected_json_bytes.saturating_add(window_json_bytes);
		if collected_json_bytes > budget {
			// What the caller can have, since the message offers it as a `limit`, priced by what
			// these steps measured rather than by what the window before them did.
			return Err(ClientError::TraceTooLarge {
				fits: steps.saturating_add(room / json_step_bytes.max(1)),
			});
		}

		returned = trace.struct_logs.len() as u64;
		collected.struct_logs.extend(trace.struct_logs);
	}

	Ok(())
}

/// What failed windows have taught a walk, carried from the first window onward.
#[derive(Clone, Copy, Debug)]
pub(super) struct Narrowing {
	/// The most a later window may ask for. Halved by each failure.
	ceiling: u64,
}

impl Default for Narrowing {
	fn default() -> Self {
		Self { ceiling: u64::MAX }
	}
}

/// Fetch one window, halving it and asking again while the node refuses it as too large.
pub(super) async fn fetch_narrowing<'a, T, F>(
	window: &mut TraceWindow,
	narrowing: &mut Narrowing,
	fetch: &mut F,
) -> Result<T, ClientError>
where
	F: FnMut(TraceWindow) -> BoxFuture<'a, Result<T, ClientError>>,
{
	let mut failed = 0;

	loop {
		match fetch(*window).await {
			Ok(fetched) => return Ok(fetched),
			Err(err) if !is_retryable(&err) => return Err(err),
			Err(err) => {
				let smaller = window.limit / 2;

				failed += 1;
				if smaller == 0 || failed >= MAX_CONSECUTIVE_FAILURES {
					return Err(err);
				}

				log::warn!(
					target: LOG_TARGET,
					"a {} step trace window failed ({err:?}); narrowing to {smaller}",
					window.limit,
				);
				narrowing.ceiling = smaller;
				window.limit = smaller;
			},
		}
	}
}

fn node_error(err: &ClientError) -> Option<&subxt::rpcs::Error> {
	match err {
		ClientError::RpcError(err) => Some(err),
		ClientError::SubxtError(subxt::Error::BackendError(subxt::error::BackendError::Rpc(
			subxt::error::RpcError::ClientError(err),
		))) => Some(err),
		_ => None,
	}
}

/// Whether a failed window is worth asking for again, smaller.
fn is_retryable(err: &ClientError) -> bool {
	const EXECUTION_FAILED: i32 = 4_003;

	matches!(
		node_error(err),
		Some(subxt::rpcs::Error::User(user))
			if matches!(
				user.code,
				EXECUTION_FAILED | jsonrpsee::types::error::OVERSIZED_RESPONSE_CODE
			)
	)
}

/// The caller's own bound on the walk, if they set one.
#[derive(Clone, Copy, Debug)]
pub(super) struct TraceWalk {
	caller_limit: Option<u64>,
}

impl TraceWalk {
	/// The first window, which exists even for a limit of zero so the runtime answers it.
	pub(super) fn first_window(&self, max_response_size: u32) -> TraceWindow {
		let steps = window_steps(PRIOR_STEP_BYTES, max_response_size);

		TraceWindow {
			step_offset: 0,
			limit: self.caller_limit.map_or(steps, |limit| limit.min(steps)),
		}
	}

	/// Steps the caller still has coming, or `None` once they have what they asked for.
	fn remaining(&self, collected_steps: u64) -> Option<u64> {
		let Some(limit) = self.caller_limit else { return Some(u64::MAX) };

		limit.checked_sub(collected_steps).filter(|remaining| *remaining > 0)
	}
}

/// Apply a window to an execution tracer selection. The other tracers have no steps to window.
pub(super) fn windowed(tracer_type: TracerTypeV1, window: Option<TraceWindow>) -> TracerTypeV2 {
	let Some(window) = window else { return tracer_type.into() };

	match tracer_type.into() {
		TracerTypeV2::ExecutionTracer(config) => {
			let mut config = config.unwrap_or_default();
			config.step_offset = window.step_offset;
			config.limit = Some(window.limit);
			TracerTypeV2::ExecutionTracer(Some(config))
		},
		other => other,
	}
}

pub(super) fn execution_tracer_walk(tracer_type: &TracerTypeV1) -> Option<TraceWalk> {
	let TracerTypeV1::ExecutionTracer(config) = tracer_type else { return None };

	Some(TraceWalk { caller_limit: config.as_ref().and_then(|config| config.limit) })
}

/// One window of an execution's steps.
#[derive(Clone, Copy, Debug)]
pub(super) struct TraceWindow {
	step_offset: u64,
	limit: u64,
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::FutureExt;
	use subxt::rpcs::UserError;

	/// What a trapped replay answers with: the state call's catch-all execution code.
	fn too_large() -> ClientError {
		const EXECUTION_ERROR: i32 = 4003;

		ClientError::RpcError(subxt::rpcs::Error::User(UserError {
			code: EXECUTION_ERROR,
			message: "Execution failed: Execution aborted due to trap: host trap".to_string(),
			data: None,
		}))
	}

	fn json_bytes_of(trace: &ExecutionTraceV1) -> u64 {
		serde_json::to_vec(trace).expect("a canned trace renders").len() as u64
	}

	/// What a node answers when the window it built is past its own response limit.
	fn oversized_response() -> ClientError {
		ClientError::RpcError(subxt::rpcs::Error::User(UserError {
			code: jsonrpsee::types::error::OVERSIZED_RESPONSE_CODE,
			message: "Response is too big".to_string(),
			data: None,
		}))
	}

	/// Uniform steps, so a test can price a trace.
	fn canned_trace(steps: usize) -> ExecutionTraceV1 {
		ExecutionTraceV1 {
			struct_logs: vec![ExecutionStepV1::default(); steps],
			..ExecutionTraceV1::default()
		}
	}

	/// Steps stamped with where they belong, so a window out of order is visible.
	fn canned_window(offset: u64, steps: usize, gas: u64) -> ExecutionTraceV1 {
		let step = |i: u64| ExecutionStepV1 {
			kind: ExecutionStepKindV1::EVMOpcode {
				pc: (offset + i) as u32,
				op: EvmOpcodeV1(0),
				stack: vec![],
				memory: vec![],
				storage: None,
			},
			..ExecutionStepV1::default()
		};

		ExecutionTraceV1 {
			gas,
			struct_logs: (0..steps as u64).map(step).collect(),
			..ExecutionTraceV1::default()
		}
	}

	fn positions(trace: &ExecutionTraceV1) -> Vec<u32> {
		trace
			.struct_logs
			.iter()
			.map(|step| match step.kind {
				ExecutionStepKindV1::EVMOpcode { pc, .. } => pc,
				_ => unreachable!("canned steps are opcodes; qed"),
			})
			.collect()
	}

	/// Walk `total` steps, recording what each request asked for.
	async fn walk(
		total: usize,
		caller_limit: Option<u64>,
		max_response_size: u32,
	) -> (ExecutionTraceV1, Vec<TraceWindow>, Result<(), ClientError>) {
		let asked = std::cell::RefCell::new(Vec::new());
		let answer = |window: TraceWindow| {
			asked.borrow_mut().push(window);

			// The runtime answers with the part of the execution the window covers.
			let remaining = total.saturating_sub(window.step_offset as usize);
			canned_window(
				window.step_offset,
				remaining.min(window.limit as usize),
				if window.step_offset == 0 { 999 } else { 0 },
			)
		};

		let walk = TraceWalk { caller_limit };
		let mut collected = answer(walk.first_window(max_response_size));
		let result = extend_with_remaining_windows(
			&mut collected,
			walk,
			max_response_size,
			walk.first_window(max_response_size),
			Narrowing::default(),
			|window| {
				let trace = answer(window);
				async move { Ok(trace) }.boxed()
			},
		)
		.await;

		(collected, asked.into_inner(), result)
	}

	/// Walk on from a full first window, recording what each later one asked for.
	async fn walk_on<A>(
		max_response_size: u32,
		answer: A,
	) -> (ExecutionTraceV1, Vec<u64>, Result<(), ClientError>)
	where
		A: Fn(TraceWindow) -> Result<ExecutionTraceV1, ClientError>,
	{
		let walk = TraceWalk { caller_limit: None };
		let first = walk.first_window(max_response_size);
		let asked = std::cell::RefCell::new(Vec::new());
		let mut collected = canned_trace(first.limit as usize);
		let result = extend_with_remaining_windows(
			&mut collected,
			walk,
			max_response_size,
			first,
			Narrowing::default(),
			|window| {
				asked.borrow_mut().push(window.limit);

				let answered = answer(window);
				async move { answered }.boxed()
			},
		)
		.await;

		(collected, asked.into_inner(), result)
	}

	fn first_window(max_response_size: u32) -> u64 {
		TraceWalk { caller_limit: None }.first_window(max_response_size).limit
	}

	/// Small enough that a short trace takes several windows, but past the absolute reserve.
	fn small_response_size(steps_per_window: u64) -> u32 {
		let for_windows = steps_per_window * PRIOR_STEP_BYTES * 2 * 100 / WINDOW_BUDGET_SHARE;

		for_windows.max(RESPONSE_RESERVE_BYTES * 3) as u32
	}

	#[tokio::test]
	async fn windows_are_walked_until_one_comes_back_short() {
		let max = small_response_size(10);
		let total = first_window(max) * 3;
		let (collected, asked, result) = walk(total as usize, None, max).await;

		result.expect("the trace fits the response");
		assert!(asked.len() > 1, "a {total} step trace takes more than one window: {asked:?}");
		assert_eq!(collected.struct_logs.len() as u64, total);
		assert_eq!(
			positions(&collected),
			(0..total as u32).collect::<Vec<_>>(),
			"the steps come back in order, each window picking up where the last stopped",
		);
	}

	#[tokio::test]
	async fn a_cheap_window_earns_a_larger_next_one() {
		let (_, asked, _) = walk(10_000, None, small_response_size(10)).await;

		assert!(asked.len() > 1);
		assert!(
			asked[1].limit > asked[0].limit,
			"the first window is sized pessimistically, the second from what a step measured: \
			 {:?}",
			asked.iter().map(|window| window.limit).collect::<Vec<_>>(),
		);
	}

	#[tokio::test]
	async fn the_callers_limit_stops_the_walk_early() {
		let max = small_response_size(10);
		let window = first_window(max);
		let limit = window + 100;
		let (collected, asked, result) = walk(10_000, Some(limit), max).await;

		result.expect("the caller asked for what the response can hold");
		assert_eq!(
			asked.iter().map(|w| w.limit).collect::<Vec<_>>(),
			vec![window, 100],
			"the last request asks only for what the caller's limit still allows",
		);
		assert_eq!(collected.struct_logs.len() as u64, limit);
	}

	#[tokio::test]
	async fn a_limit_of_zero_still_asks_the_runtime() {
		let (collected, asked, result) = walk(25, Some(0), small_response_size(10)).await;

		result.expect("no steps always fit");
		assert_eq!(asked.len(), 1);
		assert_eq!(asked[0].limit, 0);
		assert!(collected.struct_logs.is_empty());
		assert_eq!(collected.gas, 999);
	}

	#[test]
	fn only_the_execution_tracer_is_walked_and_it_keeps_the_callers_limit() {
		let walk_for = |limit| {
			let config = ExecutionTracerConfigV1 { limit, ..ExecutionTracerConfigV1::default() };
			execution_tracer_walk(&TracerTypeV1::ExecutionTracer(Some(config)))
				.expect("an execution tracer is windowed")
				.caller_limit
		};

		assert_eq!(walk_for(Some(100)), Some(100));
		assert_eq!(walk_for(None), None);
		assert!(execution_tracer_walk(&TracerTypeV1::CallTracer(None)).is_none());
	}

	#[test]
	fn a_window_never_asks_for_more_than_the_runtime_can_hold() {
		for max_response_size in [1024 * 1024, 15 * 1024 * 1024, 256 * 1024 * 1024, u32::MAX] {
			// 14 bytes is a step with no stack and no memory, which is
			// where the step ceiling binds and no byte budget would.
			let steps = window_steps(14, max_response_size);
			let bytes = window_budget_bytes(max_response_size);

			assert!(steps <= STEP_CEILING, "{max_response_size} byte response asked {steps} steps");
			assert!(bytes <= ENCODED_CEILING, "{max_response_size} byte response asked {bytes} B");
		}
	}

	#[tokio::test]
	async fn a_failed_window_is_retried_smaller() {
		let (_, asked, result) =
			walk_on(sc_cli::RPC_DEFAULT_MAX_RESPONSE_SIZE_MB * 1024 * 1024, |_| Err(too_large()))
				.await;

		result.expect_err("the retries run out and the error is returned");
		assert_eq!(asked.len(), MAX_CONSECUTIVE_FAILURES as usize, "budget: {asked:?}");
		assert!(asked.windows(2).all(|p| p[1] == p[0] / 2), "not halving: {asked:?}");
	}

	#[tokio::test]
	async fn narrowing_settles_at_a_size_the_node_serves() {
		// The tightest window any sampled trace would need.
		const SERVES: u64 = 3_833;

		let max = 64 * 1024 * 1024;
		let total = first_window(max) + 40_000;

		// A trapped replay and a response the node cannot return are both narrowed.
		for refuse in [too_large as fn() -> ClientError, oversized_response] {
			let (collected, asked, result) = walk_on(max, |window| {
				(window.limit <= SERVES)
					.then(|| {
						let left = total - window.step_offset.min(total);
						canned_trace(left.min(window.limit) as usize)
					})
					.ok_or_else(refuse)
			})
			.await;

			result.expect("halving reaches a size the node serves");
			assert_eq!(collected.struct_logs.len() as u64, total);
			let settled = asked.iter().position(|limit| *limit <= SERVES).expect("one window fits");
			assert!(
				asked[settled] > SERVES / 2,
				"narrowed to {}, far below what the node serves: {asked:?}",
				asked[settled],
			);
			assert!(
				asked[settled..].iter().all(|limit| *limit <= SERVES),
				"a window that fits is followed by one that does not: {asked:?}",
			);
		}
	}

	#[tokio::test]
	async fn a_trace_that_exactly_fills_the_response_is_returned() {
		let max = sc_cli::RPC_DEFAULT_MAX_RESPONSE_SIZE_MB * 1024 * 1024;
		// Rendered, `n` steps cost `n * per + base`, so the response holds this many of them.
		let one = json_bytes(&canned_trace(1).struct_logs).unwrap();
		let per = json_bytes(&canned_trace(2).struct_logs).unwrap() - one;
		let capacity = (response_budget_bytes(max) - (one - per)) / per;

		let walk_to = async |total: u64| {
			walk_on(max, move |window| {
				let left = total.saturating_sub(window.step_offset);
				Ok(canned_trace(left.min(window.limit) as usize))
			})
			.await
		};

		let (collected, _, result) = walk_to(capacity).await;
		result.expect("a trace that fits must be returned, not refused");
		assert_eq!(collected.struct_logs.len() as u64, capacity);

		let (_, _, result) = walk_to(capacity + 1).await;
		assert!(
			matches!(result, Err(ClientError::TraceTooLarge { fits }) if fits == capacity),
			"the refusal must name what the caller can actually ask for: {result:?}",
		);
	}

	#[test]
	fn the_reserve_covers_the_largest_possible_return_value() {
		let max = sc_cli::RPC_DEFAULT_MAX_RESPONSE_SIZE_MB * 1024 * 1024;
		// `limits::CALLDATA_BYTES`, which every frame's output is checked against in `exec`.
		let worst = ExecutionTraceV1 {
			return_value: vec![0u8; 128 * 1024].into(),
			..ExecutionTraceV1::default()
		};
		let rendered = json_bytes_of(&worst);
		let budget = response_budget_bytes(max);

		assert!(
			budget + rendered <= max as u64,
			"{budget} bytes of steps plus a {rendered} byte trace overflows {max}",
		);
	}

	#[tokio::test]
	async fn the_limit_the_refusal_names_is_one_that_succeeds() {
		let max = sc_cli::RPC_DEFAULT_MAX_RESPONSE_SIZE_MB * 1024 * 1024;
		// Cheap for the first stretch, then twelve stack words a step for the rest.
		let heterogeneous = |window: TraceWindow| {
			let expensive = ExecutionStepV1 {
				kind: ExecutionStepKindV1::EVMOpcode {
					pc: 1,
					op: EvmOpcodeV1(0x55),
					stack: vec![vec![0xab; 32].into(); 12],
					memory: vec![],
					storage: None,
				},
				..ExecutionStepV1::default()
			};
			let step = |i: u64| {
				if i < 40_000 { ExecutionStepV1::default() } else { expensive.clone() }
			};

			Ok(ExecutionTraceV1 {
				struct_logs: (window.step_offset..window.step_offset + window.limit)
					.map(step)
					.collect(),
				..ExecutionTraceV1::default()
			})
		};

		let (_, _, result) = walk_on(max, heterogeneous).await;
		let Err(ClientError::TraceTooLarge { fits }) = result else {
			panic!("a trace this long cannot be returned whole: {result:?}");
		};

		// Ask for exactly what the refusal named.
		let walk = TraceWalk { caller_limit: Some(fits) };
		let first = walk.first_window(max);
		let mut collected = heterogeneous(first).unwrap();
		let capped = extend_with_remaining_windows(
			&mut collected,
			walk,
			max,
			first,
			Narrowing::default(),
			|window| async move { heterogeneous(window) }.boxed(),
		)
		.await;

		capped.unwrap_or_else(|e| panic!("`limit: {fits}` was named but refused: {e}"));
		assert_eq!(collected.struct_logs.len() as u64, fits);
	}
}
