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

//! Checker for the statement-store v2 unstable JSON-RPC API (PR #11989).
//!
//! Two phases against a live node:
//!
//! 1. A one-shot **contract** pass asserts the fixed guarantees of the surface: submit outcomes
//!    (`new`/`known`/`invalid`/`rejected`), `matchAny` rejection at the RPC boundary, the
//!    128-filter cap, and per-connection subscription ownership.
//! 2. A **soak** loop opens one long-lived subscription with several filters, then submits and
//!    confirms live delivery for a configured duration, tracking latency, undelivered statements,
//!    unexpected `stop` events, and subscription restarts. This is the part meant to run for hours
//!    and surface leaks or subscription decay the short native tests cannot.
//!
//! Accounts `0..num_accounts` must already hold on-chain allowances (see the `setup-allowances`
//! binary); account `num_accounts` is left without one on purpose to exercise the `rejected` path.

use anyhow::{anyhow, Context};
use clap::Parser;
use codec::Encode;
use log::{error, info, warn};
use sc_statement_store::test_utils::{create_test_statement, get_keypair};
use sp_core::Bytes;
use sp_runtime::BoundedVec;
use sp_statement_store::{AddFilterResponse, SubmitOutcome, SubscribeEvent, Topic, TopicFilter};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use subxt::rpcs::{
	client::{rpc_params, RpcSubscription},
	RpcClient,
};

const SUBSCRIBE: &str = "statement_unstable_subscribe";
const UNSUBSCRIBE: &str = "statement_unstable_unsubscribe";
const SUBMIT: &str = "statement_unstable_submit";
const ADD_FILTER: &str = "statement_unstable_add_filter";
/// Server-side cap on filters per subscription; adding the `MAX + 1`-th must report `limitReached`.
const MAX_FILTERS_PER_SUBSCRIPTION: usize = 128;

#[derive(Parser, Debug)]
#[command(name = "rpc-check")]
#[command(about = "Soak and contract checker for the statement-store v2 unstable RPC", long_about = None)]
struct Args {
	/// Comma-separated RPC WebSocket endpoints. The first is the primary node used for submit and
	/// subscribe; a second, when present, drives cross-node and per-connection checks.
	#[arg(long, required = true, value_delimiter = ',')]
	rpc_endpoints: Vec<String>,

	/// Accounts provisioned with an allowance by `setup-allowances`; the checker signs with these.
	#[arg(long, default_value_t = 100)]
	num_accounts: u32,

	/// Soak duration in seconds; the loop stops once this elapses.
	#[arg(long, default_value_t = 5400)]
	duration_secs: u64,

	/// Delay between soak submissions in milliseconds.
	#[arg(long, default_value_t = 1000)]
	submit_interval_ms: u64,

	/// How long to wait for a submitted statement to arrive over the subscription, in seconds.
	#[arg(long, default_value_t = 30)]
	delivery_timeout_secs: u64,

	/// Number of filters attached to the soak subscription, each on its own topic.
	#[arg(long, default_value_t = 4)]
	filter_count: u32,

	/// Payload size of each soak statement in bytes.
	#[arg(long, default_value_t = 256)]
	data_size: usize,

	/// Seconds between soak progress reports.
	#[arg(long, default_value_t = 60)]
	report_interval_secs: u64,

	/// Skip the one-shot contract pass and run only the soak.
	#[arg(long, default_value_t = false)]
	skip_contract: bool,

	/// Maximum subscription re-opens before the soak gives up.
	#[arg(long, default_value_t = 5)]
	max_restarts: u32,
}

/// A topic whose first bytes encode `seed`, so distinct seeds yield distinct routing keys.
fn topic_for(seed: u32) -> Topic {
	let mut bytes = [0u8; 32];
	bytes[0..4].copy_from_slice(&seed.to_le_bytes());
	Topic(bytes)
}

fn now_secs() -> u32 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map(|d| d.as_secs() as u32)
		.unwrap_or(0)
}

fn match_all(topic: Topic) -> TopicFilter {
	TopicFilter::MatchAll(BoundedVec::truncate_from(vec![topic]))
}

fn match_any(topic: Topic) -> TopicFilter {
	TopicFilter::MatchAny(BoundedVec::truncate_from(vec![topic]))
}

/// Outcome of waiting for one submitted statement to arrive over the subscription.
enum Delivery {
	/// Delivered and tagged with the expected filter id.
	Ok(Duration),
	/// Delivered but the reported `filterIds` did not include the expected filter.
	WrongFilter,
	/// The subscription emitted `stop`.
	Stopped,
	/// The subscription ended or errored before delivery.
	Broken(String),
	/// No matching `newStatements` entry arrived within the timeout.
	Timeout,
}

async fn connect(url: &str) -> Result<RpcClient, anyhow::Error> {
	RpcClient::from_insecure_url(url)
		.await
		.with_context(|| format!("failed to open RPC client to {url}"))
}

async fn submit(
	rpc: &RpcClient,
	statement: &sp_statement_store::Statement,
) -> Result<SubmitOutcome, anyhow::Error> {
	let encoded: Bytes = statement.encode().into();
	rpc.request(SUBMIT, rpc_params![encoded]).await.map_err(Into::into)
}

async fn subscribe(rpc: &RpcClient) -> Result<RpcSubscription<SubscribeEvent>, anyhow::Error> {
	rpc.subscribe(SUBSCRIBE, rpc_params![], UNSUBSCRIBE).await.map_err(Into::into)
}

fn subscription_id(sub: &RpcSubscription<SubscribeEvent>) -> Result<String, anyhow::Error> {
	sub.subscription_id()
		.map(ToOwned::to_owned)
		.ok_or_else(|| anyhow!("subscription accepted without an id"))
}

async fn add_filter(
	rpc: &RpcClient,
	sub_id: &str,
	filter: TopicFilter,
) -> Result<AddFilterResponse, anyhow::Error> {
	rpc.request(ADD_FILTER, rpc_params![sub_id, filter]).await.map_err(Into::into)
}

/// Drains subscription events until the `target` statement arrives on `want_filter`, the
/// subscription stops or breaks, or `timeout` elapses.
async fn await_delivery(
	sub: &mut RpcSubscription<SubscribeEvent>,
	target: &Bytes,
	want_filter: &str,
	timeout: Duration,
) -> Delivery {
	let deadline = Instant::now() + timeout;
	loop {
		let remaining = deadline.saturating_duration_since(Instant::now());
		if remaining.is_zero() {
			return Delivery::Timeout;
		}
		match tokio::time::timeout(remaining, sub.next()).await {
			Err(_) => return Delivery::Timeout,
			Ok(None) => return Delivery::Broken("subscription stream ended".into()),
			Ok(Some(Err(e))) => return Delivery::Broken(e.to_string()),
			Ok(Some(Ok(SubscribeEvent::Stop))) => return Delivery::Stopped,
			Ok(Some(Ok(SubscribeEvent::NewStatements { statements }))) => {
				for entry in statements {
					if &entry.statement == target {
						return if entry.filter_ids.iter().any(|id| id == want_filter) {
							Delivery::Ok(
								timeout - deadline.saturating_duration_since(Instant::now()),
							)
						} else {
							Delivery::WrongFilter
						};
					}
				}
			},
			// Replay events are irrelevant here: the soak subscribes before it submits, so every
			// statement it cares about arrives live.
			Ok(Some(Ok(_))) => continue,
		}
	}
}

/// Runs the one-shot contract checks, returning the number that failed.
async fn run_contract(primary: &RpcClient, secondary: Option<&RpcClient>, args: &Args) -> u32 {
	let mut failed = 0u32;
	let mut check = |name: &str, ok: bool| {
		if ok {
			info!("contract PASS: {name}");
		} else {
			error!("contract FAIL: {name}");
			failed += 1;
		}
	};

	// Submit outcomes: a fresh statement is new, the same one again is known.
	let acct = get_keypair(0);
	let topic = topic_for(1_000_000);
	let stmt = create_test_statement(&acct, &[topic], None, vec![1, 2, 3], u32::MAX, now_secs());
	match submit(primary, &stmt).await {
		Ok(o) => check("submit new", matches!(o, SubmitOutcome::New)),
		Err(e) => check(&format!("submit new (rpc error: {e})"), false),
	}
	match submit(primary, &stmt).await {
		Ok(o) => check("resubmit known", matches!(o, SubmitOutcome::Known)),
		Err(e) => check(&format!("resubmit known (rpc error: {e})"), false),
	}

	// An already-expired statement fails validation.
	let expired =
		create_test_statement(&acct, &[topic_for(1_000_001)], None, vec![9], 1, now_secs());
	match submit(primary, &expired).await {
		Ok(o) => check("submit expired -> invalid", matches!(o, SubmitOutcome::Invalid(_))),
		Err(e) => check(&format!("submit expired (rpc error: {e})"), false),
	}

	// An account without an allowance is rejected.
	let no_allowance = get_keypair(args.num_accounts);
	let stmt = create_test_statement(
		&no_allowance,
		&[topic_for(1_000_002)],
		None,
		vec![7],
		u32::MAX,
		now_secs(),
	);
	match submit(primary, &stmt).await {
		Ok(o) => check("submit no-allowance -> rejected", matches!(o, SubmitOutcome::Rejected(_))),
		Err(e) => check(&format!("submit no-allowance (rpc error: {e})"), false),
	}

	// `matchAny` is rejected at the RPC boundary.
	match subscribe(primary).await {
		Ok(sub) => match subscription_id(&sub) {
			Ok(id) => {
				let rejected =
					add_filter(primary, &id, match_any(topic_for(2_000_000))).await.is_err();
				check("addFilter matchAny -> rejected", rejected);
			},
			Err(e) => check(&format!("subscribe for matchAny ({e})"), false),
		},
		Err(e) => check(&format!("subscribe for matchAny ({e})"), false),
	}

	// The 128-filter cap: the first MAX succeed, the next reports limitReached.
	match subscribe(primary).await {
		Ok(sub) => match subscription_id(&sub) {
			Ok(id) => {
				let mut ok_count = 0usize;
				let mut limit_hit = false;
				for i in 0..=MAX_FILTERS_PER_SUBSCRIPTION {
					match add_filter(primary, &id, match_all(topic_for(3_000_000 + i as u32))).await
					{
						Ok(AddFilterResponse::Ok(_)) => ok_count += 1,
						Ok(AddFilterResponse::LimitReached(_)) => {
							limit_hit = true;
							break;
						},
						Err(e) => {
							warn!("cap check: addFilter {i} errored: {e}");
							break;
						},
					}
				}
				check(
					&format!("filter cap ({ok_count} accepted, limit_hit={limit_hit})"),
					ok_count == MAX_FILTERS_PER_SUBSCRIPTION && limit_hit,
				);
			},
			Err(e) => check(&format!("subscribe for cap ({e})"), false),
		},
		Err(e) => check(&format!("subscribe for cap ({e})"), false),
	}

	// Per-connection ownership: a second connection cannot add a filter to the first's
	// subscription.
	match secondary {
		Some(other) => match subscribe(primary).await {
			Ok(sub) => match subscription_id(&sub) {
				Ok(id) => {
					let denied =
						add_filter(other, &id, match_all(topic_for(4_000_000))).await.is_err();
					check("cross-connection addFilter -> denied", denied);
				},
				Err(e) => check(&format!("subscribe for scoping ({e})"), false),
			},
			Err(e) => check(&format!("subscribe for scoping ({e})"), false),
		},
		None => info!("contract SKIP: cross-connection scoping (only one endpoint)"),
	}

	failed
}

#[derive(Default)]
struct SoakStats {
	submitted: u64,
	delivered: u64,
	not_new: u64,
	wrong_filter: u64,
	timeouts: u64,
	stops: u64,
	restarts: u64,
	latencies_ms: Vec<u128>,
}

impl SoakStats {
	fn percentile(&self, p: f64) -> u128 {
		if self.latencies_ms.is_empty() {
			return 0;
		}
		let mut v = self.latencies_ms.clone();
		v.sort_unstable();
		let idx = (((v.len() - 1) as f64) * p).round() as usize;
		v[idx]
	}

	fn report(&self, elapsed: Duration) {
		info!(
			"soak progress @ {}s: submitted={} delivered={} not_new={} wrong_filter={} timeouts={} stops={} restarts={} latency_ms(p50={} p95={} max={})",
			elapsed.as_secs(),
			self.submitted,
			self.delivered,
			self.not_new,
			self.wrong_filter,
			self.timeouts,
			self.stops,
			self.restarts,
			self.percentile(0.5),
			self.percentile(0.95),
			self.latencies_ms.iter().copied().max().unwrap_or(0),
		);
	}
}

/// Opens a subscription and attaches `filter_count` filters, one per topic, draining each filter's
/// (empty) replay so live delivery starts from a clean slate. Returns the subscription and the
/// `(topic, filter_id)` pairs.
async fn open_soak_subscription(
	rpc: &RpcClient,
	filter_count: u32,
) -> Result<(RpcSubscription<SubscribeEvent>, Vec<(Topic, String)>), anyhow::Error> {
	let sub = subscribe(rpc).await?;
	let sub_id = subscription_id(&sub)?;
	let mut filters = Vec::new();
	for i in 0..filter_count {
		let topic = topic_for(i);
		match add_filter(rpc, &sub_id, match_all(topic)).await? {
			AddFilterResponse::Ok(id) => filters.push((topic, id)),
			AddFilterResponse::LimitReached(_) => {
				return Err(anyhow!("unexpected limitReached while opening soak subscription"))
			},
		}
	}
	Ok((sub, filters))
}

async fn run_soak(rpc: &RpcClient, args: &Args) -> Result<SoakStats, anyhow::Error> {
	let mut stats = SoakStats::default();
	let (mut sub, filters) = open_soak_subscription(rpc, args.filter_count).await?;
	info!("soak: subscription open with {} filters", filters.len());

	let start = Instant::now();
	let duration = Duration::from_secs(args.duration_secs);
	let interval = Duration::from_millis(args.submit_interval_ms);
	let delivery_timeout = Duration::from_secs(args.delivery_timeout_secs);
	let report_every = Duration::from_secs(args.report_interval_secs);
	let mut last_report = Instant::now();
	let mut seq: u32 = 0;

	while start.elapsed() < duration {
		let filter_idx = (seq as usize) % filters.len();
		let (topic, ref filter_id) = filters[filter_idx];
		let account = get_keypair(seq % args.num_accounts);
		let data = vec![(seq & 0xff) as u8; args.data_size];
		let statement = create_test_statement(&account, &[topic], None, data, u32::MAX, seq);
		let encoded: Bytes = statement.encode().into();
		seq += 1;

		match submit(rpc, &statement).await {
			Ok(SubmitOutcome::New) => stats.submitted += 1,
			Ok(other) => {
				stats.not_new += 1;
				warn!("soak: submit returned {other:?}, expected new");
				continue;
			},
			Err(e) => {
				stats.not_new += 1;
				warn!("soak: submit rpc error: {e}");
				continue;
			},
		}

		match await_delivery(&mut sub, &encoded, filter_id, delivery_timeout).await {
			Delivery::Ok(latency) => {
				stats.delivered += 1;
				stats.latencies_ms.push(latency.as_millis());
			},
			Delivery::WrongFilter => {
				stats.wrong_filter += 1;
				warn!("soak: statement delivered without its expected filter id");
			},
			Delivery::Timeout => {
				stats.timeouts += 1;
				warn!("soak: statement not delivered within {}s", args.delivery_timeout_secs);
			},
			Delivery::Stopped => {
				stats.stops += 1;
				error!("soak: subscription emitted stop");
				if !restart(rpc, args, &mut sub, &filters, &mut stats).await {
					break;
				}
			},
			Delivery::Broken(reason) => {
				error!("soak: subscription broken: {reason}");
				if !restart(rpc, args, &mut sub, &filters, &mut stats).await {
					break;
				}
			},
		}

		if last_report.elapsed() >= report_every {
			stats.report(start.elapsed());
			last_report = Instant::now();
		}

		tokio::time::sleep(interval).await;
	}

	Ok(stats)
}

/// Re-opens the subscription after a stop or break, re-attaching the same filters. Returns false
/// once the restart budget is spent, ending the soak.
async fn restart(
	rpc: &RpcClient,
	args: &Args,
	sub: &mut RpcSubscription<SubscribeEvent>,
	filters: &[(Topic, String)],
	stats: &mut SoakStats,
) -> bool {
	if stats.restarts >= args.max_restarts as u64 {
		error!("soak: restart budget exhausted after {} re-opens", stats.restarts);
		return false;
	}
	stats.restarts += 1;
	match open_soak_subscription(rpc, filters.len() as u32).await {
		Ok((new_sub, _)) => {
			*sub = new_sub;
			warn!("soak: subscription re-opened (restart {})", stats.restarts);
			true
		},
		Err(e) => {
			error!("soak: failed to re-open subscription: {e}");
			false
		},
	}
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
	let _ = env_logger::try_init_from_env(
		env_logger::Env::default().filter_or(env_logger::DEFAULT_FILTER_ENV, "info"),
	);
	let args = Args::parse();
	if args.rpc_endpoints.is_empty() {
		return Err(anyhow!("at least one --rpc-endpoints value is required"));
	}
	if args.filter_count == 0 {
		return Err(anyhow!("--filter-count must be at least 1"));
	}

	let primary = connect(&args.rpc_endpoints[0]).await?;
	let secondary = match args.rpc_endpoints.get(1) {
		Some(url) => Some(connect(url).await?),
		None => None,
	};

	let mut contract_failures = 0;
	if args.skip_contract {
		info!("contract phase skipped");
	} else {
		info!("running contract checks against {}", args.rpc_endpoints[0]);
		contract_failures = run_contract(&primary, secondary.as_ref(), &args).await;
		info!("contract phase done: {contract_failures} failure(s)");
	}

	info!("running soak for {}s against {}", args.duration_secs, args.rpc_endpoints[0]);
	let stats = run_soak(&primary, &args).await?;
	stats.report(Duration::from_secs(args.duration_secs));

	let soak_failed = stats.timeouts > 0 ||
		stats.wrong_filter > 0 ||
		stats.stops > 0 ||
		stats.restarts > 0 ||
		stats.delivered != stats.submitted;
	info!(
		"RESULT: contract_failures={contract_failures} soak_delivered={}/{} soak_failed={soak_failed}",
		stats.delivered, stats.submitted,
	);

	if contract_failures > 0 || soak_failed {
		return Err(anyhow!(
			"checker found problems: {contract_failures} contract failure(s), soak_failed={soak_failed}"
		));
	}
	info!("checker passed");
	Ok(())
}
