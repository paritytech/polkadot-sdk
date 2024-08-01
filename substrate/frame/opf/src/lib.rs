
#![cfg_attr(not(feature = "std"), no_std)]



// Re-export all pallet parts, this is needed to properly import the pallet into the runtime.
pub use pallet::*;
mod types;
//mod functions;
pub use types::*;
pub use pallet_distribution as Distribution;


#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + Distribution::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Deposit associated with voting.
		#[pallet::constant]
		type VotingBond: Get<BalanceOf<Self>> ;

		/// The minimum duration for which votes are locked
		#[pallet::constant]
		type VoteLockingPeriod: Get<BlockNumberFor<Self>> ;

		/// The period after which nominations must be renewed
		#[pallet::constant]
		type NominationRenewalPeriod: Get<BlockNumberFor<Self>> ;

		/// The maximum number of whitelisted projects per nomination round
		#[pallet::constant]
		type MaxWhitelistedProjects: Get<u32>;

	}
	
	#[pallet::storage]
	pub type WhiteListedProjectAccounts<T: Config> = 
		StorageValue<_, BoundedVec<AccountIdOf<T>, T::MaxWhitelistedProjects>, ValueQuery>;


	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {

		/// Reward successfully claimed
		RewardClaimed {
			when: BlockNumberFor<T>,
			amount: BalanceOf<T>,
			project_account: AccountIdOf<T>,
		},


	}

	#[pallet::error]
	pub enum Error<T> {
		/// There was an attempt to increment the value in storage over `u32::MAX`.
		StorageOverflow,
	
	}



	#[pallet::call]
	impl<T: Config> Pallet<T> {


		#[pallet::call_index(0)]
		pub fn  dummy(origin: OriginFor<T>, project_account:AccountIdOf<T>) -> DispatchResult {
			
			Ok(())			
			
		}

	}




	
}