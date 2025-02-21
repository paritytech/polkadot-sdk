use codec::*;
use frame_support::{DefaultNoBound, OrdNoBound, PartialOrdNoBound, PartialEqNoBound, EqNoBound, CloneNoBound, RuntimeDebugNoBound};
// use crate::Config;
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;
use my_macros::stored;
use serde::{Serialize, Deserialize};

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
// #[derive(
// 	DefaultNoBound,
// 	OrdNoBound,
// 	PartialOrdNoBound,
// 	PartialEqNoBound,
// 	EqNoBound,
// 	CloneNoBound,
// 	Encode,
// 	Decode,
// 	RuntimeDebugNoBound,
// 	TypeInfo,
// 	MaxEncodedLen,
// )]
#[derive(Serialize)]
#[serde(bound(serialize = "T: SerializeMeBaby"))]
pub struct Testie<T>
{
	pub generic: T,
}
// #[scale_info(skip_type_params(T))]
#[stored]
// pub struct TestType<T: ore::fmt::Debug>
pub struct TestType<T, U: crate::Config, V>
{
	pub id: u32,
	pub good: bool,
	pub generic: BlockNumberFor<U>,
	pub generic_two: T,
	pub generic_three: V,
	// pub generic: T,
}

//Okay! I think I've got it. Can't remember if there was anything else but I don't think so.
