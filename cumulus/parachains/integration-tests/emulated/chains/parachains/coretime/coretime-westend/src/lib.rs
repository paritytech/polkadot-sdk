pub use coretime_westend_runtime;

pub mod genesis;

// Substrate
use frame_support::traits::OnInitialize;

// Cumulus
use emulated_integration_tests_common::{
	impl_accounts_helpers_for_parachain, impl_assert_events_helpers_for_parachain,
	impls::Parachain, xcm_emulator::decl_test_parachains,
};

// CoretimeWestend Parachain declaration
decl_test_parachains! {
	pub struct CoretimeWestend {
		genesis = genesis::genesis(),
		on_init = {
			coretime_westend_runtime::AuraExt::on_initialize(1);
		},
		runtime = coretime_westend_runtime,
		core = {
			XcmpMessageHandler: coretime_westend_runtime::XcmpQueue,
			LocationToAccountId: coretime_westend_runtime::xcm_config::LocationToAccountId,
			ParachainInfo: coretime_westend_runtime::ParachainInfo,
			MessageOrigin: cumulus_primitives_core::AggregateMessageOrigin,
		},
		pallets = {
			PolkadotXcm: coretime_westend_runtime::PolkadotXcm,
			Balances: coretime_westend_runtime::Balances,
			Broker: coretime_westend_runtime::Broker,
		}
	},
}

// CoretimeWestend implementation
impl_accounts_helpers_for_parachain!(CoretimeWestend);
impl_assert_events_helpers_for_parachain!(CoretimeWestend);
