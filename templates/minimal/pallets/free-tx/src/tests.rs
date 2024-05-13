use crate::{mock::*, *};
use frame_support::{assert_noop, assert_ok};

#[test]
fn it_works() {
	new_test_ext().execute_with(|| {
		// Go past genesis block so events get deposited
		System::set_block_number(1);
		// Dispatch a signed extrinsic.
		assert_ok!(FreeTx::free_tx(RuntimeOrigin::signed(1), true));
		// Assert that the correct event was deposited
		System::assert_last_event(Event::TxSuccess.into());
		// Check error case
		assert_noop!(FreeTx::free_tx(RuntimeOrigin::signed(1), false), Error::<Test>::TxFailed);
	});
}
