use crate::oracle::TallyOuterError;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_runtime::{traits::Saturating, FixedPointNumber, FixedU128, Percent};

pub struct SimpleAverage<T>(core::marker::PhantomData<T>);

impl<T: crate::oracle::Config> crate::oracle::Tally for SimpleAverage<T> {
	type AssetId = T::AssetId;
	type AccountId = T::AccountId;
	type BlockNumber = BlockNumberFor<T>;
	type Error = ();

	fn tally(
		_asset_id: Self::AssetId,
		votes: alloc::vec::Vec<(Self::AccountId, sp_runtime::FixedU128, Self::BlockNumber)>,
	) -> Result<(FixedU128, Percent), TallyOuterError<Self::Error>> {
		if votes.is_empty() {
			Err(TallyOuterError::YankVotes(()))
		} else {
			let count = FixedU128::saturating_from_integer(votes.len() as u32);
			let average = votes
				.into_iter()
				.map(|(_who, price, _produced_in)| price)
				.reduce(|acc, x| acc.saturating_add(x))
				.unwrap_or_default()
				.div(count);
			Ok((average, Percent::from_percent(100)))
		}
	}
}
