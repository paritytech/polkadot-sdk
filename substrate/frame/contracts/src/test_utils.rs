use frame_support::weights::Weight;
pub use sp_runtime::AccountId32;

pub const ALICE: AccountId32 = AccountId32::new([1u8; 32]);
pub const BOB: AccountId32 = AccountId32::new([2u8; 32]);
pub const CHARLIE: AccountId32 = AccountId32::new([3u8; 32]);
pub const DJANGO: AccountId32 = AccountId32::new([4u8; 32]);

pub const GAS_LIMIT: Weight = Weight::from_parts(100_000_000_000, 3 * 1024 * 1024);
pub mod builder;
