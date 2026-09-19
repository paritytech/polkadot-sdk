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

//! Live check that venue constructors still fetch and price against public APIs.
//!
//! ```text
//! cargo test -p sc-price-oracle --lib venues_are_fetched_and_priced -- --ignored --nocapture
//! ```

use crate::{
	fetcher::{Fetcher, MarketFailure},
	now_ms,
};
use futures::{channel::mpsc, StreamExt};
use pallet_price_oracle::{
	price_market,
	pricing::{self, parse_decimal, PairSettings},
	registry::StoredMarket,
	venues,
};
use sp_price_oracle::{
	market::{MarketId, VenueId},
	PairId, Price,
};
use sp_runtime::Permill;
use tokio::time::{Duration, Instant};

const PAIR: PairId = PairId(1);

fn p(s: &str) -> Price {
	parse_decimal(s).unwrap()
}

fn settings() -> PairSettings {
	PairSettings {
		max_spread: Permill::from_percent(1),
		max_trade_age_ms: 5 * 60 * 1_000,
		impact_size: p("10000"),
	}
}

/// One market per constructor in [`venues`].
fn markets() -> Vec<(&'static str, StoredMarket)> {
	[
		("binance_spot", venues::binance_spot(VenueId(0), PAIR, "DOTUSDT")),
		("binance_perp", venues::binance_perp(VenueId(1), PAIR, "DOTUSDT")),
		("okx_spot", venues::okx_spot(VenueId(2), PAIR, "DOT-USDT")),
		("okx_perp", venues::okx_perp(VenueId(3), PAIR, "DOT-USDT-SWAP", p("1"))),
		("bybit_spot", venues::bybit_spot(VenueId(4), PAIR, "DOTUSDT")),
		("bybit_perp", venues::bybit_perp(VenueId(5), PAIR, "DOTUSDT")),
		("mexc_spot", venues::mexc_spot(VenueId(6), PAIR, "DOTUSDT")),
		("mexc_perp", venues::mexc_perp(VenueId(7), PAIR, "DOT_USDT", p("0.1"))),
		("kucoin_spot", venues::kucoin_spot(VenueId(8), PAIR, "DOT-USDT")),
		("kucoin_perp", venues::kucoin_perp(VenueId(9), PAIR, "DOTUSDTM", p("1"))),
		("gate_spot", venues::gate_spot(VenueId(10), PAIR, "DOT_USDT")),
		("gate_perp", venues::gate_perp(VenueId(11), PAIR, "DOT_USDT", p("1"))),
		("bitget_perp", venues::bitget_perp(VenueId(12), PAIR, "DOTUSDT")),
		("coinbase_spot", venues::coinbase_spot(VenueId(13), PAIR, "DOT-USD")),
		("kraken_perp", venues::kraken_perp(VenueId(14), PAIR, "PF_DOTUSD")),
	]
	.into_iter()
	.map(|(name, market)| (name, market.unwrap()))
	.collect()
}

fn fetch_reason(failure: &MarketFailure) -> String {
	match failure {
		MarketFailure::Deadline => "deadline".into(),
		MarketFailure::Query(_, e) => e.to_string(),
	}
}

#[tokio::test]
#[ignore] // Requires network access to public exchanges; run with `--ignored`.
async fn venues_are_fetched_and_priced() {
	let fetcher = Fetcher::new().unwrap();
	let stored = markets();
	let wire: Vec<_> = stored
		.iter()
		.enumerate()
		.map(|(i, (_, market))| market.clone().to_wire(MarketId(i as u32)))
		.collect();
	let started = Instant::now();
	let deadline = started + Duration::from_secs(2);
	let (tx, rx) = mpsc::unbounded();
	let fetch = fetcher.fetch_markets(&wire, deadline, tx);
	let wait = async { rx.collect::<Vec<_>>().await };
	let (failures, fetched) = futures::join!(fetch, wait);

	let now = now_ms();
	let mut prices = Vec::new();
	let mut missing = Vec::new();
	fetched.into_iter().for_each(|r| {
		let (name, market) = &stored[r.market.0 as usize];
		match price_market(market, &settings(), r.responses, now) {
			Ok(price) => prices.push((market.venue, market.pair, price)),
			Err(e) => missing.push((*name, String::from_utf8_lossy(&e.0).into_owned())),
		}
	});
	failures.iter().for_each(|(id, failure)| {
		missing.push((stored[id.0 as usize].0, fetch_reason(failure)));
	});

	let n = prices.len();
	let m = stored.len();
	let quotes = pricing::aggregate(prices, &[PAIR], |_| Vec::new());
	match quotes.first() {
		Some(q) => println!("{n}/{m} priced  {price}", price = q.price),
		None => println!("{n}/{m} priced"),
	}
	missing.iter().for_each(|(name, reason)| println!("  {name}: {reason}"));

	assert!(n > 0, "no market could be fetched and priced");
}
