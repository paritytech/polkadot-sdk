// Copyright 2023 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Tests for verifying if raw ChainSpecs generated using legacy RuntimeGenesisConfig based approach
//! are identical to ChainSpec generated using JSON approach.
//! Entire file shall be removed once native runtime is removed.

macro_rules! test {
	($test_name:ident, $tested_fn:expr) => {
		#[test]
		fn $test_name() {
			sp_tracing::try_init_simple();
			let j1 = {
				use crate::chain_spec::*;
				$tested_fn.as_json(true).unwrap()
			};
			let j2 = {
				use crate::legacy_chain_spec::*;
				$tested_fn.as_json(true).unwrap()
			};
			assert_eq!(j1, j2);
		}
	};
}

test!(test00, asset_hubs::asset_hub_polkadot_development_config());
test!(test01, asset_hubs::asset_hub_polkadot_local_config());
test!(test02, asset_hubs::asset_hub_polkadot_config());
test!(test03, asset_hubs::asset_hub_kusama_development_config());
test!(test04, asset_hubs::asset_hub_kusama_local_config());
test!(test05, asset_hubs::asset_hub_kusama_config());
test!(test06, asset_hubs::asset_hub_westend_development_config());
test!(test07, asset_hubs::asset_hub_westend_local_config());
test!(test08, asset_hubs::asset_hub_westend_config());
test!(test09, asset_hubs::asset_hub_rococo_development_config());
test!(test10, asset_hubs::asset_hub_wococo_development_config());
test!(test11, asset_hubs::asset_hub_rococo_local_config());
test!(test12, asset_hubs::asset_hub_wococo_local_config());
test!(test13, asset_hubs::asset_hub_rococo_genesis_config());
test!(test14, asset_hubs::asset_hub_wococo_genesis_config());
test!(test15, collectives::collectives_polkadot_development_config());
test!(test16, collectives::collectives_polkadot_local_config());
test!(test17, contracts::contracts_rococo_development_config());
test!(test18, contracts::contracts_rococo_local_config());
test!(test19, contracts::contracts_rococo_config());
test!(test20, glutton::glutton_development_config(667.into()));
test!(test21, glutton::glutton_local_config(667.into()));
test!(test22, glutton::glutton_config(667.into()));
test!(test23, penpal::get_penpal_chain_spec(667.into(), "test"));
test!(test24, rococo_parachain::rococo_parachain_local_config());
test!(test25, rococo_parachain::staging_rococo_parachain_local_config());
test!(test26, seedling::get_seedling_chain_spec());
test!(test27, shell::get_shell_chain_spec());
test!(
	test28,
	bridge_hubs::rococo::local_config(
		"bridge-hub-rococo-local",
		"Test",
		"test",
		667.into(),
		Some("Bob".to_string()),
		|_| {}
	)
);
test!(
	test29,
	bridge_hubs::wococo::local_config(
		"bridge-hub-wococo-local",
		"Test",
		"test",
		667.into(),
		Some("Bob".to_string())
	)
);
test!(
	test30,
	bridge_hubs::kusama::local_config("bridge-hub-kusama-local", "Test", "test", 667.into())
);
test!(
	test31,
	bridge_hubs::polkadot::local_config("bridge-hub-polkadot-local", "Test", "test", 667.into())
);

