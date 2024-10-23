use crate::{pallet as ahm_controller, Role};

use frame::testing_prelude::*;

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		AhmController: ahm_controller,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = MockBlock<Self>;
}

parameter_types! {
	pub const OurRole: Role = Role::Relay;
}

impl ahm_controller::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Role = OurRole;
}
