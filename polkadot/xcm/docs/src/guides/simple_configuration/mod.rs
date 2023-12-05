//! # Simple configuration
//!
//! At the end of this guide, you'll know how to setup an XCM executor configuration
//! for a parachain only using the relaychain's asset.
//!
#![doc = docify::embed!("src/guides/simple_configuration/mod.rs", Something)]

pub mod parachain;
pub mod relaychain;
pub mod mock_message_queue;

use xcm_simulator::{
    decl_test_network,
    decl_test_parachain,
    decl_test_relay_chain,
    TestExt,
};

decl_test_parachain! {
    pub struct ParaA {
        Runtime = parachain::Runtime,
        XcmpMessageHandler = parachain::MsgQueue,
        DmpMessageHandler = parachain::MsgQueue,
        new_ext = para_ext(1),
    }
}

fn para_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::default().build_storage::<Runtime>().unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext
}

#[docify::export]
pub struct Something;
