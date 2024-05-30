// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Support of different finality engines, available in Substrate.

use crate::error::Error;
use async_trait::async_trait;
use bp_header_chain::{
	justification::{
		verify_and_optimize_justification, GrandpaEquivocationsFinder, GrandpaJustification,
		JustificationVerificationContext,
	},
	max_expected_submit_finality_proof_arguments_size, AuthoritySet, ConsensusLogReader,
	FinalityProof, FindEquivocations, GrandpaConsensusLogReader, HeaderFinalityInfo,
	HeaderGrandpaInfo, StoredHeaderGrandpaInfo,
};
use bp_runtime::{BasicOperatingMode, HeaderIdProvider, OperatingMode};
use codec::{Decode, Encode};
use num_traits::{One, Zero};
use relay_substrate_client::{
	BlockNumberOf, Chain, ChainWithGrandpa, Client, Error as SubstrateError, HashOf, HeaderOf,
	Subscription, SubstrateFinalityClient, SubstrateGrandpaFinalityClient,
};
use sp_consensus_grandpa::{AuthorityList as GrandpaAuthoritiesSet, GRANDPA_ENGINE_ID};
use sp_core::{storage::StorageKey, Bytes};
use sp_runtime::{scale_info::TypeInfo, traits::Header, ConsensusEngineId, SaturatedConversion};
use std::{fmt::Debug, marker::PhantomData};

/// Result of checking maximal expected call size.
pub enum MaxExpectedCallSizeCheck {
	/// Size is ok and call will be refunded.
	Ok,
	/// The call size exceeds the maximal expected and relayer will only get partial refund.
	Exceeds {
		/// Actual call size.
		call_size: u32,
		/// Maximal expected call size.
		max_call_size: u32,
	},
}

/// Finality engine, used by the Substrate chain.
#[async_trait]
pub trait Engine<C: Chain>: Send {
	/// Unique consensus engine identifier.
	const ID: ConsensusEngineId;
	/// A reader that can extract the consensus log from the header digest and interpret it.
	type ConsensusLogReader: ConsensusLogReader;
	/// Type of Finality RPC client used by this engine.
	type FinalityClient: SubstrateFinalityClient<C>;
	/// Type of finality proofs, used by consensus engine.
	type FinalityProof: FinalityProof<HashOf<C>, BlockNumberOf<C>> + Decode + Encode;
	/// The context needed for verifying finality proofs.
	type FinalityVerificationContext: Debug + Send;
	/// The type of the equivocation proof used by the consensus engine.
	type EquivocationProof: Clone + Debug + Send + Sync;
	/// The equivocations finder.
	type EquivocationsFinder: FindEquivocations<
		Self::FinalityProof,
		Self::FinalityVerificationContext,
		Self::EquivocationProof,
	>;
	/// The type of the key owner proof used by the consensus engine.
	type KeyOwnerProof: Send;
	/// Type of bridge pallet initialization data.
	type InitializationData: Debug + Send + Sync + 'static;
	/// Type of bridge pallet operating mode.
	type OperatingMode: OperatingMode + 'static;

	/// Returns storage at the bridged (target) chain that corresponds to some value that is
	/// missing from the storage until bridge pallet is initialized.
	///
	/// Note that we don't care about type of the value - just if it present or not.
	fn is_initialized_key() -> StorageKey;

	/// Returns `Ok(true)` if finality pallet at the bridged chain has already been initialized.
	async fn is_initialized<TargetChain: Chain>(
		target_client: &Client<TargetChain>,
	) -> Result<bool, SubstrateError> {
		Ok(target_client
			.raw_storage_value(Self::is_initialized_key(), None)
			.await?
			.is_some())
	}

	/// Returns storage key at the bridged (target) chain that corresponds to the variable
	/// that holds the operating mode of the pallet.
	fn pallet_operating_mode_key() -> StorageKey;

	/// Returns `Ok(true)` if finality pallet at the bridged chain is halted.
	async fn is_halted<TargetChain: Chain>(
		target_client: &Client<TargetChain>,
	) -> Result<bool, SubstrateError> {
		Ok(target_client
			.storage_value::<Self::OperatingMode>(Self::pallet_operating_mode_key(), None)
			.await?
			.map(|operating_mode| operating_mode.is_halted())
			.unwrap_or(false))
	}

	/// A method to subscribe to encoded finality proofs, given source client.
	async fn source_finality_proofs(
		source_client: &Client<C>,
	) -> Result<Subscription<Bytes>, SubstrateError> {
		source_client.subscribe_finality_justifications::<Self::FinalityClient>().await
	}

	/// Verify and optimize finality proof before sending it to the target node.
	///
	/// Apart from optimization, we expect this method to perform all required checks
	/// that the `header` and `proof` are valid at the current state of the target chain.
	async fn verify_and_optimize_proof<TargetChain: Chain>(
		target_client: &Client<TargetChain>,
		header: &C::Header,
		proof: &mut Self::FinalityProof,
	) -> Result<Self::FinalityVerificationContext, SubstrateError>;

	/// Checks whether the given `header` and its finality `proof` fit the maximal expected
	/// call size limit. If result is `MaxExpectedCallSizeCheck::Exceeds { .. }`, this
	/// submission won't be fully refunded and relayer will spend its own funds on that.
	fn check_max_expected_call_size(
		header: &C::Header,
		proof: &Self::FinalityProof,
	) -> MaxExpectedCallSizeCheck;

	/// Prepare initialization data for the finality bridge pallet.
	async fn prepare_initialization_data(
		client: Client<C>,
	) -> Result<Self::InitializationData, Error<HashOf<C>, BlockNumberOf<C>>>;

	/// Get the context needed for validating a finality proof.
	async fn finality_verification_context<TargetChain: Chain>(
		target_client: &Client<TargetChain>,
		at: HashOf<TargetChain>,
	) -> Result<Self::FinalityVerificationContext, SubstrateError>;

	/// Returns the finality info associated to the source headers synced with the target
	/// at the provided block.
	async fn synced_headers_finality_info<TargetChain: Chain>(
		target_client: &Client<TargetChain>,
		at: TargetChain::Hash,
	) -> Result<
		Vec<HeaderFinalityInfo<Self::FinalityProof, Self::FinalityVerificationContext>>,
		SubstrateError,
	>;

	/// Generate key ownership proof for the provided equivocation.
	async fn generate_source_key_ownership_proof(
		source_client: &Client<C>,
		at: C::Hash,
		equivocation: &Self::EquivocationProof,
	) -> Result<Self::KeyOwnerProof, SubstrateError>;
}

/// GRANDPA finality engine.
pub struct Grandpa<C>(PhantomData<C>);

impl<C: ChainWithGrandpa> Grandpa<C> {
	/// Read header by hash from the source client.
	async fn source_header(
		source_client: &Client<C>,
		header_hash: C::Hash,
	) -> Result<C::Header, Error<HashOf<C>, BlockNumberOf<C>>> {
		source_client
			.header_by_hash(header_hash)
			.await
			.map_err(|err| Error::RetrieveHeader(C::NAME, header_hash, err))
	}

	/// Read GRANDPA authorities set at given header.
	async fn source_authorities_set(
		source_client: &Client<C>,
		header_hash: C::Hash,
	) -> Result<GrandpaAuthoritiesSet, Error<HashOf<C>, BlockNumberOf<C>>> {
		let raw_authorities_set = source_client
			.grandpa_authorities_set(header_hash)
			.await
			.map_err(|err| Error::RetrieveAuthorities(C::NAME, header_hash, err))?;
		GrandpaAuthoritiesSet::decode(&mut &raw_authorities_set[..])
			.map_err(|err| Error::DecodeAuthorities(C::NAME, header_hash, err))
	}
}

#[async_trait]
impl<C: ChainWithGrandpa> Engine<C> for Grandpa<C> {
	const ID: ConsensusEngineId = GRANDPA_ENGINE_ID;
	type ConsensusLogReader = GrandpaConsensusLogReader<<C::Header as Header>::Number>;
	type FinalityClient = SubstrateGrandpaFinalityClient;
	type FinalityProof = GrandpaJustification<HeaderOf<C>>;
	type FinalityVerificationContext = JustificationVerificationContext;
	type EquivocationProof = sp_consensus_grandpa::EquivocationProof<HashOf<C>, BlockNumberOf<C>>;
	type EquivocationsFinder = GrandpaEquivocationsFinder<C>;
	type KeyOwnerProof = C::KeyOwnerProof;
	type InitializationData = bp_header_chain::InitializationData<C::Header>;
	type OperatingMode = BasicOperatingMode;

	fn is_initialized_key() -> StorageKey {
		bp_header_chain::storage_keys::best_finalized_key(C::WITH_CHAIN_GRANDPA_PALLET_NAME)
	}

	fn pallet_operating_mode_key() -> StorageKey {
		bp_header_chain::storage_keys::pallet_operating_mode_key(C::WITH_CHAIN_GRANDPA_PALLET_NAME)
	}

	async fn verify_and_optimize_proof<TargetChain: Chain>(
		target_client: &Client<TargetChain>,
		header: &C::Header,
		proof: &mut Self::FinalityProof,
	) -> Result<Self::FinalityVerificationContext, SubstrateError> {
		let verification_context = Grandpa::<C>::finality_verification_context(
			target_client,
			target_client.best_header().await?.hash(),
		)
		.await?;
		// we're risking with race here - we have decided to submit justification some time ago and
		// actual authorities set (which we have read now) may have changed, so this
		// `optimize_justification` may fail. But if target chain is configured properly, it'll fail
		// anyway, after we submit transaction and failing earlier is better. So - it is fine
		verify_and_optimize_justification(
			(header.hash(), *header.number()),
			&verification_context,
			proof,
		)
		.map(|_| verification_context)
		.map_err(|e| {
			SubstrateError::Custom(format!(
				"Failed to optimize {} GRANDPA jutification for header {:?}: {:?}",
				C::NAME,
				header.id(),
				e,
			))
		})
	}

	fn check_max_expected_call_size(
		header: &C::Header,
		proof: &Self::FinalityProof,
	) -> MaxExpectedCallSizeCheck {
		let is_mandatory = Self::ConsensusLogReader::schedules_authorities_change(header.digest());
		let call_size: u32 =
			header.encoded_size().saturating_add(proof.encoded_size()).saturated_into();
		let max_call_size = max_expected_submit_finality_proof_arguments_size::<C>(
			is_mandatory,
			proof.commit.precommits.len().saturated_into(),
		);
		if call_size > max_call_size {
			MaxExpectedCallSizeCheck::Exceeds { call_size, max_call_size }
		} else {
			MaxExpectedCallSizeCheck::Ok
		}
	}

	/// Prepare initialization data for the GRANDPA verifier pallet.
	async fn prepare_initialization_data(
		source_client: Client<C>,
	) -> Result<Self::InitializationData, Error<HashOf<C>, BlockNumberOf<C>>> {
		// In ideal world we just need to get best finalized header and then to read GRANDPA
		// authorities set (`pallet_grandpa::CurrentSetId` + `GrandpaApi::grandpa_authorities()`) at
		// this header.
		//
		// But now there are problems with this approach - `CurrentSetId` may return invalid value.
		// So here we're waiting for the next justification, read the authorities set and then try
		// to figure out the set id with bruteforce.
		let justifications = Self::source_finality_proofs(&source_client)
			.await
			.map_err(|err| Error::Subscribe(C::NAME, err))?;
		// Read next justification - the header that it finalizes will be used as initial header.
		let justification = justifications
			.next()
			.await
			.map_err(|e| Error::ReadJustification(C::NAME, e))
			.and_then(|justification| {
				justification.ok_or(Error::ReadJustificationStreamEnded(C::NAME))
			})?;

		// Read initial header.
		let justification: GrandpaJustification<C::Header> =
			Decode::decode(&mut &justification.0[..])
				.map_err(|err| Error::DecodeJustification(C::NAME, err))?;

		let (initial_header_hash, initial_header_number) =
			(justification.commit.target_hash, justification.commit.target_number);

		let initial_header = Self::source_header(&source_client, initial_header_hash).await?;
		log::trace!(target: "bridge", "Selected {} initial header: {}/{}",
			C::NAME,
			initial_header_number,
			initial_header_hash,
		);

		// Read GRANDPA authorities set at initial header.
		let initial_authorities_set =
			Self::source_authorities_set(&source_client, initial_header_hash).await?;
		log::trace!(target: "bridge", "Selected {} initial authorities set: {:?}",
			C::NAME,
			initial_authorities_set,
		);

		// If initial header changes the GRANDPA authorities set, then we need previous authorities
		// to verify justification.
		let mut authorities_for_verification = initial_authorities_set.clone();
		let scheduled_change = GrandpaConsensusLogReader::<BlockNumberOf<C>>::find_scheduled_change(
			initial_header.digest(),
		);
		assert!(
			scheduled_change.as_ref().map(|c| c.delay.is_zero()).unwrap_or(true),
			"GRANDPA authorities change at {} scheduled to happen in {:?} blocks. We expect\
			regular change to have zero delay",
			initial_header_hash,
			scheduled_change.as_ref().map(|c| c.delay),
		);
		let schedules_change = scheduled_change.is_some();
		if schedules_change {
			authorities_for_verification =
				Self::source_authorities_set(&source_client, *initial_header.parent_hash()).await?;
			log::trace!(
				target: "bridge",
				"Selected {} header is scheduling GRANDPA authorities set changes. Using previous set: {:?}",
				C::NAME,
				authorities_for_verification,
			);
		}

		// Now let's try to guess authorities set id by verifying justification.
		let mut initial_authorities_set_id = 0;
		let mut min_possible_block_number = C::BlockNumber::zero();
		loop {
			log::trace!(
				target: "bridge", "Trying {} GRANDPA authorities set id: {}",
				C::NAME,
				initial_authorities_set_id,
			);

			let is_valid_set_id = verify_and_optimize_justification(
				(initial_header_hash, initial_header_number),
				&AuthoritySet {
					authorities: authorities_for_verification.clone(),
					set_id: initial_authorities_set_id,
				}
				.try_into()
				.map_err(|_| {
					Error::ReadInvalidAuthorities(C::NAME, authorities_for_verification.clone())
				})?,
				&mut justification.clone(),
			)
			.is_ok();

			if is_valid_set_id {
				break
			}

			initial_authorities_set_id += 1;
			min_possible_block_number += One::one();
			if min_possible_block_number > initial_header_number {
				// there can't be more authorities set changes than headers => if we have reached
				// `initial_block_number` and still have not found correct value of
				// `initial_authorities_set_id`, then something else is broken => fail
				return Err(Error::GuessInitialAuthorities(C::NAME, initial_header_number))
			}
		}

		Ok(bp_header_chain::InitializationData {
			header: Box::new(initial_header),
			authority_list: initial_authorities_set,
			set_id: if schedules_change {
				initial_authorities_set_id + 1
			} else {
				initial_authorities_set_id
			},
			operating_mode: BasicOperatingMode::Normal,
		})
	}

	async fn finality_verification_context<TargetChain: Chain>(
		target_client: &Client<TargetChain>,
		at: HashOf<TargetChain>,
	) -> Result<Self::FinalityVerificationContext, SubstrateError> {
		let current_authority_set_key = bp_header_chain::storage_keys::current_authority_set_key(
			C::WITH_CHAIN_GRANDPA_PALLET_NAME,
		);
		let authority_set: AuthoritySet = target_client
			.storage_value(current_authority_set_key, Some(at))
			.await?
			.map(Ok)
			.unwrap_or(Err(SubstrateError::Custom(format!(
				"{} `CurrentAuthoritySet` is missing from the {} storage",
				C::NAME,
				TargetChain::NAME,
			))))?;

		authority_set.try_into().map_err(|e| {
			SubstrateError::Custom(format!(
				"{} `CurrentAuthoritySet` from the {} storage is invalid: {e:?}",
				C::NAME,
				TargetChain::NAME,
			))
		})
	}

	async fn synced_headers_finality_info<TargetChain: Chain>(
		target_client: &Client<TargetChain>,
		at: TargetChain::Hash,
	) -> Result<Vec<HeaderGrandpaInfo<HeaderOf<C>>>, SubstrateError> {
		let stored_headers_grandpa_info: Vec<StoredHeaderGrandpaInfo<HeaderOf<C>>> = target_client
			.typed_state_call(C::SYNCED_HEADERS_GRANDPA_INFO_METHOD.to_string(), (), Some(at))
			.await?;

		let mut headers_grandpa_info = vec![];
		for stored_header_grandpa_info in stored_headers_grandpa_info {
			headers_grandpa_info.push(stored_header_grandpa_info.try_into().map_err(|e| {
				SubstrateError::Custom(format!(
					"{} `AuthoritySet` synced to {} is invalid: {e:?} ",
					C::NAME,
					TargetChain::NAME,
				))
			})?);
		}

		Ok(headers_grandpa_info)
	}

	async fn generate_source_key_ownership_proof(
		source_client: &Client<C>,
		at: C::Hash,
		equivocation: &Self::EquivocationProof,
	) -> Result<Self::KeyOwnerProof, SubstrateError> {
		let set_id = equivocation.set_id();
		let offender = equivocation.offender();

		let opaque_key_owner_proof = source_client
			.generate_grandpa_key_ownership_proof(at, set_id, offender.clone())
			.await?
			.ok_or(SubstrateError::Custom(format!(
				"Couldn't get GRANDPA key ownership proof from {} at block: {at} \
				for offender: {:?}, set_id: {set_id} ",
				C::NAME,
				offender.clone(),
			)))?;

		let key_owner_proof =
			opaque_key_owner_proof.decode().ok_or(SubstrateError::Custom(format!(
				"Couldn't decode GRANDPA `OpaqueKeyOwnnershipProof` from {} at block: {at} 
				to `{:?}` for offender: {:?}, set_id: {set_id}, at block: {at}",
				C::NAME,
				<C::KeyOwnerProof as TypeInfo>::type_info().path,
				offender.clone(),
			)))?;

		Ok(key_owner_proof)
	}
}
