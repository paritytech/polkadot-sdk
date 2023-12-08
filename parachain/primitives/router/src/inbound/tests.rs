use super::{FromEthereumGlobalConsensus, GlobalConsensusEthereumConvertsFor};
use crate::inbound::CallIndex;
use frame_support::{parameter_types, traits::ContainsPair};
use hex_literal::hex;
use sp_core::crypto::Ss58Codec;
use xcm::v3::prelude::*;
use xcm_executor::traits::ConvertLocation;

const NETWORK: NetworkId = Ethereum { chain_id: 11155111 };
const SS58_FORMAT: u16 = 2;
const EXPECTED_SOVEREIGN_KEY: [u8; 32] =
	hex!("ce796ae65569a670d0c1cc1ac12515a3ce21b5fbf729d63d7b289baad070139d");
const EXPECTED_SOVEREIGN_ADDRESS: &'static str = "HF3T62xRQvoCCowYamEQweEyWbD5yt4mkET8UkNWxfMbvJE";

parameter_types! {
	pub EthereumNetwork: NetworkId = NETWORK;
	pub EthereumLocation: MultiLocation = MultiLocation::new(2, X1(GlobalConsensus(EthereumNetwork::get())));

	pub const CreateAssetCall: CallIndex = [1, 1];
	pub const CreateAssetExecutionFee: u128 = 123;
	pub const CreateAssetDeposit: u128 = 891;
	pub const SendTokenExecutionFee: u128 = 592;
}

#[test]
fn test_contract_location_without_network_converts_successfully() {
	let contract_location = MultiLocation { parents: 2, interior: X1(GlobalConsensus(NETWORK)) };

	let account =
		GlobalConsensusEthereumConvertsFor::<[u8; 32]>::convert_location(&contract_location)
			.unwrap();
	let address = frame_support::sp_runtime::AccountId32::new(account)
		.to_ss58check_with_version(SS58_FORMAT.into());

	println!("SS58: {}\nBytes: {:?}", address, account);

	assert_eq!(account, EXPECTED_SOVEREIGN_KEY);
	assert_eq!(address, EXPECTED_SOVEREIGN_ADDRESS);
}

#[test]
fn test_contract_location_with_network_converts_successfully() {
	let contract_location = MultiLocation { parents: 2, interior: X1(GlobalConsensus(NETWORK)) };

	let account =
		GlobalConsensusEthereumConvertsFor::<[u8; 32]>::convert_location(&contract_location)
			.unwrap();
	let address = frame_support::sp_runtime::AccountId32::new(account)
		.to_ss58check_with_version(SS58_FORMAT.into());
	assert_eq!(account, EXPECTED_SOVEREIGN_KEY);
	assert_eq!(address, EXPECTED_SOVEREIGN_ADDRESS);

	println!("SS58: {}\nBytes: {:?}", address, account);
}

#[test]
fn test_contract_location_with_incorrect_location_fails_convert() {
	let contract_location =
		MultiLocation { parents: 2, interior: X2(GlobalConsensus(Polkadot), Parachain(1000)) };

	assert_eq!(
		GlobalConsensusEthereumConvertsFor::<[u8; 32]>::convert_location(&contract_location),
		None,
	);
}

#[test]
fn test_from_ethereum_global_consensus_with_containing_asset_yields_true() {
	let origin = MultiLocation { parents: 2, interior: X1(GlobalConsensus(NETWORK)) };
	let asset = MultiLocation {
		parents: 2,
		interior: X2(GlobalConsensus(NETWORK), AccountKey20 { network: None, key: [0; 20] }),
	};
	assert!(FromEthereumGlobalConsensus::<EthereumLocation>::contains(&asset, &origin));
}

#[test]
fn test_from_ethereum_global_consensus_without_containing_asset_yields_false() {
	let origin = MultiLocation { parents: 2, interior: X1(GlobalConsensus(NETWORK)) };
	let asset =
		MultiLocation { parents: 2, interior: X2(GlobalConsensus(Polkadot), Parachain(1000)) };
	assert!(!FromEthereumGlobalConsensus::<EthereumLocation>::contains(&asset, &origin));
}

#[test]
fn test_from_ethereum_global_consensus_without_bridge_origin_yields_false() {
	let origin =
		MultiLocation { parents: 2, interior: X2(GlobalConsensus(Polkadot), Parachain(1000)) };
	let asset = MultiLocation {
		parents: 2,
		interior: X2(GlobalConsensus(NETWORK), AccountKey20 { network: None, key: [0; 20] }),
	};
	assert!(!FromEthereumGlobalConsensus::<EthereumLocation>::contains(&asset, &origin));
}
