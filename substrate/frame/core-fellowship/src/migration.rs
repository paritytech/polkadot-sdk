use super::*;
use frame_support::{
	pallet_prelude::*, storage_alias, traits::UncheckedOnRuntimeUpgrade, BoundedVec,
};
/// The log target of this pallet.
pub const LOG_TARGET: &str = "runtime::core_fellowship";

mod v0 {
	use frame_system::pallet_prelude::BlockNumberFor;

	use super::*;

	#[derive(Encode, Decode, Eq, PartialEq, Clone, TypeInfo, MaxEncodedLen, RuntimeDebug)]
	pub struct ParamsType<Balance, BlockNumber, const RANKS: usize> {
		pub active_salary: [Balance; RANKS],
		pub passive_salary: [Balance; RANKS],
		pub demotion_period: [BlockNumber; RANKS],
		pub min_promotion_period: [BlockNumber; RANKS],
		pub offboard_timeout: BlockNumber,
	}

	impl<Balance: Default + Copy, BlockNumber: Default + Copy, const RANKS: usize> Default
		for ParamsType<Balance, BlockNumber, RANKS>
	{
		fn default() -> Self {
			Self {
				active_salary: [Balance::default(); RANKS],
				passive_salary: [Balance::default(); RANKS],
				demotion_period: [BlockNumber::default(); RANKS],
				min_promotion_period: [BlockNumber::default(); RANKS],
				offboard_timeout: BlockNumber::default(),
			}
		}
	}

	/// Number of available ranks from old version.
	pub(crate) const RANK_COUNT: usize = 9;

	pub type ParamsOf<T, I> = ParamsType<<T as Config<I>>::Balance, BlockNumberFor<T>, RANK_COUNT>;

	/// V0 type for [`crate::Params`].
	#[storage_alias]
	pub type Params<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, ParamsOf<T, I>, ValueQuery>;
}

mod v1 {
	use super::*;

	pub struct Migration<T, I = ()>(PhantomData<(T, I)>);
	impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for Migration<T, I> {
		fn on_runtime_upgrade() -> frame_support::weights::Weight {
			log::info!(
				target: LOG_TARGET,
				"Running migration from v0 to v1",
			);
			// Read the old value from storage
			let old_value = v0::Params::<T, I>::take();
			// Write the new value to storage
			let new = crate::ParamsType {
				active_salary: BoundedVec::try_from(old_value.active_salary.to_vec()).unwrap(),
				passive_salary: BoundedVec::try_from(old_value.passive_salary.to_vec()).unwrap(),
				demotion_period: BoundedVec::try_from(old_value.demotion_period.to_vec()).unwrap(),
				min_promotion_period: BoundedVec::try_from(old_value.min_promotion_period.to_vec())
					.unwrap(),
				offboard_timeout: old_value.offboard_timeout,
			};
			crate::Params::<T, I>::put(new);
			T::DbWeight::get().reads_writes(1, 1)
		}
	}
}

/// [`UncheckedOnRuntimeUpgrade`] implementation [`Migration`] wrapped in a
/// [`VersionedMigration`](frame_support::migrations::VersionedMigration), which ensures that:
/// - The migration only runs once when the on-chain storage version is 0
/// - The on-chain storage version is updated to `1` after the migration executes
/// - Reads/Writes from checking/settings the on-chain storage version are accounted for
pub type Migrate<T, I> = frame_support::migrations::VersionedMigration<
	0, // The migration will only execute when the on-chain storage version is 0
	1, // The on-chain storage version will be set to 1 after the migration is complete
	v1::Migration<T, I>,
	crate::pallet::Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
