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

//! Custom subxt configuration for runtimes interacting with statement-store, used only for tests
//!
//! The runtime uses `VerifyMultiSignature` instead of the standard
//! `VerifySignature` transaction extension, and includes a `RestrictOrigins`
//! extension that encodes as a bool (`false` = 0x00 to disable origin
//! restrictions). These non-standard extensions cannot be auto-defaulted by
//! frame-decode, so this module provides a `CustomConfig` that handles them
//! explicitly.

use scale_info::PortableRegistry;
use subxt::{
	config::{
		substrate::SubstrateConfig,
		transaction_extensions::{
			ChargeAssetTxPayment, ChargeTransactionPayment, CheckGenesis, CheckMetadataHash,
			CheckMortality, CheckNonce, CheckSpecVersion, CheckTxVersion, VerifySignature,
		},
		ClientState, Config, DefaultExtrinsicParamsBuilder, TransactionExtension,
		TransactionExtensions,
	},
	dynamic::Value,
	ext::{frame_decode, scale_value::value},
	transactions::Signer,
	utils::H256,
	OnlineClient,
};

/// Wrapper around `VerifySignature` that matches the runtime's `VerifyMultiSignature` name
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

/// Custom transaction extension for `RestrictOrigins`
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

/// Custom subxt `Config`
///
/// Registers the non-standard `VerifyMultiSignature` and `RestrictOrigins`
/// transaction extensions so that subxt can correctly encode extrinsics
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

/// Builds params for CustomConfig's transaction extensions (9 defaults + RestrictOrigins)
fn build_params(
	nonce: u64,
) -> <<CustomConfig as Config>::TransactionExtensions as TransactionExtensions<CustomConfig>>::Params
{
	let (a, b, c, d, e, f, g, h, i) = DefaultExtrinsicParamsBuilder::<CustomConfig>::new()
		.immortal()
		.nonce(nonce)
		.build();
	(a, b, c, d, e, f, g, h, i, ())
}

/// Submits an extrinsic with an explicit nonce and waits for it to be finalized
pub async fn submit_extrinsic<S: Signer<CustomConfig>>(
	client: &OnlineClient<CustomConfig>,
	call: &subxt::transactions::DynamicPayload<Vec<Value>>,
	signer: &S,
	nonce: u64,
) -> Result<H256, anyhow::Error> {
	let tx_in_block = client
		.tx()
		.await?
		.sign_and_submit_then_watch(call, signer, build_params(nonce))
		.await?
		.wait_for_finalized()
		.await?;

	tx_in_block.wait_for_success().await?;
	Ok(tx_in_block.block_hash())
}

/// Gets the current nonce for an account
pub async fn get_account_nonce(
	client: &OnlineClient<CustomConfig>,
	account_id: &<CustomConfig as Config>::AccountId,
) -> Result<u64, anyhow::Error> {
	let nonce = client.tx().await?.account_nonce(account_id).await?;
	Ok(nonce)
}

/// Sets statement allowances at runtime via a sudo extrinsic signed by Alice
pub async fn set_allowances_via_sudo(
	ws_uri: &str,
	items: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<(), anyhow::Error> {
	log::info!("Setting {} statement allowances via sudo...", items.len());

	let client = OnlineClient::<CustomConfig>::from_insecure_url_with_config(
		CustomConfig::default(),
		ws_uri,
	)
	.await?;
	let alice = subxt_signer::sr25519::dev::alice();

	let items_value: Vec<Value> = items
		.into_iter()
		.map(|(key, value)| value!((Value::from_bytes(key), Value::from_bytes(value))))
		.collect();
	let call = subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			System(set_storage { items: items_value })
		}],
	);

	client
		.tx()
		.await?
		.sign_and_submit_then_watch_default(&call, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	Ok(())
}
