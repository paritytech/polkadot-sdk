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
	/**
	 * @title IStaking - Substrate Staking Precompile Interface
	 * @notice Provides smart contract access to Substrate's native staking functionality
	 * @dev This interface maps 1-to-1 with pallet-staking-async extrinsics, enabling
	 *      Ethereum-compatible contracts to interact with Substrate's Nominated Proof of Stake
	 */
	interface IStaking {
		// ═══════════════════════════════════════════════════════════════════════════════════════
		//                                        EVENTS
		// ═══════════════════════════════════════════════════════════════════════════════════════

		/**
		 * @notice Emitted when tokens are bonded to participate in staking
		 * @param stash The stash account that bonded the tokens
		 * @param amount The amount of tokens bonded (in smallest unit)
		 */
		event Bonded(address indexed stash, uint256 amount);

		/**
		 * @notice Emitted when bonded tokens are scheduled for unbonding
		 * @param stash The stash account that unbonded tokens
		 * @param amount The amount of tokens scheduled for unbonding (in smallest unit)
		 */
		event Unbonded(address indexed stash, uint256 amount);

		/**
		 * @notice Emitted when a nominator selects validators to support
		 * @param stash The nominator's stash account
		 * @param targets Array of validator addresses being nominated
		 */
		event Nominated(address indexed stash, address[] targets);

		/**
		 * @notice Emitted when validator preferences are updated
		 * @param validator The validator's stash account
		 * @param commission The commission rate (parts per billion: 0-1,000,000,000)
		 * @param blocked Whether the validator is blocked from receiving nominations
		 */
		event ValidatorPrefsSet(address indexed validator, uint256 commission, bool blocked);

		/**
		 * @notice Emitted when an account stops validating or nominating
		 * @param stash The stash account that became inactive
		 */
		event Chilled(address indexed stash);

		/**
		 * @notice Emitted when unbonded tokens are withdrawn to free balance
		 * @param stash The stash account that withdrew tokens
		 * @param amount The amount withdrawn (in smallest unit)
		 */
		event Withdrawn(address indexed stash, uint256 amount);

		/**
		 * @notice Emitted when previously unbonded tokens are re-bonded
		 * @param stash The stash account that rebonded tokens
		 * @param amount The amount rebonded (in smallest unit)
		 */
		event Rebonded(address indexed stash, uint256 amount);

		/**
		 * @notice Emitted when staking rewards are distributed
		 * @param validator The validator for which rewards were paid
		 * @param era The era for which rewards were distributed
		 */
		event RewardsPaid(address indexed validator, uint256 era);

		// ═══════════════════════════════════════════════════════════════════════════════════════
		//                                      DATA STRUCTURES
		// ═══════════════════════════════════════════════════════════════════════════════════════

		/**
		 * @notice Represents a chunk of tokens scheduled for unlocking
		 * @param value Amount of tokens in this unlock chunk (in smallest unit)
		 * @param era Era number when these tokens can be withdrawn
		 */
		struct UnlockChunk {
			uint256 value;
			uint256 era;
		}

		// ═══════════════════════════════════════════════════════════════════════════════════════
		//                                   STATE-CHANGING FUNCTIONS
		// ═══════════════════════════════════════════════════════════════════════════════════════


		/**
		 * @notice Bond tokens to participate in staking
		 * @dev Creates a new staking ledger or adds to existing one. Requires sufficient free balance.
		 * @param value Amount of tokens to bond (in smallest unit)
		 * @param payee Reward destination: 0=Staked, 1=Stash, 2=Account(caller), 3=None
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Bonded(stash, actualAmount), creates/updates staking ledger
		 *   REVERT: If insufficient balance, invalid payee, already bonded, or below minimum bond
		 * @custom:requirements
		 *   - Caller must have sufficient free balance >= value
		 *   - Value must meet minimum bond requirements (see minNominatorBond/minValidatorBond)
		 *   - Account must not already be bonded as stash or controller
		 *   - Payee must be valid (0-3)
		 * @custom:events Bonded(address indexed stash, uint256 amount)
		 */
		function bond(uint256 value, uint8 payee) external returns (bool success);

		/**
		 * @notice Add more tokens to an existing bond
		 * @dev Increases the total bonded amount for the caller's stash account
		 * @param maxAdditional Maximum additional tokens to bond (actual amount may be less if insufficient balance)
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Bonded(stash, actualAmount), increases bonded amount
		 *   REVERT: If no existing bond, insufficient free balance, or account restricted
		 * @custom:requirements
		 *   - Caller must already have an active staking ledger
		 *   - Caller must have sufficient free balance
		 * @custom:events Bonded(address indexed stash, uint256 amount)
		 */
		function bondExtra(uint256 maxAdditional) external returns (bool success);

		/**
		 * @notice Schedule bonded tokens for unbonding
		 * @dev Tokens become available for withdrawal after the unbonding period
		 * @param value Amount of bonded tokens to schedule for unbonding (in smallest unit)
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Unbonded(stash, amount), creates unbonding chunk
		 *   REVERT: If insufficient bonded tokens, would leave below minimum if active, or too many chunks
		 * @custom:requirements
		 *   - Caller must have sufficient bonded tokens >= value
		 *   - Cannot unbond below minimum active stake if actively validating/nominating
		 *   - Limited number of concurrent unbonding chunks allowed (typically 32)
		 * @custom:events Unbonded(address indexed stash, uint256 amount)
		 */
		function unbond(uint256 value) external returns (bool success);

		/**
		 * @notice Withdraw tokens that have finished unbonding
		 * @dev Moves fully unbonded tokens from staking ledger to free balance
		 * @param numSlashingSpans Number of slashing spans to process (affects weight calculation)
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Withdrawn(stash, amount), removes completed chunks
		 *   REVERT: If no staking ledger exists or account doesn't exist
		 * @custom:requirements
		 *   - Account must exist and have a staking ledger
		 *   - Will process all chunks that have completed unbonding period
		 * @custom:events Withdrawn(address indexed stash, uint256 amount)
		 */
		function withdrawUnbonded(uint32 numSlashingSpans) external returns (bool success);

		/**
		 * @notice Declare intention to validate blocks
		 * @dev Sets validator preferences and enables block production eligibility
		 * @param commission Commission rate as parts per billion (0-1,000,000,000 = 0%-100%)
		 * @param blocked Whether to block new nominations (allows existing nominators to stay)
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits ValidatorPrefsSet(validator, commission, blocked)
		 *   REVERT: If insufficient stake, invalid commission, or below minimum validator bond
		 * @custom:requirements
		 *   - Caller must have sufficient bonded stake
		 *   - Commission rate must not exceed 1,000,000,000 (100%)
		 *   - Account must meet minimum validator bond requirements
		 * @custom:events ValidatorPrefsSet(address indexed validator, uint256 commission, bool blocked)
		 */
		function validate(uint256 commission, bool blocked) external returns (bool success);

		/**
		 * @notice Nominate validators to support with staked tokens
		 * @dev Distributes nominator's stake among selected validators for potential rewards
		 * @param targets Array of validator addresses to nominate (up to MAX_NOMINATIONS)
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Nominated(stash, targets), updates nomination list
		 *   REVERT: If insufficient stake, invalid targets, too many nominations, or below minimum bond
		 * @custom:requirements
		 *   - Caller must have sufficient bonded stake
		 *   - All targets must be valid addresses (not necessarily active validators)
		 *   - Cannot exceed maximum number of nominations (typically 16)
		 *   - Account must meet minimum nominator bond requirements
		 * @custom:events Nominated(address indexed stash, address[] targets)
		 */
		function nominate(address[] calldata targets) external returns (bool success);

		/**
		 * @notice Stop all validation or nomination activities
		 * @dev Removes account from active validator/nominator sets while keeping stake bonded
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Chilled(stash), removes from validator/nominator sets
		 *   REVERT: If account has no staking ledger or is restricted
		 * @custom:requirements
		 *   - Caller must have a staking ledger (doesn't need to be actively validating/nominating)
		 * @custom:events Chilled(address indexed stash)
		 */
		function chill() external returns (bool success);

		/**
		 * @notice Re-bond previously unbonded tokens
		 * @dev Moves tokens from unbonding back to active bonded state
		 * @param value Amount to rebond from unbonding chunks (in smallest unit)
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Rebonded(stash, actualAmount), moves from unbonding to active
		 *   REVERT: If insufficient unbonding tokens or no staking ledger
		 * @custom:requirements
		 *   - Must have sufficient tokens in unbonding state >= value
		 *   - Account must have a staking ledger with unbonding chunks
		 * @custom:events Rebonded(address indexed stash, uint256 amount)
		 */
		function rebond(uint256 value) external returns (bool success);

		/**
		 * @notice Trigger payout of staking rewards for a specific era
		 * @dev Distributes accumulated rewards to validator and their nominators
		 * @param validatorStash Address of the validator to pay rewards for
		 * @param era Era number for which to pay rewards (must be payable)
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits RewardsPaid(validator, era), distributes rewards
		 *   REVERT: If era not payable, validator not active in era, or already claimed
		 * @custom:requirements
		 *   - Era must be within payable range (typically current_era - 84 to current_era - 1)
		 *   - Validator must have been active in the specified era
		 *   - Rewards must not have been previously claimed for this era
		 * @custom:events RewardsPaid(address indexed validator, uint256 era)
		 */
		function payoutStakers(address validatorStash, uint256 era) external returns (bool success);

		// ═══════════════════════════════════════════════════════════════════════════════════════
		//                                      QUERY FUNCTIONS
		// ═══════════════════════════════════════════════════════════════════════════════════════

		/**
		 * @notice Get the complete staking ledger for an account
		 * @dev Returns bonding information including active stake and unbonding schedule
		 * @param stash The stash account to query
		 * @return total Total amount ever bonded (active + unbonding)
		 * @return active Currently active bonded amount earning rewards
		 * @return unlocking Array of unbonding chunks with values and unlock eras
		 * @custom:behavior
		 *   STAKER: Returns actual ledger data with total, active, and unbonding chunks
		 *   NON_STAKER: Returns (0, 0, []) for accounts without staking ledger
		 */
		function ledger(address stash) external view returns (
			uint256 total,
			uint256 active,
			UnlockChunk[] memory unlocking
		);

		/**
		 * @notice Get nominator information and targets
		 * @dev Returns the validators being nominated and nomination metadata
		 * @param nominator The nominator account to query
		 * @return targets Array of validator addresses being nominated
		 * @return submittedIn Era when the nomination was last updated
		 * @return suppressed Whether nominations are temporarily suppressed
		 * @custom:behavior
		 *   NOMINATOR: Returns actual nomination data with targets, submission era, and suppression status
		 *   NON_NOMINATOR: Returns ([], 0, false) for accounts not nominating
		 */
		function nominators(address nominator) external view returns (
			address[] memory targets,
			uint256 submittedIn,
			bool suppressed
		);

		/**
		 * @notice Get validator preferences and status
		 * @dev Returns commission rate and blocking status for a validator
		 * @param validator The validator account to query
		 * @return commission Commission rate in parts per billion (0-1,000,000,000)
		 * @return blocked Whether the validator is blocking new nominations
		 * @custom:behavior
		 *   ANY_ACCOUNT: Always returns current validator preferences (defaults to 0, false)
		 */
		function validators(address validator) external view returns (
			uint256 commission,
			bool blocked
		);

		/**
		 * @notice Get the current active era index
		 * @dev Returns the era currently being used for staking calculations
		 * @return era The current era number
		 * @custom:behavior
		 *   ALWAYS: Returns current active era index, or 0 if no era is active
		 */
		function currentEra() external view returns (uint256 era);

		/**
		 * @notice Get the minimum bond required for nominators
		 * @dev Returns the minimum amount needed to participate as a nominator
		 * @return amount Minimum nominator bond in smallest token unit
		 * @custom:behavior
		 *   ALWAYS: Returns current minimum nominator bond requirement
		 */
		function minNominatorBond() external view returns (uint256 amount);

		/**
		 * @notice Get the minimum bond required for validators
		 * @dev Returns the minimum amount needed to participate as a validator
		 * @return amount Minimum validator bond in smallest token unit
		 * @custom:behavior
		 *   ALWAYS: Returns current minimum validator bond requirement
		 */
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
