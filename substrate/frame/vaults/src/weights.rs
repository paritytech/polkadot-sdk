//! Placeholder weight info. Real benchmarks land in a follow-up.

use frame::deps::frame_support::weights::Weight;

pub trait WeightInfo {
	fn open_vault() -> Weight;
	fn deposit_collateral_for() -> Weight;
	fn withdraw_collateral() -> Weight;
	fn borrow() -> Weight;
	fn repay_for() -> Weight;
	fn change_rate() -> Weight;
	fn close_vault() -> Weight;
	fn poke() -> Weight;
	fn enter_final_recovery() -> Weight;
	fn exit_final_recovery() -> Weight;
	fn register_branch() -> Weight;
	fn set_param() -> Weight;
	fn enable_frozen_mode() -> Weight;
	fn on_idle_one_vault() -> Weight;
}

impl WeightInfo for () {
	fn open_vault() -> Weight {
		Weight::zero()
	}
	fn deposit_collateral_for() -> Weight {
		Weight::zero()
	}
	fn withdraw_collateral() -> Weight {
		Weight::zero()
	}
	fn borrow() -> Weight {
		Weight::zero()
	}
	fn repay_for() -> Weight {
		Weight::zero()
	}
	fn change_rate() -> Weight {
		Weight::zero()
	}
	fn close_vault() -> Weight {
		Weight::zero()
	}
	fn poke() -> Weight {
		Weight::zero()
	}
	fn enter_final_recovery() -> Weight {
		Weight::zero()
	}
	fn exit_final_recovery() -> Weight {
		Weight::zero()
	}
	fn register_branch() -> Weight {
		Weight::zero()
	}
	fn set_param() -> Weight {
		Weight::zero()
	}
	fn enable_frozen_mode() -> Weight {
		Weight::zero()
	}
	fn on_idle_one_vault() -> Weight {
		Weight::zero()
	}
}
