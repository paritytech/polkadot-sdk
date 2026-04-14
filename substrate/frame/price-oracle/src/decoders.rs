//! Price feed endpoint decoders.
//!
//! Each public function takes raw HTTP response bytes from a specific price API
//! and extracts the DOT/USD price as `FixedU128`. These are `no_std`-compatible
//! (using `alloc` + `serde_json` with the `alloc` feature).

extern crate alloc;

use sp_runtime::FixedU128;

/// Parse a price string (e.g. "4.206") into `FixedU128`.
///
/// Works in `no_std` by parsing the decimal string directly instead of going
/// through `f64` and `FixedU128::from_float` (which requires `std`).
pub fn parse_price_string(s: &str) -> Option<FixedU128> {
	let s = s.trim();
	if s.is_empty() || s.starts_with('-') {
		return None;
	}

	let (integer_str, frac_str) = match s.find('.') {
		Some(dot) => (&s[..dot], &s[dot + 1..]),
		None => (s, ""),
	};

	let integer_part: u128 = integer_str.parse().ok()?;
	let (frac_part, frac_digits) = if frac_str.is_empty() {
		(0u128, 0u32)
	} else {
		(frac_str.parse::<u128>().ok()?, frac_str.len() as u32)
	};

	// FixedU128 has 18 decimal places of precision (DIV = 10^18).
	let divisor = 10u128.checked_pow(frac_digits)?;
	Some(FixedU128::from_rational(integer_part * divisor + frac_part, divisor))
}

/// Parse a float value into `FixedU128`, rejecting negatives.
///
/// Uses string formatting to avoid `FixedU128::from_float` (which requires `std`).
fn parse_price_float(n: f64) -> Option<FixedU128> {
	if n < 0.0 || n.is_nan() || n.is_infinite() {
		return None;
	}
	// Format with enough precision and reuse the string parser.
	let s = alloc::format!("{:.18}", n);
	parse_price_string(&s)
}

/// Binance: `{"symbol":"DOTUSDT","price":"4.20600000"}`
pub fn decode_binance(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("price")?.as_str()?;
	parse_price_string(s)
}

/// CoinLore: `[{"id":"45219", ..., "price_usd":"4.20", ...}]`
pub fn decode_coinlore(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.as_array()?.first()?.get("price_usd")?.as_str()?;
	parse_price_string(s)
}

/// CryptoCompare: `{"USD":4.202}`
pub fn decode_cryptocompare(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("USD")?.as_f64()?;
	parse_price_float(n)
}

/// CoinGecko: `{"polkadot":{"usd":4.20}}`
pub fn decode_coingecko(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("polkadot")?.get("usd")?.as_f64()?;
	parse_price_float(n)
}

/// CoinMarketCap: `{"data":[{"quote":[{"price":4.20}]}]}`
pub fn decode_coinmarketcap(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("data")?.as_array()?.first()?.get("quote")?.as_array()?.first()?.get("price")?.as_f64()?;
	parse_price_float(n)
}

/// CoinPaprika: `{"quotes":{"USD":{"price":4.20}}}`
pub fn decode_coinpaprika(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("quotes")?.get("USD")?.get("price")?.as_f64()?;
	parse_price_float(n)
}

/// LiveCoinWatch: `{"rate":4.20}`
pub fn decode_livecoinwatch(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("rate")?.as_f64()?;
	parse_price_float(n)
}

/// Dia: `{"Price":4.20}`
pub fn decode_dia(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let n = v.get("Price")?.as_f64()?;
	parse_price_float(n)
}

/// Coinbase: `{"data":{"amount":"4.20"}}`
pub fn decode_coinbase(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("data")?.get("amount")?.as_str()?;
	parse_price_string(s)
}

/// Kraken: `{"result":{"DOTUSD":{"c":["4.20"]}}}`
pub fn decode_kraken(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("result")?.get("DOTUSD")?.get("c")?.as_array()?.first()?.as_str()?;
	parse_price_string(s)
}

/// OKX: `{"data":[{"last":"4.20"}]}`
pub fn decode_okx(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("data")?.as_array()?.first()?.get("last")?.as_str()?;
	parse_price_string(s)
}

/// Bybit: `{"result":{"list":[{"lastPrice":"4.20"}]}}`
pub fn decode_bybit(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v
		.get("result")?
		.get("list")?
		.as_array()?
		.first()?
		.get("lastPrice")?
		.as_str()?;
	parse_price_string(s)
}

/// KuCoin: `{"data":{"price":"4.20"}}`
pub fn decode_kucoin(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("data")?.get("price")?.as_str()?;
	parse_price_string(s)
}

/// Crypto.com: `{"result":{"data":[{"a":"4.20"}]}}`
pub fn decode_cryptocom(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.get("result")?.get("data")?.as_array()?.first()?.get("a")?.as_str()?;
	parse_price_string(s)
}

/// Gate.io: `[{"last":"4.20"}]`
pub fn decode_gateio(body: &[u8]) -> Option<FixedU128> {
	let v: serde_json::Value = serde_json::from_slice(body).ok()?;
	let s = v.as_array()?.first()?.get("last")?.as_str()?;
	parse_price_string(s)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn assert_plausible(price: FixedU128) {
		assert!(
			price > FixedU128::from_u32(0) && price < FixedU128::from_u32(10_000),
			"Implausible price: {:?}",
			price
		);
	}

	#[test]
	fn binance() {
		let body = br#"{"symbol":"DOTUSDT","price":"4.20600000"}"#;
		let price = decode_binance(body).unwrap();
		assert_plausible(price);

		assert!(decode_binance(br#"{"symbol":"DOTUSDT"}"#).is_none());
		assert!(decode_binance(br#"{"price":123}"#).is_none()); // not a string
	}

	#[test]
	fn coinlore() {
		let body = br#"[{"id":"45219","price_usd":"4.20"}]"#;
		let price = decode_coinlore(body).unwrap();
		assert_plausible(price);

		assert!(decode_coinlore(br#"[{"id":"45219"}]"#).is_none());
		assert!(decode_coinlore(br#"[]"#).is_none());
	}

	#[test]
	fn cryptocompare() {
		let body = br#"{"USD":4.202}"#;
		let price = decode_cryptocompare(body).unwrap();
		assert_plausible(price);

		assert!(decode_cryptocompare(br#"{"EUR":1.5}"#).is_none());
	}

	#[test]
	fn coingecko() {
		let body = br#"{"polkadot":{"usd":4.20}}"#;
		let price = decode_coingecko(body).unwrap();
		assert_plausible(price);

		assert!(decode_coingecko(br#"{"polkadot":{}}"#).is_none());
	}

	#[test]
	fn coinmarketcap() {
		let body = br#"{"data":[{"quote":[{"price":4.20}]}]}"#;
		let price = decode_coinmarketcap(body).unwrap();
		assert_plausible(price);

		assert!(decode_coinmarketcap(br#"{"data":[]}"#).is_none());
	}

	#[test]
	fn coinpaprika() {
		let body = br#"{"quotes":{"USD":{"price":4.20}}}"#;
		let price = decode_coinpaprika(body).unwrap();
		assert_plausible(price);

		assert!(decode_coinpaprika(br#"{"quotes":{}}"#).is_none());
	}

	#[test]
	fn livecoinwatch() {
		let body = br#"{"rate":4.20}"#;
		let price = decode_livecoinwatch(body).unwrap();
		assert_plausible(price);

		assert!(decode_livecoinwatch(br#"{"notrate":4.20}"#).is_none());
	}

	#[test]
	fn dia() {
		let body = br#"{"Price":4.20}"#;
		let price = decode_dia(body).unwrap();
		assert_plausible(price);

		assert!(decode_dia(br#"{"price":4.20}"#).is_none()); // case sensitive
	}

	#[test]
	fn coinbase() {
		let body = br#"{"data":{"amount":"4.20"}}"#;
		let price = decode_coinbase(body).unwrap();
		assert_plausible(price);

		assert!(decode_coinbase(br#"{"data":{}}"#).is_none());
	}

	#[test]
	fn kraken() {
		let body = br#"{"result":{"DOTUSD":{"c":["4.20","lot"]}}}"#;
		let price = decode_kraken(body).unwrap();
		assert_plausible(price);

		assert!(decode_kraken(br#"{"result":{"DOTUSD":{"c":[]}}}"#).is_none());
	}

	#[test]
	fn okx() {
		let body = br#"{"data":[{"last":"4.20"}]}"#;
		let price = decode_okx(body).unwrap();
		assert_plausible(price);

		assert!(decode_okx(br#"{"data":[]}"#).is_none());
	}

	#[test]
	fn bybit() {
		let body = br#"{"result":{"list":[{"lastPrice":"4.20"}]}}"#;
		let price = decode_bybit(body).unwrap();
		assert_plausible(price);

		assert!(decode_bybit(br#"{"result":{"list":[]}}"#).is_none());
	}

	#[test]
	fn kucoin() {
		let body = br#"{"data":{"price":"4.20"}}"#;
		let price = decode_kucoin(body).unwrap();
		assert_plausible(price);

		assert!(decode_kucoin(br#"{"data":{}}"#).is_none());
	}

	#[test]
	fn cryptocom() {
		let body = br#"{"result":{"data":[{"a":"4.20"}]}}"#;
		let price = decode_cryptocom(body).unwrap();
		assert_plausible(price);

		assert!(decode_cryptocom(br#"{"result":{"data":[]}}"#).is_none());
	}

	#[test]
	fn gateio() {
		let body = br#"[{"last":"4.20"}]"#;
		let price = decode_gateio(body).unwrap();
		assert_plausible(price);

		assert!(decode_gateio(br#"[]"#).is_none());
	}

	#[test]
	fn parse_price_string_works() {
		let price = parse_price_string("5.23").unwrap();
		let expected = FixedU128::from_rational(523, 100);
		assert_eq!(price, expected);
	}

	#[test]
	fn parse_price_string_rejects_negative() {
		assert!(parse_price_string("-1.5").is_none());
	}

	#[test]
	fn parse_price_string_rejects_invalid() {
		assert!(parse_price_string("abc").is_none());
		assert!(parse_price_string("").is_none());
	}
}
