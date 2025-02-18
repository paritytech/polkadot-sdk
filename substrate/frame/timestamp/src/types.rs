use codec::*;
use frame_support::{DefaultNoBound, OrdNoBound, PartialOrdNoBound, PartialEqNoBound, EqNoBound, CloneNoBound, RuntimeDebugNoBound};
// use crate::Config;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use my_macros::stored;

// #[derive(
// 	Default,
// 	Ord,
// 	PartialOrd,
// 	PartialEq,
// 	Eq,
// 	Clone,
// 	Encode,
// 	Decode,
// 	RuntimeDebug,
// 	TypeInfo,
// 	MaxEncodedLen,
// )]
#[derive(
	DefaultNoBound,
	OrdNoBound,
	PartialOrdNoBound,
	PartialEqNoBound,
	EqNoBound,
	CloneNoBound,
	Encode,
	Decode,
	RuntimeDebugNoBound,
	TypeInfo,
	MaxEncodedLen,
)]
// #[derive(CloneNoBound)]
#[scale_info(skip_type_params(T))]
// #[stored]
pub struct TestType<T: crate::Config>
{
	pub id: u32,
	pub good: bool,
	pub generic: BlockNumberFor<T>,
	// pub generic_two: U,
	// pub generic: T,
}
