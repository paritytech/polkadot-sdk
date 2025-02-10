#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_support::{
    pallet_prelude::*,
    traits::Get,
    Blake2_128Concat,
};
use frame_system::pallet_prelude::*;
use sp_runtime::traits::Hash;
//use sp_std::vec::Vec;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
    use super::*;

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    #[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct SubnetInfo<T: Config> {
        pub king: T::AccountId,
        pub title: BoundedVec<u8, T::MaxTitleLength>,
        pub performance_params: PerformanceParams,
        pub verification_type: VerificationType,
    }

    #[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, TypeInfo, MaxEncodedLen)]
    pub enum VerificationType {
        Performance,
        Stake,
        Custom(BoundedVec<u8, ConstU32<100>>),
    }

    #[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct PerformanceParams {
        pub min_cpu_cores: u32,
        pub min_memory: u32,
        pub min_storage: u32,
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + scale_info::TypeInfo {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        
        #[pallet::constant]
        type MaxTitleLength: Get<u32>;
        
        #[pallet::constant]
        type MaxSubnetsPerKing: Get<u32>;

        type WeightInfo: WeightInfo;
    }

    #[pallet::storage]
    pub type Subnets<T: Config> = StorageDoubleMap
        <_,
        Blake2_128Concat,
        T::AccountId,  // King
        Blake2_128Concat,
        T::Hash,       // Subnet ID
        SubnetInfo<T>
    >;

    #[pallet::storage]
    pub type VerifiedProviders<T: Config> = StorageNMap
        <_,
        (
            NMapKey<Blake2_128Concat, T::AccountId>,  // King
            NMapKey<Blake2_128Concat, T::Hash>,       // Subnet ID
            NMapKey<Blake2_128Concat, T::AccountId>   // Provider
        ),
        Option<bool>
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        SubnetCreated { king: T::AccountId, subnet_id: T::Hash },
        ProviderVerified { subnet_id: T::Hash, provider: T::AccountId },
    }

    #[pallet::error]
    pub enum Error<T> {
        SubnetLimitReached,
        SubnetNotFound,
        ProviderAlreadyVerified,
        UnauthorizedKing,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

    // Weight info trait
    //#[pallet::call_index(1)]
    pub trait WeightInfo {
        fn create_subnet() -> Weight;
        fn verify_provider() -> Weight;
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(T::WeightInfo::create_subnet())]
        pub fn create_subnet(
            origin: OriginFor<T>,
            title: BoundedVec<u8, T::MaxTitleLength>,
            performance_params: PerformanceParams,
            verification_type: VerificationType,
        ) -> DispatchResult {
            let king = ensure_signed(origin)?;
            
            let subnet_id = Self::generate_subnet_id(&king, &title);
            
            let subnet_info = SubnetInfo {
                king: king.clone(),
                title,
                performance_params,
                verification_type,
            };

            Subnets::<T>::try_mutate(&king, &subnet_id, |maybe_subnet| -> DispatchResult {
                ensure!(maybe_subnet.is_none(), Error::<T>::SubnetLimitReached);
                *maybe_subnet = Some(subnet_info);
                Ok(())
            })?;

            Self::deposit_event(Event::SubnetCreated { king, subnet_id });
            Ok(())
        }

        #[pallet::weight(T::WeightInfo::verify_provider())]
        pub fn verify_provider(
            origin: OriginFor<T>,
            subnet_id: T::Hash,
            provider: T::AccountId,
        ) -> DispatchResult {
            let king = ensure_signed(origin)?;
            
            ensure!(
                Subnets::<T>::contains_key(&king, &subnet_id),
                Error::<T>::SubnetNotFound
            );

            VerifiedProviders::<T>::try_mutate(
                (king, subnet_id, provider.clone()),
                |maybe_verified| -> DispatchResult {
                    if maybe_verified.is_some() {
                        return Err(Error::<T>::ProviderAlreadyVerified.into());
                    }
                    *maybe_verified = Some(Some(true));
                    Ok(())
                }
            )?;

            Self::deposit_event(Event::ProviderVerified { 
                subnet_id,
                provider 
            });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn generate_subnet_id(king: &T::AccountId, title: &[u8]) -> T::Hash {
            let mut data = Vec::new();
            data.extend_from_slice(&king.encode());
            data.extend_from_slice(title);
            T::Hashing::hash_of(&data)
        }
    }
}