use super::*;
use frame_system::pallet_prelude::BlockNumberFor;

pub type BalanceOf<T> =
	<<T as Config>::Currency as fungible::Inspect<<T as frame_system::Config>::AccountId>>::Balance;

/// A global extrinsic index, formed as the extrinsic index within a block, together with that
/// block's height. This allows a transaction in which a multisig operation of a particular
/// composite was created to be uniquely identified.
#[derive(
	Copy, Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen,
)]
pub struct Timepoint<BlockNumber> {
	/// The height of the chain at the point in time.
	pub height: BlockNumber,
	/// The index of the extrinsic at the point in time.
	pub index: u32,
}

/// An open multisig operation.
///
#[derive(Clone, Eq, PartialEq, Encode, Decode, Default, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(T))]
pub struct MultisigProposal<T: Config> {
	/// Proposal creator.
	pub creator: T::AccountId,
	pub creation_deposit: BalanceOf<T>,
	/// The extrinsic when the multisig operation was opened.
	pub when: Timepoint<BlockNumberFor<T>>,
	/// The approvers achieved so far, including the depositor.
	/// The approvers are stored in a BoundedBTreeSet to ensure faster lookup and operations (approve, revoke).
	/// It's also bounded to ensure that the size don't go over the required limit by the Runtime.
	pub approvers: BoundedBTreeSet<T::AccountId, T::MaxSignatories>,
	/// The block number until which this multisig operation is valid. None means no expiry.
	pub expire_after: Option<BlockNumberFor<T>>,
}

#[derive(
	Clone, PartialEq, Eq, Encode, Decode, Default, RuntimeDebugNoBound, TypeInfo, MaxEncodedLen,
)]
#[scale_info(skip_type_params(T))]
pub struct MultisigAccountDetails<T: Config> {
	/// The owners of the multisig account. This is a BoundedBTreeSet to ensure faster operations (add, remove).
	/// As well as lookups and faster set operations to ensure approvers is always a subset from owners. (e.g. in case of removal of an owner during an active proposal)
	pub owners: BoundedBTreeSet<T::AccountId, T::MaxSignatories>,
	/// The threshold of approvers required for the multisig account to be able to execute a call.
	pub threshold: u32,
	pub creator: T::AccountId,
	pub deposit: BalanceOf<T>,
}

impl<T: Config> MultisigAccountDetails<T> {
	/// Check if the given account is an owner of the multisig account.
	pub fn has_owner(&self, who: &T::AccountId) -> bool {
		self.owners.contains(who)
	}
}
