#![allow(unused_variables)]

use crate::{weights::WeightInfo, Call, Config, PhantomData};
use alloc::vec::Vec;
use alloy::{
	primitives::IntoLogData,
	sol_types::{Revert, SolCall},
};
pub use pallet_revive::{precompiles::*, AddressMapper};
use sp_core::{H160, H256};
use sp_runtime::traits::StaticLookup;

alloy::sol!("src/precompiles/IERC20.sol");
use IERC20::{IERC20Events, *};

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

impl<Runtime, PrecompileConfig, Instance: 'static> Precompile
	for ERC20<Runtime, PrecompileConfig, Instance>
where
	PrecompileConfig: AssetPrecompileConfig,
	Runtime: crate::Config<Instance> + pallet_revive::Config,
	<<PrecompileConfig as AssetPrecompileConfig>::AssetIdExtractor as AssetIdExtractor>::AssetId:
		Into<<Runtime as Config<Instance>>::AssetId>,
	Call<Runtime, Instance>: Into<<Runtime as pallet_revive::Config>::RuntimeCall>,
	alloy::primitives::U256: From<<Runtime as Config<Instance>>::Balance>
		+ TryInto<<Runtime as Config<Instance>>::Balance>,
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
			IERC20Calls::transfer(call) => Self::transfer(asset_id, call, env),
			IERC20Calls::totalSupply(call) => Self::total_supply(asset_id, call, env),
			IERC20Calls::balanceOf(call) => Self::balance_of(asset_id, call, env),
			IERC20Calls::allowance(call) => Self::allowance(asset_id, call, env),
			IERC20Calls::approve(call) => Self::approve(asset_id, call, env),
			IERC20Calls::transferFrom(call) => Self::transfer_from(asset_id, call, env),
		}
	}
}

impl<Runtime, PrecompileConfig, Instance: 'static> ERC20<Runtime, PrecompileConfig, Instance>
where
	PrecompileConfig: AssetPrecompileConfig,
	Runtime: crate::Config<Instance> + pallet_revive::Config,
	<<PrecompileConfig as AssetPrecompileConfig>::AssetIdExtractor as AssetIdExtractor>::AssetId:
		Into<<Runtime as Config<Instance>>::AssetId>,
	Call<Runtime, Instance>: Into<<Runtime as pallet_revive::Config>::RuntimeCall>,
	alloy::primitives::U256: From<<Runtime as Config<Instance>>::Balance>
		+ TryInto<<Runtime as Config<Instance>>::Balance>,
{
	/// Get the caller as an `H160` address.
	fn caller(env: &mut impl ExtWithInfo<T = Runtime>) -> Result<H160, Error> {
		env.caller()
			.account_id()
			.map(<Runtime as pallet_revive::Config>::AddressMapper::to_address)
			.map_err(|_| Error::Revert(Revert { reason: "Invalid caller".into() }))
	}

	/// Convert a `U256` value to the balance type of the pallet.
	fn balance(
		value: alloy::primitives::U256,
	) -> Result<<Runtime as Config<Instance>>::Balance, Error> {
		value
			.try_into()
			.map_err(|_| Error::Revert(Revert { reason: "Balance conversion failed".into() }))
	}

	/// Call the runtime with the given call.
	fn call_runtime(
		env: &mut impl ExtWithInfo<T = Runtime>,
		call: Call<Runtime, Instance>,
	) -> Result<(), Error> {
		env.call_runtime(call.into()).map_err(|err| Error::Error(err.error.into()))?;
		Ok(())
	}

	/// Deposit an event to the runtime.
	fn deposit_event(env: &mut impl ExtWithInfo<T = Runtime>, event: IERC20Events) {
		let (topics, data) = event.into_log_data().split();
		let topics = topics.into_iter().map(|v| H256(v.0)).collect::<Vec<_>>();
		env.deposit_event(topics, data.to_vec());
	}

	/// Execute the transfer call.
	fn transfer(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &transferCall,
		env: &mut impl ExtWithInfo<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let from = Self::caller(env)?;
		let dest = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(
			&call.to.into_array().into(),
		);

		Self::call_runtime(
			env,
			Call::<Runtime, Instance>::transfer {
				id: asset_id.into(),
				target: <Runtime as frame_system::Config>::Lookup::unlookup(dest),
				amount: Self::balance(call.value.clone())?,
			},
		)?;

		Self::deposit_event(
			env,
			IERC20Events::Transfer(IERC20::Transfer {
				from: from.0.into(),
				to: call.to,
				value: call.value,
			}),
		);

		return Ok(transferCall::abi_encode_returns(&true));
	}

	/// Execute the total supply call.
	fn total_supply(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &totalSupplyCall,
		env: &mut impl ExtWithInfo<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		use frame_support::traits::fungibles::Inspect;
		env.charge(<Runtime as Config<Instance>>::WeightInfo::total_issuance())?;

		let value = crate::Pallet::<Runtime, Instance>::total_issuance(asset_id);
		return Ok(totalSupplyCall::abi_encode_returns(&value.into()));
	}

	/// Execute the balance_of call.
	fn balance_of(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &balanceOfCall,
		env: &mut impl ExtWithInfo<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as Config<Instance>>::WeightInfo::balance())?;
		let account = call.account.into_array().into();
		let account = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&account);
		let value = crate::Pallet::<Runtime, Instance>::balance(asset_id, account);
		return Ok(balanceOfCall::abi_encode_returns(&value.into()));
	}

	/// Execute the allowance call.
	fn allowance(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &allowanceCall,
		env: &mut impl ExtWithInfo<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as Config<Instance>>::WeightInfo::allowance())?;
		use frame_support::traits::fungibles::approvals::Inspect;
		let owner = call.owner.into_array().into();
		let owner = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&owner);

		let spender = call.spender.into_array().into();
		let spender = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&spender);

		let value = crate::Pallet::<Runtime, Instance>::allowance(asset_id, &owner, &spender);
		return Ok(balanceOfCall::abi_encode_returns(&value.into()));
	}

	/// Execute the approve call.
	fn approve(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &approveCall,
		env: &mut impl ExtWithInfo<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let owner = Self::caller(env)?;

		let spender = call.spender.into_array().into();
		let spender = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&spender);

		let value: <Runtime as Config<Instance>>::Balance = Self::balance(call.value)?;
		Self::call_runtime(
			env,
			Call::<Runtime, Instance>::approve_transfer {
				id: asset_id.into(),
				delegate: <Runtime as frame_system::Config>::Lookup::unlookup(spender),
				amount: value,
			},
		)?;

		Self::deposit_event(
			env,
			IERC20Events::Approval(IERC20::Approval {
				owner: owner.0.into(),
				spender: call.spender,
				value: call.value,
			}),
		);

		return Ok(approveCall::abi_encode_returns(&true));
	}

	/// Execute the transfer_from call.
	fn transfer_from(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &transferFromCall,
		env: &mut impl ExtWithInfo<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let from = call.from.into_array().into();
		let from = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&from);

		let to = call.to.into_array().into();
		let to = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&to);

		Self::call_runtime(
			env,
			Call::<Runtime, Instance>::transfer_approved {
				id: asset_id.into(),
				owner: <Runtime as frame_system::Config>::Lookup::unlookup(from),
				destination: <Runtime as frame_system::Config>::Lookup::unlookup(to),
				amount: Self::balance(call.value)?,
			},
		)?;

		Self::deposit_event(
			env,
			IERC20Events::Transfer(IERC20::Transfer {
				from: call.from,
				to: call.to,
				value: call.value,
			}),
		);

		return Ok(transferFromCall::abi_encode_returns(&true));
	}
}
