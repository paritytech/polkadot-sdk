use crate::{
	types::{PalletMigration, *},
	*,
};
use core::marker::PhantomData;

#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum DelegatedStakingStage<AccountId> {
	Delegators(Option<AccountId>),
	Agents(Option<AccountId>),
}

pub enum RcDelegatedStakingMessage<T: pallet_delegated_staking::Config> {
	Delegators { key: T::AccountId, value: pallet_delegated_staking::Delegators<T> },
	Agents { key: T::AccountId, value: pallet_delegated_staking::Agents<T> },
}

pub struct DelegatedStakingMigrator<T>(PhantomData<T>);

impl<T: Config> PalletMigration for DelegatedStakingMigrator<T> {
	type Key = DelegatedStakingStage<T::AccountId>;
	type Error = Error<T>;

	fn migrate_many(
			last_key: Option<Self::Key>,
			weight_counter: &mut WeightMeter,
		) -> Result<Option<Self::Key>, Self::Error> {
		todo!("will complete after I write the manual migration")
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::RcMigrationCheck for DelegatedStakingMigrator<T> {
	type RcPrePayload = ();
	fn pre_check() -> Self::RcPrePayload {
		todo!()
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload) {
		todo!();
	}
}
