use frame_support::traits::GetTreasury;
use crate::{Config, Pallet};

// Default GetTreasury implementation for testing.
impl<T: Config<I>, I: 'static> GetTreasury<T::AccountId> for Pallet<T, I>
where
    T::AccountId: Default
{
    fn get_treasury() -> T::AccountId {
        T::AccountId::default()
    }
}