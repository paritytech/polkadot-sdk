#![allow(unused_imports)]

use crate::{Call, Config, PhantomData};
use alloc::vec::Vec;
use alloy::sol_types::{Revert, SolValue};
use core::num::NonZero;
pub use pallet_revive::{precompiles::*, AddressMapper};
use sp_runtime::traits::{Get, StaticLookup};
alloy::sol!("src/precompiles/IERC20.sol");

/// Mean of extracting the asset id from the precompile address.
pub trait AssetIdExtractor {
	type AssetId;
	/// Extracts the asset id from the address.
	fn asset_id_from_address(address: &[u8; 20]) -> Result<Self::AssetId, Error>;
}

/// The configuration of a pallet-assets precompile.
pub trait AssetPrecompileConfig {
	/// The Address matcher used by the precompile.
	const MATCHER: AddressMatcher;

	/// The [`AssetIdExtractor`] used by the precompile.
	type AssetIdExtractor: AssetIdExtractor;
}

/// An `AssetIdExtractor` that decode the asset id from the address.
pub struct AddressAssetIdExtractor;

impl AssetIdExtractor for AddressAssetIdExtractor {
	type AssetId = u32;
	fn asset_id_from_address(addr: &[u8; 20]) -> Result<Self::AssetId, Error> {
		let bytes: [u8; 4] = addr[0..4].try_into().expect("slice is 4 bytes; qed");
		let index = u32::from_be_bytes(bytes);
		return Ok(index.into());
	}
}

#[test]
fn asset_id_extractor_works() {
	use crate::{make_precompile_assets_config, precompiles::alloy::hex};
	make_precompile_assets_config!(TestConfig, 0x0120);

	let address: [u8; 20] =
		hex::const_decode_to_array(b"0000053900000000000000000000000001200000").unwrap();
	assert!(TestConfig::MATCHER.matches(&address));
	assert_eq!(
		<TestConfig as AssetPrecompileConfig>::AssetIdExtractor::asset_id_from_address(&address)
			.unwrap(),
		1337u32
	);
}

/// A macro to generate an `AssetPrecompileConfig` implementation for a given name prefix.
#[macro_export]
macro_rules! make_precompile_assets_config {
	($name:ident, $prefix:literal) => {
		pub struct $name;
		impl $crate::precompiles::AssetPrecompileConfig for $name {
			const MATCHER: $crate::precompiles::AddressMatcher =
				$crate::precompiles::AddressMatcher::Prefix(
					core::num::NonZero::new($prefix).unwrap(),
				);
			type AssetIdExtractor = $crate::precompiles::AddressAssetIdExtractor;
		}
	};
}

/// An ERC20 precompile.
pub struct ERC20<Runtime, PrecompileConfig, Instance> {
	_phantom: PhantomData<(Runtime, PrecompileConfig, Instance)>,
}

const BALANCE_CONVERSION_FAILED: &str = "Balance conversion failed";

impl<Runtime, PrecompileConfig, Instance: 'static> Precompile
	for ERC20<Runtime, PrecompileConfig, Instance>
where
	PrecompileConfig: AssetPrecompileConfig,
	Runtime: crate::Config<Instance> + pallet_revive::Config,
	<<PrecompileConfig as AssetPrecompileConfig>::AssetIdExtractor as AssetIdExtractor>::AssetId:
		Into<<Runtime as Config<Instance>>::AssetId>,
	Call<Runtime, Instance>: Into<<Runtime as pallet_revive::Config>::RuntimeCall>,
	alloy::primitives::U256: TryInto<<Runtime as Config<Instance>>::Balance>,
{
	type T = Runtime;
	type Interface = IERC20::IERC20Calls;
	const MATCHER: AddressMatcher = PrecompileConfig::MATCHER;

	fn call_with_info(
		address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl ExtWithInfo<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		use IERC20::*;

		let asset_id = PrecompileConfig::AssetIdExtractor::asset_id_from_address(address)?.into();

		match input {
			IERC20Calls::transfer(transferCall { to, value }) => {
				// Are we ok using te 0 address for root call?
				let from = env
					.caller()
					.account_id()
					.map(<Self::T as pallet_revive::Config>::AddressMapper::to_address)
					.unwrap_or_default();

				let dest = to.into_array().into();
				let dest = <Self::T as pallet_revive::Config>::AddressMapper::to_account_id(&dest);
				let amount: <Runtime as Config<Instance>>::Balance =
					value.clone().try_into().map_err(|_| {
						Error::Revert(Revert { reason: BALANCE_CONVERSION_FAILED.into() })
					})?;

				let call: <Self::T as pallet_revive::Config>::RuntimeCall =
					Call::<Runtime, Instance>::transfer {
						id: asset_id.into(),
						target: <Self::T as frame_system::Config>::Lookup::unlookup(dest),
						amount,
					}
					.into();

				env.call_runtime(call).map_err(|err| Error::Error(err.error.into()))?;
				let ret: (bool,) = transferFromReturn { _0: true }.into();

				// dispatch event
				let _ev = IERC20::IERC20Events::Transfer(IERC20::Transfer {
					from: from.0.into(),
					to: *to,
					value: *value,
				});

				// fn deposit_event(&mut self, topics: Vec<H256>, data: Vec<u8>);
				// env.deposit_event(ev);

				return Ok(ret.abi_encode());
			},
			IERC20Calls::totalSupply(totalSupplyCall { .. }) => {},
			IERC20Calls::balanceOf(balanceOfCall { .. }) => {},
			IERC20Calls::allowance(allowanceCall { .. }) => {},
			IERC20Calls::approve(approveCall { .. }) => {},
			IERC20Calls::transferFrom(transferFromCall { .. }) => {},
		}

		Ok(Default::default())
	}
}
