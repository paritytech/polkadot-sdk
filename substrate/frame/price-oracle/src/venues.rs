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

//! Definitions of the markets of well-known exchanges.
//!
//! Each function builds one market as stored by [`Pallet::set_market`](crate::Pallet::set_market),
//! with an order book query and a recent trades query. The definitions are generic over the
//! asset pair. The venue and pair are those the runtime assigns, and the symbol names any pair
//! the exchange lists, in the exchange's own convention.
//!
//! Each definition is tested against a sample response of the exchange. A definition is `None`
//! only if one of its values does not fit the bounds of the registry.

use crate::{
	registry::{MaxParamName, MaxParamValue, StoredMarket, StoredQuery, StoredRequest},
	schema::{LevelLayout, Path, PathStep, ResponseSchema, TimeFormat},
};
use alloc::{format, vec::Vec};
use frame_support::{traits::Get, BoundedVec};
use sp_price_oracle::{
	market::{Method, QueryTag, VenueId},
	PairId, Price,
};
use sp_runtime::traits::One;

/// The tag of the order book query of every market defined in this module.
const BOOK: QueryTag = QueryTag(0);
/// The tag of the trades query of every market defined in this module.
const TRADES: QueryTag = QueryTag(1);

/// The time after which a request is abandoned. A venue that does not answer in time is left
/// out of the tick.
const TIMEOUT_MS: u32 = 1_000;
/// The maximum size of an order book response. A book of [`DEPTH`] levels is about 6 KiB.
const MAX_BOOK_BYTES: u32 = 8 * 1024;
/// The maximum size of a trades response. A single trade with its envelope is about 300 bytes.
const MAX_TRADES_BYTES: u32 = 512;
/// The number of levels requested where the exchange allows choosing.
const DEPTH: &str = "100";
/// The number of trades requested where the exchange allows choosing. Only the latest trade is
/// needed, and exchanges return the most recent ones.
const TRADES_LIMIT: &str = "1";
/// The layout of a level given as a `[price, amount, ..]` array, used by most exchanges.
const ARRAY: LevelLayout = LevelLayout::Array { price: 0, amount: 1 };

/// The Binance spot market of `symbol`.
pub fn binance_spot(venue: VenueId, pair: PairId, symbol: &str) -> Option<StoredMarket> {
	binance(venue, pair, "api.binance.com", "/api/v3", symbol)
}

/// The Binance USDⓈ-M perpetual of `symbol`. Sizes are in the base asset.
pub fn binance_perp(venue: VenueId, pair: PairId, symbol: &str) -> Option<StoredMarket> {
	binance(venue, pair, "fapi.binance.com", "/fapi/v1", symbol)
}

/// The book is `{"bids": [[price, qty], ..], "asks": [..]}` and the trades are
/// `[{"time": ms, ..}, ..]`.
fn binance(
	venue: VenueId,
	pair: PairId,
	host: &str,
	prefix: &str,
	symbol: &str,
) -> Option<StoredMarket> {
	market(
		venue,
		pair,
		Price::one(),
		book(
			request(
				host,
				&format!("{prefix}/depth"),
				&[("symbol", symbol), ("limit", DEPTH)],
				MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("bids")?],
			&[PathStep::key("asks")?],
			ARRAY,
		)?,
		trades(
			request(
				host,
				&format!("{prefix}/trades"),
				&[("symbol", symbol), ("limit", TRADES_LIMIT)],
				MAX_TRADES_BYTES,
			)?,
			&[],
			&[PathStep::key("time")?],
			TimeFormat::Millis,
		)?,
	)
}

/// The OKX spot market of `inst_id`.
pub fn okx_spot(venue: VenueId, pair: PairId, inst_id: &str) -> Option<StoredMarket> {
	okx(venue, pair, inst_id, Price::one())
}

/// The OKX perpetual swap of `inst_id`. Sizes are in contracts of `contract_size` base units,
/// the instrument's `ctVal`.
pub fn okx_perp(
	venue: VenueId,
	pair: PairId,
	inst_id: &str,
	contract_size: Price,
) -> Option<StoredMarket> {
	okx(venue, pair, inst_id, contract_size)
}

/// The book is `{"data": [{"bids": [[price, size, ..], ..], "asks": [..]}]}` and the trades
/// are `{"data": [{"ts": "ms", ..}, ..]}`.
fn okx(venue: VenueId, pair: PairId, inst_id: &str, contract_size: Price) -> Option<StoredMarket> {
	const HOST: &str = "www.okx.com";
	market(
		venue,
		pair,
		contract_size,
		book(
			request(
				HOST,
				"/api/v5/market/books",
				&[("instId", inst_id), ("sz", DEPTH)],
				MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("data")?, PathStep::Index(0), PathStep::key("bids")?],
			&[PathStep::key("data")?, PathStep::Index(0), PathStep::key("asks")?],
			ARRAY,
		)?,
		trades(
			request(
				HOST,
				"/api/v5/market/trades",
				&[("instId", inst_id), ("limit", TRADES_LIMIT)],
				MAX_TRADES_BYTES,
			)?,
			&[PathStep::key("data")?],
			&[PathStep::key("ts")?],
			TimeFormat::Millis,
		)?,
	)
}

/// The Bybit spot market of `symbol`.
pub fn bybit_spot(venue: VenueId, pair: PairId, symbol: &str) -> Option<StoredMarket> {
	bybit(venue, pair, "spot", symbol)
}

/// The Bybit USDT perpetual of `symbol`. Sizes are in the base asset.
pub fn bybit_perp(venue: VenueId, pair: PairId, symbol: &str) -> Option<StoredMarket> {
	bybit(venue, pair, "linear", symbol)
}

/// The book is `{"result": {"b": [[price, size], ..], "a": [..]}}` and the trades are
/// `{"result": {"list": [{"time": "ms", ..}, ..]}}`.
fn bybit(venue: VenueId, pair: PairId, category: &str, symbol: &str) -> Option<StoredMarket> {
	const HOST: &str = "api.bybit.com";
	market(
		venue,
		pair,
		Price::one(),
		book(
			request(
				HOST,
				"/v5/market/orderbook",
				&[("category", category), ("symbol", symbol), ("limit", DEPTH)],
				MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("result")?, PathStep::key("b")?],
			&[PathStep::key("result")?, PathStep::key("a")?],
			ARRAY,
		)?,
		trades(
			request(
				HOST,
				"/v5/market/recent-trade",
				&[("category", category), ("symbol", symbol), ("limit", TRADES_LIMIT)],
				MAX_TRADES_BYTES,
			)?,
			&[PathStep::key("result")?, PathStep::key("list")?],
			&[PathStep::key("time")?],
			TimeFormat::Millis,
		)?,
	)
}

/// The MEXC spot market of `symbol`.
///
/// The book is `{"bids": [[price, qty], ..], "asks": [..]}` and the trades are
/// `[{"time": ms, ..}, ..]`.
pub fn mexc_spot(venue: VenueId, pair: PairId, symbol: &str) -> Option<StoredMarket> {
	const HOST: &str = "api.mexc.com";
	market(
		venue,
		pair,
		Price::one(),
		book(
			request(
				HOST,
				"/api/v3/depth",
				&[("symbol", symbol), ("limit", DEPTH)],
				MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("bids")?],
			&[PathStep::key("asks")?],
			ARRAY,
		)?,
		trades(
			request(
				HOST,
				"/api/v3/trades",
				&[("symbol", symbol), ("limit", TRADES_LIMIT)],
				MAX_TRADES_BYTES,
			)?,
			&[],
			&[PathStep::key("time")?],
			TimeFormat::Millis,
		)?,
	)
}

/// The MEXC USDT perpetual of `symbol`. Sizes are in contracts of `contract_size` base units,
/// the instrument's `contractSize`.
///
/// The book is `{"data": {"bids": [[price, vol, ..], ..], "asks": [..]}}` and the trades are
/// `{"data": [{"t": ms, ..}, ..]}`.
pub fn mexc_perp(
	venue: VenueId,
	pair: PairId,
	symbol: &str,
	contract_size: Price,
) -> Option<StoredMarket> {
	const HOST: &str = "contract.mexc.com";
	market(
		venue,
		pair,
		contract_size,
		book(
			request(
				HOST,
				&format!("/api/v1/contract/depth/{symbol}"),
				&[("limit", DEPTH)],
				MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("data")?, PathStep::key("bids")?],
			&[PathStep::key("data")?, PathStep::key("asks")?],
			ARRAY,
		)?,
		trades(
			request(
				HOST,
				&format!("/api/v1/contract/deals/{symbol}"),
				&[("limit", TRADES_LIMIT)],
				MAX_TRADES_BYTES,
			)?,
			&[PathStep::key("data")?],
			&[PathStep::key("t")?],
			TimeFormat::Millis,
		)?,
	)
}

/// The KuCoin spot market of `symbol`.
///
/// The book is `{"data": {"bids": [[price, size], ..], "asks": [..]}}` and the ticker is
/// `{"data": {"time": ms, ..}}`.
pub fn kucoin_spot(venue: VenueId, pair: PairId, symbol: &str) -> Option<StoredMarket> {
	const HOST: &str = "api.kucoin.com";
	market(
		venue,
		pair,
		Price::one(),
		book(
			request(
				HOST,
				"/api/v1/market/orderbook/level2_100",
				&[("symbol", symbol)],
				MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("data")?, PathStep::key("bids")?],
			&[PathStep::key("data")?, PathStep::key("asks")?],
			ARRAY,
		)?,
		trades(
			request(
				HOST,
				"/api/v1/market/orderbook/level1",
				&[("symbol", symbol)],
				MAX_TRADES_BYTES,
			)?,
			&[PathStep::key("data")?],
			&[PathStep::key("time")?],
			TimeFormat::Millis,
		)?,
	)
}

/// The KuCoin USDT perpetual of `symbol`. Sizes are in lots of `contract_size` base units, the
/// contract's `multiplier`.
///
/// The book is `{"data": {"bids": [[price, size], ..], "asks": [..]}}` and the ticker, the
/// last trade, is `{"data": {"ts": ns, ..}}`.
pub fn kucoin_perp(
	venue: VenueId,
	pair: PairId,
	symbol: &str,
	contract_size: Price,
) -> Option<StoredMarket> {
	const HOST: &str = "api-futures.kucoin.com";
	market(
		venue,
		pair,
		contract_size,
		book(
			request(HOST, "/api/v1/level2/depth100", &[("symbol", symbol)], MAX_BOOK_BYTES)?,
			&[PathStep::key("data")?, PathStep::key("bids")?],
			&[PathStep::key("data")?, PathStep::key("asks")?],
			ARRAY,
		)?,
		trades(
			request(HOST, "/api/v1/ticker", &[("symbol", symbol)], MAX_TRADES_BYTES)?,
			&[PathStep::key("data")?],
			&[PathStep::key("ts")?],
			TimeFormat::Nanos,
		)?,
	)
}

/// The Gate.io spot market of `currency_pair`.
///
/// The book is `{"bids": [[price, amount], ..], "asks": [..]}` and the trades are
/// `[{"create_time_ms": "ms", ..}, ..]`.
pub fn gate_spot(venue: VenueId, pair: PairId, currency_pair: &str) -> Option<StoredMarket> {
	const HOST: &str = "api.gateio.ws";
	market(
		venue,
		pair,
		Price::one(),
		book(
			request(
				HOST,
				"/api/v4/spot/order_book",
				&[("currency_pair", currency_pair), ("limit", DEPTH)],
				MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("bids")?],
			&[PathStep::key("asks")?],
			ARRAY,
		)?,
		trades(
			request(
				HOST,
				"/api/v4/spot/trades",
				&[("currency_pair", currency_pair), ("limit", TRADES_LIMIT)],
				MAX_TRADES_BYTES,
			)?,
			&[],
			&[PathStep::key("create_time_ms")?],
			TimeFormat::Millis,
		)?,
	)
}

/// The Gate.io USDT perpetual of `contract`. Sizes are in contracts of `contract_size` base
/// units, the contract's `quanto_multiplier`.
///
/// The book is `{"bids": [{"p": price, "s": size}, ..], "asks": [..]}` and the trades are
/// `[{"create_time": s, ..}, ..]`.
pub fn gate_perp(
	venue: VenueId,
	pair: PairId,
	contract: &str,
	contract_size: Price,
) -> Option<StoredMarket> {
	const HOST: &str = "api.gateio.ws";
	market(
		venue,
		pair,
		contract_size,
		book(
			request(
				HOST,
				"/api/v4/futures/usdt/order_book",
				&[("contract", contract), ("limit", DEPTH)],
				MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("bids")?],
			&[PathStep::key("asks")?],
			LevelLayout::Object { price: bounded("p")?, amount: bounded("s")? },
		)?,
		trades(
			request(
				HOST,
				"/api/v4/futures/usdt/trades",
				&[("contract", contract), ("limit", TRADES_LIMIT)],
				MAX_TRADES_BYTES,
			)?,
			&[],
			&[PathStep::key("create_time")?],
			TimeFormat::Seconds,
		)?,
	)
}

/// The Bitget USDT perpetual of `symbol`. Sizes are in the base asset.
///
/// The book is `{"data": {"bids": [[price, size], ..], "asks": [..]}}` and the trades are
/// `{"data": [{"ts": "ms", ..}, ..]}`.
pub fn bitget_perp(venue: VenueId, pair: PairId, symbol: &str) -> Option<StoredMarket> {
	const HOST: &str = "api.bitget.com";
	let product = ("productType", "USDT-FUTURES");
	market(
		venue,
		pair,
		Price::one(),
		book(
			request(
				HOST,
				"/api/v2/mix/market/merge-depth",
				&[product, ("symbol", symbol)],
				MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("data")?, PathStep::key("bids")?],
			&[PathStep::key("data")?, PathStep::key("asks")?],
			ARRAY,
		)?,
		trades(
			request(
				HOST,
				"/api/v2/mix/market/fills",
				&[product, ("symbol", symbol), ("limit", TRADES_LIMIT)],
				MAX_TRADES_BYTES,
			)?,
			&[PathStep::key("data")?],
			&[PathStep::key("ts")?],
			TimeFormat::Millis,
		)?,
	)
}

/// The Coinbase spot market of `product_id`.
///
/// The book is `{"pricebook": {"bids": [{"price": .., "size": ..}, ..], "asks": [..]}}` and
/// the trades are `{"trades": [{"time": iso8601, ..}, ..]}`.
pub fn coinbase_spot(venue: VenueId, pair: PairId, product_id: &str) -> Option<StoredMarket> {
	const HOST: &str = "api.coinbase.com";
	market(
		venue,
		pair,
		Price::one(),
		book(
			request(
				HOST,
				"/api/v3/brokerage/market/product_book",
				&[("product_id", product_id), ("limit", DEPTH)],
				// Levels are objects with whitespace, about twice the size of arrays.
				2 * MAX_BOOK_BYTES,
			)?,
			&[PathStep::key("pricebook")?, PathStep::key("bids")?],
			&[PathStep::key("pricebook")?, PathStep::key("asks")?],
			LevelLayout::Object { price: bounded("price")?, amount: bounded("size")? },
		)?,
		trades(
			request(
				HOST,
				&format!("/api/v3/brokerage/market/products/{product_id}/ticker"),
				&[("limit", TRADES_LIMIT)],
				MAX_TRADES_BYTES,
			)?,
			&[PathStep::key("trades")?],
			&[PathStep::key("time")?],
			TimeFormat::Iso8601,
		)?,
	)
}

/// The Kraken Futures perpetual of `symbol`. Sizes are in the base asset.
///
/// The book is `{"orderBook": {"bids": [[price, size], ..], "asks": [..]}}` and the ticker,
/// with the last trade, is `{"ticker": {"lastTime": iso8601, ..}}`.
pub fn kraken_perp(venue: VenueId, pair: PairId, symbol: &str) -> Option<StoredMarket> {
	const HOST: &str = "futures.kraken.com";
	market(
		venue,
		pair,
		Price::one(),
		book(
			request(HOST, "/derivatives/api/v3/orderbook", &[("symbol", symbol)], MAX_BOOK_BYTES)?,
			&[PathStep::key("orderBook")?, PathStep::key("bids")?],
			&[PathStep::key("orderBook")?, PathStep::key("asks")?],
			ARRAY,
		)?,
		trades(
			request(
				HOST,
				&format!("/derivatives/api/v3/tickers/{symbol}"),
				&[],
				// The ticker carries the day's statistics as well.
				2 * MAX_TRADES_BYTES,
			)?,
			&[PathStep::key("ticker")?],
			&[PathStep::key("lastTime")?],
			TimeFormat::Iso8601,
		)?,
	)
}

/// Convert `s` to a bounded byte vector.
fn bounded<S: Get<u32>>(s: &str) -> Option<BoundedVec<u8, S>> {
	s.as_bytes().to_vec().try_into().ok()
}

/// Build a bounded JSON path from `steps`.
fn path(steps: &[PathStep]) -> Option<Path> {
	steps.to_vec().try_into().ok()
}

/// Build the stored `GET` request for `host` and `path` with the given query parameters.
fn request(
	host: &str,
	path: &str,
	query: &[(&str, &str)],
	max_response_bytes: u32,
) -> Option<StoredRequest> {
	let query: Vec<(BoundedVec<u8, MaxParamName>, BoundedVec<u8, MaxParamValue>)> = query
		.iter()
		.map(|(n, v)| Some((bounded(n)?, bounded(v)?)))
		.collect::<Option<_>>()?;
	Some(StoredRequest {
		method: Method::Get,
		host: bounded(host)?,
		path: bounded(path)?,
		query: query.try_into().ok()?,
		headers: BoundedVec::new(),
		body: BoundedVec::new(),
		timeout_ms: TIMEOUT_MS,
		max_response_bytes,
	})
}

/// Build the order book query of a market.
fn book(
	request: StoredRequest,
	bids: &[PathStep],
	asks: &[PathStep],
	layout: LevelLayout,
) -> Option<StoredQuery> {
	let schema = ResponseSchema::OrderBook { bids: path(bids)?, asks: path(asks)?, layout };
	Some(StoredQuery { tag: BOOK, request, schema })
}

/// Build the trades query of a market.
fn trades(
	request: StoredRequest,
	rows: &[PathStep],
	time: &[PathStep],
	format: TimeFormat,
) -> Option<StoredQuery> {
	let schema = ResponseSchema::Trades { trades: path(rows)?, time: path(time)?, format };
	Some(StoredQuery { tag: TRADES, request, schema })
}

/// Assemble an active market from its two queries.
fn market(
	venue: VenueId,
	pair: PairId,
	contract_size: Price,
	book: StoredQuery,
	trades: StoredQuery,
) -> Option<StoredMarket> {
	let queries = alloc::vec![book, trades].try_into().ok()?;
	Some(StoredMarket { venue, pair, queries, contract_size, active: true })
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		price_market,
		pricing::{parse_decimal, PairSettings},
	};
	use sp_price_oracle::runtime_api::ParseError;
	use sp_runtime::Permill;

	/// 2026-09-17T10:00:00Z. The fixtures were recorded in the two hours before it, with the
	/// recorded asset trading near 1 USDT.
	const NOW_MS: u64 = 1_789_639_200_000;
	/// Long enough to cover the recording session.
	const MAX_TRADE_AGE_MS: u32 = 2 * 60 * 60 * 1_000;
	const VENUE: VenueId = VenueId(0);
	const PAIR: PairId = PairId(0);

	macro_rules! fixture {
		($name:literal) => {
			include_bytes!(concat!("../fixtures/", $name, ".json")).as_slice()
		};
	}

	fn p(s: &str) -> Price {
		parse_decimal(s).unwrap()
	}

	fn settings() -> PairSettings {
		PairSettings {
			max_spread: Permill::from_percent(1),
			max_trade_age_ms: MAX_TRADE_AGE_MS,
			impact_size: p("5000"),
		}
	}

	/// The recorded responses of a market, tagged for `price_market`.
	fn responses(book: &[u8], trades: &[u8]) -> Vec<(QueryTag, Vec<u8>)> {
		vec![(BOOK, book.to_vec()), (TRADES, trades.to_vec())]
	}

	/// Price `market` from its recorded responses. The price must be near 1 USDT, and the
	/// market must turn stale once the recorded trade is older than allowed.
	fn assert_prices(market: &StoredMarket, book: &[u8], trades: &[u8]) {
		let price = price_market(market, &settings(), responses(book, trades), NOW_MS).unwrap();
		assert!(price > p("0.95") && price < p("1.05"), "unexpected price {price:?}");

		let later = NOW_MS + MAX_TRADE_AGE_MS as u64 + 1;
		let stale = price_market(market, &settings(), responses(book, trades), later);
		assert_eq!(stale, Err(ParseError(b"StaleTrades".to_vec())));
	}

	#[test]
	fn binance_spot_fixture() {
		let market = binance_spot(VENUE, PAIR, "DOTUSDT").unwrap();
		assert_prices(&market, fixture!("binance_spot_book"), fixture!("binance_spot_trades"));
	}

	#[test]
	fn binance_perp_fixture() {
		let market = binance_perp(VENUE, PAIR, "DOTUSDT").unwrap();
		assert_prices(&market, fixture!("binance_perp_book"), fixture!("binance_perp_trades"));
	}

	#[test]
	fn okx_spot_fixture() {
		let market = okx_spot(VENUE, PAIR, "DOT-USDT").unwrap();
		assert_prices(&market, fixture!("okx_spot_book"), fixture!("okx_spot_trades"));
	}

	#[test]
	fn okx_perp_fixture() {
		let market = okx_perp(VENUE, PAIR, "DOT-USDT-SWAP", p("1")).unwrap();
		assert_prices(&market, fixture!("okx_perp_book"), fixture!("okx_perp_trades"));
	}

	#[test]
	fn bybit_spot_fixture() {
		let market = bybit_spot(VENUE, PAIR, "DOTUSDT").unwrap();
		assert_prices(&market, fixture!("bybit_spot_book"), fixture!("bybit_spot_trades"));
	}

	#[test]
	fn bybit_perp_fixture() {
		let market = bybit_perp(VENUE, PAIR, "DOTUSDT").unwrap();
		assert_prices(&market, fixture!("bybit_perp_book"), fixture!("bybit_perp_trades"));
	}

	#[test]
	fn mexc_spot_fixture() {
		let market = mexc_spot(VENUE, PAIR, "DOTUSDT").unwrap();
		assert_prices(&market, fixture!("mexc_spot_book"), fixture!("mexc_spot_trades"));
	}

	#[test]
	fn mexc_perp_fixture() {
		let market = mexc_perp(VENUE, PAIR, "DOT_USDT", p("0.1")).unwrap();
		assert_prices(&market, fixture!("mexc_perp_book"), fixture!("mexc_perp_trades"));
	}

	#[test]
	fn kucoin_spot_fixture() {
		let market = kucoin_spot(VENUE, PAIR, "DOT-USDT").unwrap();
		assert_prices(&market, fixture!("kucoin_spot_book"), fixture!("kucoin_spot_trades"));
	}

	#[test]
	fn kucoin_perp_fixture() {
		let market = kucoin_perp(VENUE, PAIR, "DOTUSDTM", p("1")).unwrap();
		assert_prices(&market, fixture!("kucoin_perp_book"), fixture!("kucoin_perp_trades"));
	}

	#[test]
	fn gate_spot_fixture() {
		let market = gate_spot(VENUE, PAIR, "DOT_USDT").unwrap();
		assert_prices(&market, fixture!("gate_spot_book"), fixture!("gate_spot_trades"));
	}

	#[test]
	fn gate_perp_fixture() {
		let market = gate_perp(VENUE, PAIR, "DOT_USDT", p("1")).unwrap();
		assert_prices(&market, fixture!("gate_perp_book"), fixture!("gate_perp_trades"));
	}

	#[test]
	fn bitget_perp_fixture() {
		let market = bitget_perp(VENUE, PAIR, "DOTUSDT").unwrap();
		assert_prices(&market, fixture!("bitget_perp_book"), fixture!("bitget_perp_trades"));
	}

	#[test]
	fn coinbase_spot_fixture() {
		let market = coinbase_spot(VENUE, PAIR, "DOT-USD").unwrap();
		assert_prices(&market, fixture!("coinbase_spot_book"), fixture!("coinbase_spot_trades"));
	}

	#[test]
	fn kraken_perp_fixture() {
		let market = kraken_perp(VENUE, PAIR, "PF_DOTUSD").unwrap();
		assert_prices(&market, fixture!("kraken_perp_book"), fixture!("kraken_perp_trades"));
	}

	#[test]
	fn oversized_response_is_rejected() {
		let mut market = binance_spot(VENUE, PAIR, "DOTUSDT").unwrap();
		market.queries[0].request.max_response_bytes = 16;
		let responses = responses(fixture!("binance_spot_book"), fixture!("binance_spot_trades"));
		let rejected = price_market(&market, &settings(), responses, NOW_MS);
		assert_eq!(rejected, Err(ParseError(b"response too large".to_vec())));
	}

	#[test]
	fn contract_size_scales_the_book() {
		// At a thousandth of a unit per contract the recorded book holds a few hundred USDT, too
		// thin for 5000 USDT of impact size.
		let mut market = binance_spot(VENUE, PAIR, "DOTUSDT").unwrap();
		market.contract_size = p("0.001");
		let responses = responses(fixture!("binance_spot_book"), fixture!("binance_spot_trades"));
		let thin = price_market(&market, &settings(), responses, NOW_MS);
		assert_eq!(thin, Err(ParseError(b"BookTooThin".to_vec())));
	}
}
