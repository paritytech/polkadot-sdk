use polkadot_sdk::pallet_revive::precompiles::PrimitivePrecompile;
use polkadot_sdk::pallet_revive::Config;
use polkadot_sdk::pallet_revive::precompiles::BuiltinAddressMatcher;
use core::num::NonZero;

use ui_tests::runtime::Runtime;
use core::marker::PhantomData;

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

const _: (PrecompileA<Runtime>, PrecompileB<Runtime>) = (PrecompileA(PhantomData::<Runtime>), PrecompileB(PhantomData::<Runtime>));

fn main() {}
