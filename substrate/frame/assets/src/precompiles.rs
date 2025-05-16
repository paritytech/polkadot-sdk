#![allow(unused_variables)]

use crate::{weights::WeightInfo, Call, Config, PhantomData, TransferFlags};
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
pub struct ERC20<Runtime, PrecompileConfig, Instance = ()> {
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
	alloy::primitives::U256: TryInto<<Runtime as Config<Instance>>::Balance>,

	// Note can't use From as it's not implemented for alloy::primitives::U256 for unsigned types
	alloy::primitives::U256: TryFrom<<Runtime as Config<Instance>>::Balance>,
{
	type T = Runtime;
	type Interface = IERC20::IERC20Calls;
	const MATCHER: AddressMatcher = PrecompileConfig::MATCHER;

	fn call(
		address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
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
	alloy::primitives::U256: TryInto<<Runtime as Config<Instance>>::Balance>,

	// Note can't use From as it's not implemented for alloy::primitives::U256 for unsigned types
	alloy::primitives::U256: TryFrom<<Runtime as Config<Instance>>::Balance>,
{
	/// Get the caller as an `H160` address.
	fn caller(env: &mut impl Ext<T = Runtime>) -> Result<H160, Error> {
		env.caller()
			.account_id()
			.map(<Runtime as pallet_revive::Config>::AddressMapper::to_address)
			.map_err(|_| Error::Revert(Revert { reason: "Invalid caller".into() }))
	}

	/// Convert a `U256` value to the balance type of the pallet.
	fn to_balance(
		value: alloy::primitives::U256,
	) -> Result<<Runtime as Config<Instance>>::Balance, Error> {
		value
			.try_into()
			.map_err(|_| Error::Revert(Revert { reason: "Balance conversion failed".into() }))
	}

	/// Convert a balance to a `U256` value.
	/// Note this is needed cause From is not implemented for unsigned integer types
	fn to_u256(
		value: <Runtime as Config<Instance>>::Balance,
	) -> Result<alloy::primitives::U256, Error> {
		Ok(alloy::primitives::U256::try_from(value)
			.map_err(|_| Error::Revert(Revert { reason: "Balance conversion failed".into() }))?)
	}

	/// Deposit an event to the runtime.
	fn deposit_event(env: &mut impl Ext<T = Runtime>, event: IERC20Events) {
		let (topics, data) = event.into_log_data().split();
		let topics = topics.into_iter().map(|v| H256(v.0)).collect::<Vec<_>>();
		env.deposit_event(topics, data.to_vec());
	}

	/// Execute the transfer call.
	fn transfer(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &transferCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as Config<Instance>>::WeightInfo::transfer())?;

		let from = Self::caller(env)?;
		let dest = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(
			&call.to.into_array().into(),
		);

		let f = TransferFlags { keep_alive: false, best_effort: false, burn_dust: false };
		crate::Pallet::<Runtime, Instance>::do_transfer(
			asset_id,
			&<Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&from),
			&dest,
			Self::to_balance(call.value.clone())?,
			None,
			f,
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
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		use frame_support::traits::fungibles::Inspect;
		env.charge(<Runtime as Config<Instance>>::WeightInfo::total_issuance())?;

		let value = Self::to_u256(crate::Pallet::<Runtime, Instance>::total_issuance(asset_id))?;
		return Ok(totalSupplyCall::abi_encode_returns(&value));
	}

	/// Execute the balance_of call.
	fn balance_of(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &balanceOfCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as Config<Instance>>::WeightInfo::balance())?;
		let account = call.account.into_array().into();
		let account = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&account);
		let value = Self::to_u256(crate::Pallet::<Runtime, Instance>::balance(asset_id, account))?;
		return Ok(balanceOfCall::abi_encode_returns(&value));
	}

	/// Execute the allowance call.
	fn allowance(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &allowanceCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<Runtime as Config<Instance>>::WeightInfo::allowance())?;
		use frame_support::traits::fungibles::approvals::Inspect;
		let owner = call.owner.into_array().into();
		let owner = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&owner);

		let spender = call.spender.into_array().into();
		let spender = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&spender);
		let value = Self::to_u256(crate::Pallet::<Runtime, Instance>::allowance(
			asset_id, &owner, &spender,
		))?;

		return Ok(balanceOfCall::abi_encode_returns(&value));
	}

	/// Execute the approve call.
	fn approve(
		asset_id: <Runtime as Config<Instance>>::AssetId,
		call: &approveCall,
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let owner = Self::caller(env)?;

		let spender = call.spender.into_array().into();
		let spender = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&spender);

		let value: <Runtime as Config<Instance>>::Balance = Self::to_balance(call.value)?;

		crate::Pallet::<Runtime, Instance>::do_approve_transfer(
			asset_id,
			&<Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&owner),
			&spender,
			Self::to_balance(call.value.clone())?,
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
		env: &mut impl Ext<T = Runtime>,
	) -> Result<Vec<u8>, Error> {
		let from = call.from.into_array().into();
		let from = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&from);

		let to = call.to.into_array().into();
		let to = <Runtime as pallet_revive::Config>::AddressMapper::to_account_id(&to);

		crate::Pallet::<Runtime, Instance>::do_transfer_approved(
			asset_id,
			&from,
			&to,
			Self::to_balance(call.value.clone())?,
		)?;

		Self::call_runtime(
			env,
			Call::<Runtime, Instance>::transfer_approved {
				id: asset_id.into(),
				owner: <Runtime as frame_system::Config>::Lookup::unlookup(from),
				destination: <Runtime as frame_system::Config>::Lookup::unlookup(to),
				amount: Self::to_balance(call.value)?,
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

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		mock::{new_test_ext, Assets, Balances, ERC20Config, RuntimeOrigin, System, Test},
		precompiles::alloy::hex,
	};
	use alloy::primitives::U256;
	use frame_support::{assert_ok, traits::Currency};
	use pallet_revive::DepositLimit;
	use sp_core::H160;
	use sp_runtime::Weight;

	#[test]
	fn asset_id_extractor_works() {
		let address: [u8; 20] =
			hex::const_decode_to_array(b"0000053900000000000000000000000001200000").unwrap();
		assert!(ERC20Config::MATCHER.matches(&address));
		assert_eq!(
			<ERC20Config as AssetPrecompileConfig>::AssetIdExtractor::asset_id_from_address(
				&address
			)
			.unwrap(),
			1337u32
		);
	}

	#[test]
	fn precompile_transfer_works() {
		new_test_ext().execute_with(|| {
			let asset_id = 0u32;
			let asset_addr =
				hex::const_decode_to_array(b"0000000000000000000000000000000001200000").unwrap();

			let owner = 1;
			let to = 2;

			Balances::make_free_balance_be(&owner, 100);
			Balances::make_free_balance_be(&to, 100);

			let to_addr = <Test as pallet_revive::Config>::AddressMapper::to_address(&to);
			assert_ok!(Assets::force_create(RuntimeOrigin::root(), asset_id, owner, true, 1));
			assert_ok!(Assets::mint(RuntimeOrigin::signed(owner), asset_id, owner, 100));

			System::reset_events();

			let data =
				IERC20::transferCall { to: to_addr.0.into(), value: U256::from(10) }.abi_encode();

			pallet_revive::Pallet::<Test>::bare_call(
				RuntimeOrigin::signed(1),
				H160::from(asset_addr),
				0u64,
				Weight::MAX,
				DepositLimit::Unchecked,
				data,
			);

			dbg!(System::events());
		});
	}
}
