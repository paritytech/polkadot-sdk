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

//! Fetch and price the markets of [`pallet_price_oracle::venues`] through the node [`Fetcher`].
//!
//! ```text
//! cargo test -p sc-price-oracle --lib venues_are_fetched_and_priced -- --nocapture
//! ```

use crate::{
	fetcher::{Fetcher, MarketResponses},
	now_ms,
};
use futures::{channel::mpsc, StreamExt};
use pallet_price_oracle::{
	price_market,
	pricing::{parse_decimal, PairSettings},
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

/// One market per constructor in [`venues`], tagged with the constructor name.
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

fn name_of(stored: &[(&'static str, StoredMarket)], id: MarketId) -> &'static str {
	stored[id.0 as usize].0
}

/// Print elapsed time, response sizes and whether the market priced.
fn report_fetched(
	stored: &[(&'static str, StoredMarket)],
	fetched: MarketResponses,
	elapsed: Duration,
) {
	let (name, market) = &stored[fetched.market.0 as usize];
	println!("{name}  {elapsed:?}");
	fetched.responses.iter().for_each(|(tag, body)| {
		println!("  query {tag:?}: {} bytes", body.len());
	});
	match price_market(market, &settings(), fetched.responses, now_ms()) {
		Ok(price) => println!("  priced at {price}"),
		Err(e) => println!("  not priced: {}", String::from_utf8_lossy(&e.0)),
	}
	println!();
}

#[tokio::test]
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
	let parse = async {
		rx.map(|fetched| (started.elapsed(), fetched)).collect::<Vec<_>>().await
	};
	let (failures, fetched) = futures::join!(fetch, parse);
	assert!(!fetched.is_empty(), "no market could be fetched");
	fetched.into_iter().for_each(|(elapsed, r)| report_fetched(&stored, r, elapsed));
	failures.iter().for_each(|(id, failure)| {
		println!("{}  {:?}", name_of(&stored, *id), started.elapsed());
		println!("  not fetched: {failure:?}");
		println!();
	});
}
