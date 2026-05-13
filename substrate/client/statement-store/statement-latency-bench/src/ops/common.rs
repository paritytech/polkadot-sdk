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

//! Shared pure helpers for `statement-ops-bench` subcommands.

use anyhow::{anyhow, Result};
use futures::{Stream, StreamExt};
use sc_statement_store::test_utils::get_keypair;
use sp_core::{blake2_256, sr25519, Pair};
use sp_statement_store::{Statement, StatementEvent};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Aggregate latency statistics for a sequence of samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stats {
	pub min: f64,
	pub avg: f64,
	pub max: f64,
	pub count: usize,
}

/// Compute min/avg/max over the provided samples.
///
/// Returns `None` if the input is empty so callers can format an explicit
/// "no samples" message instead of NaNs.
pub fn calc_stats(values: impl IntoIterator<Item = f64>) -> Option<Stats> {
	let values: Vec<_> = values.into_iter().collect();
	if values.is_empty() {
		return None;
	}
	let min = values.iter().copied().fold(f64::INFINITY, f64::min);
	let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
	let avg = values.iter().sum::<f64>() / values.len() as f64;
	Some(Stats { min, avg, max, count: values.len() })
}

/// Build a signed sr25519 statement.
///
/// The expiry is stored as `(timestamp << 32) | seq`; both halves are
/// independently configurable so callers can produce strictly-increasing
/// expiries even when the wall-clock timestamp is identical.
pub fn build_statement(
	keypair: &sr25519::Pair,
	topic: [u8; 32],
	channel: [u8; 32],
	expiry_timestamp_secs: u32,
	seq: u32,
	data: Vec<u8>,
) -> Statement {
	let mut statement = Statement::new();
	statement.set_channel(channel);
	statement.set_expiry_from_parts(expiry_timestamp_secs, seq);
	statement.set_topic(0, topic.into());
	statement.set_plain_data(data);
	statement.sign_sr25519_private(keypair);
	statement
}

/// Derive a topic deterministically from a run id, scope tag and index.
pub fn derive_topic(run_id: u64, scope: &str, idx: u32) -> [u8; 32] {
	blake2_256(format!("ops-bench-topic-{run_id}-{scope}-{idx}").as_bytes())
}

/// Derive a channel deterministically from a run id, scope tag and index.
pub fn derive_channel(run_id: u64, scope: &str, idx: u32) -> [u8; 32] {
	blake2_256(format!("ops-bench-channel-{run_id}-{scope}-{idx}").as_bytes())
}

/// Provider of the current unix timestamp in seconds.
///
/// Abstracted so time-dependent code can be unit-tested with a fixed clock.
pub trait Clock: Send + Sync {
	fn now_unix_secs(&self) -> u64;
}

pub struct SystemClock;

impl Clock for SystemClock {
	fn now_unix_secs(&self) -> u64 {
		SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("System clock before UNIX_EPOCH")
			.as_secs()
	}
}

/// Return the unix-timestamp-in-seconds that is `offset_secs` ahead of `clock.now`.
pub fn expiry_seconds_from_now(clock: &dyn Clock, offset_secs: u64) -> u32 {
	clock.now_unix_secs().saturating_add(offset_secs) as u32
}

/// Parse an optional SURI/seed phrase into an sr25519 keypair.
///
/// `None` falls back to the first deterministic benchmark account
/// (`sc_statement_store::test_utils::get_keypair(0)`), matching the
/// behaviour of the existing ring-topology bench.
pub fn parse_seed(seed: Option<&str>) -> Result<sr25519::Pair> {
	match seed {
		Some(suri) => sr25519::Pair::from_string(suri, None)
			.map_err(|e| anyhow!("Invalid sr25519 SURI {suri:?}: {e:?}")),
		None => Ok(get_keypair(0)),
	}
}

/// Parse a 32-byte topic from a hex string (with or without the `0x` prefix).
///
/// Used by clap's `value_parser` for the `--topic` flag and by tests.
pub fn parse_topic_hex(s: &str) -> std::result::Result<[u8; 32], String> {
	let stripped = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
	if stripped.len() != 64 {
		return Err(format!(
			"--topic must be 64 hex chars (32 bytes), got {} chars",
			stripped.len()
		));
	}
	let mut out = [0u8; 32];
	for (i, chunk) in stripped.as_bytes().chunks(2).enumerate() {
		let hi = hex_nibble(chunk[0])?;
		let lo = hex_nibble(chunk[1])?;
		out[i] = (hi << 4) | lo;
	}
	Ok(out)
}

fn hex_nibble(c: u8) -> std::result::Result<u8, String> {
	match c {
		b'0'..=b'9' => Ok(c - b'0'),
		b'a'..=b'f' => Ok(c - b'a' + 10),
		b'A'..=b'F' => Ok(c - b'A' + 10),
		_ => Err(format!("invalid hex character: {:?}", c as char)),
	}
}

/// Consume the initial dump of a statement subscription, returning the total
/// number of statements observed.
///
/// Per the `subscribe_statement` RPC contract, the subscription first delivers
/// any matching statements already in the store in one or more
/// `NewStatements { remaining, .. }` events. `remaining = Some(0)` (or `None`,
/// treated as "no more in the initial dump") marks the end of the dump and the
/// start of the live stream. The function stops at that boundary.
pub async fn drain_initial_batch<S>(stream: &mut S, timeout: Duration) -> Result<usize>
where
	S: Stream<Item = anyhow::Result<StatementEvent>> + Unpin,
{
	let mut total = 0usize;
	loop {
		let next = match tokio::time::timeout(timeout, stream.next()).await {
			Ok(Some(Ok(event))) => event,
			Ok(Some(Err(e))) => {
				return Err(anyhow!("Subscription stream error during initial drain: {e}"))
			},
			Ok(None) => return Err(anyhow!("Subscription closed before initial drain completed")),
			Err(_) => return Err(anyhow!("Initial drain timed out after {timeout:?}")),
		};

		match next {
			StatementEvent::NewStatements { statements, remaining } => {
				total += statements.len();
				match remaining {
					Some(0) | None => return Ok(total),
					Some(_) => continue,
				}
			},
		}
	}
}

/// Wait for the next `NewStatements` event from the subscription, returning the
/// number of statements delivered.
///
/// Used after `drain_initial_batch` when we expect a live-streamed statement
/// (e.g. our own newly-submitted one in the `propagation` subcommand).
pub async fn next_statement_batch<S>(stream: &mut S, timeout: Duration) -> Result<usize>
where
	S: Stream<Item = anyhow::Result<StatementEvent>> + Unpin,
{
	match tokio::time::timeout(timeout, stream.next()).await {
		Ok(Some(Ok(StatementEvent::NewStatements { statements, .. }))) => Ok(statements.len()),
		Ok(Some(Err(e))) => Err(anyhow!("Subscription stream error: {e}")),
		Ok(None) => Err(anyhow!("Subscription closed before delivering statement")),
		Err(_) => Err(anyhow!("Timed out waiting for statement after {timeout:?}")),
	}
}

/// Fixed-value clock used by sibling test modules to make expiry maths deterministic.
#[cfg(test)]
pub struct FixedClock(pub u64);

#[cfg(test)]
impl Clock for FixedClock {
	fn now_unix_secs(&self) -> u64 {
		self.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use futures::stream;
	use sp_core::Bytes;

	#[test]
	fn calc_stats_empty_returns_none() {
		assert!(calc_stats(std::iter::empty::<f64>()).is_none());
	}

	#[test]
	fn calc_stats_single_value() {
		let stats = calc_stats([1.5]).unwrap();
		assert_eq!(stats.count, 1);
		assert_eq!(stats.min, 1.5);
		assert_eq!(stats.max, 1.5);
		assert_eq!(stats.avg, 1.5);
	}

	#[test]
	fn calc_stats_identical_values() {
		let stats = calc_stats([2.0, 2.0, 2.0, 2.0]).unwrap();
		assert_eq!(stats.count, 4);
		assert_eq!(stats.min, 2.0);
		assert_eq!(stats.max, 2.0);
		assert_eq!(stats.avg, 2.0);
	}

	#[test]
	fn calc_stats_descending_input() {
		let stats = calc_stats([5.0, 4.0, 3.0, 2.0, 1.0]).unwrap();
		assert_eq!(stats.count, 5);
		assert_eq!(stats.min, 1.0);
		assert_eq!(stats.max, 5.0);
		assert_eq!(stats.avg, 3.0);
	}

	#[test]
	fn calc_stats_mixed_values() {
		let stats = calc_stats([0.1, 0.9, 0.3, 0.7, 0.5]).unwrap();
		assert_eq!(stats.count, 5);
		assert_eq!(stats.min, 0.1);
		assert_eq!(stats.max, 0.9);
		assert!((stats.avg - 0.5).abs() < 1e-9);
	}

	#[test]
	fn build_statement_round_trip() {
		let keypair = get_keypair(7);
		let topic = derive_topic(42, "build_test", 1);
		let channel = derive_channel(42, "build_test", 1);
		let data = vec![0xAB; 64];
		let s = build_statement(&keypair, topic, channel, 1_000_000, 9, data.clone());

		assert_eq!(s.topic(0).map(|t| t.0), Some(topic));
		assert_eq!(s.channel(), Some(channel));
		assert_eq!(s.get_expiration_timestamp_secs(), 1_000_000);
		assert_eq!(s.data().cloned(), Some(data));
		assert_eq!(s.expiry(), ((1_000_000u64) << 32) | 9);
		assert!(matches!(
			s.verify_signature(),
			sp_statement_store::SignatureVerificationResult::Valid(_)
		));
	}

	#[test]
	fn derive_topic_deterministic() {
		assert_eq!(derive_topic(1, "x", 0), derive_topic(1, "x", 0));
	}

	#[test]
	fn derive_topic_varies_by_inputs() {
		let a = derive_topic(1, "x", 0);
		assert_ne!(a, derive_topic(2, "x", 0));
		assert_ne!(a, derive_topic(1, "y", 0));
		assert_ne!(a, derive_topic(1, "x", 1));
	}

	#[test]
	fn derive_channel_differs_from_topic() {
		assert_ne!(derive_topic(1, "x", 0), derive_channel(1, "x", 0));
	}

	#[test]
	fn expiry_seconds_from_now_uses_clock() {
		let clock = FixedClock(2_000_000);
		assert_eq!(expiry_seconds_from_now(&clock, 60), 2_000_060);
	}

	#[test]
	fn parse_seed_none_returns_default_keypair() {
		let kp = parse_seed(None).unwrap();
		assert_eq!(kp.public(), get_keypair(0).public());
	}

	#[test]
	fn parse_seed_alice_returns_alice() {
		let kp = parse_seed(Some("//Alice")).unwrap();
		let alice = sr25519::Pair::from_string("//Alice", None).unwrap();
		assert_eq!(kp.public(), alice.public());
	}

	#[test]
	fn parse_seed_garbage_errors() {
		assert!(parse_seed(Some("not a valid SURI %%%")).is_err());
	}

	#[test]
	fn parse_topic_hex_without_prefix() {
		let s = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
		let t = parse_topic_hex(s).unwrap();
		assert_eq!(t[0], 0x00);
		assert_eq!(t[1], 0x11);
		assert_eq!(t[15], 0xff);
		assert_eq!(t[16], 0x00);
		assert_eq!(t[31], 0xff);
	}

	#[test]
	fn parse_topic_hex_with_0x_prefix() {
		let lower =
			parse_topic_hex("0x00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
				.unwrap();
		let upper =
			parse_topic_hex("0X00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
				.unwrap();
		assert_eq!(lower, upper);
	}

	#[test]
	fn parse_topic_hex_uppercase_digits() {
		let lower =
			parse_topic_hex("deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef")
				.unwrap();
		let upper =
			parse_topic_hex("DEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF")
				.unwrap();
		assert_eq!(lower, upper);
	}

	#[test]
	fn parse_topic_hex_rejects_short_input() {
		assert!(parse_topic_hex("00").is_err());
		assert!(parse_topic_hex("").is_err());
	}

	#[test]
	fn parse_topic_hex_rejects_long_input() {
		let s = "00".repeat(33);
		assert!(parse_topic_hex(&s).is_err());
	}

	#[test]
	fn parse_topic_hex_rejects_non_hex_chars() {
		let s = format!("zz{}", "00".repeat(31));
		assert!(parse_topic_hex(&s).is_err());
	}

	fn ev(remaining: Option<u32>, count: usize) -> StatementEvent {
		StatementEvent::NewStatements {
			statements: (0..count).map(|i| Bytes(vec![i as u8])).collect(),
			remaining,
		}
	}

	#[tokio::test]
	async fn drain_initial_batch_stops_on_remaining_zero() {
		let events = vec![
			Ok(ev(Some(3), 2)),
			Ok(ev(Some(1), 2)),
			Ok(ev(Some(0), 1)),
			// This trailing event must not be consumed.
			Ok(ev(None, 99)),
		];
		let mut s = stream::iter(events);
		let total = drain_initial_batch(&mut s, Duration::from_secs(1)).await.unwrap();
		assert_eq!(total, 5);
		let leftover = s.next().await.unwrap().unwrap();
		match leftover {
			StatementEvent::NewStatements { statements, .. } => assert_eq!(statements.len(), 99),
		}
	}

	#[tokio::test]
	async fn drain_initial_batch_stops_on_remaining_none() {
		let events = vec![Ok(ev(None, 4))];
		let mut s = stream::iter(events);
		let total = drain_initial_batch(&mut s, Duration::from_secs(1)).await.unwrap();
		assert_eq!(total, 4);
	}

	#[tokio::test]
	async fn drain_initial_batch_handles_empty_dump() {
		let events = vec![Ok(ev(Some(0), 0))];
		let mut s = stream::iter(events);
		let total = drain_initial_batch(&mut s, Duration::from_secs(1)).await.unwrap();
		assert_eq!(total, 0);
	}

	#[tokio::test(start_paused = true)]
	async fn drain_initial_batch_times_out() {
		let mut s = stream::pending::<anyhow::Result<StatementEvent>>();
		let timeout = Duration::from_millis(50);
		let fut = drain_initial_batch(&mut s, timeout);
		tokio::pin!(fut);

		// Advance virtual time past the timeout and confirm the future errors.
		let (result, _) = tokio::join!(fut, async {
			tokio::time::advance(timeout + Duration::from_millis(1)).await;
		});
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn next_statement_batch_returns_count() {
		let events = vec![Ok(ev(None, 3))];
		let mut s = stream::iter(events);
		let n = next_statement_batch(&mut s, Duration::from_secs(1)).await.unwrap();
		assert_eq!(n, 3);
	}

	#[tokio::test(start_paused = true)]
	async fn next_statement_batch_times_out() {
		let mut s = stream::pending::<anyhow::Result<StatementEvent>>();
		let timeout = Duration::from_millis(50);
		let fut = next_statement_batch(&mut s, timeout);
		tokio::pin!(fut);
		let (result, _) = tokio::join!(fut, async {
			tokio::time::advance(timeout + Duration::from_millis(1)).await;
		});
		assert!(result.is_err());
	}

	#[test]
	fn build_statement_data_size_preserved() {
		let kp = get_keypair(0);
		let s = build_statement(
			&kp,
			derive_topic(1, "x", 0),
			derive_channel(1, "x", 0),
			1_000_000,
			0,
			vec![0u8; 512],
		);
		// Round-trip through SCALE to ensure the structure encodes.
		let bytes = s.encode();
		assert!(!bytes.is_empty());
	}
}
