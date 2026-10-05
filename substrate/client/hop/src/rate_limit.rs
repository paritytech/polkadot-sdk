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

//! Per-account and global submit rate limiting for HOP.
//!
//! Two layers of token-bucket limiting:
//!
//! 1. **Per-account**: two buckets per `SenderId` (request rate + bandwidth). Both must admit a
//!    call for it to proceed.
//! 2. **Global**: one aggregate bandwidth bucket shared across all senders. A coordinated
//!    multi-account attack that stays within per-account limits can still exhaust the global
//!    bucket, preventing pool exhaustion.
//!
//! The per-sender map is capped at [`RateLimitConfig::max_tracked_senders`] entries.
//! When full, the sender with the most remaining token capacity is evicted — keeping
//! exhausted senders tracked so they cannot re-enter with a fresh bucket.
//!
//! Refill happens lazily on each check using monotonic `Instant`s, so idle
//! users never block a background task.

use crate::types::SenderId;
use parking_lot::{Mutex, RwLock};
use std::{
	collections::HashMap,
	sync::Arc,
	time::{Duration, Instant},
};

/// How long a rate-limit entry can sit untouched before maintenance evicts it.
const STALE_ENTRY_TTL: Duration = Duration::from_secs(3600);

/// A classic token bucket: `tokens` refills at `refill_per_sec` up to `capacity`.
#[derive(Debug, Clone)]
struct TokenBucket {
	tokens: f64,
	capacity: f64,
	refill_per_sec: f64,
	last: Instant,
}

impl TokenBucket {
	fn new(capacity: f64, refill_per_sec: f64) -> Self {
		Self { tokens: capacity, capacity, refill_per_sec, last: Instant::now() }
	}

	/// Refill based on elapsed time and cap at `capacity`.
	fn refill(&mut self, now: Instant) {
		let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
		if elapsed > 0.0 {
			self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
			self.last = now;
		}
	}

	/// Try to consume `n` tokens. On failure returns the `Duration` until enough
	/// tokens will have refilled to satisfy the request.
	fn try_consume(&mut self, n: f64, now: Instant) -> Result<(), Duration> {
		self.refill(now);
		if self.tokens >= n {
			self.tokens -= n;
			Ok(())
		} else {
			let deficit = n - self.tokens;
			let secs =
				if self.refill_per_sec > 0.0 { deficit / self.refill_per_sec } else { f64::MAX };
			Err(Duration::from_secs_f64(secs.clamp(0.0, u64::MAX as f64)))
		}
	}
}

#[derive(Debug)]
struct UserRateState {
	requests: TokenBucket,
	bandwidth: TokenBucket,
	last_touch: Instant,
}

/// Configuration for the per-account and global submit rate limiter.
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
	/// If false, `RateLimiter::check` always admits immediately.
	pub enabled: bool,
	/// Sustained requests per account per minute.
	pub submit_rate_per_min: u32,
	/// Burst size for the request bucket.
	pub submit_burst: u32,
	/// Sustained bytes per account per minute.
	pub bandwidth_per_min: u64,
	/// Burst size for the bandwidth bucket, in bytes.
	pub bandwidth_burst: u64,
	/// Aggregate (cross-account) sustained bandwidth in bytes per minute.
	pub global_bandwidth_per_min: u64,
	/// Burst size for the global bandwidth bucket, in bytes.
	pub global_bandwidth_burst: u64,
	/// Maximum number of distinct senders tracked simultaneously. When the limit
	/// is reached the entry with the most remaining token capacity is evicted,
	/// keeping rate-exhausted senders tracked so they cannot bypass limits by
	/// re-entering with a fresh bucket.
	pub max_tracked_senders: usize,
}

impl RateLimitConfig {
	/// Create a disabled config (admit everything).
	pub fn disabled() -> Self {
		Self {
			enabled: false,
			submit_rate_per_min: 0,
			submit_burst: 0,
			bandwidth_per_min: 0,
			bandwidth_burst: 0,
			global_bandwidth_per_min: 0,
			global_bandwidth_burst: 0,
			// When disabled, check() returns immediately and get_or_create is
			// never called, so this value is irrelevant. usize::MAX is explicit.
			max_tracked_senders: usize::MAX,
		}
	}
}

/// Per-account and global token-bucket rate limiter for HOP submissions.
pub struct RateLimiter {
	cfg: RateLimitConfig,
	users: RwLock<HashMap<SenderId, Arc<Mutex<UserRateState>>>>,
	/// Single bandwidth bucket shared across all senders.
	global_bandwidth: Mutex<TokenBucket>,
}

impl RateLimiter {
	/// Build a rate limiter from configuration.
	pub fn new(cfg: RateLimitConfig) -> Self {
		let global_bandwidth = Mutex::new(TokenBucket::new(
			cfg.global_bandwidth_burst as f64,
			cfg.global_bandwidth_per_min as f64 / 60.0,
		));
		Self { cfg, users: RwLock::new(HashMap::new()), global_bandwidth }
	}

	fn new_state(&self, now: Instant) -> UserRateState {
		let requests = TokenBucket::new(
			self.cfg.submit_burst as f64,
			self.cfg.submit_rate_per_min as f64 / 60.0,
		);
		let bandwidth = TokenBucket::new(
			self.cfg.bandwidth_burst as f64,
			self.cfg.bandwidth_per_min as f64 / 60.0,
		);
		UserRateState { requests, bandwidth, last_touch: now }
	}

	fn get_or_create(&self, sender_id: &SenderId, now: Instant) -> Arc<Mutex<UserRateState>> {
		// Fast path: sender already tracked — read lock only.
		if let Some(state) = self.users.read().get(sender_id).cloned() {
			return state;
		}

		let mut users = self.users.write();

		// Double-check after acquiring the write lock; another thread may have
		// inserted between the read-lock drop and this write-lock acquisition.
		if let Some(state) = users.get(sender_id) {
			return state.clone();
		}

		// New sender: enforce the map size cap before inserting.
		if users.len() >= self.cfg.max_tracked_senders {
			// Evict the sender with the most remaining token capacity. Evicting
			// by LRU would let a rate-exhausted sender re-enter with a fresh
			// bucket; keeping it tracked preserves its exhausted state.
			// try_lock() avoids blocking under the write lock; locked entries
			// (actively being checked) are skipped as poor eviction candidates.
			let to_evict = users
				.iter()
				.filter_map(|(id, s)| {
					s.try_lock().map(|guard| {
						// Normalise to [0.0, 1.0]; take the min across both
						// buckets so a sender exhausted on either is not evicted.
						let refreshed = |b: &TokenBucket| {
							(b.tokens + b.last.elapsed().as_secs_f64() * b.refill_per_sec)
								.min(b.capacity)
						};
						let req_fill =
							refreshed(&guard.requests) / guard.requests.capacity.max(f64::EPSILON);
						let bw_fill = refreshed(&guard.bandwidth) /
							guard.bandwidth.capacity.max(f64::EPSILON);
						(req_fill.min(bw_fill), *id)
					})
				})
				.max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
				.map(|(_, id)| id);

			if let Some(id) = to_evict {
				users.remove(&id);
				tracing::debug!(
					target: "hop",
					limit = self.cfg.max_tracked_senders,
					"Rate-limiter sender cap reached; evicted most-capacity entry",
				);
			}
		}

		let state = Arc::new(Mutex::new(self.new_state(now)));
		users.insert(*sender_id, state.clone());
		state
	}

	/// Check whether this account may submit `data_len` bytes right now.
	///
	/// Returns `Ok(())` on admission (tokens consumed from both the per-sender
	/// and global buckets) or `Err(retry_after_secs)` when any bucket is empty.
	/// On rejection all previously consumed tokens are refunded so the caller
	/// may retry without a phantom charge.
	pub fn check(&self, sender_id: &SenderId, data_len: u64) -> Result<(), u64> {
		if !self.cfg.enabled {
			return Ok(());
		}

		let now = Instant::now();

		// --- Per-sender check ---
		// Keep the Arc so we can re-lock for refund if the global check fails.
		let state_arc = self.get_or_create(sender_id, now);
		{
			let mut state = state_arc.lock();
			state.last_touch = now;

			if let Some(wait) = state.requests.try_consume(1.0, now).err() {
				return Err(wait.as_secs().max(1));
			}

			// Bandwidth exhausted: refund the request token already consumed and reject.
			// Both charges are rolled back so the caller can retry after refill.
			if let Err(wait) = state.bandwidth.try_consume(data_len as f64, now) {
				// Refund the request token we just took so the two buckets stay consistent.
				state.requests.tokens = (state.requests.tokens + 1.0).min(state.requests.capacity);
				return Err(wait.as_secs().max(1));
			}
		}

		// --- Global check ---
		// Per-sender lock is released before taking the global lock to avoid
		// any circular dependency with future code paths.
		if let Err(wait) = self.global_bandwidth.lock().try_consume(data_len as f64, now) {
			// Global pool is saturated: refund the per-sender tokens we just consumed
			// so this call has zero net effect on both layers.
			let mut state = state_arc.lock();
			state.bandwidth.tokens =
				(state.bandwidth.tokens + data_len as f64).min(state.bandwidth.capacity);
			state.requests.tokens = (state.requests.tokens + 1.0).min(state.requests.capacity);
			return Err(wait.as_secs().max(1));
		}

		Ok(())
	}

	/// Drop entries that haven't been touched in `STALE_ENTRY_TTL`.
	/// Called from the pool's maintenance loop.
	pub fn evict_stale(&self) {
		if !self.cfg.enabled {
			return;
		}
		let now = Instant::now();
		let mut users = self.users.write();
		users.retain(|_, state| {
			let state = state.lock();
			now.saturating_duration_since(state.last_touch) < STALE_ENTRY_TTL
		});
	}

	/// Number of tracked senders (for tests / metrics).
	pub fn tracked_senders(&self) -> usize {
		self.users.read().len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const SENDER_A: SenderId = [1u8; 32];
	const SENDER_B: SenderId = [2u8; 32];
	const SENDER_C: SenderId = [3u8; 32];

	fn test_cfg() -> RateLimitConfig {
		RateLimitConfig {
			enabled: true,
			submit_rate_per_min: 60,
			submit_burst: 3,
			bandwidth_per_min: 6_000,
			bandwidth_burst: 6_000,
			// Global bucket large enough not to interfere with per-sender tests.
			global_bandwidth_per_min: 1_000_000,
			global_bandwidth_burst: 1_000_000,
			// Cap large enough not to interfere with per-sender tests.
			max_tracked_senders: 1_000_000,
		}
	}

	fn tight_global_cfg() -> RateLimitConfig {
		RateLimitConfig {
			enabled: true,
			// Per-sender limits are generous so they don't trigger first.
			submit_rate_per_min: 600,
			submit_burst: 100,
			bandwidth_per_min: 1_000_000,
			bandwidth_burst: 1_000_000,
			// Global bucket is tight: only 5_000 bytes of burst.
			global_bandwidth_per_min: 5_000,
			global_bandwidth_burst: 5_000,
			max_tracked_senders: 1_000_000,
		}
	}

	/// Config with a very small sender cap so eviction behaviour can be tested
	/// without creating hundreds of thousands of senders.
	fn small_cap_cfg(cap: usize) -> RateLimitConfig {
		RateLimitConfig {
			enabled: true,
			submit_rate_per_min: 600,
			submit_burst: 1_000,
			bandwidth_per_min: 1_000_000_000,
			bandwidth_burst: 1_000_000_000,
			global_bandwidth_per_min: 1_000_000_000,
			global_bandwidth_burst: 1_000_000_000,
			max_tracked_senders: cap,
		}
	}

	#[test]
	fn disabled_admits_everything() {
		let rl = RateLimiter::new(RateLimitConfig::disabled());
		for _ in 0..100 {
			rl.check(&SENDER_A, 1_000_000_000).unwrap();
		}
	}

	#[test]
	fn burst_then_limited() {
		let rl = RateLimiter::new(test_cfg());
		// Burst of 3.
		rl.check(&SENDER_A, 100).unwrap();
		rl.check(&SENDER_A, 100).unwrap();
		rl.check(&SENDER_A, 100).unwrap();
		// 4th is limited.
		let err = rl.check(&SENDER_A, 100).unwrap_err();
		assert!(err >= 1);
	}

	#[test]
	fn bandwidth_exhaustion_limits() {
		let rl = RateLimiter::new(test_cfg());
		// Consume all 6000 bytes of burst in one call.
		rl.check(&SENDER_A, 6_000).unwrap();
		// Next call, even 1 byte, should be rejected.
		assert!(rl.check(&SENDER_A, 1).is_err());
	}

	#[test]
	fn isolated_per_sender() {
		let rl = RateLimiter::new(test_cfg());
		for _ in 0..3 {
			rl.check(&SENDER_A, 100).unwrap();
		}
		// A is limited, B is fresh.
		assert!(rl.check(&SENDER_A, 100).is_err());
		rl.check(&SENDER_B, 100).unwrap();
	}

	#[test]
	fn refills_over_time() {
		let cfg = RateLimitConfig {
			enabled: true,
			submit_rate_per_min: 60,
			submit_burst: 1,
			bandwidth_per_min: 600_000,
			bandwidth_burst: 600_000,
			global_bandwidth_per_min: 1_000_000_000,
			global_bandwidth_burst: 1_000_000_000,
			max_tracked_senders: 1_000_000,
		};
		let rl = RateLimiter::new(cfg);
		rl.check(&SENDER_A, 100).unwrap();
		assert!(rl.check(&SENDER_A, 100).is_err());

		// Fake a 2-second advance by mutating the bucket's `last`.
		{
			let state = rl.get_or_create(&SENDER_A, Instant::now());
			let mut state = state.lock();
			state.requests.last -= Duration::from_secs(2);
		}
		// Should now succeed (1 request/sec refill, 2 seconds elapsed).
		rl.check(&SENDER_A, 100).unwrap();
	}

	#[test]
	fn evict_stale_removes_untouched() {
		let rl = RateLimiter::new(test_cfg());
		rl.check(&SENDER_A, 100).unwrap();
		assert_eq!(rl.tracked_senders(), 1);

		// Backdate last_touch.
		{
			let state = rl.get_or_create(&SENDER_A, Instant::now());
			let mut state = state.lock();
			state.last_touch -= STALE_ENTRY_TTL + Duration::from_secs(1);
		}
		rl.evict_stale();
		assert_eq!(rl.tracked_senders(), 0);
	}

	// --- Global bucket tests ---

	#[test]
	fn global_bandwidth_exhaustion_blocks_all_senders() {
		let rate_limiter = RateLimiter::new(tight_global_cfg());

		// Sender A exhausts the global bucket (5_000 bytes of burst).
		rate_limiter.check(&SENDER_A, 5_000).unwrap();

		// Sender B is within its own per-sender limit but the global bucket is empty.
		assert!(
			rate_limiter.check(&SENDER_B, 1).is_err(),
			"sender B should be blocked by the global limit"
		);

		// Sender A is also blocked now (both per-sender bandwidth and global are exhausted).
		assert!(rate_limiter.check(&SENDER_A, 1).is_err(), "sender A should be blocked too");
	}

	#[test]
	fn global_rejection_refunds_per_sender_tokens() {
		let rl = RateLimiter::new(tight_global_cfg());

		// Exhaust the global bucket with sender A.
		rl.check(&SENDER_A, 5_000).unwrap();

		// Sender B attempts a submission — global rejects it.
		assert!(rl.check(&SENDER_B, 1).is_err());

		// Sender B's per-sender buckets must have been refunded: if we now advance
		// the global clock to refill the global bucket, sender B should succeed on
		// the very next attempt without hitting a per-sender limit.
		{
			let mut global = rl.global_bandwidth.lock();
			global.last -= Duration::from_secs(10); // fast-forward global refill
		}
		rl.check(&SENDER_B, 1)
			.expect("sender B should succeed once global bucket refills");
	}

	#[test]
	fn per_sender_limit_still_applies_when_global_is_available() {
		let rl = RateLimiter::new(tight_global_cfg());

		// Exhaust sender A's per-sender request burst (100 requests).
		for _ in 0..100 {
			rl.check(&SENDER_A, 1).unwrap();
		}

		// Sender A is blocked by its own per-sender limit even though the
		// global bucket still has capacity (we sent 100 bytes, global allows 5_000).
		assert!(rl.check(&SENDER_A, 1).is_err(), "per-sender request limit should still apply");

		// Sender C is unaffected.
		rl.check(&SENDER_C, 1).unwrap();
	}

	#[test]
	fn disabled_limiter_ignores_global_bucket() {
		let rl = RateLimiter::new(RateLimitConfig::disabled());
		// Should admit without touching any bucket even at enormous payload size.
		for _ in 0..1_000 {
			rl.check(&SENDER_A, u64::MAX / 2).unwrap();
		}
	}

	// --- Sender-cap tests ---

	#[test]
	fn sender_cap_prevents_unbounded_map_growth() {
		let rl = RateLimiter::new(small_cap_cfg(3));

		// Fill the map to the cap with three distinct senders.
		let senders: Vec<SenderId> = (0u8..3).map(|i| [i; 32]).collect();
		for s in &senders {
			rl.check(s, 1).unwrap();
		}
		assert_eq!(rl.tracked_senders(), 3);

		// A fourth sender must still be admitted (eviction makes room).
		let new_sender: SenderId = [0xFFu8; 32];
		rl.check(&new_sender, 1).unwrap();

		// Map must not exceed the cap.
		assert_eq!(rl.tracked_senders(), 3, "map must not grow past the cap");
	}

	#[test]
	fn exhausted_sender_survives_eviction_over_fresh_sender() {
		let rl = RateLimiter::new(small_cap_cfg(2));

		// A: exhaust its entire request burst (1_000 tokens in small_cap_cfg).
		for _ in 0..1_000 {
			rl.check(&SENDER_A, 1).unwrap();
		}
		assert!(rl.check(&SENDER_A, 1).is_err(), "A should be rate-limited");

		// B: fresh entry — only 1 request consumed, nearly full tokens.
		rl.check(&SENDER_B, 1).unwrap();
		assert_eq!(rl.tracked_senders(), 2);

		// C causes an eviction.  B has far more remaining capacity than A,
		// so B must be chosen for eviction — not A.  Evicting A would hand it
		// a fresh bucket and defeat the rate limit.
		rl.check(&SENDER_C, 1).unwrap();
		assert_eq!(rl.tracked_senders(), 2, "map must not exceed the cap");

		assert!(rl.users.read().contains_key(&SENDER_A), "exhausted A must survive");
		assert!(!rl.users.read().contains_key(&SENDER_B), "fresh B should be evicted");
		assert!(rl.users.read().contains_key(&SENDER_C), "C should be present");

		// A is still rate-limited — it did NOT receive a fresh bucket.
		assert!(rl.check(&SENDER_A, 1).is_err(), "A must still be rate-limited after eviction");
	}

	#[test]
	fn evicted_fresh_sender_can_resubmit_without_constraint() {
		// Verify the harmless inverse: a sender that WAS evicted (because it
		// had the most remaining capacity) can re-enter successfully — there is
		// no security concern when a non-exhausted sender loses its entry.
		let rl = RateLimiter::new(small_cap_cfg(2));

		// A: exhaust its request burst.
		for _ in 0..1_000 {
			rl.check(&SENDER_A, 1).unwrap();
		}
		assert!(rl.check(&SENDER_A, 1).is_err(), "A should be rate-limited");

		// B: fresh entry.
		rl.check(&SENDER_B, 1).unwrap();

		// C enters; B (most tokens remaining) is evicted, not A.
		rl.check(&SENDER_C, 1).unwrap();
		assert!(!rl.users.read().contains_key(&SENDER_B), "fresh B should have been evicted");

		// B re-enters — this is acceptable because B was not rate-limited when
		// evicted, so granting it a fresh bucket is not a security bypass.
		rl.check(&SENDER_B, 1).unwrap();
	}
}
