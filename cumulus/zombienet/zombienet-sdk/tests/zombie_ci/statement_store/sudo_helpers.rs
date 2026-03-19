// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Sudo extrinsic helpers for statement store tests.
//!
//! Uses the metadata macro to generate typed calls for the people-westend
//! individuality runtime. A minimal custom config handles the runtime's
//! `VerifyMultiSignature` extension and skips unknown custom extensions

use std::path::PathBuf;

use anyhow::anyhow;
use codec::Encode;
use log::info;
use scale_info::PortableRegistry;
use sp_core::Pair;
use sp_statement_store::{statement_allowance_key, StatementAllowance};
use subxt::{
	config::{
		transaction_extensions::{
			ChargeAssetTxPayment, ChargeTransactionPayment, CheckGenesis, CheckMetadataHash,
			CheckMortality, CheckNonce, CheckSpecVersion, CheckTxVersion, VerifySignatureDetails,
		},
		ClientState, Config, SubstrateConfig, TransactionExtension,
	},
	error::TransactionExtensionError,
	metadata::ArcMetadata,
	utils::MultiAddress,
	OnlineClient,
};
use zombienet_sdk::{LocalFileSystem, Network, NetworkConfigBuilder};

use super::common::get_keypair;

#[subxt::subxt(
	runtime_metadata_insecure_url = "wss://people-2104-node-0.parity-versi.parity.io:443"
)]
pub(super) mod people_api {}

type RuntimeCall = people_api::runtime_types::people_westend_runtime::RuntimeCall;
type SystemCall = people_api::runtime_types::frame_system::pallet::Call;

/// Handles the `VerifyMultiSignature` extension (same encoding as `VerifySignature`)
struct VerifyMultiSignature<T: Config>(VerifySignatureDetails<T>);

impl<T: Config> TransactionExtension<T> for VerifyMultiSignature<T> {
	type Decoded = VerifySignatureDetails<T>;
	type Params = ();

	fn new(
		_client: &ClientState<T>,
		_params: Self::Params,
	) -> Result<Self, TransactionExtensionError> {
		Ok(VerifyMultiSignature(VerifySignatureDetails::Disabled))
	}

	fn inject_signature(&mut self, account: &T::AccountId, signature: &T::Signature) {
		self.0 = VerifySignatureDetails::Signed {
			signature: signature.clone(),
			account: account.clone(),
		};
	}
}

impl<T: Config> subxt::ext::frame_decode::extrinsics::TransactionExtension<PortableRegistry>
	for VerifyMultiSignature<T>
{
	const NAME: &str = "VerifyMultiSignature";

	fn encode_value_to(
		&self,
		type_id: u32,
		type_resolver: &PortableRegistry,
		v: &mut Vec<u8>,
	) -> Result<(), subxt::ext::frame_decode::extrinsics::TransactionExtensionError> {
		use subxt::ext::scale_encode::EncodeAsType;
		self.0.encode_as_type_to(type_id, type_resolver, v)?;
		Ok(())
	}
	fn encode_value_for_signer_payload_to(
		&self,
		_type_id: u32,
		_type_resolver: &PortableRegistry,
		v: &mut Vec<u8>,
	) -> Result<(), subxt::ext::frame_decode::extrinsics::TransactionExtensionError> {
		v.clear();
		Ok(())
	}
	fn encode_implicit_to(
		&self,
		_type_id: u32,
		_type_resolver: &PortableRegistry,
		v: &mut Vec<u8>,
	) -> Result<(), subxt::ext::frame_decode::extrinsics::TransactionExtensionError> {
		v.clear();
		Ok(())
	}
}

// macro_rules! define_skip_extensions {
// 	($($name:ident => $lit:expr),+ $(,)?) => { $(
// 		struct $name;
//
// 		impl<T: Config> TransactionExtension<T> for $name {
// 			type Decoded = u8;
// 			type Params = ();
//
// 			fn new(
// 				_client: &ClientState<T>,
// 				_params: Self::Params,
// 			) -> Result<Self, TransactionExtensionError> {
// 				Ok($name)
// 			}
// 		}
//
// 		impl subxt::ext::frame_decode::extrinsics::TransactionExtension<PortableRegistry>
// 			for $name
// 		{
// 			const NAME: &str = $lit;
//
// 			fn encode_value_to(
// 				&self,
// 				_type_id: u32,
// 				_type_resolver: &PortableRegistry,
// 				v: &mut Vec<u8>,
// 			) -> Result<(), subxt::ext::frame_decode::extrinsics::TransactionExtensionError> {
// 				v.push(0x00);
// 				Ok(())
// 			}
// 			fn encode_implicit_to(
// 				&self,
// 				_type_id: u32,
// 				_type_resolver: &PortableRegistry,
// 				_v: &mut Vec<u8>,
// 			) -> Result<(), subxt::ext::frame_decode::extrinsics::TransactionExtensionError> {
// 				Ok(())
// 			}
// 		}
// 	)+ };
// }

// define_skip_extensions!(
// 	AsPerson => "AsPerson",
// 	AsProofOfInkParticipant => "AsProofOfInkParticipant",
// 	ScoreAsParticipant => "ScoreAsParticipant",
// 	GameAsInvited => "GameAsInvited",
// 	PeopleLiteAuth => "PeopleLiteAuth",
// 	AsCoinage => "AsCoinage",
// 	RestrictOrigins => "RestrictOrigins",
// );

type IndividualityExtrinsicParams<T> = (
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
);

/// Custom config identical to [`SubstrateConfig`] but with extensions for the
/// individuality runtime (`VerifyMultiSignature` + custom skip handlers)
#[derive(Debug, Clone)]
struct IndividualityConfig(SubstrateConfig);

impl Default for IndividualityConfig {
	fn default() -> Self {
		IndividualityConfig(SubstrateConfig::default())
	}
}

impl Config for IndividualityConfig {
	type AccountId = <SubstrateConfig as Config>::AccountId;
	type Address = MultiAddress<Self::AccountId, ()>;
	type Signature = <SubstrateConfig as Config>::Signature;
	type Hasher = <SubstrateConfig as Config>::Hasher;
	type Header = <SubstrateConfig as Config>::Header;
	type AssetId = <SubstrateConfig as Config>::AssetId;
	type TransactionExtensions = IndividualityExtrinsicParams<Self>;

	fn genesis_hash(&self) -> Option<subxt::config::HashFor<Self>> {
		self.0.genesis_hash()
	}
	fn metadata_for_spec_version(&self, spec_version: u32) -> Option<ArcMetadata> {
		self.0.metadata_for_spec_version(spec_version)
	}
	fn set_metadata_for_spec_version(&self, spec_version: u32, metadata: ArcMetadata) {
		self.0.set_metadata_for_spec_version(spec_version, metadata)
	}
}

/// Creates storage items for custom per-participant allowances.
pub(super) fn create_allowance_items(
	allowances: &[(u32, StatementAllowance)],
) -> Vec<(Vec<u8>, Vec<u8>)> {
	let mut items = Vec::with_capacity(allowances.len());
	for (idx, allowance) in allowances {
		let keypair = get_keypair(*idx);
		let account_id = keypair.public();
		let storage_key = statement_allowance_key(account_id.0);
		items.push((storage_key.to_vec(), allowance.encode()));
	}
	items
}

/// Creates uniform allowance storage items for a range of participants
pub(super) fn create_uniform_allowance_items(
	count: u32,
	allowance: StatementAllowance,
) -> Vec<(Vec<u8>, Vec<u8>)> {
	let allowance_encoded = allowance.encode();
	let mut items = Vec::with_capacity(count as usize);
	for idx in 0..count {
		let keypair = get_keypair(idx);
		let account_id = keypair.public();
		let storage_key = statement_allowance_key(account_id.0);
		items.push((storage_key.to_vec(), allowance_encoded.clone()));
	}
	items
}

/// Sets statement allowances via sudo -> frame_system::set_storage extrinsic
async fn set_allowances_via_sudo(
	ws_uri: &str,
	items: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<(), anyhow::Error> {
	info!("Setting {} statement allowances via sudo...", items.len());

	let para_client =
		OnlineClient::<IndividualityConfig>::from_url(ws_uri).await?;
	let alice = subxt_signer::sr25519::dev::alice();

	let call = people_api::tx().sudo().sudo(RuntimeCall::System(
		SystemCall::set_storage { items },
	));

	para_client
		.tx()
		.await?
		.sign_and_submit_then_watch_default(&call, &alice)
		.await?
		.wait_for_finalized_success()
		.await?;

	info!("Statement allowances set and finalized");
	Ok(())
}

/// Spawns a network with the sudo-enabled chain spec and sets allowances at runtime
pub(super) async fn spawn_network_sudo(
	collators: &[&str],
	allowance_items: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
	let images = zombienet_sdk::environment::get_images_from_env();

	let base_dir = std::env::var("ZOMBIENET_SDK_BASE_DIR")
		.ok()
		.map(PathBuf::from)
		.unwrap_or_else(|| std::env::temp_dir().join(format!("zombienet-{}", std::process::id())));
	std::fs::create_dir_all(&base_dir)
		.map_err(|e| anyhow!("Failed to create base directory: {}", e))?;

	let participant_count = allowance_items.len();

	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("westend-local")
				.with_default_command("polkadot")
				.with_default_image(images.polkadot.as_str())
				.with_default_args(vec!["-lparachain=debug".into()])
				.with_validator(|node| node.with_name("validator-0"))
				.with_validator(|node| node.with_name("validator-1"))
		})
		.with_parachain(|p| {
			let p = p
				.with_id(2101)
				.with_chain_spec_path("https://raw.githubusercontent.com/paritytech/chainspecs/denzelpenzel/versi-people-2101/versi/parachain/versi-people-2101/chainspec.json")
				.with_default_command("polkadot-parachain")
				.with_default_image(images.cumulus.as_str())
				.with_default_args(vec![
					"--force-authoring".into(),
					"--authoring".into(),
					"slot-based".into(),
					"--max-runtime-instances=32".into(),
					"-linfo,statement-store=info,statement-gossip=info".into(),
					"--enable-statement-store".into(),
					format!("--rpc-max-connections={}", participant_count + 1000).as_str().into(),
					format!(
						"--rpc-max-subscriptions-per-connection={}",
						(participant_count * 16).max(32)
					)
						.as_str()
						.into(),
				])
				.with_collator(|n| n.with_name(collators[0]));

			collators[1..]
				.iter()
				.fold(p, |acc, &name| acc.with_collator(|n| n.with_name(name)))
		})
		.with_global_settings(|global_settings| {
			global_settings.with_base_dir(base_dir.to_str().expect("Valid UTF-8 path"))
		})
		.build()
		.map_err(|e| {
			let errs = e.into_iter().map(|e| e.to_string()).collect::<Vec<_>>().join(" ");
			anyhow!("config errs: {errs}")
		})?;

	let spawn_fn = zombienet_sdk::environment::get_spawn_fn();
	let network = spawn_fn(config).await?;
	assert!(network.wait_until_is_up(60).await.is_ok());

	info!("Waiting for parachain to produce blocks...");
	let first_collator = collators[0];
	let node = network.get_node(first_collator)?;
	node.wait_metric_with_timeout("block_height{status=\"best\"}", |height| height >= 1.0, 300u64)
		.await?;
	info!("Parachain is producing blocks");

	set_allowances_via_sudo(node.ws_uri(), allowance_items).await?;

	Ok(network)
}
