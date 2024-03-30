use crate::*;

/// Parachain registration API.
pub trait Registrar {
	/// The account ID type that encodes a parachain manager ID.
	type AccountId;

	/// Report the manager (permissioned owner) of a parachain, if there is one.
	fn manager_of(id: ParaId) -> Option<Self::AccountId>;

	/// All lease holding parachains. Ordered ascending by `ParaId`. On-demand
	/// parachains are not included.
	fn parachains() -> Vec<ParaId>;

	/// Return if a `ParaId` is a lease holding Parachain.
	fn is_parachain(id: ParaId) -> bool {
		Self::parachains().binary_search(&id).is_ok()
	}

	/// Return if a `ParaId` is a Parathread (on-demand parachain).
	fn is_parathread(id: ParaId) -> bool;

	/// Return if a `ParaId` is registered in the system.
	fn is_registered(id: ParaId) -> bool {
		Self::is_parathread(id) || Self::is_parachain(id)
	}

	/// Apply a lock to the para registration so that it cannot be modified by
	/// the manager directly. Instead the para must use its sovereign governance
	/// or the governance of the relay chain.
	fn apply_lock(id: ParaId);

	/// Remove any lock on the para registration.
	fn remove_lock(id: ParaId);

	/// Register a Para ID under control of `who`. Registration may be be
	/// delayed by session rotation.
	fn register(
		who: Self::AccountId,
		id: ParaId,
		genesis_head: HeadData,
		validation_code: ValidationCode,
	) -> DispatchResult;

	/// Deregister a Para ID, free any data, and return any deposits.
	fn deregister(id: ParaId) -> DispatchResult;

	/// Elevate a para to parachain status.
	fn make_parachain(id: ParaId) -> DispatchResult;

	/// Downgrade lease holding parachain into parathread (on-demand parachain)
	fn make_parathread(id: ParaId) -> DispatchResult;

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn worst_head_data() -> HeadData;

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn worst_validation_code() -> ValidationCode;

	/// Execute any pending state transitions for paras.
	/// For example onboarding to on-demand parachain, or upgrading on-demand to
	/// lease holding parachain.
	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn execute_pending_transitions();
}

/// Runtime hook for when we swap a lease holding parachain and an on-demand parachain.
#[impl_trait_for_tuples::impl_for_tuples(30)]
pub trait OnSwap {
	/// Updates any needed state/references to enact a logical swap of two parachains. Identity,
	/// code and `head_data` remain equivalent for all parachains/threads, however other properties
	/// such as leases, deposits held and thread/chain nature are swapped.
	fn on_swap(one: ParaId, other: ParaId);
}
