# Plan: Deprecating `RuntimeVersion` for JAM runtimes

## 1. Goal

For parachains running on JAM (via the parachain service) we want to stop relying on
`RuntimeVersion` as the source of truth about a runtime. Instead:

- **A runtime is identified by its code hash.** Node and runtime both use the hash of the code
  to tell runtimes apart and to detect upgrades.
- **The node learns everything it needs from data embedded in the runtime blob**, without
  executing it:
  - which runtime APIs (and which versions) the runtime implements,
  - which consensus-relevant "system features" it uses (state version, ...).
- **`RuntimeVersion` / `Core::version` stay only for backwards compatibility** with UIs and
  wallets (Polkadot-JS, PAPI). Only `spec_version` remains meaningful there.

## 2. Background: what `RuntimeVersion` is used for today

`RuntimeVersion` bundles a lot of different information:

| Field | What it is used for today |
|---|---|
| `spec_name` | Runtime upgrade detection (`LastRuntimeUpgrade`), `can_set_code`, Polkadot-JS registry/types selection, wasm overrides |
| `spec_version` | Runtime upgrade detection, `CheckSpecVersion` tx extension, Polkadot-JS metadata caching, RFC-78 metadata hash, wasm overrides / code substitutes |
| `transaction_version` | `CheckTxVersion` tx extension |
| `impl_name`, `impl_version`, `authoring_version` | Effectively unused |
| `apis` | Node: which runtime APIs exist (`has_api`, `api_version`). Polkadot-JS: whether the `Metadata` API is available |
| `system_version` | Consensus: trie state version, extrinsics root version, whether `:pending_code` is used |

### 2.1 Runtime side consumers

- **Upgrade detection** in `frame-executive` (`runtime_upgraded()`) compares the current version
  with `frame_system::LastRuntimeUpgrade` using `spec_version` and `spec_name`
  (`substrate/frame/system/src/lib.rs:1267`).
- **`can_set_code`** checks the `spec_name`/`spec_version` of new code. This will be changed
  separately and is not relevant for JAM chains (they upgrade by hash).
- **`CheckSpecVersion` / `CheckTxVersion`** put `spec_version` / `transaction_version` into the
  signed payload of every transaction.
- **`system_version`** decides whether `set_code` writes to `:code` or `:pending_code`
  (`substrate/frame/system/src/lib.rs:1596`, `validate_block_core.rs:255`).

### 2.2 Node side consumers

- **State version:** the node derives the trie state version from `system_version` when executing
  blocks (`substrate/client/service/src/client/call_executor.rs`) and the extrinsics root
  version when building blocks (`substrate/client/block-builder/src/lib.rs:333`). This is
  consensus critical.
- **Runtime API discovery:** `has_api` / `api_version` are used by aura, babe, the transaction
  pool, offchain workers, block builder, the cumulus collator and more.
- **Wasm overrides & code substitutes** are keyed by `spec_name` / `spec_version`
  (`substrate/client/service/src/client/{code_provider,wasm_override,wasm_substitutes}.rs`).
- **RPC:** `state_getRuntimeVersion`, `state_subscribeRuntimeVersion` and the `newRuntime`
  event of `chainHead_v1_follow`.

### 2.3 How the clients use it

**PAPI (polkadot-api v2):**

- Identifies runtimes by the hash of `:code` and caches metadata per code hash. It never calls
  `Core_version`.
- Fetches the code hash whenever `chainHead_v1_follow` emits a `newRuntime` event.
- Discovers runtime APIs from the metadata (v15+), not from `RuntimeVersion.apis`.
- Takes `spec_version` / `transaction_version` for signing from the **`System::Version`
  metadata constant**. The runtime computes this constant when metadata is requested; it must
  exist and be a struct.

**Polkadot-JS (`@polkadot/api` v17):**

- Never calls `Core_version` directly; everything comes from the `state_getRuntimeVersion` and
  `state_subscribeRuntimeVersion` RPCs.
- Reloads metadata **only when `specVersion` changes** (equality check, no ordering is required).
  If `specVersion` stays the same across an upgrade, Polkadot-JS silently keeps using the old
  metadata.
- Caches metadata/registries by `genesisHash + specVersion` and `specName + specVersion`.
- Uses `apis` to check that the `Metadata` runtime API (version >= 2) exists; otherwise it falls
  back to v14 metadata.
- Signs transactions with the `specVersion` / `transactionVersion` from the RPC.

**Ledger:** no longer uses `transaction_version`; app versions that did are long incompatible.

## 3. Decisions

### 3.1 Keep `RuntimeVersion` as a compatibility shim

We keep `Core::version` and `RuntimeVersion`, but only `spec_version` has to be set (and
increased on every upgrade) by the developer. All other fields are filled with defaults:

| Field | Value |
|---|---|
| `spec_name` | Package name of the runtime crate (`CARGO_PKG_NAME`) |
| `impl_name` | Same as `spec_name` |
| `spec_version` | Provided by the developer, increased on every upgrade |
| `impl_version`, `authoring_version` | `0` |
| `transaction_version` | `0` |
| `apis` | `RUNTIME_API_VERSIONS` (generated anyway) |
| `system_version` | Latest legacy value, only for compatibility (see 3.3) |

Why `spec_version` must stay:

- Polkadot-JS reloads metadata only when it changes (§2.3).
- Both clients sign with it (`CheckSpecVersion`), so a transaction signed for the old runtime is
  rejected by a new one.
- The RFC-78 metadata hash (`CheckMetadataHash`) contains `spec_version` and is computed at build
  time, so a static `spec_version` stays consistent with what wallets compute.

`spec_version` is not derived from the code hash: the RFC-78 metadata hash is computed at build
time, before the code hash is known.

### 3.2 Generate `Core::version` from `impl_runtime_apis!`

`impl_runtime_apis!` gets a `#[spec_version(N)]` attribute on the `Core` implementation:

```rust
impl_runtime_apis! {
	#[spec_version(7)]
	impl sp_api::Core<Block> for Runtime {
		fn execute_block(block: <Block as BlockT>::LazyBlock) {
			Executive::execute_block(block)
		}

		fn initialize_block(header: &<Block as BlockT>::Header) -> ExtrinsicInclusionMode {
			Executive::initialize_block(header)
		}
	}
}
```

Rules:

- With the attribute, `fn version()` is omitted by the developer and generated by the macro.
- A manually implemented `fn version()` without the attribute emits a **deprecation warning**
  (via `proc-macro-warning`, already used by `frame-support-procedural`).
- Providing both is a compile error.
- `execute_block` / `initialize_block` still have to be written manually, as they depend on
  `Executive`, which the macro does not know about.

The macro also generates:

- `pub const RUNTIME_VERSION: RuntimeVersion`, to be used as `frame_system::Config::Version`
  (needed by `CheckSpecVersion` and the `System::Version` metadata constant).
- The `runtime_version` custom section, as `#[sp_version::runtime_version]` does today.

### 3.3 `system_version` becomes a `SystemFeatures` bitfield

`system_version` is a single number that bundles independent features, and the node interprets
unknown values as "latest". It is replaced by a bitfield:

```rust
bitflags! {
	pub struct SystemFeatures: u32 {
		const STATE_V1           = 1 << 0;
		const EXTRINSICS_ROOT_V1 = 1 << 1;
	}
}
```

Mapping of the existing system versions:

| Legacy `system_version` | Meaning | `SystemFeatures` |
|---|---|---|
| 0 | State V0, extrinsics root V0 | `0b00` |
| 1 | State V1 | `0b01` |
| 2 | + Extrinsics root V1 | `0b11` |
| 3 | + Delayed upgrades via `:pending_code` | `0b11` |

The `:pending_code` behaviour of version 3 does not get a flag: on JAM delayed upgrades are the
default (`pallet-jam-system` upgrades by hash via `PendingCodeHash`).

The node **rejects a runtime that sets a bit it does not know**.

The legacy `u8` stays in `RuntimeVersion` for compatibility. The node only falls back to it for
runtimes that do not ship the `SystemFeatures` section.

### 3.4 Runtime APIs and `SystemFeatures` are embedded as custom sections

The node reads both from the runtime blob without executing it:

- **Runtime APIs:** the existing `runtime_apis` section, generated by `impl_runtime_apis!`.
- **System features:** a new `system_features` section.

**Wasm:** works out of the box. Any `#[link_section = "name"]` static becomes a wasm custom
section; sections with the same name are concatenated. This is already how `runtime_apis` and
`runtime_version` are produced and read (`read_embedded_version` in
`substrate/client/executor/src/wasm_runtime.rs`).

**PVM:** the blob format supports it, the tooling does not yet:

- Section ids `>= 128` are optional and skipped by parsers that do not know them, so adding
  sections does not break existing PolkaVM / JAM implementations.
- `ProgramBlobBuilder::add_custom_section` already exists (used for debug info).
- **Missing:** `polkavm-linker` has no way to turn an ELF section into a custom blob section.
  Unknown allocated sections end up as read-only data. Today the `runtime_apis` statics are
  therefore just dead bytes in the program memory of a PVM runtime.
- **Missing:** `ProgramParts` does not expose unknown optional sections, and
  `RuntimeBlob::custom_section_contents` only supports wasm.

We fix this **upstream in PolkaVM** (see 4.1), so that the same `#[link_section]` code works for
both targets.

### 3.5 Runtime upgrade detection by code hash

`pallet-jam-system` stores a `CurrentCodeHash`:

- set at genesis,
- updated where the pending upgrade is applied (`cumulus/jam/pallet-jam-system/src/upgrade.rs`).

Upgrade detection (whether to run migrations / `on_runtime_upgrade`) compares against this hash
instead of `LastRuntimeUpgrade`'s `spec_name` / `spec_version`.

### 3.6 `chainHead_v1_follow` detects new runtimes by code hash

Today the `newRuntime` event is emitted when the `RuntimeVersion` of a block differs from its
parent (`substrate/client/rpc-spec-v2/src/chain_head/chain_head_follow.rs:252`). This is
changed to compare the hash of `:code`. That is cheaper and does not depend on developers
bumping `spec_version`.

The event payload (`ChainHeadRuntimeVersion`) carries almost the entire `RuntimeVersion`
(`specName`, `implName`, `specVersion`, `implVersion`, `apis`, `transactionVersion`), as mandated
by the JSON-RPC spec. Until the spec changes, it is filled from the compatibility
`RuntimeVersion`.

**The JSON-RPC spec should be changed to remove the runtime version from the `newRuntime`
event** and carry the code hash instead. Clients can fetch everything else they need (runtime
APIs, `System::Version`) from the metadata.

## 4. Work items

### 4.1 Upstream: PolkaVM

- `polkavm-linker`: copy ELF sections with a well-known prefix (e.g. `.polkavm_custom.<id>`) into
  optional blob sections instead of treating them as read-only data.
- Reserve section ids for `runtime_apis` and `system_features`, or add one generic "named custom
  section" id to avoid clashes with future upstream sections.
- `polkavm-common`: expose custom sections when parsing a blob.

### 4.2 Upstream: JSON-RPC spec

- Change the `newRuntime` event of `chainHead_v1_follow` to carry the code hash instead of the
  runtime version (`spec`). Afterwards update `rpc-spec-v2` accordingly.

### 4.3 Primitives & macros

- `sp-version`: add `SystemFeatures` (+ conversion from the legacy `system_version`), and the
  `system_features` section emission.
- `sp-api` (`impl_runtime_apis!`): `#[spec_version(N)]`, generated `fn version()`, generated
  `RUNTIME_VERSION` const + `runtime_version` section, deprecation warning for manual
  `fn version()`.

### 4.4 Node

- `sc-executor`: read `runtime_apis` and `system_features` from PVM blobs (extend
  `RuntimeBlob::custom_section_contents`).
- `call_executor.rs` / `block-builder`: take state version and extrinsics root version from
  `SystemFeatures`, falling back to the legacy `system_version`. Reject unknown bits.
- `rpc-spec-v2`: detect runtime changes in `chainHead_v1_follow` by code hash.
- Optional: key wasm overrides / code substitutes by code hash instead of `spec_version`.

### 4.5 Runtime / pallets

- `pallet-jam-system`: `CurrentCodeHash` storage, set at genesis and on upgrade.
- Upgrade detection for JAM runtimes based on `CurrentCodeHash`.
- JAM runtimes switch to `#[spec_version(N)]`.

## 5. Open points

- **Scope of the deprecation warning:** ~35 runtimes in this repository implement
  `fn version()` manually. Either they are all migrated together with the macro change, or the
  warning is introduced gradually.
- **Section id reservation** has to be agreed on with the PolkaVM maintainers.
