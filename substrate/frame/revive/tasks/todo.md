# PGAS-backed storage deposits — implementation plan

Spec: `substrate/frame/revive/.claude/pgas-deposit.md`.

## Decisions locked

1. **Charge policy:** before hitting DOT, compute the full deposit amount a call would incur and check whether PGAS can cover the whole amount. If yes → all-PGAS. If no → all-DOT. Never mix PGAS and DOT inside a single charge.
2. **Migration:** add a `historic_deposit: BalanceOf<T>` field on `ContractInfo<T>` (per contract). Migration seeds it with the contract's current `total_deposit()`. On refunds this is consumed **before** `DotConvertibleByContractUser` — anyone getting a refund from it gets DOT, because we know pre-upgrade deposits were never PGAS.
3. **New trait on `pallet_revive::Config`** (`PgasDeposit` or similar) for the PGAS asset access. Default impl `()` is a no-op (DOT-only, today's behavior).

## Open items to confirm during implementation

- Where does PGAS "live" while backing a storage deposit? Proposal: transfer PGAS from user → pallet account (no hold). Reason: `pallet-pgas-allowance` today uses `fungibles::Balanced` (withdraw/resolve) without holds; the pallet account does not dispatch so holding vs. not holding is equivalent. Confirm no code paths would sweep pallet-account PGAS.
- Whether `CodeInfo`'s storage layout bump needs a storage migration or can piggy-back on `behaviour_version`.

---

## Task breakdown

### A. Config surface

- [ ] Define trait `PgasDeposit<T: Config>` in `src/evm/fees.rs` (or new `src/pgas.rs`) exposing:
  - `fn reducible_pgas(who: &AccountIdOf<T>) -> BalanceOf<T>`
  - `fn withdraw_pgas(who, amount) -> Option<Credit>` + `deposit_pgas(who, credit)`
  - Default impl for `()` returns zero / None (keeps existing runtimes working).
- [ ] Add `type PgasDeposit: PgasDeposit<Self>` to `pallet_revive::Config` with default `()` under the macro helper.
- [ ] Add new deposit-related `HoldReason` entries only if we actually need them (probably not — see "open items").

### B. New storage + types

- [ ] `DotConvertibleByContractUser<T> = StorageDoubleMap<_, Identity H160, Blake2_128Concat AccountIdOf<T>, BalanceOf<T>, ValueQuery>`.
- [ ] Add `pub historic_deposit: BalanceOf<T>` to `ContractInfo<T>` (`src/storage.rs:90`). Default `0` for new contracts.
- [ ] New enum `DepositAsset { DotConvertible, Pgas }` in `src/lib.rs` (module-private, pub(crate)).

### C. Contract storage deposit (`Pallet::charge_deposit` / `refund_deposit`)

Files: `src/lib.rs:2601,2651`, `src/metering/storage.rs:513` (`ReservingExt::charge`).

- [ ] Rework `ReservingExt::charge` to pass through the full **per-coalesced-contract** charge/refund so we can decide PGAS vs DOT once. (The `execute_postponed_deposits` loop already coalesces charges per contract — good.)
- [ ] **Charge:**
  1. If `PgasDeposit::reducible_pgas(origin) >= amount`: withdraw PGAS credit from origin; resolve into contract account. Do not touch `DotConvertibleByContractUser`. Emit `DepositCharged { contract, who, amount, asset: Pgas }`.
  2. Else: existing DOT path (respecting `collect_deposit_from_hold`). Emit the same event with `asset: DotConvertible`. Increment `DotConvertibleByContractUser[c, u] += amount`.
- [ ] **Refund:**
  1. Read `historic = contract_info.historic_deposit`.
  2. `historic_refund = min(amount, historic)`. If non-zero: run existing DOT refund path (transfer from contract back to origin as DOT, because historic is guaranteed DOT backing). Decrement `contract_info.historic_deposit` by `historic_refund`. `amount -= historic_refund`.
  3. `dot_left = DotConvertibleByContractUser[c, u]`. `dot_refund = min(amount, dot_left)`. If non-zero: existing DOT refund path. Decrement map entry (remove if 0). `amount -= dot_refund`.
  4. Remainder: withdraw PGAS from contract account; resolve to origin. Emit event.

Note: the existing DOT refund uses `T::Currency::transfer_on_hold` when there's a hold on the contract account. We need to check that the hold amount covers "historic + new DOT". Since the contract holds `storage_base_deposit + extra_deposit` all as DOT on the existing hold, historic dip is consistent.

- [ ] For PGAS refund: the contract account is not a dispatching account; PGAS sits as free balance on it. Use `fungibles::Balanced::withdraw` on the contract → `resolve` on origin. If the contract balance is short, that's a bug → hard error.

### D. Termination (`exec.rs:1769` + `metering/storage.rs`)

- [ ] Confirm `transaction_meter.terminate(contract, refund)` only hits a single origin refund in practice. Add debug assertion that the refund is ≤ `historic_deposit + sum_user_entries + pgas_backing` for the contract.
- [ ] When a contract is terminated: if there's residual `historic_deposit`, refund it to the *caller of terminate* (usually the contract beneficiary path already handles this — re-check). Zero the `DotConvertibleByContractUser` rows for that contract and the `historic_deposit`.

### E. Code upload deposit (`vm/mod.rs`)

- [ ] Extend `CodeInfo<T>` with `deposit_asset: DepositAsset`. Bump `behaviour_version` or add behind `Option` for backwards-decoding.
- [ ] `ContractBlob::store_code` (`vm/mod.rs:184`): select PGAS if `PgasDeposit::reducible_pgas(owner) >= deposit`; store asset tag accordingly. Branch the charge path.
- [ ] `ContractBlob::remove` + `CodeInfo::decrement_refcount` (`vm/mod.rs:162,282`): refund in the recorded asset.
- [ ] Keep `HoldReason::CodeUploadDepositReserve` for the DOT path only; PGAS path uses plain asset transfer to the pallet account.

### F. Address mapping deposit (`address.rs:144`)

- [ ] Add a new pallet storage map `MappingDepositAsset<T> = StorageMap<_, Identity, H160, DepositAsset>` (only populated when PGAS was used; DOT is the implicit default).
- [ ] `map()`: prefer PGAS if covered, otherwise existing DOT `hold()`. Write `MappingDepositAsset` when PGAS.
- [ ] `unmap()`: branch on `MappingDepositAsset` presence. Clear it on unmap.

### G. Migration

- [ ] New migration module `src/migrations/pgas_deposit_historic.rs`.
  - For every `AccountInfoOf` whose `account_type` is `Contract`, set `historic_deposit = storage_base_deposit + storage_byte_deposit + storage_item_deposit`.
  - Note: we do not need to initialize `DotConvertibleByContractUser` — empty map means "no per-user DOT entitlement beyond the historic bucket".
- [ ] For `CodeInfo`: no data migration needed if we default `deposit_asset` to `DotConvertible` (matches historic behavior).
- [ ] For `OriginalAccount`: same, historic mappings stay DOT-backed.
- [ ] Tag the migration with proper `STORAGE_VERSION` bump. Add `try-runtime` checks.

### H. Tests (mock + unit)

`src/mock.rs` changes:
- [ ] Wire `pallet-assets` (or the chosen fungibles impl) into the mock with a PGAS asset id. Implement `pallet_revive::Config::PgasDeposit`.
- [ ] Helpers: `mint_pgas(who, amount)`, `pgas_balance(who)`.

New tests in `src/tests.rs` (or a new `src/tests/pgas_deposit.rs`):
- [ ] Alice all-PGAS storage deposit → refund is all-PGAS; map stays empty; contract PGAS balance matches.
- [ ] Alice DOT-only (no PGAS) → map bumps; refund ≤ map returns DOT; map decrements.
- [ ] Alice pays DOT, later a larger refund event → excess over map returns PGAS (engineer via storage growth price change or manual refund helper).
- [ ] Two users into one contract, one PGAS, one DOT → each gets their own asset on refund.
- [ ] Termination with mixed PGAS+DOT backing refunds correctly; rows and historic zeroed.
- [ ] Code upload: PGAS path + DOT path; removal refunds in same asset.
- [ ] `map_account` / `unmap_account` both asset paths.

Migration test:
- [ ] Pre-upgrade state: contract with DOT deposit. Post-migration: `historic_deposit` == total pre-deposit. First refund returns DOT from the historic bucket even though map is empty.

### I. prdoc

- [ ] `prdoc/pr_NNNN.prdoc` — bump level **minor** (new `Config` associated type + storage layout change). Confirm with user before `gh-pr-init`.

### J. Verification

- [ ] `SKIP_WASM_BUILD=1 cargo check -p pallet-revive --all-targets --all-features`
- [ ] `SKIP_WASM_BUILD=1 cargo clippy -p pallet-revive --all-targets --all-features`
- [ ] `cargo test -p pallet-revive --profile testnet`
- [ ] `cargo +nightly fmt` (only touched files)

## Implementation order

1. A, B (scaffolding without behavior change).
2. C (contract storage — the load-bearing part).
3. D (termination path).
4. E, F (code-upload + address-mapping — parallel, simpler).
5. G (migration) — after behavior is stable.
6. H (tests) — continuous, finished after G.
7. I, J (prdoc + verify).

## Review section (filled in after implementation)

_empty_
