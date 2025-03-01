use crate::ah::mock::AccountId;
use codec::Decode;
use frame::traits::TrailingZeroInput;

/// Convert a number to a 32 byte account id.
pub fn acc(who: u32) -> AccountId {
	<AccountId as Decode>::decode(&mut TrailingZeroInput::new(who.to_le_bytes().as_ref())).unwrap()
}
