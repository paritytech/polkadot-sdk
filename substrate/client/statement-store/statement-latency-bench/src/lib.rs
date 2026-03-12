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

//! Shared types, helpers, subxt config, and extension definitions used by both the
//! `setup-allowances` and `statement-latency-bench` binaries.

use anyhow::{anyhow, Context};
use codec::Encode;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use log::debug;
use serde::{Deserialize, Serialize};
use sp_core::{blake2_256, sr25519, Pair};
use std::{any::Any, sync::Arc};
use subxt::{
	config::{
		transaction_extensions::{
			AnyOf, ChargeAssetTxPayment, ChargeTransactionPayment, CheckGenesis, CheckMetadataHash,
			CheckMortality, CheckNonce, CheckSpecVersion, CheckTxVersion, TransactionExtension,
			VerifySignatureDetails,
		},
		Config, ExtrinsicParams, ExtrinsicParamsEncoder,
	},
	utils::Static,
	PolkadotConfig,
};

pub struct VerifyMultiSignature<T: Config>(VerifySignatureDetails<T>);

impl<T: Config> ExtrinsicParams<T> for VerifyMultiSignature<T> {
	type Params = ();

	fn new(
		_client: &subxt::client::ClientState<T>,
		_params: Self::Params,
	) -> Result<Self, subxt::config::ExtrinsicParamsError> {
		Ok(VerifyMultiSignature(VerifySignatureDetails::Disabled))
	}
}

impl<T: Config> ExtrinsicParamsEncoder for VerifyMultiSignature<T> {
	fn encode_value_to(&self, v: &mut Vec<u8>) {
		self.0.encode_to(v);
	}

	fn inject_signature(&mut self, account: &dyn Any, signature: &dyn Any) {
		let account = account
			.downcast_ref::<T::AccountId>()
			.expect("A T::AccountId should have been provided")
			.clone();
		let signature = signature
			.downcast_ref::<T::Signature>()
			.expect("A T::Signature should have been provided")
			.clone();
		self.0 = VerifySignatureDetails::Signed { signature, account };
	}
}

impl<T: Config> TransactionExtension<T> for VerifyMultiSignature<T> {
	type Decoded = Static<VerifySignatureDetails<T>>;

	fn matches(identifier: &str, _type_id: u32, _types: &::scale_info::PortableRegistry) -> bool {
		identifier == "VerifyMultiSignature" || identifier == "VerifySignature"
	}
}

/// Macro to define named skip handlers for custom non-empty transaction extensions
///
/// Each generated struct matches by its identifier name via `stringify!($name)` and encodes as
/// `0x00` (first-variant enum / `None`). Invoke with actual extension names when targeting
/// runtimes with custom non-empty extensions
macro_rules! define_skip_unknown_extensions {
	($($name:ident),+ $(,)?) => { $(
		pub struct $name;

		impl<T: Config> ExtrinsicParams<T> for $name {
			type Params = ();

			fn new(
				_client: &subxt::client::ClientState<T>,
				_params: Self::Params,
			) -> Result<Self, subxt::config::ExtrinsicParamsError> {
				Ok($name)
			}
		}

		impl ExtrinsicParamsEncoder for $name {
			fn encode_value_to(&self, v: &mut Vec<u8>) {
				v.push(0x00);
			}
		}

		impl<T: Config> TransactionExtension<T> for $name {
			type Decoded = Static<u8>;

			fn matches(
				identifier: &str,
				_type_id: u32,
				_types: &::scale_info::PortableRegistry,
			) -> bool {
				identifier == stringify!($name)
			}
		}
	)+ };
}

// Skip handlers for custom non-empty extensions in the people-westend runtime
// Zero-sized extensions (e.g. ProvideForVoucherClaimer) are auto-skipped by AnyOf
define_skip_unknown_extensions!(
	AsPerson,
	AsProofOfInkParticipant,
	ScoreAsParticipant,
	GameAsInvited,
	PeopleLiteAuth,
	AsCoinage,
	RestrictOrigins, // encodes as a bool false (0x00) disables it
);

pub type CustomExtrinsicParams<T> = AnyOf<
	T,
	(
		VerifyMultiSignature<T>,
		CheckSpecVersion,
		CheckTxVersion,
		CheckNonce,
		CheckGenesis<T>,
		CheckMortality<T>,
		ChargeAssetTxPayment<T>,
		ChargeTransactionPayment,
		CheckMetadataHash,
		AsPerson,
		AsProofOfInkParticipant,
		ScoreAsParticipant,
		GameAsInvited,
		PeopleLiteAuth,
		AsCoinage,
		RestrictOrigins,
	),
>;

/// Custom subxt [`Config`] identical to [`PolkadotConfig`] but using [`CustomExtrinsicParams`]
#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum CustomConfig {}

impl Config for CustomConfig {
	type AccountId = <PolkadotConfig as Config>::AccountId;
	type Address = <PolkadotConfig as Config>::Address;
	type Signature = <PolkadotConfig as Config>::Signature;
	type Hasher = <PolkadotConfig as Config>::Hasher;
	type Header = <PolkadotConfig as Config>::Header;
	type ExtrinsicParams = CustomExtrinsicParams<Self>;
	type AssetId = <PolkadotConfig as Config>::AssetId;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundStats {
	pub round: usize,
	pub send_duration_secs: f64,
	pub receive_duration_secs: f64,
	pub full_latency_secs: f64,
	pub sent_count: u32,
	pub received_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
	pub min: f64,
	pub avg: f64,
	pub max: f64,
}

pub fn parse_messages_pattern(pattern: &str) -> Result<Vec<(usize, usize)>, anyhow::Error> {
	pattern
		.split(',')
		.map(|part| {
			let part = part.trim();
			let (count_str, size_str) = part
				.split_once(':')
				.ok_or_else(|| anyhow!("Invalid pattern '{part}'. Expected 'count:size'"))?;

			let count = count_str
				.parse::<usize>()
				.with_context(|| format!("Invalid count '{count_str}' in pattern '{part}'"))?;
			let size = size_str
				.parse::<usize>()
				.with_context(|| format!("Invalid size '{size_str}' in pattern '{part}'"))?;

			Ok((count, size))
		})
		.collect()
}

pub fn messages_per_client(pattern: &[(usize, usize)]) -> usize {
	pattern.iter().map(|(count, _)| count).sum()
}

pub fn calc_stats(values: impl Iterator<Item = f64>) -> Stats {
	let values: Vec<_> = values.collect();
	let min = values.iter().copied().fold(f64::INFINITY, f64::min);
	let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
	let avg = values.iter().sum::<f64>() / values.len() as f64;
	Stats { min, avg, max }
}

pub fn is_leader(client_id: u32) -> bool {
	client_id == 0
}

pub fn generate_topic(test_run_id: u64, client_id: u32, round: usize, msg_idx: u32) -> [u8; 32] {
	let topic_str = format!("{test_run_id}-{client_id}-{round}-{msg_idx}");
	blake2_256(topic_str.as_bytes())
}

/// Generate a deterministic keypair for a given client index
pub fn get_keypair(idx: u32) -> sr25519::Pair {
	sr25519::Pair::from_string(&format!("//StatementBench//{idx}"), None)
		.expect("Derivation path is always valid; qed")
}

pub async fn connect_to_endpoints(
	endpoints: &[String],
) -> Result<Vec<Arc<WsClient>>, anyhow::Error> {
	let mut clients = Vec::with_capacity(endpoints.len());

	for endpoint in endpoints {
		let client = WsClientBuilder::default()
			.max_concurrent_requests(10000)
			.build(endpoint)
			.await
			.with_context(|| format!("Failed to connect to {endpoint}"))?;
		clients.push(Arc::new(client));
		debug!("Connected to {}", endpoint);
	}

	Ok(clients)
}
