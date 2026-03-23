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

use anyhow::Context;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use log::debug;
use scale_info::PortableRegistry;
use sp_core::{Pair, sr25519};
use std::sync::Arc;
use subxt::{
	config::{
		ClientState, Config, TransactionExtension,
		substrate::SubstrateConfig,
		transaction_extensions::{
			ChargeAssetTxPayment, ChargeTransactionPayment, CheckGenesis, CheckMetadataHash,
			CheckMortality, CheckNonce, CheckSpecVersion, CheckTxVersion, VerifySignature,
		},
	},
	ext::frame_decode,
};

pub struct VerifyMultiSignature<T: Config>(VerifySignature<T>);

impl<T: Config> frame_decode::extrinsics::TransactionExtension<PortableRegistry>
	for VerifyMultiSignature<T>
{
	const NAME: &str = "VerifyMultiSignature";

	fn encode_value_to(
		&self,
		type_id: u32,
		type_resolver: &PortableRegistry,
		v: &mut Vec<u8>,
	) -> Result<(), frame_decode::extrinsics::TransactionExtensionError> {
		self.0.encode_value_to(type_id, type_resolver, v)
	}

	fn encode_value_for_signer_payload_to(
		&self,
		type_id: u32,
		type_resolver: &PortableRegistry,
		v: &mut Vec<u8>,
	) -> Result<(), frame_decode::extrinsics::TransactionExtensionError> {
		self.0.encode_value_for_signer_payload_to(type_id, type_resolver, v)
	}

	fn encode_implicit_to(
		&self,
		type_id: u32,
		type_resolver: &PortableRegistry,
		v: &mut Vec<u8>,
	) -> Result<(), frame_decode::extrinsics::TransactionExtensionError> {
		self.0.encode_implicit_to(type_id, type_resolver, v)
	}
}

impl<T: Config> TransactionExtension<T> for VerifyMultiSignature<T> {
	type Decoded = <VerifySignature<T> as TransactionExtension<T>>::Decoded;
	type Params = ();

	fn new(
		client: &ClientState<T>,
		params: Self::Params,
	) -> Result<Self, subxt::error::TransactionExtensionError> {
		Ok(VerifyMultiSignature(VerifySignature::new(client, params)?))
	}

	fn inject_signature(&mut self, account_id: &T::AccountId, signature: &T::Signature) {
		self.0.inject_signature(account_id, signature);
	}
}

/// Custom transaction extension for `RestrictOrigins`.
///
/// This extension encodes as `false` (0x00) to disable origin restrictions
/// It is a `bool` in the runtime (not `Option<T>`), so frame-decode cannot
/// auto-default it and it must be handled explicitly
pub struct RestrictOrigins;

impl frame_decode::extrinsics::TransactionExtension<PortableRegistry> for RestrictOrigins {
	const NAME: &str = "RestrictOrigins";

	fn encode_value_to(
		&self,
		_type_id: u32,
		_type_resolver: &PortableRegistry,
		v: &mut Vec<u8>,
	) -> Result<(), frame_decode::extrinsics::TransactionExtensionError> {
		// Encode `false` disables origin restriction
		v.push(0x00);
		Ok(())
	}

	fn encode_implicit_to(
		&self,
		_type_id: u32,
		_type_resolver: &PortableRegistry,
		_v: &mut Vec<u8>,
	) -> Result<(), frame_decode::extrinsics::TransactionExtensionError> {
		Ok(())
	}
}

impl<T: Config> TransactionExtension<T> for RestrictOrigins {
	type Decoded = u8;
	type Params = ();

	fn new(
		_client: &ClientState<T>,
		_params: Self::Params,
	) -> Result<Self, subxt::error::TransactionExtensionError> {
		Ok(RestrictOrigins)
	}
}

#[derive(Debug, Clone)]
pub struct CustomConfig(SubstrateConfig);

impl Default for CustomConfig {
	fn default() -> Self {
		CustomConfig(SubstrateConfig::new())
	}
}

impl Config for CustomConfig {
	type AccountId = <SubstrateConfig as Config>::AccountId;
	type Address = subxt::utils::MultiAddress<Self::AccountId, ()>;
	type Signature = <SubstrateConfig as Config>::Signature;
	type Hasher = <SubstrateConfig as Config>::Hasher;
	type Header = <SubstrateConfig as Config>::Header;
	type AssetId = <SubstrateConfig as Config>::AssetId;
	type TransactionExtensions = (
		VerifyMultiSignature<Self>,
		CheckSpecVersion,
		CheckTxVersion,
		CheckNonce,
		CheckGenesis<Self>,
		CheckMortality<Self>,
		ChargeAssetTxPayment<Self>,
		ChargeTransactionPayment,
		CheckMetadataHash,
		RestrictOrigins,
	);

	fn genesis_hash(&self) -> Option<subxt::config::HashFor<Self>> {
		self.0.genesis_hash()
	}

	fn spec_and_transaction_version_for_block_number(
		&self,
		block_number: u64,
	) -> Option<(u32, u32)> {
		self.0.spec_and_transaction_version_for_block_number(block_number)
	}

	fn metadata_for_spec_version(&self, spec_version: u32) -> Option<subxt::metadata::ArcMetadata> {
		self.0.metadata_for_spec_version(spec_version)
	}

	fn set_metadata_for_spec_version(
		&self,
		spec_version: u32,
		metadata: subxt::metadata::ArcMetadata,
	) {
		self.0.set_metadata_for_spec_version(spec_version, metadata)
	}
}

/// Generate a deterministic keypair for a given client index.
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
