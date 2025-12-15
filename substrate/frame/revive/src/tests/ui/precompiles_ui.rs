use core::marker::PhantomData;

use pallet_revive::Config;
use pallet_revive::precompiles::{BuiltinAddressMatcher, PrimitivePrecompile};
use core::num::NonZero;

pub struct PrecompileA<T>(PhantomData<T>);

impl<T: Config> PrimitivePrecompile for PrecompileA<T> {
	type T = T;
	const MATCHER: BuiltinAddressMatcher = BuiltinAddressMatcher::Fixed(NonZero::new(0x666).unwrap());
	const HAS_CONTRACT_INFO: bool = false;
}

pub struct PrecompileB<T>(PhantomData<T>);

impl<T: Config> PrimitivePrecompile for PrecompileB<T> {
	type T = T;
	const MATCHER: BuiltinAddressMatcher = BuiltinAddressMatcher::Fixed(NonZero::new(0x666).unwrap());
	const HAS_CONTRACT_INFO: bool = false;
}

fn main() {}
