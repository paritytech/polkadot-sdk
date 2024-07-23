use super::*;
use primitives::error::{
	ArithmeticError::*,
	Error::{self, *},
	TokenError::*,
};

#[test]
fn total_supply_works() {
	new_test_ext().execute_with(|| {
		let _ = env_logger::try_init();
		let addr = instantiate("./smart-contract/target/ink/reentrancy.wasm", INIT_VALUE, vec![]);

	});
}

