use log::warn;
use sp_runtime::FixedU128;
use std::time::Duration;

const LOG_TARGET: &str = "price-oracle::fetcher";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

const BINANCE_URL: &str = "https://data-api.binance.vision/api/v3/ticker/price?symbol=DOTUSDT";
const COINLORE_URL: &str = "https://api.coinlore.net/api/ticker/?id=45219";
const CRYPTOCOMPARE_URL: &str = "https://min-api.cryptocompare.com/data/price?fsym=DOT&tsyms=USD";

pub struct PriceFetcher {
	client: reqwest::Client,
}

impl PriceFetcher {
	pub fn new() -> Self {
		let client = reqwest::Client::builder()
			.timeout(REQUEST_TIMEOUT)
			.build()
			.expect("Failed to build HTTP client; qed");
		Self { client }
	}

	pub async fn fetch_dot_usd_price(&self) -> Result<FixedU128, String> {
		match self.fetch_and_parse(BINANCE_URL, parse_binance).await {
			Ok(price) => return Ok(price),
			Err(e) => warn!(target: LOG_TARGET, "Binance failed: {}, trying CoinLore", e),
		}

		match self.fetch_and_parse(COINLORE_URL, parse_coinlore).await {
			Ok(price) => return Ok(price),
			Err(e) => warn!(target: LOG_TARGET, "CoinLore failed: {}, trying CryptoCompare", e),
		}

		self.fetch_and_parse(CRYPTOCOMPARE_URL, parse_cryptocompare).await
	}

	async fn fetch_and_parse(
		&self,
		url: &str,
		parser: fn(&[u8]) -> Result<FixedU128, String>,
	) -> Result<FixedU128, String> {
		let bytes = self
			.client
			.get(url)
			.send()
			.await
			.map_err(|e| format!("Request to {} failed: {}", url, e))?
			.bytes()
			.await
			.map_err(|e| format!("Reading body from {} failed: {}", url, e))?;

		parser(&bytes)
	}
}

/// `{"symbol":"DOTUSDT","price":"4.20600000"}`
fn parse_binance(body: &[u8]) -> Result<FixedU128, String> {
	let v: serde_json::Value =
		serde_json::from_slice(body).map_err(|e| format!("JSON parse: {}", e))?;
	let price_str = v.get("price").and_then(|v| v.as_str()).ok_or("missing 'price' field")?;
	parse_price_string(price_str)
}

/// `[{"id":"45219", ..., "price_usd":"4.20", ...}]`
fn parse_coinlore(body: &[u8]) -> Result<FixedU128, String> {
	let v: serde_json::Value =
		serde_json::from_slice(body).map_err(|e| format!("JSON parse: {}", e))?;
	let price_str = v
		.as_array()
		.and_then(|arr| arr.first())
		.and_then(|obj| obj.get("price_usd"))
		.and_then(|v| v.as_str())
		.ok_or("missing 'price_usd' in CoinLore response")?;
	parse_price_string(price_str)
}

/// `{"USD":4.202}`
fn parse_cryptocompare(body: &[u8]) -> Result<FixedU128, String> {
	let v: serde_json::Value =
		serde_json::from_slice(body).map_err(|e| format!("JSON parse: {}", e))?;
	let price_num = v
		.get("USD")
		.and_then(|v| v.as_f64())
		.ok_or("missing 'USD' field in CryptoCompare response")?;
	if price_num < 0.0 {
		return Err("Negative price".into());
	}
	Ok(FixedU128::from_float(price_num))
}

fn parse_price_string(s: &str) -> Result<FixedU128, String> {
	let price: f64 = s.parse().map_err(|e| format!("Price parse error: {}", e))?;
	if price < 0.0 {
		return Err("Negative price".into());
	}
	Ok(FixedU128::from_float(price))
}

#[cfg(test)]
mod parsing_tests {
	use super::*;

	#[test]
	fn binance_parsing() {
		let body = br#"{"symbol":"DOTUSDT","price":"4.20600000"}"#;
		let price = parse_binance(body).unwrap();
		assert!(price > FixedU128::from_u32(4) && price < FixedU128::from_u32(5));

		assert!(parse_binance(br#"{"symbol":"DOTUSDT"}"#).is_err());
		assert!(parse_binance(br#"{"price":123}"#).is_err()); // price not a string
	}

	#[test]
	fn coinlore_parsing() {
		let body = br#"[{"id":"45219","price_usd":"4.20"}]"#;
		let price = parse_coinlore(body).unwrap();
		assert!(price > FixedU128::from_u32(4) && price < FixedU128::from_u32(5));

		assert!(parse_coinlore(br#"[{"id":"45219"}]"#).is_err());
		assert!(parse_coinlore(br#"[]"#).is_err());
	}

	#[test]
	fn cryptocompare_parsing() {
		let body = br#"{"USD":4.202}"#;
		let price = parse_cryptocompare(body).unwrap();
		assert!(price > FixedU128::from_u32(4) && price < FixedU128::from_u32(5));

		assert!(parse_cryptocompare(br#"{"EUR":1.5}"#).is_err());
	}

	#[test]
	fn parse_price_string_works() {
		let price = parse_price_string("5.23").unwrap();
		let expected = FixedU128::from_rational(523, 100);
		assert_eq!(price, expected);
	}

	#[test]
	fn parse_price_string_rejects_negative() {
		assert!(parse_price_string("-1.5").is_err());
	}
}

/// Live tests — run with:
/// ```bash
/// cargo test -p polkadot-node-price-oracle --features live-test live_
/// ```
#[cfg(all(test, feature = "live-test"))]
mod live_tests {
	use super::*;
	use std::process::Command;

	fn curl_get(url: &str) -> Vec<u8> {
		let output = Command::new("curl")
			.args(["-s", "--fail", "--max-time", "15", url])
			.output()
			.expect("curl not installed");
		assert!(
			output.status.success(),
			"curl to {url} failed: {}",
			String::from_utf8_lossy(&output.stderr)
		);
		output.stdout
	}

	fn assert_plausible_dot_price(price: FixedU128, source: &str) {
		assert!(
			price > FixedU128::from_rational(1, 100) && price < FixedU128::from_rational(10_000, 1),
			"{source} returned implausible DOT price: {price:?}"
		);
	}

	#[test]
	fn live_binance() {
		let body = curl_get(BINANCE_URL);
		let price = parse_binance(&body).expect("Binance response format changed");
		assert_plausible_dot_price(price, "Binance");
	}

	#[test]
	fn live_coinlore() {
		let body = curl_get(COINLORE_URL);
		let price = parse_coinlore(&body).expect("CoinLore response format changed");
		assert_plausible_dot_price(price, "CoinLore");
	}

	#[test]
	fn live_cryptocompare() {
		let body = curl_get(CRYPTOCOMPARE_URL);
		let price = parse_cryptocompare(&body).expect("CryptoCompare response format changed");
		assert_plausible_dot_price(price, "CryptoCompare");
	}
}
