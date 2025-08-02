// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
	weights::WeightInfo, ActiveEra, BalanceOf, Call, Config, Ledger, MinNominatorBond,
	MinValidatorBond, Nominators, Pallet, RewardDestination, Validators, ValidatorPrefs,
};
use alloc::vec::Vec;
use alloy_core as alloy;
use alloy_core::{
	primitives::IntoLogData,
	sol,
	sol_types::{Revert, SolCall},
};
use pallet_revive::precompiles::{
	AddressMatcher, Error, Ext, Precompile, RuntimeCosts, H256,
};
use sp_runtime::{traits::StaticLookup, DispatchError, Perbill};

sol! {
	/// The Staking precompile interface
	/// Provides access to basic staking functionality matching 1-1 with pallet extrinsics
	interface IStaking {
		/// Events

		/// When someone's bond is created or increased.
		event Bonded(address indexed stash, uint256 amount);
		/// When someone's bond is decreased.
		event Unbonded(address indexed stash, uint256 amount);
		/// When someone nominated a set of validators.
		event Nominated(address indexed stash, address[] targets);
		/// When someone's validator preferences are set.
		event ValidatorPrefsSet(address indexed validator, uint256 commission, bool blocked);
		/// When someone's role switched back to chilled.
		event Chilled(address indexed stash);
		/// When someone withdrew unbonded funds.
		event Withdrawn(address indexed stash, uint256 amount);
		/// When someone rebonded funds.
		event Rebonded(address indexed stash, uint256 amount);
		/// When rewards are paid out.
		event RewardsPaid(address indexed validator, uint256 era);

		/// Helper structs
		struct UnlockChunk {
			uint256 value;
			uint256 era;
		}

		/// State-changing functions (matching pallet calls)
		
		/// Bond tokens to become a staker. Payee: 0 = Staked, 1 = Stash, 2 = Account(caller), 3 = None
		function bond(uint256 value, uint8 payee) external returns (bool);
		
		/// Add more funds to an existing bond
		function bondExtra(uint256 maxAdditional) external returns (bool);
		
		/// Schedule a portion of the stash to be unlocked
		function unbond(uint256 value) external returns (bool);
		
		/// Withdraw unbonded funds after the unbonding period
		function withdrawUnbonded(uint32 numSlashingSpans) external returns (bool);
		
		/// Declare the desire to validate
		function validate(uint256 commission, bool blocked) external returns (bool);
		
		/// Declare the desire to nominate a set of validators
		function nominate(address[] calldata targets) external returns (bool);
		
		/// Stop validating or nominating
		function chill() external returns (bool);
		
		/// Rebond a portion of unbonded funds
		function rebond(uint256 value) external returns (bool);
		
		/// Pay out rewards for a validator in a specific era
		function payoutStakers(address validatorStash, uint256 era) external returns (bool);

		/// Query functions (basic storage reads)
		
		/// Get the staking ledger of an account
		function ledger(address stash) external view returns (
			uint256 total,
			uint256 active,
			UnlockChunk[] memory unlocking
		);
		
		/// Get nominator preferences
		function nominators(address nominator) external view returns (
			address[] memory targets,
			uint256 submittedIn,
			bool suppressed
		);
		
		/// Get validator preferences
		function validators(address validator) external view returns (
			uint256 commission,
			bool blocked
		);
		
		/// Get the current era index
		function currentEra() external view returns (uint256 era);
		
		/// Get minimum nominator bond
		function minNominatorBond() external view returns (uint256 amount);
		
		/// Get minimum validator bond
		function minValidatorBond() external view returns (uint256 amount);
	}
}

/// Staking precompile.
pub struct StakingPrecompile<T>(core::marker::PhantomData<T>);

impl<T> Precompile for StakingPrecompile<T>
where
	T: Config + pallet_revive::Config,
	T::AccountId: From<[u8; 20]> + Into<[u8; 20]>,
	BalanceOf<T>: TryFrom<alloy::primitives::U256> + Into<alloy::primitives::U256>,
	Call<T>: Into<<T as pallet_revive::Config>::RuntimeCall>,
{
	type T = T;
	type Interface = IStaking::IStakingCalls;
	const MATCHER: AddressMatcher =
		AddressMatcher::Fixed(core::num::NonZero::new(0x0800).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		match input {
			IStaking::IStakingCalls::bond(call) => Self::bond(call, env),
			IStaking::IStakingCalls::bondExtra(call) => Self::bond_extra(call, env),
			IStaking::IStakingCalls::unbond(call) => Self::unbond(call, env),
			IStaking::IStakingCalls::withdrawUnbonded(call) => Self::withdraw_unbonded(call, env),
			IStaking::IStakingCalls::nominate(call) => Self::nominate(call, env),
			IStaking::IStakingCalls::validate(call) => Self::validate(call, env),
			IStaking::IStakingCalls::chill(_) => Self::chill(env),
			IStaking::IStakingCalls::rebond(call) => Self::rebond(call, env),
			IStaking::IStakingCalls::payoutStakers(call) => Self::payout_stakers(call, env),
			// Query functions
			IStaking::IStakingCalls::ledger(call) => Self::ledger(call, env),
			IStaking::IStakingCalls::nominators(call) => Self::nominators(call, env),
			IStaking::IStakingCalls::validators(call) => Self::validators(call, env),
			IStaking::IStakingCalls::currentEra(_) => Self::current_era(env),
			IStaking::IStakingCalls::minNominatorBond(_) => Self::min_nominator_bond(env),
			IStaking::IStakingCalls::minValidatorBond(_) => Self::min_validator_bond(env),
		}
	}
}

const ERR_INVALID_CALLER: &str = "Invalid caller";
const ERR_BALANCE_CONVERSION_FAILED: &str = "Balance conversion failed";
const ERR_INVALID_PAYEE: &str = "Invalid payee type";
const ERR_COMMISSION_TOO_HIGH: &str = "Commission rate too high";

impl<T> StakingPrecompile<T>
where
	T: Config + pallet_revive::Config,
	T::AccountId: From<[u8; 20]> + Into<[u8; 20]>,
	BalanceOf<T>: TryFrom<alloy::primitives::U256> + Into<alloy::primitives::U256>,
	Call<T>: Into<<T as pallet_revive::Config>::RuntimeCall>,
{
	/// Get the caller as an AccountId.
	fn caller(env: &mut impl Ext<T = T>) -> Result<T::AccountId, Error> {
		match env.caller() {
			pallet_revive::Origin::Signed(account_id) => Ok(account_id),
			_ => Err(Error::Revert(Revert { reason: ERR_INVALID_CALLER.into() })),
		}
	}

	/// Convert a `U256` value to the balance type of the pallet.
	fn to_balance(value: alloy::primitives::U256) -> Result<BalanceOf<T>, Error> {
		value
			.try_into()
			.map_err(|_| Error::Revert(Revert { reason: ERR_BALANCE_CONVERSION_FAILED.into() }))
	}

	/// Convert a balance to a `U256` value.
	fn to_u256(value: BalanceOf<T>) -> Result<alloy::primitives::U256, Error> {
		Ok(value.into())
	}

	/// Convert an AccountId to an address.
	fn to_address(account: &T::AccountId) -> alloy::primitives::Address {
		let bytes: [u8; 20] = account.clone().into();
		alloy::primitives::Address::from(bytes)
	}

	/// Convert an address to an AccountId.
	fn to_account_id(address: &alloy::primitives::Address) -> T::AccountId {
		T::AccountId::from(address.into_array())
	}

	/// Deposit an event to the runtime.
	fn deposit_event(
		env: &mut impl Ext<T = T>,
		event: IStaking::IStakingEvents,
	) -> Result<(), Error> {
		let (topics, data) = event.into_log_data().split();
		let topics = topics.into_iter().map(|v| H256(v.0)).collect::<Vec<_>>();
		env.gas_meter_mut().charge(RuntimeCosts::DepositEvent {
			num_topic: topics.len() as u32,
			len: data.len() as u32,
		})?;
		env.deposit_event(topics, data.to_vec());
		Ok(())
	}

	/// Execute the bond call.
	fn bond(call: &IStaking::bondCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::bond())?;

		let stash = Self::caller(env)?;
		let value = Self::to_balance(call.value)?;

		// Convert payee type
		let payee = match call.payee {
			0 => RewardDestination::Staked,
			1 => RewardDestination::Stash,
			2 => RewardDestination::Account(stash.clone()),
			3 => RewardDestination::None,
			_ => return Err(Error::Revert(Revert { reason: ERR_INVALID_PAYEE.into() })),
		};

		// Call pallet function
		Pallet::<T>::bond(
			frame_system::RawOrigin::Signed(stash.clone()).into(),
			value,
			payee,
		)
		.map_err(|e| match e {
			DispatchError::Other(_) => Error::Revert(Revert { reason: "Bond failed".into() }),
			DispatchError::Module(module_error) => Error::Revert(Revert {
				reason: alloc::format!("Module error: {:?}", module_error).into(),
			}),
			_ => Error::Revert(Revert { reason: "Bond failed".into() }),
		})?;

		// Emit event
		Self::deposit_event(
			env,
			IStaking::IStakingEvents::Bonded(IStaking::Bonded {
				stash: Self::to_address(&stash),
				amount: call.value,
			}),
		)?;

		Ok(IStaking::bondCall::abi_encode_returns(&true))
	}

	/// Execute the bond_extra call.
	fn bond_extra(
		call: &IStaking::bondExtraCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::bond_extra())?;

		let stash = Self::caller(env)?;
		let max_additional = Self::to_balance(call.maxAdditional)?;

		// Call pallet function
		Pallet::<T>::bond_extra(
			frame_system::RawOrigin::Signed(stash.clone()).into(),
			max_additional,
		)
		.map_err(|_| Error::Revert(Revert { reason: "Bond extra failed".into() }))?;

		// Emit event
		Self::deposit_event(
			env,
			IStaking::IStakingEvents::Bonded(IStaking::Bonded {
				stash: Self::to_address(&stash),
				amount: call.maxAdditional,
			}),
		)?;

		Ok(IStaking::bondExtraCall::abi_encode_returns(&true))
	}

	/// Execute the unbond call.
	fn unbond(call: &IStaking::unbondCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::unbond())?;

		let stash = Self::caller(env)?;
		let value = Self::to_balance(call.value)?;

		// Call pallet function
		Pallet::<T>::unbond(frame_system::RawOrigin::Signed(stash.clone()).into(), value)
			.map_err(|_| Error::Revert(Revert { reason: "Unbond failed".into() }))?;

		// Emit event
		Self::deposit_event(
			env,
			IStaking::IStakingEvents::Unbonded(IStaking::Unbonded {
				stash: Self::to_address(&stash),
				amount: call.value,
			}),
		)?;

		Ok(IStaking::unbondCall::abi_encode_returns(&true))
	}

	/// Execute the withdraw_unbonded call.
	fn withdraw_unbonded(
		call: &IStaking::withdrawUnbondedCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::withdraw_unbonded_kill())?;

		let stash = Self::caller(env)?;

		// Call pallet function
		Pallet::<T>::withdraw_unbonded(
			frame_system::RawOrigin::Signed(stash.clone()).into(),
			call.numSlashingSpans,
		)
		.map_err(|_| Error::Revert(Revert { reason: "Withdraw unbonded failed".into() }))?;

		// Emit event (we don't know the exact amount withdrawn, so use 0)
		Self::deposit_event(
			env,
			IStaking::IStakingEvents::Withdrawn(IStaking::Withdrawn {
				stash: Self::to_address(&stash),
				amount: alloy::primitives::U256::ZERO,
			}),
		)?;

		Ok(IStaking::withdrawUnbondedCall::abi_encode_returns(&true))
	}

	/// Execute the nominate call.
	fn nominate(
		call: &IStaking::nominateCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::nominate(call.targets.len() as u32))?;

		let nominator = Self::caller(env)?;

		// Convert targets to AccountIds and then to lookups
		let targets: Result<Vec<T::AccountId>, Error> = call
			.targets
			.iter()
			.map(|addr| Ok(Self::to_account_id(addr)))
			.collect();
		let targets = targets?;
		
		// Convert to lookup sources
		let target_lookups: Vec<_> = targets
			.iter()
			.map(|account| <T as frame_system::Config>::Lookup::unlookup(account.clone()))
			.collect();

		// Call pallet function
		Pallet::<T>::nominate(
			frame_system::RawOrigin::Signed(nominator.clone()).into(),
			target_lookups,
		)
		.map_err(|_| Error::Revert(Revert { reason: "Nominate failed".into() }))?;

		// Emit event
		Self::deposit_event(
			env,
			IStaking::IStakingEvents::Nominated(IStaking::Nominated {
				stash: Self::to_address(&nominator),
				targets: call.targets.clone(),
			}),
		)?;

		Ok(IStaking::nominateCall::abi_encode_returns(&true))
	}

	/// Execute the validate call.
	fn validate(
		call: &IStaking::validateCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::validate())?;

		let validator = Self::caller(env)?;

		// Convert commission from U256 to Perbill
		// Commission is expected to be in parts per billion (10^9)
		let commission_value = call.commission.to::<u32>();
		let commission = if commission_value > 1_000_000_000u32 {
			return Err(Error::Revert(Revert { reason: ERR_COMMISSION_TOO_HIGH.into() }));
		} else {
			Perbill::from_parts(commission_value)
		};

		let prefs = ValidatorPrefs { commission, blocked: call.blocked };

		// Call pallet function
		Pallet::<T>::validate(frame_system::RawOrigin::Signed(validator.clone()).into(), prefs)
			.map_err(|_| Error::Revert(Revert { reason: "Validate failed".into() }))?;

		// Emit event
		Self::deposit_event(
			env,
			IStaking::IStakingEvents::ValidatorPrefsSet(IStaking::ValidatorPrefsSet {
				validator: Self::to_address(&validator),
				commission: call.commission,
				blocked: call.blocked,
			}),
		)?;

		Ok(IStaking::validateCall::abi_encode_returns(&true))
	}

	/// Execute the chill call.
	fn chill(env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::chill())?;

		let stash = Self::caller(env)?;

		// Call pallet function
		Pallet::<T>::chill(frame_system::RawOrigin::Signed(stash.clone()).into())
			.map_err(|_| Error::Revert(Revert { reason: "Chill failed".into() }))?;

		// Emit event
		Self::deposit_event(
			env,
			IStaking::IStakingEvents::Chilled(IStaking::Chilled {
				stash: Self::to_address(&stash),
			}),
		)?;

		Ok(IStaking::chillCall::abi_encode_returns(&true))
	}

	/// Execute the rebond call.
	fn rebond(call: &IStaking::rebondCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::rebond(1))?; // Approximate weight

		let stash = Self::caller(env)?;
		let value = Self::to_balance(call.value)?;

		// Call pallet function
		Pallet::<T>::rebond(frame_system::RawOrigin::Signed(stash.clone()).into(), value)
			.map_err(|_| Error::Revert(Revert { reason: "Rebond failed".into() }))?;

		// Emit event
		Self::deposit_event(
			env,
			IStaking::IStakingEvents::Rebonded(IStaking::Rebonded {
				stash: Self::to_address(&stash),
				amount: call.value,
			}),
		)?;

		Ok(IStaking::rebondCall::abi_encode_returns(&true))
	}

	/// Execute the payout_stakers call.
	fn payout_stakers(
		call: &IStaking::payoutStakersCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::payout_stakers_alive_staked(1))?; // Approximate weight

		let validator_stash = Self::to_account_id(&call.validatorStash);
		let era = call.era.to::<u32>();

		// Call pallet function
		Pallet::<T>::payout_stakers(
			frame_system::RawOrigin::Signed(Self::caller(env)?).into(),
			validator_stash.clone(),
			era,
		)
		.map_err(|_| Error::Revert(Revert { reason: "Payout stakers failed".into() }))?;

		// Emit event
		Self::deposit_event(
			env,
			IStaking::IStakingEvents::RewardsPaid(IStaking::RewardsPaid {
				validator: call.validatorStash,
				era: call.era,
			}),
		)?;

		Ok(IStaking::payoutStakersCall::abi_encode_returns(&true))
	}

	/// Execute the ledger query.
	fn ledger(call: &IStaking::ledgerCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		// Query operations are typically free, but we'll charge minimal weight
		env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		let stash = Self::to_account_id(&call.stash);

		if let Some(ledger) = Ledger::<T>::get(&stash) {
			let total = Self::to_u256(ledger.total)?;
			let active = Self::to_u256(ledger.active)?;

			let unlocking: Result<Vec<IStaking::UnlockChunk>, Error> = ledger
				.unlocking
				.iter()
				.map(|chunk| {
					Ok(IStaking::UnlockChunk {
						value: Self::to_u256(chunk.value)?,
						era: alloy::primitives::U256::from(chunk.era),
					})
				})
				.collect();

			Ok(IStaking::ledgerCall::abi_encode_returns(&IStaking::ledgerReturn {
				total,
				active,
				unlocking: unlocking?,
			}))
		} else {
			// Return empty ledger for non-stakers
			Ok(IStaking::ledgerCall::abi_encode_returns(&IStaking::ledgerReturn {
				total: alloy::primitives::U256::ZERO,
				active: alloy::primitives::U256::ZERO,
				unlocking: Vec::<IStaking::UnlockChunk>::new(),
			}))
		}
	}

	/// Execute the nominators query.
	fn nominators(
		call: &IStaking::nominatorsCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		let nominator = Self::to_account_id(&call.nominator);

		if let Some(nominations) = Nominators::<T>::get(&nominator) {
			let targets: Vec<alloy::primitives::Address> =
				nominations.targets.iter().map(|acc| Self::to_address(acc)).collect();

			Ok(IStaking::nominatorsCall::abi_encode_returns(&IStaking::nominatorsReturn {
				targets,
				submittedIn: alloy::primitives::U256::from(nominations.submitted_in),
				suppressed: nominations.suppressed,
			}))
		} else {
			// Return empty nominations for non-nominators
			Ok(IStaking::nominatorsCall::abi_encode_returns(&IStaking::nominatorsReturn {
				targets: Vec::<alloy::primitives::Address>::new(),
				submittedIn: alloy::primitives::U256::ZERO,
				suppressed: false,
			}))
		}
	}

	/// Execute the validators query.
	fn validators(
		call: &IStaking::validatorsCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		let validator = Self::to_account_id(&call.validator);

		let prefs = Validators::<T>::get(&validator);
		let commission = alloy::primitives::U256::from(prefs.commission.deconstruct());

		Ok(IStaking::validatorsCall::abi_encode_returns(&IStaking::validatorsReturn {
			commission,
			blocked: prefs.blocked,
		}))
	}

	/// Execute the current_era query.
	fn current_era(env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		let era = ActiveEra::<T>::get()
			.map(|info| info.index)
			.unwrap_or_default();

		Ok(IStaking::currentEraCall::abi_encode_returns(&alloy::primitives::U256::from(era)))
	}


	/// Execute the min_nominator_bond query.
	fn min_nominator_bond(env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		let min_bond = MinNominatorBond::<T>::get();
		let amount = Self::to_u256(min_bond)?;

		Ok(IStaking::minNominatorBondCall::abi_encode_returns(&amount))
	}

	/// Execute the min_validator_bond query.
	fn min_validator_bond(env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		let min_bond = MinValidatorBond::<T>::get();
		let amount = Self::to_u256(min_bond)?;

		Ok(IStaking::minValidatorBondCall::abi_encode_returns(&amount))
	}
}
