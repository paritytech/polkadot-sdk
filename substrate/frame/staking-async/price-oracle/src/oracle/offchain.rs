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

//! The offchain worker machinery of the price-oracle pallet.
//!
//! The main pallet stores a [`crate::oracle::Endpoints`] storage item, which is a mapping of
//! asset-id to [`Endpoint`] defined below.
//!
//! The fields in the endpoint are all set upon registering the asset. They define how this endpoint
//! should be queried by the offchain worker:
//!
//! * What is the URL (incl query parameters)
//! * What method should be used.
//! * What fields should go the header
//! * What fields should go the body
//! * What parsing method should be used to extract the price from the response body.
//!
//! ### Selection logic
//!
//! * The validators who are running the collators, which in turn run the offchain worker, select a
//!   random endpoint from the list of available endpoints.
//! * If the endpoint has `requires_api_key` set to `true`, the offchain worker will first try to
//!   fetch the API key from the offchain database. If not present, it will try another one.
//! * Once an eligible endpoint is found, the request is constructed based on the information in the
//!   endpoint.
//! * The response data is parsed using the parsing method defined in the endpoint.
//!
//! ### Manager Binary
//!
//! A `oracle-manager` binary is provided alongside this pallet. It allows for:
//!
//! * read/write on all offchain database entries.
//! * a backup price-submitter binary that can be be ran alongside the offchain worker. Once
//!   enabled, it will first set the kill switch to `true` to disable the wasm offchain-worker, and
//!   use the same session keys to submit the price updates to the chain.
//!
//! ### Offchain Database
//!
//! * The offchain database is a key-value store that is accessible to offchain workers. In this
//!   pallet, it is used to store:
//!
//! * a boolean `kill` switch which, if set, the offchain worker will stop polling for prices.
//! * arbitrary key-value stores that can be used to store API keys.
//!
//! ### Parsing Methods
//!
//! The parsing methods are ultimately hardcoded, and should be one of the few options defined in
//! [`ParsingMethod`]. Each parsing method knows how to extract the price from a specific API
//! response format.

use crate::{ocw_log, oracle};
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_system::{
	offchain::{SendSignedTransaction, Signer},
	pallet_prelude::BlockNumberFor,
};
use scale_info::TypeInfo;
use sp_core::{ConstU32, Get};
use sp_runtime::{
	offchain::{http, storage::StorageValueRef, Duration},
	traits::Zero,
	BoundedVec, FixedU128,
};

/// Abstraction type around the functionality of the offchain worker.
pub struct OracleOffchainWorker<T>(core::marker::PhantomData<T>);

/// Various error types that can occur in the offchain worker.
///
/// These errors cannot be propagated anywhere, and are only used for logging, therefore the need
/// for `#[allow(dead_code)]`.
#[derive(Debug)]
#[allow(dead_code)]
pub enum OffchainError {
	/// The offchain worker doesn't have the right signing keys.
	CannotSign,
	/// The HTTP request timed out.
	TimedOut,
	/// Error from the inner [`sp_runtime::offchain::http::Error`].
	HttpError(sp_runtime::offchain::http::Error),
	/// Error from the inner [`sp_core::offchain::HttpError`].
	CoreHttpError(sp_core::offchain::HttpError),
	/// The status code is not 200.
	UnexpectedStatusCode(u16),
	/// The response data could not be parsed with the given [`ParsingMethod`] rules.
	ParseError(serde_json::Error),
	/// The endpoint URL is not a valid utf8 string.
	InvalidEndpoint,
	/// Other misc. errors.
	Other(&'static str),
}

impl From<&'static str> for OffchainError {
	fn from(e: &'static str) -> Self {
		OffchainError::Other(e)
	}
}

// TODO: hardcoded for now, a bit messy to move to Config.
pub type MaxHeaders = ConstU32<4>;
pub type MaxHeaderNameLength = ConstU32<128>;
pub type MaxEndpointLength = ConstU32<256>;
pub type MaxBodyLength = ConstU32<256>;
pub type MaxRawRequestDataLength = ConstU32<256>;
pub type MaxOffchainDatabaseKeyLength = ConstU32<8>;

/// The endpoint information that is stored onchain in [`crate::oracle::Endpoints`], key-ed by an
/// asset-id.
///
/// It stores fine-grained information about how this endpoint should be queried, allowing the
/// offchain worker to autonomously query it.
///
/// The information that is put into the request (query-params, body, header) could either be
/// hardcoded values ([`RequestData::Raw`]), or fetched from the offchain data-base
/// ([`RequestData::OffchainDatabase`]).
#[derive(
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Debug,
	Clone,
	Eq,
	PartialEq,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct Endpoint {
	/// The URL of the endpoint to query. Should include any query-parameters as well.
	///
	/// Note: we don't support query-parameters that are fetched from the offchain data-base.
	pub url: BoundedVec<u8, MaxEndpointLength>,
	/// The HTTP method to use.
	pub method: Method,
	/// The headers to append to the request. Often used for API-keys.
	pub headers: BoundedVec<Header, MaxHeaders>,
	/// The body of the request.
	pub body: RequestData,
	/// The deadline for the request.
	///
	/// If not provided, the default fetched from
	/// [`crate::oracle::Config::DefaultRequestDeadline`].
	pub deadline: Option<u64>,
	/// Whether this endpoint absolutely requires an API key to be used, or if it can be used with
	/// or without an API key.
	///
	/// If `true`, this API can only be registered if either its `body` or one of the `headers`
	/// contains [`RequestData::OffchainDatabase`]. If `true`, if this endpoint is selected, the
	/// offchain worker will first try to fetch the API key from the offchain database. If not
	/// present, it will try another one.
	///
	/// If `false`, it means that API may be used with or without an API key. Implies that:
	/// * this API key may be registered with any type of (or none) `body` and `headers`
	/// * If selected by the offchain worker, it will be used in any case.
	pub requires_api_key: bool,
	/// Which parsing method should be used to extract the price from the response body.
	pub parsing_method: ParsingMethod,
}


/// Different HTTP methods.
#[derive(
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Debug,
	Clone,
	Copy,
	Eq,
	PartialEq,
	Default,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum Method {
	/// GET request.
	#[default]
	Get,
	/// POST request.
	Post,
}

impl Into<http::Method> for Method {
	fn into(self) -> sp_runtime::offchain::http::Method {
		match self {
			Self::Get => http::Method::Get,
			Self::Post => http::Method::Post,
		}
	}
}

/// Different endpoint parsing methods.
#[derive(
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Debug,
	Clone,
	Eq,
	PartialEq,
	Default,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum ParsingMethod {
	/// CryptoCompare API (free tier).
	///
	/// Example: https://min-api.cryptocompare.com/data/price?fsym=DOT&tsyms=USD
	///
	/// Response format: `{"USD": 1.702}`
	#[default]
	CryptoCompareFree,
	/// Binance API (free tier).
	///
	/// Example: https://data-api.binance.vision/api/v3/ticker/price?symbol=DOTUSDT
	///
	/// Response format: `{"symbol": "DOTUSDT", "price": "1.70600000"}`
	BinanceFree,
	/// CoinLore API (free tier).
	///
	/// Example: https://api.coinlore.net/api/ticker/?id=45219
	///
	/// Response format: `[{"id": "45219", ..., "price_usd": "1.70", ...}]`
	CoinLoreFree,
}


/// Some data that can be added to the request.
#[derive(
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Debug,
	Clone,
	Eq,
	PartialEq,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub enum RequestData {
	/// A raw hardcoded value.
	Raw(BoundedVec<u8, MaxRawRequestDataLength>),
	/// A reference to an offchain database key.
	OffchainDatabase(BoundedVec<u8, MaxOffchainDatabaseKeyLength>),
}

impl Default for RequestData {
	fn default() -> Self {
		RequestData::Raw(BoundedVec::default())
	}
}

/// The header information attached to the request.
#[derive(
	TypeInfo,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Debug,
	Clone,
	Eq,
	PartialEq,
	MaxEncodedLen,
	serde::Serialize,
	serde::Deserialize,
)]
pub struct Header {
	/// Header name.
	pub name: BoundedVec<u8, MaxHeaderNameLength>,
	/// Header value.
	pub value: RequestData,
}

impl<T: crate::oracle::Config> OracleOffchainWorker<T> {
	/// Validate that an [`Endpoint`] is valid.
	///
	/// Checks that:
	///
	/// * The `url` is valid UTF-8.
	/// * If `requires_api_key` is `true`, the `body` or one of the `headers` must contain
	///   [`RequestData::OffchainDatabase`].
	pub fn validate_endpoint(endpoint: &Endpoint) -> Result<(), OffchainError> {
		// Check URL is valid UTF-8.
		core::str::from_utf8(&endpoint.url).map_err(|_| OffchainError::InvalidEndpoint)?;

		// If API key is required, ensure at least one offchain database reference exists.
		if endpoint.requires_api_key {
			let has_offchain_key = matches!(endpoint.body, RequestData::OffchainDatabase(_)) ||
				endpoint
					.headers
					.iter()
					.any(|header| matches!(header.value, RequestData::OffchainDatabase(_)));

			if !has_offchain_key {
				return Err(OffchainError::Other(
					"requires_api_key is true but no OffchainDatabase reference found",
				));
			}
		}

		Ok(())
	}

	/// Returns a list of all offchain database keys that an endpoint requires.
	///
	/// This is used to check if all required keys are available before attempting to use an
	/// endpoint.
	fn required_keys(endpoint: &Endpoint) -> Vec<Vec<u8>> {
		let mut keys = Vec::new();

		if let RequestData::OffchainDatabase(ref key) = endpoint.body {
			keys.push(key.to_vec());
		}

		for header in endpoint.headers.iter() {
			if let RequestData::OffchainDatabase(ref key) = header.value {
				keys.push(key.to_vec());
			}
		}

		keys
	}

	/// Check if an endpoint's requirements are met.
	///
	/// Returns `true` if:
	/// * The endpoint does not require an API key (`requires_api_key == false`), OR
	/// * All required offchain database keys are available.
	fn check_endpoint_requirements(endpoint: &Endpoint) -> bool {
		if !endpoint.requires_api_key {
			return true;
		}

		let required_keys = Self::required_keys(endpoint);
		required_keys.iter().all(|key| {
			let storage = StorageValueRef::persistent(key);
			storage.get::<Vec<u8>>().ok().flatten().is_some()
		})
	}

	/// Fetch the response body from an endpoint.
	///
	/// This method sends the HTTP request and returns the raw response body bytes.
	fn fetch_endpoint(endpoint: &Endpoint) -> Result<Vec<u8>, OffchainError> {
		// Helper to resolve RequestData to actual bytes.
		let resolve_data = |data: &RequestData| -> Result<Vec<u8>, OffchainError> {
			match data {
				RequestData::Raw(bytes) => Ok(bytes.to_vec()),
				RequestData::OffchainDatabase(key) => {
					let storage = StorageValueRef::persistent(key);
					storage
						.get::<Vec<u8>>()
						.ok()
						.flatten()
						.ok_or(OffchainError::Other("offchain database key not found"))
				},
			}
		};

		let deadline = sp_io::offchain::timestamp()
			.add(Duration::from_millis(endpoint.deadline.unwrap_or(T::DefaultRequestDeadline::get())));
		let url =
			core::str::from_utf8(&endpoint.url).map_err(|_| OffchainError::InvalidEndpoint)?;

		// Resolve body data.
		let body_bytes = resolve_data(&endpoint.body)?;

		// Start building the request.
		let mut request =
			http::Request::new(url).method(endpoint.method.into()).deadline(deadline);

		// Add headers, resolving any offchain database references.
		for Header { name, value } in endpoint.headers.iter() {
			let name_str =
				core::str::from_utf8(name).map_err(|_| OffchainError::InvalidEndpoint)?;
			let value_bytes = resolve_data(value)?;
			let value_str =
				core::str::from_utf8(&value_bytes).map_err(|_| OffchainError::InvalidEndpoint)?;
			request = request.add_header(name_str, value_str);
		}

		// Send the request. Handle body if present.
		if !body_bytes.is_empty() {
			request = request.body([body_bytes]);
		}

		let pending = request.send().map_err(OffchainError::CoreHttpError)?;

		let response = pending
			.try_wait(deadline)
			.map_err(|_pending_request| OffchainError::TimedOut)?
			.map_err(OffchainError::HttpError)?;

		if response.code != 200 {
			return Err(OffchainError::UnexpectedStatusCode(response.code));
		}

		let body = response.body().collect::<Vec<u8>>();
		Ok(body)
	}

	/// Parse the response body bytes according to the given parsing method.
	///
	/// Returns the price as a [`FixedU128`] value.
	fn parse_response(method: &ParsingMethod, body: Vec<u8>) -> Result<FixedU128, OffchainError> {
		ocw_log!(debug, "parsing body: {:?}", body);

		let v: serde_json::Value =
			serde_json::from_slice(&body).map_err(|e| OffchainError::ParseError(e))?;

		match method {
			ParsingMethod::CryptoCompareFree => {
				// Expected format: {"USD": 1.702}
				match v {
					serde_json::Value::Object(obj) if obj.contains_key("USD") => {
						use alloc::string::ToString;
						let price_str = obj["USD"]
							.as_number()
							.map(|n| n.to_string())
							.ok_or("failed to parse USD field")?;
						ocw_log!(debug, "CryptoCompareFree price_str: {:?}", price_str);
						let price =
							FixedU128::from_float_str(&price_str).map_err(OffchainError::Other)?;
						Ok(price)
					},
					_ => Err(OffchainError::Other("invalid CryptoCompareFree response format")),
				}
			},
			ParsingMethod::BinanceFree => {
				// Expected format: {"symbol": "DOTUSDT", "price": "1.70600000"}
				match v {
					serde_json::Value::Object(obj) if obj.contains_key("price") => {
						let price_str =
							obj["price"].as_str().ok_or("failed to parse price field as string")?;
						ocw_log!(debug, "BinanceFree price_str: {:?}", price_str);
						let price =
							FixedU128::from_float_str(price_str).map_err(OffchainError::Other)?;
						Ok(price)
					},
					_ => Err(OffchainError::Other("invalid BinanceFree response format")),
				}
			},
			ParsingMethod::CoinLoreFree => {
				// Expected format: [{"id": "45219", ..., "price_usd": "1.70", ...}]
				match v {
					serde_json::Value::Array(arr) if !arr.is_empty() => {
						if let serde_json::Value::Object(obj) = &arr[0] {
							if obj.contains_key("price_usd") {
								let price_str = obj["price_usd"]
									.as_str()
									.ok_or("failed to parse price_usd field as string")?;
								ocw_log!(debug, "CoinLoreFree price_str: {:?}", price_str);
								let price = FixedU128::from_float_str(price_str)
									.map_err(OffchainError::Other)?;
								return Ok(price);
							}
						}
						Err(OffchainError::Other("invalid CoinLoreFree response format"))
					},
					_ => Err(OffchainError::Other("invalid CoinLoreFree response format")),
				}
			},
		}
	}

	pub(crate) fn offchain_worker(
		local_block_number: BlockNumberFor<T>,
	) -> Result<u32, OffchainError> {
		// Only run at the specified interval.
		if local_block_number % T::PriceUpdateInterval::get() != Zero::zero() {
			return Ok(0);
		}

		ocw_log!(debug, "Offchain worker starting at #{:?}", local_block_number);

		// Setup signer.
		let signer = Signer::<T, T::AuthorityId>::all_accounts();
		if !signer.can_sign() {
			ocw_log!(error, "cannot sign!");
			return Err(OffchainError::CannotSign);
		}

		let mut assets_updated = 0;

		// Iterate over all tracked assets and their endpoints.
		for (asset_id, endpoints) in oracle::StorageManager::<T>::tracked_assets_with_endpoints() {
			ocw_log!(debug, "Processing asset {:?} with {} endpoints", asset_id, endpoints.len());

			// Filter endpoints to only those that meet requirements.
			let eligible_endpoints: Vec<&Endpoint> =
				endpoints.iter().filter(|e| Self::check_endpoint_requirements(e)).collect();

			if eligible_endpoints.is_empty() {
				ocw_log!(
					warn,
					"No eligible endpoints for asset {:?} (all require unavailable API keys)",
					asset_id
				);
				continue;
			}

			// Randomly select one endpoint from the eligible set.
			let random_u8 = sp_io::offchain::random_seed()[0];
			let selected_endpoint =
				eligible_endpoints[random_u8 as usize % eligible_endpoints.len()];

			ocw_log!(
				debug,
				"Selected endpoint for asset {:?}: {:?}",
				asset_id,
				core::str::from_utf8(&selected_endpoint.url).unwrap_or("<invalid utf8>")
			);

			// Fetch the response body.
			let body = match Self::fetch_endpoint(selected_endpoint) {
				Ok(body) => body,
				Err(e) => {
					ocw_log!(error, "Failed to fetch price for asset {:?}: {:?}", asset_id, e);
					continue;
				},
			};

			// Parse the response body.
			let price = match Self::parse_response(&selected_endpoint.parsing_method, body) {
				Ok(price) => price,
				Err(e) => {
					ocw_log!(error, "Failed to parse price for asset {:?}: {:?}", asset_id, e);
					continue;
				},
			};

			ocw_log!(debug, "Fetched price: {:?} for asset {:?}", price, asset_id);

			// Submit a vote transaction.
			let call =
				crate::oracle::Call::<T>::vote { asset_id, price, produced_in: local_block_number };

			// TODO: handle
			let _res = signer.send_signed_transaction(|_account| call.clone());
			ocw_log!(info, "Submitted vote for asset {:?}", asset_id);

			assets_updated += 1;
		}

		ocw_log!(info, "Offchain worker completed, updated {} assets", assets_updated);
		Ok(assets_updated)
	}
}

#[cfg(test)]
mod unit_tests {
	use super::*;
	use crate::oracle::mock::Runtime;

	type Worker = OracleOffchainWorker<Runtime>;

	// -- Parsing tests --

	#[test]
	fn crypto_compare_free_parsing() {
		// Valid response - can parse USD field.
		let body = br#"{"USD":1.702}"#.to_vec();
		assert!(Worker::parse_response(&ParsingMethod::CryptoCompareFree, body).is_ok());

		// Missing USD key.
		let body = br#"{"EUR":1.5}"#.to_vec();
		assert!(Worker::parse_response(&ParsingMethod::CryptoCompareFree, body).is_err());

		// USD is not a number.
		let body = br#"{"USD":"not a number"}"#.to_vec();
		assert!(Worker::parse_response(&ParsingMethod::CryptoCompareFree, body).is_err());
	}

	#[test]
	fn binance_free_parsing() {
		// Valid response - can parse price field.
		let body = br#"{"symbol":"DOTUSDT","price":"1.70600000"}"#.to_vec();
		assert!(Worker::parse_response(&ParsingMethod::BinanceFree, body).is_ok());

		// Missing price key.
		let body = br#"{"symbol":"DOTUSDT"}"#.to_vec();
		assert!(Worker::parse_response(&ParsingMethod::BinanceFree, body).is_err());

		// Price is not a valid number string.
		let body = br#"{"symbol":"DOTUSDT","price":"invalid"}"#.to_vec();
		assert!(Worker::parse_response(&ParsingMethod::BinanceFree, body).is_err());
	}

	#[test]
	fn coin_lore_free_parsing() {
		// Valid response - can parse price_usd field.
		let body = br#"[{"id":"45219","price_usd":"1.70"}]"#.to_vec();
		assert!(Worker::parse_response(&ParsingMethod::CoinLoreFree, body).is_ok());

		// Missing price_usd key.
		let body = br#"[{"id":"45219","symbol":"DOT"}]"#.to_vec();
		assert!(Worker::parse_response(&ParsingMethod::CoinLoreFree, body).is_err());

		// price_usd is not a valid number string.
		let body = br#"[{"id":"45219","price_usd":"invalid"}]"#.to_vec();
		assert!(Worker::parse_response(&ParsingMethod::CoinLoreFree, body).is_err());
	}

	// -- Endpoint validation tests --

	#[test]
	fn validate_endpoint_accepts_valid_endpoint() {
		let endpoint = Endpoint {
			url: b"https://api.example.com/price".to_vec().try_into().unwrap(),
			method: Method::Get,
			headers: Default::default(),
			body: RequestData::default(),
			deadline: None,
			requires_api_key: false,
			parsing_method: ParsingMethod::CryptoCompareFree,
		};
		assert!(Worker::validate_endpoint(&endpoint).is_ok());
	}

	#[test]
	fn validate_endpoint_rejects_invalid_utf8_url() {
		let endpoint = Endpoint {
			url: vec![0xff, 0xfe].try_into().unwrap(),
			method: Method::Get,
			headers: Default::default(),
			body: RequestData::default(),
			deadline: None,
			requires_api_key: false,
			parsing_method: ParsingMethod::CryptoCompareFree,
		};
		assert!(Worker::validate_endpoint(&endpoint).is_err());
	}

	#[test]
	fn validate_endpoint_requires_offchain_key_when_api_key_required() {
		// requires_api_key=true but no OffchainDatabase reference -> should fail.
		let endpoint = Endpoint {
			url: b"https://api.example.com/price".to_vec().try_into().unwrap(),
			method: Method::Get,
			headers: Default::default(),
			body: RequestData::Raw(Default::default()),
			deadline: None,
			requires_api_key: true,
			parsing_method: ParsingMethod::CryptoCompareFree,
		};
		assert!(Worker::validate_endpoint(&endpoint).is_err());

		// requires_api_key=true with OffchainDatabase in body -> should pass.
		let endpoint = Endpoint {
			url: b"https://api.example.com/price".to_vec().try_into().unwrap(),
			method: Method::Get,
			headers: Default::default(),
			body: RequestData::OffchainDatabase(b"api_key".to_vec().try_into().unwrap()),
			deadline: None,
			requires_api_key: true,
			parsing_method: ParsingMethod::CryptoCompareFree,
		};
		assert!(Worker::validate_endpoint(&endpoint).is_ok());

		// requires_api_key=true with OffchainDatabase in header -> should pass.
		let endpoint = Endpoint {
			url: b"https://api.example.com/price".to_vec().try_into().unwrap(),
			method: Method::Get,
			headers: vec![Header {
				name: b"Authorization".to_vec().try_into().unwrap(),
				value: RequestData::OffchainDatabase(b"api_key".to_vec().try_into().unwrap()),
			}]
			.try_into()
			.unwrap(),
			body: RequestData::default(),
			deadline: None,
			requires_api_key: true,
			parsing_method: ParsingMethod::CryptoCompareFree,
		};
		assert!(Worker::validate_endpoint(&endpoint).is_ok());
	}
}

#[cfg(test)]
mod ocw_with_localhost_tests {

}
