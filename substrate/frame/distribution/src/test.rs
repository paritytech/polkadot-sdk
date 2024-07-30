pub use super::*;
use crate::mock::*;
use frame_support::{assert_noop, assert_ok};
use sp_runtime::AccountId32;

pub fn next_block() {
	System::set_block_number(System::block_number() + 1);
	AllPalletsWithSystem::on_initialize(System::block_number());
}

pub fn run_to_block(n: BlockNumberFor<Test>) {
	while System::block_number() < n {
		if System::block_number() > 1 {
			AllPalletsWithSystem::on_finalize(System::block_number());
		}
		next_block();
	}
}
