use codec::*;
use frame_support::{DefaultNoBound, Stored};
// use crate::Config;
// use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::BlockNumberFor;
use scale_info::TypeInfo;

// #[derive(MaxEncodedLen, Encode, Decode, DefaultNoBound, TypeInfo)]
// #[scale_info(skip_type_params(T, U))]
#[derive(Stored)]
pub struct TestType<T: crate::Config> {
    pub id: u32,
    pub good: bool,
    pub generic: BlockNumberFor<T>,
}

// So basically there's only the derives, which are necessary for the functions that are going to be called
// in the pallet upon expansion, and then the param skipping functionality for each of those derives that
// make sense. You want to skip when its crate::Config bound but more generally when T doesn't make sense to
// bind. So therefore when making this uber derive, I'll basically just have all the traits and change them
// to nobound and skip_type if its being bound? Or perhaps you decide which ones to skip. I'm sure they'll
// let me know.

// yeah so I think this covers all possibilities nicely, or at least nice enough to ship

// If the second isn't bound it doesn't skip it, good. If it is bound it does skip it. Good
// For default, it skips both automatically, typically you'd want the not bound one bound by default
// So if choose not to skip a generic that isn't bound by Config, you'll have to add that bound to the generic.
// So this will have some sort of spice to it.