pub use coretime_rococo_runtime;

pub mod genesis;

// Substrate
use frame_support::traits::OnInitialize;

// Cumulus
use emulated_integration_tests_common::{
	impl_accounts_helpers_for_parachain, impl_assert_events_helpers_for_parachain,
	impls::Parachain, xcm_emulator::decl_test_parachains,
};

// CoretimeRococo Parachain declaration
decl_test_parachains! {
	pub struct CoretimeRococo {
		genesis = genesis::genesis(),
		on_init = {
			coretime_rococo_runtime::AuraExt::on_initialize(1);
		},
		runtime = coretime_rococo_runtime,
		core = {
			XcmpMessageHandler: coretime_rococo_runtime::XcmpQueue,
			LocationToAccountId: coretime_rococo_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: coretime_rococo_runtime::ParachainInfo,
			MessageOrigin: cumulus_primitives_core::AggregateMessageOrigin,
		},
		pallets = {
			PolkadotXcm: coretime_rococo_runtime::PolkadotXcm,
			Balances: coretime_rococo_runtime::Balances,
			Broker: coretime_rococo_runtime::Broker,
		}
	},
}

// CoretimeRococo implementation
impl_accounts_helpers_for_parachain!(CoretimeRococo);
impl_assert_events_helpers_for_parachain!(CoretimeRococo);
