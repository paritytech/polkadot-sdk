use frame_support::weights::Weight;
use xcm_procedural::{
	impl_xcm_fungible_weight_info_provider, impl_xcm_generic_weight_info_provider,
};

extern crate self as pallet_xcm_benchmarks;

pub mod xcm_weights {
	use frame_support::weights::Weight;

	pub trait XcmGenericWeightInfo {
		fn query_response() -> Weight;
		fn transact() -> Weight;
		fn clear_origin() -> Weight;
		fn descend_origin() -> Weight;
		fn report_error() -> Weight;
		fn report_holding() -> Weight;
		fn buy_execution() -> Weight;
		fn pay_fees() -> Weight;
		fn refund_surplus() -> Weight;
		fn set_error_handler() -> Weight;
		fn set_appendix() -> Weight;
		fn clear_error() -> Weight;
		fn asset_claimer() -> Weight;
		fn claim_asset() -> Weight;
		fn trap() -> Weight;
		fn subscribe_version() -> Weight;
		fn unsubscribe_version() -> Weight;
		fn burn_asset() -> Weight;
		fn expect_asset() -> Weight;
		fn expect_origin() -> Weight;
		fn expect_error() -> Weight;
		fn expect_transact_status() -> Weight;
		fn query_pallet() -> Weight;
		fn expect_pallet() -> Weight;
		fn report_transact_status() -> Weight;
		fn clear_transact_status() -> Weight;
		fn universal_origin() -> Weight;
		fn set_fees_mode() -> Weight;
		fn set_topic() -> Weight;
		fn clear_topic() -> Weight;
		fn alias_origin() -> Weight;
		fn unpaid_execution() -> Weight;
		fn execute_with_origin() -> Weight;
		fn exchange_asset() -> Weight;
	}

	pub trait XcmFungibleWeightInfo {
		fn withdraw_asset() -> Weight;
		fn reserve_asset_deposited() -> Weight;
		fn receive_teleported_asset() -> Weight;
		fn transfer_asset() -> Weight;
		fn transfer_reserve_asset() -> Weight;
		fn deposit_asset() -> Weight;
		fn deposit_reserve_asset() -> Weight;
		fn initiate_reserve_withdraw() -> Weight;
		fn initiate_teleport() -> Weight;
		fn initiate_transfer() -> Weight;
	}
}

struct DummyWeightInfo;

impl DummyWeightInfo {
	fn query_response() -> Weight {
		Weight::from_parts(1, 0)
	}
	fn transact() -> Weight {
		Weight::from_parts(2, 0)
	}
	fn clear_origin() -> Weight {
		Weight::from_parts(3, 0)
	}
	fn descend_origin() -> Weight {
		Weight::from_parts(4, 0)
	}
	fn report_error() -> Weight {
		Weight::from_parts(5, 0)
	}
	fn report_holding() -> Weight {
		Weight::from_parts(6, 0)
	}
	fn buy_execution() -> Weight {
		Weight::from_parts(7, 0)
	}
	fn pay_fees() -> Weight {
		Weight::from_parts(8, 0)
	}
	fn refund_surplus() -> Weight {
		Weight::from_parts(9, 0)
	}
	fn set_error_handler() -> Weight {
		Weight::from_parts(10, 0)
	}
	fn set_appendix() -> Weight {
		Weight::from_parts(11, 0)
	}
	fn clear_error() -> Weight {
		Weight::from_parts(12, 0)
	}
	fn asset_claimer() -> Weight {
		Weight::from_parts(13, 0)
	}
	fn claim_asset() -> Weight {
		Weight::from_parts(14, 0)
	}
	fn trap() -> Weight {
		Weight::from_parts(15, 0)
	}
	fn subscribe_version() -> Weight {
		Weight::from_parts(16, 0)
	}
	fn unsubscribe_version() -> Weight {
		Weight::from_parts(17, 0)
	}
	fn burn_asset() -> Weight {
		Weight::from_parts(18, 0)
	}
	fn expect_asset() -> Weight {
		Weight::from_parts(19, 0)
	}
	fn expect_origin() -> Weight {
		Weight::from_parts(20, 0)
	}
	fn expect_error() -> Weight {
		Weight::from_parts(21, 0)
	}
	fn expect_transact_status() -> Weight {
		Weight::from_parts(22, 0)
	}
	fn query_pallet() -> Weight {
		Weight::from_parts(23, 0)
	}
	fn expect_pallet() -> Weight {
		Weight::from_parts(24, 0)
	}
	fn report_transact_status() -> Weight {
		Weight::from_parts(25, 0)
	}
	fn clear_transact_status() -> Weight {
		Weight::from_parts(26, 0)
	}
	fn universal_origin() -> Weight {
		Weight::from_parts(27, 0)
	}
	fn set_fees_mode() -> Weight {
		Weight::from_parts(28, 0)
	}
	fn set_topic() -> Weight {
		Weight::from_parts(29, 0)
	}
	fn clear_topic() -> Weight {
		Weight::from_parts(30, 0)
	}
	fn alias_origin() -> Weight {
		Weight::from_parts(31, 0)
	}
	fn unpaid_execution() -> Weight {
		Weight::from_parts(32, 0)
	}
	fn execute_with_origin() -> Weight {
		Weight::from_parts(33, 0)
	}
	fn exchange_asset() -> Weight {
		Weight::from_parts(34, 0)
	}

	fn withdraw_asset() -> Weight {
		Weight::from_parts(41, 0)
	}
	fn reserve_asset_deposited() -> Weight {
		Weight::from_parts(42, 0)
	}
	fn receive_teleported_asset() -> Weight {
		Weight::from_parts(43, 0)
	}
	fn transfer_asset() -> Weight {
		Weight::from_parts(44, 0)
	}
	fn transfer_reserve_asset() -> Weight {
		Weight::from_parts(45, 0)
	}
	fn deposit_asset() -> Weight {
		Weight::from_parts(46, 0)
	}
	fn deposit_reserve_asset() -> Weight {
		Weight::from_parts(47, 0)
	}
	fn initiate_reserve_withdraw() -> Weight {
		Weight::from_parts(48, 0)
	}
	fn initiate_teleport() -> Weight {
		Weight::from_parts(49, 0)
	}
	fn initiate_transfer() -> Weight {
		Weight::from_parts(50, 0)
	}
}

impl_xcm_generic_weight_info_provider!(DummyWeightInfo);
impl_xcm_fungible_weight_info_provider!(DummyWeightInfo);

#[test]
fn generic_provider_works() {
	let w = <DummyWeightInfo as pallet_xcm_benchmarks::xcm_weights::XcmGenericWeightInfo>::
		exchange_asset();
	assert_eq!(w, Weight::from_parts(34, 0));
}

#[test]
fn fungible_provider_works() {
	let w = <DummyWeightInfo as pallet_xcm_benchmarks::xcm_weights::XcmFungibleWeightInfo>::
		initiate_transfer();
	assert_eq!(w, Weight::from_parts(50, 0));
}
