//! # Simple configuration
//!
//! At the end of this guide, you'll know how to setup an XCM executor configuration
//! for a parachain to be able to handle assets using the relaychain's asset.
//!
//! ## Empty configuration
//!
//! We'll first start with an empty XCM configuration like the following:
//!
#![doc = docify::embed!("src/guides/simple_configuration/parachain.rs", XcmConfig)]
#![doc = docify::embed!("src/guides/simple_configuration/parachain.rs", XcmConfigImpl)]
//!
//! There are a lot of configuration items, which might look overwhelming.
//! Not all of them are needed though, and can be left as the unit type, `()`.
//! This usually means we turn off the functionality provided by that configuration item.
//! The ones filled in are the only ones really needed to compile.
//! We'll first take a look at those.
//!
//! ## UniversalLocation
//!
//! The UniversalLocation configuration item is the absolute location of your chain.
//! This means that it's a `Location` with no parents that starts with a `GlobalConsensus` junction.
//! That's all it is, so you need to configure it like so:
//!
#![doc = docify::embed!("src/guides/simple_configuration/parachain.rs", UniversalLocation)]
//!
//! In this example, our chain's universal location is parachain 2222 in Polkadot.
//!
//! ## Weigher
//!
//! We need a way to weigh XCM programs, which means weighing each instruction.
//! A simple way of weighing instructions is to assign them a base weight value to all of them.
//! This works, but it is not very accurate, as different instructions use more resources when being executed.
//! A better approach is to benchmark each instruction to find out the actual weight used by each.
//!
#![doc = docify::embed!("src/guides/simple_configuration/parachain.rs", Weigher)]
//!
//! ## AssetTransactor
//!
//! Our configuration right now doesn't allow any useful programs.
//! Let's add a way to handle assets.
//! This is the `AssetTransactor`
#![doc = docify::embed!("src/guides/simple_configuration/parachain.rs", asset_handling)]

pub mod parachain;
pub mod relaychain;
pub mod mock_message_queue;

use sp_runtime::BuildStorage;
use frame::deps::frame_system;
use xcm_simulator::{
    decl_test_network,
    decl_test_parachain,
    decl_test_relay_chain,
    TestExt,
};

decl_test_parachain! {
    pub struct ParaA {
        Runtime = parachain::Runtime,
        XcmpMessageHandler = parachain::MessageQueue,
        DmpMessageHandler = parachain::MessageQueue,
        new_ext = para_ext(),
    }
}

decl_test_relay_chain! {
    pub struct Relay {
        Runtime = relaychain::Runtime,
		RuntimeCall = relaychain::RuntimeCall,
		RuntimeEvent = relaychain::RuntimeEvent,
		XcmConfig = relaychain::XcmConfig,
		MessageQueue = relaychain::MessageQueue,
		System = relaychain::System,
		new_ext = relay_ext(),
    }
}

decl_test_network! {
	pub struct MockNet {
		relay_chain = Relay,
		parachains = vec![
			(1000, ParaA),
		],
	}
}

fn para_ext() -> sp_io::TestExternalities {
    use parachain::{Runtime, System};
    let t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
	});
    ext
}

pub fn relay_ext() -> sp_io::TestExternalities {
	use relaychain::{Runtime, System};
	let t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	// pallet_balances::GenesisConfig::<Runtime> {
	// 	balances: vec![
	// 		(ALICE, INITIAL_BALANCE),
	// 	],
	// }
	// .assimilate_storage(&mut t)
	// .unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}

#[docify::export]
pub struct Something;
