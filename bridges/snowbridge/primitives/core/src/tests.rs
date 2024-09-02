use crate::{ChannelId, ParaId, TokenIdOf};
use hex_literal::hex;
use xcm::prelude::{
	GeneralIndex, GeneralKey, GlobalConsensus, Location, PalletInstance, Parachain, Westend,
};
use xcm_executor::traits::ConvertLocation;

const EXPECT_CHANNEL_ID: [u8; 32] =
	hex!("c173fac324158e77fb5840738a1a541f633cbec8884c6a601c567d2b376a0539");

// The Solidity equivalent code is tested in Gateway.t.sol:testDeriveChannelID
#[test]
fn generate_channel_id() {
	let para_id: ParaId = 1000.into();
	let channel_id: ChannelId = para_id.into();
	assert_eq!(channel_id, EXPECT_CHANNEL_ID.into());
}

#[test]
fn test_describe_relay_token() {
	let asset_location: Location = Location::new(1, [GlobalConsensus(Westend)]);
	assert_eq!(TokenIdOf::convert_location(&asset_location).is_some(), true);
}

#[test]
fn test_describe_primary_token_from_parachain() {
	let asset_location: Location = Location::new(1, [GlobalConsensus(Westend), Parachain(2000)]);
	assert_eq!(TokenIdOf::convert_location(&asset_location).is_some(), true);
}

#[test]
fn test_describe_token_with_pallet_instance_prefix() {
	let asset_location: Location =
		Location::new(1, [GlobalConsensus(Westend), Parachain(2000), PalletInstance(8)]);
	assert_eq!(TokenIdOf::convert_location(&asset_location).is_some(), true);
}

#[test]
fn test_describe_token_with_general_index_prefix() {
	let asset_location: Location =
		Location::new(1, [GlobalConsensus(Westend), Parachain(2000), GeneralIndex(1)]);
	assert_eq!(TokenIdOf::convert_location(&asset_location).is_some(), true);
}

#[test]
fn test_describe_token_with_general_key_prefix() {
	let asset_location: Location = Location::new(
		1,
		[GlobalConsensus(Westend), Parachain(2000), GeneralKey { length: 32, data: [1; 32] }],
	);
	assert_eq!(TokenIdOf::convert_location(&asset_location).is_some(), true);
}
