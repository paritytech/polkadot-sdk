# FRAME Pallet Development Guide

You are an expert FRAME pallet developer. Apply these patterns — derived from production pallets like `pallet-election-provider-multi-block` — when writing or reviewing FRAME code.

All names here are examples and use dummy concepts. You should use your own names.

## Code Organization

### File Layout
```
src/
├── lib.rs              # Pallet definition: Config, Storage, Calls, Events, Errors, Hooks
├── types.rs            # Shared types, state machines, utility structs
├── helpers.rs          # Helper macros and internal utilities
├── weights.rs          # Auto-generated weight info
├── benchmarking.rs     # Benchmarks
├── mock.rs             # Test mock runtime
├── tests.rs            # Unit/integration tests
└── migration.rs        # Storage migrations (if needed)
```

For complex pallets, split into sub-modules (e.g., `signed/`, `unsigned/`, `verifier/`) where:
- Parent pallet orchestrates and owns the state machine
- Sub-modules depend ONLY on the parent, never on each other
- Reverse linking uses explicit traits, not direct imports

### Naming Conventions
- `set_*` for storage mutations
- `ensure_*` for validation functions returning `Result`
- Use tabs for indentation, 100 char line width
- Avoid trailing whitespaces. Always use `cargo fmt` to format the code.

## Documentation

* Document all aspects of the code in the top level `lib.rs` file. in `//!` comments.
* Use real rust-docs syntax, so `[`crate::types::Foo::function`]` rather than just `Foo::function`.


## Config Trait

```rust
#[pallet::config]
pub trait Config: frame_system::Config {
    type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

    // Constants as Get<T>
    type MaxItems: Get<u32>;
    type BudgetPerBlock: Get<Weight>;

    // External integrations as trait bounds
    type DataProvider: SomeTrait<AccountIdOf<Self>>;
    type Currency: ReservableCurrency<Self::AccountId>;

    // Tiered origins (if needed)
    type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
    type ManagerOrigin: EnsureOrigin<Self::RuntimeOrigin>;

    // Weights
    type WeightInfo: WeightInfo;
}
```

**Rules:**
- Specify ALL trait bounds on associated types upfront
- Use `Get<T>` for configurable constants, not hardcoded values
- Prefer trait-based integrations (strategy pattern) over concrete types
- Tiered origins for different privilege levels

## Storage Design

### Wrapper Types for COMPLEX Storage Access
Never access COMPLEX storage items (when there are multiple storage items that are entangled together) directly from business logic. Wrap them:

```rust
pub(crate) struct TaskStore<T>(PhantomData<T>);

impl<T: Config> TaskStore<T> {
    pub(crate) fn insert(when: BlockNumberFor<T>, task: TaskOf<T>) -> Result<u32, Error<T>> {
        // All storage validation and access in one place
    }
    pub(crate) fn remove(when: BlockNumberFor<T>, index: u32) { ... }
    pub(crate) fn get(when: BlockNumberFor<T>) -> Vec<TaskOf<T>> { ... }
    pub(crate) fn kill() { ... }
}
```

Where this `TaskStore` is a warpper type managing multiple `#[pallet::storage]` items.

### Storage Item Patterns
```rust
// Simple value with default
#[pallet::storage]
pub type Round<T: Config> = StorageValue<_, u32, ValueQuery>;

// Map with explicit hasher
#[pallet::storage]
pub type Agenda<T: Config> = StorageMap<_, Twox64Concat, BlockNumberFor<T>, BoundedVec<...>>;

// Double map for round-scoped data
#[pallet::storage]
pub type PagedData<T: Config> = StorageDoubleMap<
    _, Twox64Concat, u32, Twox64Concat, PageIndex, DataOf<T>
>;
```

**Rules:**
- Use `BoundedVec` / `BoundedBTreeMap` — never unbounded collections.
  - If you see `#[pallet::unbounded]`, then a reason must be given
- Document storage invariants explicitly in comments

#### Looping On Storage

There should never be an unbounded loop on storage. For example, `StorageMap::iter()` if we don't know how many keys are in the map will kill the blockchain.

#### Storage Cleanup

All storage items must have a path to be cleaned. If the data is stored for a user, a deposit has to be placed for it, and refunded once the storage is cleaned.

Even for system level operations, we can have permissionless transactions that delete the data, and are free (`Pays::No`) IFF they are successful. This can then be used by bots to freely clear up the stale data.
## Dispatchable Calls

```rust
#[pallet::call]
impl<T: Config> Pallet<T> {
    #[pallet::call_index(0)]
    #[pallet::weight(T::WeightInfo::my_call())]
    pub fn my_call(origin: OriginFor<T>, param: BoundedVec<u8, T::MaxLen>) -> DispatchResultWithPostInfo {
        let who = ensure_signed(origin)?;
        // ... logic ...
        Ok(PostDispatchInfo {
            actual_weight: Some(actual),
            pays_fee: Pays::Yes,
        })
    }
}
```

**Rules:**
- Always use `#[pallet::call_index(N)]` — explicit, never implicit
- Return `DispatchResultWithPostInfo` when actual weight may differ from worst-case
- Use `Box<T>` for large parameters to avoid stack overflow
- Tiered origin checks: try lower privilege first, fall back to higher (if needed)

```rust
T::ManagerOrigin::ensure_origin(origin.clone())
    .map(|_| ())
    .or_else(|_| T::AdminOrigin::ensure_origin(origin).map(|_| ()))?;
```

## Hooks & Weight Management

### on_poll (Preferred for Scheduled Work)
```rust
fn on_poll(_now: BlockNumberFor<T>, weight_meter: &mut WeightMeter) {
    // 1. Check minimum weight before ANY work
    if !weight_meter.can_consume(T::DbWeight::get().reads(1)) {
        Self::deposit_event(Event::InsufficientWeight { ... });
        return;
    }

    // 2. Read state, consume weight
    let state = SomeStorage::<T>::get();
    weight_meter.consume(T::DbWeight::get().reads(1));

    // 3. Pre-compute worst-case weight for the work
    let work_weight = Self::compute_work_weight(&state);

    // 4. Check BEFORE executing
    if !weight_meter.can_consume(work_weight) {
        Self::deposit_event(Event::OutOfWeight { required: work_weight, had: weight_meter.remaining() });
        return;
    }

    // 5. Execute and consume
    Self::do_work(&state);
    weight_meter.consume(work_weight);
}
```

### on_idle (For Best-Effort Overflow)
```rust
fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
    let mut meter = WeightMeter::with_limit(remaining_weight);
    if meter.try_consume(T::WeightInfo::on_idle_base()).is_err() {
        return Weight::zero();
    }
    // Process items until weight exhausted
    while let Some(item) = Self::next_pending_item() {
        let item_weight = T::WeightInfo::process_item();
        if !meter.can_consume(item_weight) { break; }
        Self::process(item);
        meter.consume(item_weight);
    }
    meter.consumed()
}
```

### Weight Pattern: Lazy Execution Tuple
```rust
fn per_block_exec(phase: Phase) -> (Weight, Box<dyn Fn(&mut WeightMeter)>) {
    match phase {
        Phase::Active => {
            let weight = T::WeightInfo::active_work();
            let exec = Box::new(move |meter: &mut WeightMeter| {
                Self::do_active_work();
                meter.consume(weight);
            });
            (weight, exec)
        },
        _ => (T::WeightInfo::noop(), Box::new(|_| {})),
    }
}
```

**Rules:**
- ALWAYS check weight BEFORE consuming it
- Pre-compute worst-case weights, then check availability
- Emit diagnostic events when weight constraints prevent work
- Use `(Weight, Box<dyn Fn>)` for deferred execution after weight check
- Use `saturating_add` when combining weights

## State Machines

A useful pattern to think of blockchain state as a state machine, where applicable.

```rust
#[derive(Encode, Decode, TypeInfo, Clone, PartialEq, Eq, Debug)]
pub enum Phase<T: Config> {
    Off,
    Active(BlockNumberFor<T>),    // Inner value = blocks remaining
    Cooldown(BlockNumberFor<T>),
    Done,
}

impl<T: Config> Phase<T> {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Off,
            Self::Active(0) => Self::Cooldown(T::CooldownPeriod::get()),
            Self::Active(n) => Self::Active(n.saturating_sub(1)),
            Self::Cooldown(0) => Self::Done,
            Self::Cooldown(n) => Self::Cooldown(n.saturating_sub(1)),
            Self::Done => Self::Done,
        }
    }
    pub fn is_active(&self) -> bool { matches!(self, Self::Active(_)) }
}
```

**Rules:**
- Phase variants carry their countdown/state inline
- `next()` is pure and deterministic
- Provide `is_*()` helper methods
- Gate transitions on external conditions explicitly (e.g., `&& verifier_done()`)

## Events and Errors

Use passive names.

```rust
#[pallet::event]
#[pallet::generate_deposit(pub(super) fn deposit_event)]
pub enum Event<T: Config> {
    // Diagnostic events include context for debugging
    TaskExecuted { task_id: u32, result: DispatchResult },
    OutOfWeight { required: Weight, had: Weight },
    PhaseTransitioned { from: Phase<T>, to: Phase<T> },
}

#[pallet::error]
pub enum Error<T> {
    // Minimal, specific, actionable
    AgendaFull,
    TaskNotFound,
    InsufficientDeposit,
    BeyondSchedulingHorizon,
}
```

**Rules:**
- Emit moderate events, Events are not for debugging but rather for light clients.
- Errors are minimal and specific (not generic `InvalidInput`)
- Separate error types for different contexts (dispatch vs validation vs internal)

## Defensive Programming

```rust
// Use .defensive() for expected-but-not-fatal errors
let _ = SomeOperation::try_do().defensive();

// Use .defensive_saturating_sub() for countdown logic
remaining.defensive_saturating_sub(One::one())

// Use ensure!() with specific errors for validation
ensure!(deposit >= min_deposit, Error::<T>::InsufficientDeposit);

// Proof-based unwrap: explain WHY it can't fail
let item = storage.get(key).expect("item was checked to exist above; qed");

// Never use bare .unwrap() — always .expect("reason; qed") or .defensive()

// Use saturating arithmetic for all balance/weight operations
a.saturating_add(b)
a.saturating_sub(b)
a.saturating_mul(b)

// Debug assertions for invariants (zero cost in release)
debug_assert!(invariant_holds, "invariant X violated");
```

## Testing

### Mock Runtime Setup
```rust
pub type AccountId = u64;  // Simple types for tests

construct_runtime!(
    pub enum Runtime {
        System: frame_system,
        Balances: pallet_balances,
        MyPallet: my_pallet,
    }
);

// Use parameter_types! for configurable test values
parameter_types! {
    pub static MaxItems: u32 = 100;
    pub static BudgetPerBlock: Weight = Weight::from_parts(1_000_000, 0);
}

// Use () for WeightInfo in tests
impl my_pallet::Config for Runtime {
    type WeightInfo = ();
    // ...
}
```

### Test Helpers

Use test helpers to keep the tests clean and concise.

Test setup should ideally be self-explanatory: by reading the first few lines of the test setup, one should be able to understand what the test is testing and what is the initial state of the system.


```rust
fn roll_to(n: BlockNumber) {
    while System::block_number() < n {
        System::on_finalize(System::block_number());
        System::set_block_number(System::block_number() + 1);
        System::on_initialize(System::block_number());
        MyPallet::on_poll(System::block_number(), &mut WeightMeter::new());
    }
}

fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
    pallet_balances::GenesisConfig::<Runtime> { balances: vec![(1, 1000), (2, 1000)] }
        .assimilate_storage(&mut t).unwrap();
    t.into()
}
```

### Test Structure

Use Given/When/Then pattern.
```rust
#[test]
fn schedule_and_execute_works() {
    new_test_ext().execute_with(|| {
        // Given
        assert_ok!(MyPallet::schedule(RuntimeOrigin::signed(1), ...));
        // When
        roll_to(target_block);
        // Then
        assert_eq!(Events::get(), vec![Event::TaskExecuted { ... }]);
    });
}
```

## Benchmarking

```rust
#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn schedule_task() -> Result<(), BenchmarkError> {
        // Setup: create worst-case conditions
        let caller: T::AccountId = whitelisted_caller();
        T::Currency::make_free_balance_be(&caller, deposit * 2);

        #[extrinsic_call]
        _(RawOrigin::Signed(caller), param1, param2);

        // Verify: assert expected state changes
        assert!(Agenda::<T>::contains_key(target_block));
        Ok(())
    }

    #[benchmark(pov_mode = Measured)]
    fn execute_task() -> Result<(), BenchmarkError> {
        #[block]
        { Pallet::<T>::do_execute_task(task); }
        Ok(())
    }
}
```

**Rules:**
- Keep setup lean and concise.
- Benchmark worst-case scenarios!!!! MOST IMPORTANT THING TO DO.
- Use `whitelisted_caller()` for benchmark accounts

## Integrity & Try-State

```rust
fn integrity_test() {
    // Check config consistency
    assert!(T::MaxItems::get() > 0, "MaxItems must be positive");
    assert!(
        T::WeightInfo::process_item().all_lt(T::BudgetPerBlock::get()),
        "Single item must fit in per-block budget"
    );
}

#[cfg(feature = "try-runtime")]
fn try_state(_n: BlockNumberFor<T>) -> Result<(), TryRuntimeError> {
    Self::do_try_state()
}

fn do_try_state() -> Result<(), TryRuntimeError> {
    // Verify all storage invariants hold
    let agenda_count: u32 = Agenda::<T>::iter().count() as u32;
    ensure!(agenda_count <= T::MaxAgendas::get(), "Too many agendas");
    Ok(())
}
```
