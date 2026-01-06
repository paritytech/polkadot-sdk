use polkadot_sdk::pallet_revive::precompiles::Precompile;
use polkadot_sdk::pallet_revive::Config;
use polkadot_sdk::pallet_revive::precompiles::AddressMatcher;
use core::num::NonZeroU16;
use alloy_core::sol;

use ui_tests::runtime::Runtime;
use core::marker::PhantomData;

sol! {
	interface IPrecompileA {
		function callA() external;
	}
}

pub struct PrecompileA<T>(PhantomData<T>);

impl<T: Config> Precompile for PrecompileA<T> {
	type T = T;
	type Interface = IPrecompileA::IPrecompileACalls;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZeroU16::new(0x666).unwrap());
	const HAS_CONTRACT_INFO: bool = false;
}

sol! {
	interface IPrecompileB {
		function callB() external;
	}
}

pub struct PrecompileB<T>(PhantomData<T>);

impl<T: Config> Precompile for PrecompileB<T> {
	type T = T;
	type Interface = IPrecompileB::IPrecompileBCalls;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZeroU16::new(0x666).unwrap());
	const HAS_CONTRACT_INFO: bool = false;
}

const _: (PrecompileA<Runtime>, PrecompileB<Runtime>) = (PrecompileA(PhantomData::<Runtime>), PrecompileB(PhantomData::<Runtime>));

const _: () = polkadot_sdk::pallet_revive::precompiles::check_collision_for::<Runtime, (PrecompileA<Runtime>, PrecompileB<Runtime>)>();

fn main() {}
