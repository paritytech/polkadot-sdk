// This file implements ordered spend payouts for the Treasury pallet.
// WHY: Without ordering, any caller can trigger any mature spend, allowing
// smaller/newer spends to drain liquidity before larger/older ones execute.
// FIX: We introduce a `NextSpendIndex` storage item that acts as a FIFO queue
// pointer. Only the spend at `NextSpendIndex` (oldest unpaid approved spend)
// may be paid out. After success or expiry, the pointer advances.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use frame_support::{
        pallet_prelude::*,
        traits::{
            Currency, ExistenceRequirement, Get, Imbalance, OnUnbalanced,
            ReservableCurrency, WithdrawReasons,
        },
    };
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{AccountIdConversion, Saturating, Zero};
    use sp_std::prelude::*;

    type BalanceOf<T, I = ()> = <<T as Config<I>>::Currency as Currency<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

    type PositiveImbalanceOf<T, I = ()> = <<T as Config<I>>::Currency as Currency<
        <T as frame_system::Config>::AccountId,
    >>::PositiveImbalance;

    type NegativeImbalanceOf<T, I = ()> = <<T as Config<I>>::Currency as Currency<
        <T as frame_system::Config>::AccountId,
    >>::NegativeImbalance;

    /// A spend record stored on-chain.
    #[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, MaxEncodedLen, TypeInfo)]
    pub struct SpendRecord<Balance, BlockNumber, AccountId> {
        /// Amount to pay out.
        pub amount: Balance,
        /// Who to pay.
        pub beneficiary: AccountId,
        /// Block at which payment becomes valid (maturity).
        pub valid_from: BlockNumber,
        /// Block at which this spend expires if unpaid.
        pub expire_at: BlockNumber,
        /// Whether this spend has been paid.
        pub paid: bool,
    }

    #[pallet::config]
    pub trait Config<I: 'static = ()>: frame_system::Config {
        type RuntimeEvent: From<Event<Self, I>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: Currency<Self::AccountId> + ReservableCurrency<Self::AccountId>;

        /// Origin required to approve spends.
        type ApproveOrigin: EnsureOrigin<Self::RuntimeOrigin>;

        /// Fraction of a treasury spend that is burned, the rest is sent to the
        /// beneficiary.
        #[pallet::constant]
        type Burn: Get<sp_runtime::Permill>;

        /// The treasury's pallet id, used for deriving its sovereign account ID.
        #[pallet::constant]
        type PalletId: Get<frame_support::PalletId>;

        /// How many blocks after `valid_from` a spend may be claimed before expiring.
        #[pallet::constant]
        type SpendPeriod: Get<BlockNumberFor<Self>>;

        /// Handler for the unbalanced decrease when treasury funds are burned.
        type BurnDestination: OnUnbalanced<NegativeImbalanceOf<Self, I>>;

        /// Handler for the unbalanced decrease when spending.
        type SpendFunds: OnUnbalanced<NegativeImbalanceOf<Self, I>>;

        /// Maximum number of approvals that can be pending at any given time.
        #[pallet::constant]
        type MaxApprovals: Get<u32>;
    }

    #[pallet::pallet]
    pub struct Pallet<T, I = ()>(PhantomData<(T, I)>);

    /// Number of proposals that have been made.
    #[pallet::storage]
    #[pallet::getter(fn proposal_count)]
    pub(crate) type ProposalCount<T, I = ()> = StorageValue<_, u32, ValueQuery>;

    /// Spends that have been approved and are awaiting payout, stored by index.
    /// WHY: Using a map keyed by sequential index enables O(1) lookup of
    /// NextSpendIndex and efficient skipping of expired entries.
    #[pallet::storage]
    #[pallet::getter(fn spends)]
    pub(crate) type Spends<T: Config<I>, I: 'static = ()> = StorageMap<
        _,
        Twox64Concat,
        u32,
        SpendRecord<BalanceOf<T, I>, BlockNumberFor<T>, T::AccountId>,
        OptionQuery,
    >;

    /// The index of the next spend to attempt payout.
    /// WHY: This is the core of the FIFO enforcement. Only the spend at this
    /// index may be paid. All others are blocked until this one is paid or
    /// expires, preventing newer/smaller spends from jumping the queue.
    #[pallet::storage]
    #[pallet::getter(fn next_spend_index)]
    pub(crate) type NextSpendIndex<T, I = ()> = StorageValue<_, u32, ValueQuery>;

    /// The next spend index to be assigned when a new spend is approved.
    #[pallet::storage]
    pub(crate) type SpendCount<T, I = ()> = StorageValue<_, u32, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config<I>, I: 'static = ()> {
        /// A spend was approved and enqueued.
        SpendApproved {
            spend_index: u32,
            amount: BalanceOf<T, I>,
            beneficiary: T::AccountId,
        },
        /// A spend was successfully paid out.
        SpendProcessed { spend_index: u32 },
        /// A spend expired without being paid; queue advanced.
        SpendExpired { spend_index: u32 },
        /// Treasury funds were burned.
        Burnt { burnt_funds: BalanceOf<T, I> },
        /// Spending funds from the treasury.
        Spending { budget_remaining: BalanceOf<T, I> },
        /// Treasury is now updated.
        Rollover { rollover_balance: BalanceOf<T, I> },
        /// Some funds have been allocated.
        Deposit { value: BalanceOf<T, I> },
    }

    #[pallet::error]
    pub enum Error<T, I = ()> {
        /// No spend exists at the given index.
        InvalidSpendIndex,
        /// This spend is not the next in queue.
        /// WHY: Callers must pay the oldest unpaid spend first. This error
        /// surfaces the ordering invariant to the extrinsic layer.
        NotNextInQueue,
        /// The spend has not yet matured.
        SpendNotMature,
        /// The spend has already been paid.
        AlreadyProcessed,
        /// Insufficient treasury balance to pay this spend.
        InsufficientFunds,
        /// The spend has not yet expired (cannot skip).
        NotExpired,
    }

    #[pallet::call]
    impl<T: Config<I>, I: 'static> Pallet<T, I> {
        /// Approve a new spend from the treasury.
        /// Only callable by `ApproveOrigin`.
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn approve_spend(
            origin: OriginFor<T>,
            #[pallet::compact] amount: BalanceOf<T, I>,
            beneficiary: T::AccountId,
        ) -> DispatchResult {
            T::ApproveOrigin::ensure_origin(origin)?;

            let now = frame_system::Pallet::<T>::block_number();
            // Spend matures after one SpendPeriod.
            let valid_from = now.saturating_add(T::SpendPeriod::get());
            // Spend expires after another SpendPeriod (caller must pay within window).
            let expire_at = valid_from.saturating_add(T::SpendPeriod::get());

            let index = SpendCount::<T, I>::get();
            SpendCount::<T, I>::put(index.saturating_add(1));

            Spends::<T, I>::insert(
                index,
                SpendRecord {
                    amount,
                    beneficiary: beneficiary.clone(),
                    valid_from,
                    expire_at,
                    paid: false,
                },
            );

            Self::deposit_event(Event::SpendApproved {
                spend_index: index,
                amount,
                beneficiary,
            });
            Ok(())
        }

        /// Pay out the next pending spend in the queue.
        ///
        /// WHY permissionless: Anyone (bots, keepers, beneficiaries) can call
        /// this, but only the spend at `NextSpendIndex` will execute. This
        /// prevents out-of-order payouts while keeping the system liveness open.
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn payout_spend(origin: OriginFor<T>, spend_index: u32) -> DispatchResult {
            ensure_signed(origin)?;

            let next = NextSpendIndex::<T, I>::get();

            // ORDERING INVARIANT: Reject any spend that is not the head of queue.
            // WHY: This is the primary guard preventing newer spends from
            // jumping ahead of older ones when balances are tight.
            ensure!(spend_index == next, Error::<T, I>::NotNextInQueue);

            let mut spend = Spends::<T, I>::get(spend_index)
                .ok_or(Error::<T, I>::InvalidSpendIndex)?;

            ensure!(!spend.paid, Error::<T, I>::AlreadyProcessed);

            let now = frame_system::Pallet::<T>::block_number();
            ensure!(now >= spend.valid_from, Error::<T, I>::SpendNotMature);

            // Expired spends must be skipped via `skip_expired_spend`, not paid.
            // WHY: We must not silently drop expired spends here; force explicit skip
            // so events are emitted and the caller understands state transitions.
            ensure!(now < spend.expire_at, Error::<T, I>::NotExpired);

            let treasury_account = Self::account_id();
            let balance = T::Currency::free_balance(&treasury_account);
            ensure!(balance >= spend.amount, Error::<T, I>::InsufficientFunds);

            // Transfer funds to beneficiary.
            T::Currency::transfer(
                &treasury_account,
                &spend.beneficiary,
                spend.amount,
                ExistenceRequirement::KeepAlive,
            )?;

            spend.paid = true;
            Spends::<T, I>::insert(spend_index, &spend);

            // Advance the queue pointer to unblock the next spend.
            // WHY: Only after successful payment do we advance, ensuring
            // the invariant holds even if transfer fails mid-call.
            NextSpendIndex::<T, I>::put(next.saturating_add(1));

            Self::deposit_event(Event::SpendProcessed { spend_index });
            Ok(())
        }

        /// Skip a spend that has expired without being paid, advancing the queue.
        ///
        /// WHY: Without this, a spend with insufficient funds that has expired
        /// would permanently stall the queue. This is the permissionless escape
        /// hatch described in the design discussion. Any bot or user can call
        /// this after expiry to unblock subsequent spends.
        #[pallet::call_index(2)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn skip_expired_spend(origin: OriginFor<T>, spend_index: u32) -> DispatchResult {
            ensure_signed(origin)?;

            let next = NextSpendIndex::<T, I>::get();

            // Only the head of queue can be skipped.
            // WHY: Skipping arbitrary queue positions would break FIFO ordering.
            ensure!(spend_index == next, Error::<T, I>::NotNextInQueue);

            let spend = Spends::<T, I>::get(spend_index)
                .ok_or(Error::<T, I>::InvalidSpendIndex)?;

            ensure!(!spend.paid, Error::<T, I>::AlreadyProcessed);

            let now = frame_system::Pallet::<T>::block_number();
            // Can only skip after expiry window has passed.
            // WHY: Prevent premature skipping that would deny valid payouts.
            ensure!(now >= spend.expire_at, Error::<T, I>::NotExpired);

            // Advance queue without paying.
            NextSpendIndex::<T, I>::put(next.saturating_add(1));

            Self::deposit_event(Event::SpendExpired { spend_index });
            Ok(())
        }
    }

    impl<T: Config<I>, I: 'static> Pallet<T, I> {
        /// The account ID of the treasury.
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }
    }
}
