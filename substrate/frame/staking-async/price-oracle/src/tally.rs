use sp_runtime::{traits::Saturating, FixedPointNumber, FixedU128, Percent};

pub struct SimpleAverage<T>(core::marker::PhantomData<T>);

impl<T: crate::oracle::Config> crate::oracle::Tally for SimpleAverage<T> {
	type AssetId = T::AssetId;
	type AccountId = T::AccountId;
	type Error = ();

	fn tally(
		_asset_id: Self::AssetId,
		votes: alloc::vec::Vec<(Self::AccountId, sp_runtime::FixedU128)>,
	) -> Result<(FixedU128, Percent), Self::Error> {
		if votes.is_empty() {
			Err(())
		} else {
			let count = FixedU128::saturating_from_integer(votes.len() as u32);
			let average = votes
				.into_iter()
				.map(|(_, price)| price)
				.reduce(|acc, x| acc.saturating_add(x))
				.unwrap_or_default()
				.div(count);
			Ok((average, Percent::from_percent(100)))
		}
	}
}
