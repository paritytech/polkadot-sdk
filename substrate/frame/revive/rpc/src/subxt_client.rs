//! The generated subxt client.
use subxt::config::{signed_extensions, Config, PolkadotConfig};

#[subxt::subxt(
	runtime_metadata_path = "kitchensink.scale",
	substitute_type(
		path = "pallet_revive::primitives::EthContractResult<A>",
		with = "::subxt::utils::Static<::pallet_revive::EthContractResult<A>>"
	),
	substitute_type(
		path = "pallet_revive::primitives::EthTransactKind",
		with = "::subxt::utils::Static<::pallet_revive::EthTransactKind>"
	),
	substitute_type(
		path = "sp_weights::weight_v2::Weight",
		with = "::subxt::utils::Static<::sp_weights::Weight>"
	)
)]
mod src_chain {}
pub use src_chain::*;

/// The configuration for the source chain.
pub enum SrcChainConfig {}
impl Config for SrcChainConfig {
	type Hash = <PolkadotConfig as Config>::Hash;
	type AccountId = <PolkadotConfig as Config>::AccountId;
	type Address = <PolkadotConfig as Config>::Address;
	type Signature = <PolkadotConfig as Config>::Signature;
	type Hasher = <PolkadotConfig as Config>::Hasher;
	type Header = <PolkadotConfig as Config>::Header;
	type AssetId = <PolkadotConfig as Config>::AssetId;
	type ExtrinsicParams = signed_extensions::AnyOf<
		Self,
		(
			signed_extensions::CheckSpecVersion,
			signed_extensions::CheckTxVersion,
			signed_extensions::CheckNonce,
			signed_extensions::CheckGenesis<Self>,
			signed_extensions::CheckMortality<Self>,
			signed_extensions::ChargeAssetTxPayment<Self>,
			signed_extensions::ChargeTransactionPayment,
			signed_extensions::CheckMetadataHash,
		),
	>;
}
