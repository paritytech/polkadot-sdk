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
	precompiles::staking::IStaking::bondReturn, weights::WeightInfo, ActiveEra, BalanceOf, Call,
	Config, Ledger, MaxNominatorsCount, MinCommission, MinNominatorBond, MinValidatorBond,
	Nominators, Pallet, RewardDestination, StakingLedger, ValidatorPrefs, Validators,
};
use alloc::vec::Vec;
use alloy_core as alloy;
use alloy_core::{
	primitives::{IntoLogData, U256},
	sol,
	sol_types::{Revert, SolCall},
};
use pallet_revive::{
	precompiles::{AddressMatcher, Error, Ext, Precompile, RuntimeCosts, H256},
	AddressMapper,
};
use sp_core::H160;
use sp_runtime::{
	traits::{Get, StaticLookup},
	DispatchError, Perbill,
};

sol! {
	/**
	 * @title IStaking - Polkadot Staking Precompile Interface
	 * @notice Provides smart contract access to Polkadot's native staking functionality
	 * @dev This interface mostly maps 1-to-1 with pallet-staking-async extrinsics, enabling
	 *      Ethereum-compatible contracts to interact with Polkadot's Nominated Proof of Stake
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
		 * @param commission The commission rate (represented as parts per billion: 0-1,000,000,000)
		 * @param blocked Whether the validator is blocked from receiving nominations
		 */
		event Validated(address indexed validator, uint256 commission, bool blocked);

		/**
		 * @notice Emitted when an account stops validating or nominating aka is chilled
		 * @param stash The stash account that became inactive
		 */
		event Chilled(address indexed stash);

		/**
		 * @notice Emitted when previously unbonded tokens are withdrawn to free balance
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
		 * @notice Emitted when staking rewards of a validator are distributed
		 * @dev This event is only called when the all of the rewards associated with `validator` for `era` are paid out.
		 TODO: make sure emitted on last page only
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
		 TODO: double check correct era.
		 */
		struct UnlockChunk {
			uint256 value;
			uint256 era;
		}

		// ═══════════════════════════════════════════════════════════════════════════════════════
		//                                   STATE-CHANGING FUNCTIONS
		// ═══════════════════════════════════════════════════════════════════════════════════════

		/**
		 * @notice Bond tokens to participate in staking with compounding rewards
		 * @dev Creates a new staking ledger with rewards automatically restaked
		 * @param requested Maximum amount of tokens to bond (in smallest unit). If requested > free_balance, only `stakedAmount = min(requested, free_balance)`` will be bonded.
		 * @return success Always returns true if any tokens are bonded
		 * @custom:behavior
		 *   SUCCESS: Emits `Bonded(stash, stakedAmount), creates staking ledger
		 *   REVERT: If filtered account, insufficient balance, already bonded, already paired, or below minimum bond
		 * @custom:requirements
		 *   - Account must not be filtered by T::Filter (governance can block accounts)
		 *   - Value must be >= minChilledBond() (NOT minNominatorBond or minValidatorBond)
		 *   - Account must not already be bonded as stash (AlreadyBonded error)
		 *   - Account must not already be bonded as controller (AlreadyPaired error)
		 *   - Caller must have sufficient free balance
		 * @custom:events Bonded(address indexed stash, uint256 amount)
		 * @custom:edge_case
		 *   If requested > free_balance, only min(requested, free_balance) will actually be bonded.
		 *   The pallet emits its own event with the actual bonded amount.
		 *   This precompile emits the requested amount for interface consistency.
		 TODO note the case, precompile should also emit events with the actual bonded amount.
		 * @custom:stability
		 *   - T::Filter can be changed via governance, blocking previously valid accounts
		 *   - minChilledBond() calculation can change if governance updates MinValidatorBond/MinNominatorBond
		 */
		function bond(uint256 requested) external;

		/**
		 * @notice Set reward destination of caller to `payee`.
		 TODO: interface has changed.
		 * @dev Changes where staking rewards are sent - to `payee`, and not compounding.
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, updates reward destination to Account(caller)
		 *   REVERT: If no staking ledger exists or account restricted
		 * @custom:requirements
		 *   - Caller must have an active staking ledger
		 */
		function setPayee(address payee) external returns (bool success);

		/**
		 * @notice Set reward destination to compound (restake automatically)
		 * @dev Changes where staking rewards are sent - automatically restaked
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, updates reward destination to Staked
		 *   REVERT: If no staking ledger exists or account restricted
		 * @custom:requirements
		 *   - Caller must have an active staking ledger
		 */
		function setCompound() external returns (bool success);

		/**
		 * @notice Add more tokens to an existing bond
		 * @dev Increases the total bonded amount for the caller's stash account
		 * @param maxAdditional Maximum additional tokens to bond (actual amount may be less if insufficient balance)
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Bonded(stash, requestedAmount), increases bonded amount
		 *   REVERT: If no existing bond, insufficient free balance, or account restricted
		 * @custom:requirements
		 *   - Caller must already have an active staking ledger
		 *   - Caller must have sufficient free balance
		 * @custom:events Bonded(address indexed stash, uint256 amount)
		 * @custom:edge_case
		 *   If maxAdditional > free_balance, only min(maxAdditional, free_balance) will actually be bonded.
		 *   The pallet emits its own event with the actual bonded amount.
		 *   This precompile emits the requested amount for interface consistency.
		 */
		function bondExtra(uint256 maxAdditional) external returns (bool success);

		/**
		 * @notice Schedule bonded tokens for unbonding
		 * @dev Tokens become available for withdrawal after the unbonding period. The actual unbdonding period should be later checked with `ledger(account).unlocking`.
		 * @param value Amount of bonded tokens to schedule for unbonding (in smallest unit)
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Unbonded(stash, actualAmount), creates unbonding chunk
		 *   REVERT: If insufficient bonded tokens, would leave below minimum if active, too many chunks, or not bonded
		 * @custom:requirements
		 *   - Account must have an existing staking ledger
		 *   - Must have unlocking chunks < maxUnlockingChunks() after auto-withdraw
		 *   - Cannot unbond below minimum active stake if actively validating/nominating
		 * @custom:auto_withdraw
		 *   - If maxUnlockingChunks() limit is reached, automatically withdraws fully unbonded funds first
		 *   - This may change account state before processing the unbond request
		 * @custom:amount_adjustment
		 *   - Actual unbonded amount = min(value, ledger.active)
		 *   - If remaining active would be < existentialDeposit, unbonds all remaining active
		 * @custom:events Unbonded(address indexed stash, uint256 amount)
		 * @custom:stability_risk
		 *   - maxUnlockingChunks() limit affects when auto-withdraw occurs
		 *   - Minimum bond requirements can change via governance
		 */
		function unbond(uint256 value) external returns (bool success);

		// TODO
		function unbondAll() external returns (bool success);

		/**
		 * @notice Withdraw tokens that have finished unbonding
		 * @dev Moves fully unbonded tokens from staking ledger to free balance
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Withdrawn(stash, amount), removes completed chunks
		 *   REVERT: If no staking ledger exists or account doesn't exist
		 * @custom:requirements
		 *   - Account must exist and have a staking ledger
		 *   - Will process all chunks that have completed unbonding period
		 * @custom:events Withdrawn(address indexed stash, uint256 amount)
		 */
		function withdrawUnbonded() external returns (bool success);

		/**
		 * @notice Declare intention to validate blocks
		 * @dev Sets validator preferences and enables block production eligibility
		 * @param commission Commission rate as parts per billion (0-1,000,000,000 = 0%-100%)
		 * @param blocked Whether to block new nominations (allows existing nominators to stay -- use `kick` to remove them)
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Validated(validator, commission, blocked)
		 *   REVERT: If insufficient stake, commission too low, commission too high, or not bonded
		 * @custom:requirements
		 *   - Caller must have active bonded stake >= minValidatorBond()
		 *   - Commission rate must be >= minCommission() (can change via governance!)
		 *   - Commission rate must not exceed 1,000,000,000 (100%)
		 *   - Account must have an existing staking ledger
		 * @custom:events Validated(address indexed validator, uint256 commission, bool blocked)
		 * @custom:stability_risk
		 *   - minCommission() can be changed via governance, potentially invalidating previously valid commission rates
		 *   - minValidatorBond() can be changed via governance, potentially invalidating previously valid stakes
		 */
		function validate(uint256 commission, bool blocked) external returns (bool success);

		// TODO
		function kick(address nominator) external returns (bool success);

		/**
		 * @notice Nominate validators to support with staked tokens
		 * @dev Distributes nominator's stake among selected validators for potential rewards
		 * @param targets Array of validator addresses to nominate
		 * @return success Always returns true on successful execution
		 * @custom:behavior
		 *   SUCCESS: Returns true, emits Nominated(stash, processedTargets), updates nomination list
		 *   REVERT: If system full, insufficient stake, invalid targets, too many targets, empty targets, or not bonded
		 * @custom:requirements
		 *   - Account must have active bonded stake >= minNominatorBond()
		 *   - System must not be at maxNominatorsCount() limit (for new nominators only)
		 *   - Targets array must not be empty (EmptyTargets error)
		 *   - Targets count must be <= NominationsQuota based on stake amount
		 *   - All targets must be either: existing nominations OR active non-blocked validators
		 *   - Account must have an existing staking ledger
		 * @custom:target_processing
		 *   - Targets are automatically sorted and deduplicated by the pallet
		 *   - Only previously nominated targets OR active non-blocked validators are accepted
		 *   - Final target list may differ from input due to deduplication
		 * @custom:events Nominated(address indexed stash, address[] targets)
		 * @custom:stability_risk
		 *   - maxNominatorsCount() can block new nominators when system is full
		 *   - NominationsQuota calculation can change, affecting max targets allowed
		 *   - Validator blocked status can change, affecting target validity
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
		 * @notice Get the staked balance of `stash`. Returns total and active stake, the difference of the two being the amount queued for unbdoning. This amount can be either `rebond`-ed, or `withdrawUnbonded`.
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
		function nominator(address nominator) external view returns (
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
		function validator(address validator) external view returns (
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
		function era() external view returns (uint256 era);

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

		/**
		 * @notice Get the minimum commission rate for validators
		 * @dev Returns the minimum commission rate that validators must set
		 * @return commission Minimum commission rate in parts per billion (0-1,000,000,000)
		 * @custom:behavior
		 *   ALWAYS: Returns current minimum commission requirement
		 * @custom:stability_risk This value can change via governance, affecting validate() calls
		 */
		function minCommission() external view returns (uint256 commission);

		/**
		 * @notice Get the minimum bond required for initial bonding (chilled state)
		 * @dev Returns the minimum amount needed to create a new bond
		 * @return amount Minimum chilled bond = min(minValidatorBond, minNominatorBond).max(existentialDeposit)
		 * @custom:behavior
		 *   ALWAYS: Returns current minimum bond for initial bonding
		 * @custom:note This is different from minValidatorBond and minNominatorBond
		 */
		function minChilledBond() external view returns (uint256 amount);

		/**
		 * @notice Get the maximum number of nominators allowed in the system
		 * @dev Returns the global limit on total nominators
		 * @return count Maximum number of nominators (0 = no limit)
		 * @custom:behavior
		 *   ALWAYS: Returns current maximum nominator count
		 * @custom:stability_risk This affects nominate() success when system is full
		 */
		function maxNominatorsCount() external view returns (uint256 count);

		/**
		 * @notice Get the maximum number of unlocking chunks allowed per account
		 * @dev Returns the limit on concurrent unbonding operations
		 * @return count Maximum unlocking chunks per account
		 * @custom:behavior
		 *   ALWAYS: Returns current max unlocking chunks limit
		 * @custom:stability_risk This affects unbond() behavior with auto-withdraw
		 */
		function maxUnlockingChunks() external view returns (uint256 count);
	}
}

/// Staking precompile.
pub struct StakingPrecompile<T>(core::marker::PhantomData<T>);

impl<T> Precompile for StakingPrecompile<T>
where
	T: Config + pallet_revive::Config,
	U256: TryInto<BalanceOf<T>> + TryFrom<BalanceOf<T>>,
{
	type T = T;
	type Interface = IStaking::IStakingCalls;
	// TODO: make location configurable.
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(core::num::NonZero::new(0x0800).unwrap());
	const HAS_CONTRACT_INFO: bool = false;

	fn call(
		_address: &[u8; 20],
		input: &Self::Interface,
		env: &mut impl Ext<T = Self::T>,
	) -> Result<Vec<u8>, Error> {
		match input {
			IStaking::IStakingCalls::bond(call) => Self::bond(call, env),
			IStaking::IStakingCalls::setPayee(call) => Self::set_payee(call, env),
			IStaking::IStakingCalls::setCompound(call) => Self::set_compound(call, env),
			IStaking::IStakingCalls::bondExtra(call) => Self::bond_extra(call, env),
			IStaking::IStakingCalls::unbond(call) => Self::unbond(call, env),
			IStaking::IStakingCalls::unbondAll(call) => Self::unbond_all(call, env),
			IStaking::IStakingCalls::withdrawUnbonded(call) => Self::withdraw_unbonded(call, env),
			IStaking::IStakingCalls::nominate(call) => Self::nominate(call, env),
			IStaking::IStakingCalls::validate(call) => Self::validate(call, env),
			IStaking::IStakingCalls::kick(call) => Self::kick(call, env),
			IStaking::IStakingCalls::chill(_call) => Self::chill(_call, env),
			IStaking::IStakingCalls::rebond(call) => Self::rebond(call, env),
			IStaking::IStakingCalls::payoutStakers(call) => Self::payout_stakers(call, env),
			// Query functions
			IStaking::IStakingCalls::ledger(call) => Self::ledger(call, env),
			IStaking::IStakingCalls::nominator(call) => Self::nominator(call, env),
			IStaking::IStakingCalls::validator(call) => Self::validator(call, env),
			IStaking::IStakingCalls::era(_call) => Self::era(_call, env),
			IStaking::IStakingCalls::minNominatorBond(_) => Self::min_nominator_bond(env),
			IStaking::IStakingCalls::minValidatorBond(_) => Self::min_validator_bond(env),
			IStaking::IStakingCalls::minCommission(_) => Self::min_commission(env),
			IStaking::IStakingCalls::minChilledBond(_) => Self::min_chilled_bond(env),
			IStaking::IStakingCalls::maxNominatorsCount(_) => Self::max_nominators_count(env),
			IStaking::IStakingCalls::maxUnlockingChunks(_) => Self::max_unlocking_chunks(env),
		}
	}
}

const ERR_INVALID_CALLER: &str = "Invalid caller";
const ERR_BALANCE_CONVERSION_FAILED: &str = "Balance conversion failed";
const ERR_COMMISSION_TOO_HIGH: &str = "Commission rate too high";

impl<T> StakingPrecompile<T>
where
	T: Config + pallet_revive::Config,
	U256: TryInto<BalanceOf<T>> + TryFrom<BalanceOf<T>>,
{
	/// Get the caller as an `H160` address.
	fn caller(env: &mut impl Ext<T = T>) -> Result<H160, Error> {
		env.caller()
			.account_id()
			.map(<T as pallet_revive::Config>::AddressMapper::to_address)
			.map_err(|_| Error::Revert(Revert { reason: ERR_INVALID_CALLER.into() }))
	}

	fn account_id(caller: &H160) -> T::AccountId {
		<T as pallet_revive::Config>::AddressMapper::to_account_id(caller)
	}

	fn runtime_origin(id: &T::AccountId) -> T::RuntimeOrigin {
		frame_system::RawOrigin::Signed(id.clone()).into()
	}

	/// Convert a `U256` value to the balance type of the pallet.
	fn to_balance(value: U256) -> Result<BalanceOf<T>, Error> {
		value
			.try_into()
			.map_err(|_| Error::Revert(Revert { reason: ERR_BALANCE_CONVERSION_FAILED.into() }))
	}

	/// Convert a balance to a `U256` value.
	fn to_u256(value: BalanceOf<T>) -> U256 {
		value.try_into().map_err(|_| ()).expect(
			"Runtime Balance is always at most u128; can be converted to U256 without overflow; qed",
		)
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

	fn bond(call: &IStaking::bondCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::bond())?;

		let stash_address = Self::caller(env)?;
		let stash_id = Self::account_id(&stash_address);
		let value = Self::to_balance(call.requested)?;

		Pallet::<T>::bond(Self::runtime_origin(&stash_id), value, RewardDestination::Staked)?;

		let amount_bonded = Self::to_u256(
			StakingLedger::<T>::get(sp_staking::StakingAccount::Stash(stash_id.clone()))
				.map(|ledger| ledger.active)
				.unwrap_or_default(),
		);

		Self::deposit_event(
			env,
			IStaking::IStakingEvents::Bonded(IStaking::Bonded {
				stash: stash_address.0.into(),
				amount: amount_bonded,
			}),
		)?;

		Ok(Default::default())
	}

	fn bond_extra(
		call: &IStaking::bondExtraCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		env.charge(<T as crate::Config>::WeightInfo::bond_extra())?;

		let stash_address = Self::caller(env)?;
		let stash_id = Self::account_id(&stash_address);
		let max_additional = Self::to_balance(call.maxAdditional)?;

		// TODO: some places we do the dispatch and some places direct fn call. Only difference is
		// call filter, which is already applied in the revive level. Likely best to move all to fn
		// calls.
		let extra = Pallet::<T>::do_bond_extra(&stash_id, max_additional)?;

		Self::deposit_event(
			env,
			IStaking::IStakingEvents::Bonded(IStaking::Bonded {
				stash: stash_address.0.into(),
				amount: Self::to_u256(extra),
			}),
		)?;

		Ok(Default::default())
	}

	/// Execute the unbond call.
	fn unbond(call: &IStaking::unbondCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(<T as crate::Config>::WeightInfo::unbond())?;

		// let stash = Self::caller(env)?;
		// let value = Self::to_balance(call.value)?;

		// // Call pallet function
		// Pallet::<T>::unbond(frame_system::RawOrigin::Signed(stash.clone()).into(), value)
		// 	.map_err(|_| Error::Revert(Revert { reason: "Unbond failed".into() }))?;

		// // Emit event
		// Self::deposit_event(
		// 	env,
		// 	IStaking::IStakingEvents::Unbonded(IStaking::Unbonded {
		// 		stash: Self::to_address(&stash),
		// 		amount: call.value,
		// 	}),
		// )?;

		// Ok(IStaking::unbondCall::abi_encode_returns(&true))
	}

	fn unbond_all(
		call: &IStaking::unbondAllCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(<T as crate::Config>::WeightInfo::unbond_all())?;

		// let stash = Self::caller(env)?;

		// // Call pallet function
		// Pallet::<T>::unbond_all(frame_system::RawOrigin::Signed(stash.clone()).into())
		// 	.map_err(|_| Error::Revert(Revert { reason: "Unbond all failed".into() }))?;

		// // Emit event
		// Self::deposit_event(
		// 	env,
		// 	IStaking::IStakingEvents::Unbonded(IStaking::Unbonded {
		// 		stash: Self::to_address(&stash),
		// 		amount: U256::ZERO, // We don't know the exact amount unbonded
		// 	}),
		// )?;

		// Ok(IStaking::unbondCall::abi_encode_returns(&true))
	}

	/// Execute the withdraw_unbonded call.
	fn withdraw_unbonded(
		call: &IStaking::withdrawUnbondedCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(<T as crate::Config>::WeightInfo::withdraw_unbonded_kill())?;

		// let stash = Self::caller(env)?;

		// // Call pallet function
		// Pallet::<T>::withdraw_unbonded(
		// 	frame_system::RawOrigin::Signed(stash.clone()).into(),
		// 	call.numSlashingSpans,
		// )
		// .map_err(|_| Error::Revert(Revert { reason: "Withdraw unbonded failed".into() }))?;

		// // Emit event (we don't know the exact amount withdrawn, so use 0)
		// Self::deposit_event(
		// 	env,
		// 	IStaking::IStakingEvents::Withdrawn(IStaking::Withdrawn {
		// 		stash: Self::to_address(&stash),
		// 		amount: U256::ZERO,
		// 	}),
		// )?;

		// Ok(IStaking::withdrawUnbondedCall::abi_encode_returns(&true))
	}

	/// Execute the nominate call.
	fn nominate(
		call: &IStaking::nominateCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(<T as crate::Config>::WeightInfo::nominate(call.targets.len() as u32))?;

		// let nominator = Self::caller(env)?;

		// // Convert targets to AccountIds and then to lookups
		// let targets: Result<Vec<T::AccountId>, Error> = call
		// 	.targets
		// 	.iter()
		// 	.map(|addr| Ok(Self::to_account_id(addr)))
		// 	.collect();
		// let targets = targets?;

		// // Convert to lookup sources
		// let target_lookups: Vec<_> = targets
		// 	.iter()
		// 	.map(|account| <T as frame_system::Config>::Lookup::unlookup(account.clone()))
		// 	.collect();

		// // Call pallet function
		// Pallet::<T>::nominate(
		// 	frame_system::RawOrigin::Signed(nominator.clone()).into(),
		// 	target_lookups,
		// )
		// .map_err(|_| Error::Revert(Revert { reason: "Nominate failed".into() }))?;

		// // Emit event
		// Self::deposit_event(
		// 	env,
		// 	IStaking::IStakingEvents::Nominated(IStaking::Nominated {
		// 		stash: Self::to_address(&nominator),
		// 		targets: call.targets.clone(),
		// 	}),
		// )?;

		// Ok(IStaking::nominateCall::abi_encode_returns(&true))
	}

	/// Execute the validate call.
	fn validate(
		call: &IStaking::validateCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(<T as crate::Config>::WeightInfo::validate())?;

		// let validator = Self::caller(env)?;

		// // Convert commission from U256 to Perbill
		// // Commission is expected to be in parts per billion (10^9)
		// let commission_value = call.commission.to::<u32>();
		// let commission = if commission_value > 1_000_000_000u32 {
		// 	return Err(Error::Revert(Revert { reason: ERR_COMMISSION_TOO_HIGH.into() }));
		// } else {
		// 	Perbill::from_parts(commission_value)
		// };

		// let prefs = ValidatorPrefs { commission, blocked: call.blocked };

		// // Call pallet function
		// Pallet::<T>::validate(frame_system::RawOrigin::Signed(validator.clone()).into(), prefs)
		// 	.map_err(|_| Error::Revert(Revert { reason: "Validate failed".into() }))?;

		// // Emit event
		// Self::deposit_event(
		// 	env,
		// 	IStaking::IStakingEvents::Validated(IStaking::Validated {
		// 		validator: Self::to_address(&validator),
		// 		commission: call.commission,
		// 		blocked: call.blocked,
		// 	}),
		// )?;

		// Ok(IStaking::validateCall::abi_encode_returns(&true))
	}

	fn kick(call: &IStaking::kickCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!()
	}

	/// Execute the chill call.
	fn chill(call: &IStaking::chillCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(<T as crate::Config>::WeightInfo::chill())?;

		// let stash = Self::caller(env)?;

		// // Call pallet function
		// Pallet::<T>::chill(frame_system::RawOrigin::Signed(stash.clone()).into())
		// 	.map_err(|_| Error::Revert(Revert { reason: "Chill failed".into() }))?;

		// // Emit event
		// Self::deposit_event(
		// 	env,
		// 	IStaking::IStakingEvents::Chilled(IStaking::Chilled {
		// 		stash: Self::to_address(&stash),
		// 	}),
		// )?;

		// Ok(IStaking::chillCall::abi_encode_returns(&true))
	}

	/// Execute the rebond call.
	fn rebond(call: &IStaking::rebondCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(<T as crate::Config>::WeightInfo::rebond(1))?; // Approximate weight

		// let stash = Self::caller(env)?;
		// let value = Self::to_balance(call.value)?;

		// // Call pallet function
		// Pallet::<T>::rebond(frame_system::RawOrigin::Signed(stash.clone()).into(), value)
		// 	.map_err(|_| Error::Revert(Revert { reason: "Rebond failed".into() }))?;

		// // Emit event
		// Self::deposit_event(
		// 	env,
		// 	IStaking::IStakingEvents::Rebonded(IStaking::Rebonded {
		// 		stash: Self::to_address(&stash),
		// 		amount: call.value,
		// 	}),
		// )?;

		// Ok(IStaking::rebondCall::abi_encode_returns(&true))
	}

	/// Execute the payout_stakers call.
	fn payout_stakers(
		call: &IStaking::payoutStakersCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(<T as crate::Config>::WeightInfo::payout_stakers_alive_staked(1))?; //
		// Approximate weight

		// let validator_stash = Self::to_account_id(&call.validatorStash);
		// let era = call.era.to::<u32>();

		// // Call pallet function
		// Pallet::<T>::payout_stakers(
		// 	frame_system::RawOrigin::Signed(Self::caller(env)?).into(),
		// 	validator_stash.clone(),
		// 	era,
		// )
		// .map_err(|_| Error::Revert(Revert { reason: "Payout stakers failed".into() }))?;

		// // Emit event
		// Self::deposit_event(
		// 	env,
		// 	IStaking::IStakingEvents::RewardsPaid(IStaking::RewardsPaid {
		// 		validator: call.validatorStash,
		// 		era: call.era,
		// 	}),
		// )?;

		// Ok(IStaking::payoutStakersCall::abi_encode_returns(&true))
	}

	fn set_payee(
		call: &IStaking::setPayeeCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		todo!()
	}

	fn set_compound(
		call: &IStaking::setCompoundCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		todo!()
	}
}

// read-only fns
impl<T> StakingPrecompile<T>
where
	T: Config + pallet_revive::Config,
	U256: TryInto<BalanceOf<T>> + TryFrom<BalanceOf<T>>,
{
	/// Execute the ledger query.
	fn ledger(call: &IStaking::ledgerCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!()
		// // Query operations are typically free, but we'll charge minimal weight
		// env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		// let stash = Self::to_account_id(&call.stash);

		// if let Some(ledger) = Ledger::<T>::get(&stash) {
		// 	let total = Self::to_u256(ledger.total)?;
		// 	let active = Self::to_u256(ledger.active)?;

		// 	let unlocking: Result<Vec<IStaking::UnlockChunk>, Error> = ledger
		// 		.unlocking
		// 		.iter()
		// 		.map(|chunk| {
		// 			Ok(IStaking::UnlockChunk {
		// 				value: Self::to_u256(chunk.value)?,
		// 				era: U256::from(chunk.era),
		// 			})
		// 		})
		// 		.collect();

		// 	Ok(IStaking::ledgerCall::abi_encode_returns(&IStaking::ledgerReturn {
		// 		total,
		// 		active,
		// 		unlocking: unlocking?,
		// 	}))
		// } else {
		// 	// Return empty ledger for non-stakers
		// 	Ok(IStaking::ledgerCall::abi_encode_returns(&IStaking::ledgerReturn {
		// 		total: U256::ZERO,
		// 		active: U256::ZERO,
		// 		unlocking: Vec::<IStaking::UnlockChunk>::new(),
		// 	}))
		// }
	}

	/// Execute the nominators query.
	fn nominator(
		call: &IStaking::nominatorCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		// let nominator = Self::to_account_id(&call.nominator);

		// if let Some(nominations) = Nominators::<T>::get(&nominator) {
		// 	let targets: Vec<alloy::primitives::Address> =
		// 		nominations.targets.iter().map(|acc| Self::to_address(acc)).collect();

		// 	Ok(IStaking::nominatorsCall::abi_encode_returns(&IStaking::nominatorsReturn {
		// 		targets,
		// 		submittedIn: U256::from(nominations.submitted_in),
		// 		suppressed: nominations.suppressed,
		// 	}))
		// } else {
		// 	// Return empty nominations for non-nominators
		// 	Ok(IStaking::nominatorsCall::abi_encode_returns(&IStaking::nominatorsReturn {
		// 		targets: Vec::<alloy::primitives::Address>::new(),
		// 		submittedIn: U256::ZERO,
		// 		suppressed: false,
		// 	}))
		// }
	}

	/// Execute the validators query.
	fn validator(
		call: &IStaking::validatorCall,
		env: &mut impl Ext<T = T>,
	) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		// let validator = Self::to_account_id(&call.validator);

		// let prefs = Validators::<T>::get(&validator);
		// let commission = U256::from(prefs.commission.deconstruct());

		// Ok(IStaking::validatorsCall::abi_encode_returns(&IStaking::validatorsReturn {
		// 	commission,
		// 	blocked: prefs.blocked,
		// }))
	}

	/// Execute the current_era query.
	fn era(call: &IStaking::eraCall, env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		// let era = ActiveEra::<T>::get()
		// 	.map(|info| info.index)
		// 	.unwrap_or_default();

		// Ok(IStaking::currentEraCall::abi_encode_returns(&U256::from(era)))
	}

	/// Execute the min_nominator_bond query.
	fn min_nominator_bond(env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		// let min_bond = MinNominatorBond::<T>::get();
		// let amount = Self::to_u256(min_bond)?;

		// Ok(IStaking::minNominatorBondCall::abi_encode_returns(&amount))
	}

	/// Execute the min_validator_bond query.
	fn min_validator_bond(env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		// let min_bond = MinValidatorBond::<T>::get();
		// let amount = Self::to_u256(min_bond)?;

		// Ok(IStaking::minValidatorBondCall::abi_encode_returns(&amount))
	}

	/// Execute the min_commission query.
	fn min_commission(env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!()
		// env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		// let min_commission = MinCommission::<T>::get();
		// let commission = U256::from(min_commission.deconstruct());

		// Ok(IStaking::minCommissionCall::abi_encode_returns(&commission))
	}

	/// Execute the min_chilled_bond query.
	fn min_chilled_bond(env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!();
		// env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		// let min_bond = Pallet::<T>::min_chilled_bond();
		// let amount = Self::to_u256(min_bond)?;

		// Ok(IStaking::minChilledBondCall::abi_encode_returns(&amount))
	}

	/// Execute the max_nominators_count query.
	fn max_nominators_count(env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!();
		// env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		// let max_count = MaxNominatorsCount::<T>::get().unwrap_or(0);
		// let count = U256::from(max_count);

		// Ok(IStaking::maxNominatorsCountCall::abi_encode_returns(&count))
	}

	/// Execute the max_unlocking_chunks query.
	fn max_unlocking_chunks(env: &mut impl Ext<T = T>) -> Result<Vec<u8>, Error> {
		todo!();
		// env.charge(frame_support::weights::Weight::from_parts(1000, 0))?;

		// let max_chunks = T::MaxUnlockingChunks::get();
		// let count = U256::from(max_chunks);

		// Ok(IStaking::maxUnlockingChunksCall::abi_encode_returns(&count))
	}
}
