#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_support::{
    pallet_prelude::*,
    traits::Get,
    weights::Weight,
    Blake2_128Concat,
};
use frame_system::pallet_prelude::*;
use sp_runtime::{traits::Hash, Vec};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use super::*;

    #[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct ProviderInfo<T: Config> {
        pub subnet_id: T::Hash,
        pub resources: ResourceInfo,
        pub status: ProviderStatus,
        pub total_rewards: u128,
    }

    #[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct ResourceInfo {
        pub computational_capacity: u32,
        pub memory: u32,
        pub storage: u32,
        pub bandwidth: u32,
    }

    #[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, TypeInfo, MaxEncodedLen)]
    pub enum ProviderStatus {
        Active,
        Inactive,
        Suspended,
    }

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_subnet::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type WeightInfo: WeightInfo;
    }

    #[pallet::storage]
    pub type Providers<T: Config> = StorageMap
        <_,
        Blake2_128Concat,
        T::AccountId,
        ProviderInfo<T>
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        ProviderRegistered { provider: T::AccountId, subnet_id: T::Hash },
        ResourcesUpdated { provider: T::AccountId },
        StatusChanged { provider: T::AccountId, status: ProviderStatus },
    }

    #[pallet::error]
    pub enum Error<T> {
        AlreadyRegistered,
        NotRegistered,
        InvalidResources,
        InvalidSubnet,
    }

    pub trait WeightInfo {
        fn register_provider() -> Weight;
        fn update_resources() -> Weight;
        fn update_status() -> Weight;
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(<T as pallet::Config>::WeightInfo::register_provider())]
        pub fn register_provider(
            origin: OriginFor<T>,
            subnet_id: T::Hash,
            resources: ResourceInfo,
        ) -> DispatchResult {
            let provider = ensure_signed(origin)?;
            
            ensure!(!Providers::<T>::contains_key(&provider), Error::<T>::AlreadyRegistered);
            
            // Verify subnet exists
            ensure!(
                pallet_subnet::Subnets::<T>::contains_key(subnet_id),
                Error::<T>::InvalidSubnet
            );

            let provider_info = ProviderInfo {
                subnet_id,
                resources,
                status: ProviderStatus::Active,
                total_rewards: 0,
            };

            Providers::<T>::insert(&provider, provider_info);
            
            Self::deposit_event(Event::ProviderRegistered { 
                provider,
                subnet_id 
            });
            
            Ok(())
        }
    }
}