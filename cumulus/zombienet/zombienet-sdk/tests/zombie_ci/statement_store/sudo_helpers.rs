// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Sudo extrinsic helpers and custom subxt config for statement store tests
//!
//! Contains the `CustomConfig` type (with `CustomExtrinsicParams`) needed to
//! submit extrinsics on people-westend, plus helpers that set statement
//! allowances at runtime via `Sudo::sudo(System::set_storage(...))`

use std::{any::Any, path::PathBuf, time::Duration};

use anyhow::anyhow;
use codec::Encode;
use futures::StreamExt;
use log::info;
use sp_core::Pair;
use sp_statement_store::{statement_allowance_key, StatementAllowance};
use zombienet_sdk::{
	subxt::{
		config::{
			transaction_extensions::{
				AnyOf, ChargeAssetTxPayment, ChargeTransactionPayment, CheckGenesis,
				CheckMetadataHash, CheckMortality, CheckNonce, CheckSpecVersion, CheckTxVersion,
				TransactionExtension, VerifySignatureDetails,
			},
			Config, DefaultExtrinsicParamsBuilder, ExtrinsicParams, ExtrinsicParamsEncoder,
		},
		dynamic::Value,
		ext::scale_value::value,
		tx::{signer::Signer, DynamicPayload, TxStatus},
		utils::{Static, H256},
		OnlineClient, PolkadotConfig,
	},
	LocalFileSystem, Network, NetworkConfigBuilder,
};

use super::common::get_keypair;

pub(super) struct VerifyMultiSignature<T: Config>(VerifySignatureDetails<T>);

impl<T: Config> ExtrinsicParams<T> for VerifyMultiSignature<T> {
	type Params = ();

	fn new(
		_client: &zombienet_sdk::subxt::client::ClientState<T>,
		_params: Self::Params,
	) -> Result<Self, zombienet_sdk::subxt::config::ExtrinsicParamsError> {
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
				_client: &zombienet_sdk::subxt::client::ClientState<T>,
				_params: Self::Params,
			) -> Result<Self, zombienet_sdk::subxt::config::ExtrinsicParamsError> {
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

pub(super) type CustomExtrinsicParams<T> = AnyOf<
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
pub(super) enum CustomConfig {}

impl Config for CustomConfig {
	type AccountId = <PolkadotConfig as Config>::AccountId;
	type Address = <PolkadotConfig as Config>::Address;
	type Signature = <PolkadotConfig as Config>::Signature;
	type Hasher = <PolkadotConfig as Config>::Hasher;
	type Header = <PolkadotConfig as Config>::Header;
	type ExtrinsicParams = CustomExtrinsicParams<Self>;
	type AssetId = <PolkadotConfig as Config>::AssetId;
}

/// Creates storage items for custom per-participant allowances
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

/// Creates a sudo -> frame_system::set_storage call to set statement allowances
fn create_set_storage_call(items: Vec<(Vec<u8>, Vec<u8>)>) -> DynamicPayload {
	let items_value: Vec<Value> = items
		.into_iter()
		.map(|(key, value)| value!((Value::from_bytes(key), Value::from_bytes(value))))
		.collect();

	zombienet_sdk::subxt::tx::dynamic(
		"Sudo",
		"sudo",
		vec![value! {
			System(set_storage { items: items_value })
		}],
	)
}

/// Submits an extrinsic with an explicit nonce and waits for it to be included in a block
async fn submit_sudo_extrinsic<S: Signer<CustomConfig>>(
	client: &OnlineClient<CustomConfig>,
	call: &DynamicPayload,
	signer: &S,
	nonce: u64,
) -> Result<
	zombienet_sdk::subxt::tx::TxProgress<CustomConfig, OnlineClient<CustomConfig>>,
	anyhow::Error,
> {
	let dp = DefaultExtrinsicParamsBuilder::<CustomConfig>::new()
		.immortal()
		.nonce(nonce)
		.build();
	let extensions =
		(dp.0, dp.1, dp.2, dp.3, dp.4, dp.5, dp.6, dp.7, dp.8, (), (), (), (), (), (), ());

	let mut tx = client
		.tx()
		.create_signed(call, signer, extensions)
		.await?
		.submit_and_watch()
		.await?;

	while let Some(status) = tx.next().await.transpose()? {
		match status {
			TxStatus::InBestBlock(tx_in_block) => {
				tx_in_block.wait_for_success().await?;
				return Ok(tx);
			},
			TxStatus::InFinalizedBlock(ref tx_in_block) => {
				tx_in_block.wait_for_success().await?;
				return Ok(tx);
			},
			TxStatus::Error { message } |
			TxStatus::Invalid { message } |
			TxStatus::Dropped { message } => {
				return Err(anyhow!("Error submitting sudo tx: {message}"));
			},
			_ => continue,
		}
	}

	Err(anyhow!("Transaction event stream ended without being included in a block"))
}

/// Waits for a tx to finalize
async fn wait_for_tx_finalization<Tx>(
	tx_stream: &mut Tx,
	timeout_secs: u64,
) -> Result<H256, anyhow::Error>
where
	Tx: futures::Stream<
			Item = Result<
				TxStatus<CustomConfig, OnlineClient<CustomConfig>>,
				zombienet_sdk::subxt::Error,
			>,
		> + Unpin,
{
	let watch_future = async {
		while let Some(status) = tx_stream.next().await.transpose()? {
			match status {
				TxStatus::InFinalizedBlock(ref tx_in_block) => {
					tx_in_block.wait_for_success().await?;
					return Ok(tx_in_block.block_hash());
				},
				TxStatus::Error { message } |
				TxStatus::Invalid { message } |
				TxStatus::Dropped { message } => {
					return Err(anyhow!("Tx error during finalization: {message}"));
				},
				_ => continue,
			}
		}
		Err(anyhow!("Transaction stream ended without finalization"))
	};

	tokio::time::timeout(Duration::from_secs(timeout_secs), watch_future)
		.await
		.map_err(|_| anyhow!("Timeout waiting for tx finalization after {}s", timeout_secs))?
}

/// Gets the current nonce for an account
async fn get_account_nonce(
	client: &OnlineClient<CustomConfig>,
	account_id: &<CustomConfig as Config>::AccountId,
) -> Result<u64, anyhow::Error> {
	let nonce = client.tx().account_nonce(account_id).await?;
	Ok(nonce)
}

/// Sets statement allowances via sudo -> frame_system::set_storage extrinsic
async fn set_allowances_via_sudo(
	para_client: &OnlineClient<CustomConfig>,
	items: Vec<(Vec<u8>, Vec<u8>)>,
) -> Result<(), anyhow::Error> {
	info!("Setting {} statement allowances via sudo...", items.len());

	let alice = zombienet_sdk::subxt_signer::sr25519::dev::alice();
	let alice_account_id =
		<zombienet_sdk::subxt_signer::sr25519::Keypair as Signer<CustomConfig>>::account_id(&alice);

	let current_nonce = get_account_nonce(para_client, &alice_account_id).await?;
	let set_storage_call = create_set_storage_call(items);

	let mut tx_stream =
		submit_sudo_extrinsic(para_client, &set_storage_call, &alice, current_nonce).await?;
	let block_hash = wait_for_tx_finalization(&mut tx_stream, 120).await?;
	info!("Statement allowances set and finalized in block {:?}", block_hash);

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

	let para_client = node.wait_client::<CustomConfig>().await?;
	set_allowances_via_sudo(&para_client, allowance_items).await?;

	Ok(network)
}
