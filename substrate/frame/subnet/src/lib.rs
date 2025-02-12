#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

// Updated imports
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

    #[pallet::pallet]
    #[pallet::without_storage_info]
    pub struct Pallet<T>(_);

    // Resource configuration that defines minimum requirements
    #[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct ResourceConfig {
        pub min_computational_capacity: u32,
        pub min_memory: u32,
        pub min_storage: u32,
        pub min_bandwidth: u32,
    }

    // Subnet information including resource configuration and provider list
    #[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct SubnetInfo<T: Config> {
        pub king: T::AccountId,
        pub resource_config: ResourceConfig,
        pub providers: BoundedVec<T::AccountId, T::MaxProvidersPerSubnet>,
    }

    // Metrics for monitoring subnet performance and resource utilization
    #[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, TypeInfo, MaxEncodedLen)]
    pub struct SubnetMetrics {
        pub total_resources: ResourceConfig,
        pub active_providers: u32,
        pub total_rewards_distributed: u128,
    }

    // Weight calculations trait for extrinsic operations
    pub trait WeightInfo {
        fn create_subnet() -> Weight;
        fn add_provider() -> Weight;
        fn update_metrics() -> Weight;
    }

    #[pallet::config]
    pub trait Config: frame_system::Config + pallet_king::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        
        #[pallet::constant]
        type MaxProvidersPerSubnet: Get<u32>;

        type WeightInfo: WeightInfo;
    }

    // Storage for subnet information
    #[pallet::storage]
    pub type Subnets<T: Config> = StorageMap
        <_,
        Blake2_128Concat,
        T::Hash,  // Subnet ID
        SubnetInfo<T>
    >;

    // Storage for subnet metrics
    #[pallet::storage]
    pub type SubnetMetricsStorage<T: Config> = StorageMap
        <_,
        Blake2_128Concat,
        T::Hash,  // Subnet ID
        SubnetMetrics
    >;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        SubnetCreated { subnet_id: T::Hash, king: T::AccountId },
        ResourcesUpdated { subnet_id: T::Hash },
        MetricsUpdated { subnet_id: T::Hash },
    }

    #[pallet::error]
    pub enum Error<T> {
        SubnetNotFound,
        ResourceRequirementsNotMet,
        MaxProvidersReached,
        UnauthorizedKing,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::weight(<T as pallet::Config>::WeightInfo::create_subnet())]
        pub fn create_subnet(
            origin: OriginFor<T>,
            resource_config: ResourceConfig,
        ) -> DispatchResult {
            let king = ensure_signed(origin)?;
            
            // Fix the king verification by checking the correct storage
            let king_id = T::Hashing::hash_of(&king.encode());
            ensure!(
                pallet_king::Subnets::<T>::contains_key(&king, king_id),
                Error::<T>::UnauthorizedKing
            );

            let subnet_id = Self::generate_subnet_id(&king, &resource_config);
            
            let metrics = SubnetMetrics {
                total_resources: resource_config.clone(),
                active_providers: 0,
                total_rewards_distributed: 0,
            };

            let subnet_info = SubnetInfo {
                king: king.clone(),
                resource_config,
                providers: BoundedVec::default(),
            };

            Subnets::<T>::insert(subnet_id, subnet_info);
            SubnetMetricsStorage::<T>::insert(subnet_id, metrics);
            
            Self::deposit_event(Event::SubnetCreated { 
                subnet_id,
                king 
            });
            
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn generate_subnet_id(king: &T::AccountId, config: &ResourceConfig) -> T::Hash {
            let mut data = Vec::new();
            data.extend_from_slice(&king.encode());
            data.extend_from_slice(&config.encode());
            T::Hashing::hash_of(&data)
        }
    }
}