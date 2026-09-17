# Working on pallet-revive

These rules apply to this pallet and the crates below it. Read the [README](README.md) for the
pallet's behavior and the SDK's [contribution guide](../../../docs/contributor/CONTRIBUTING.md) for
the workflow. Use the linked documents for detailed procedures; this file records the rules and why
they matter.

## Contract syscalls must remain backwards compatible

The API exposed to contracts is the syscall API. Deployed contract code is immutable: it cannot be
updated to accommodate a changed syscall. Backwards compatibility is a hard requirement.

Preserve existing syscall identifiers, signatures, argument and return encodings, memory contracts,
and observable semantics, including error behavior. Do not remove or repurpose an existing syscall.
When a different interface or incompatible behavior is needed, add a new syscall and keep the old
one supported. Updating Rust wrappers, generated bindings, or crate versions does not update
deployed contracts and cannot make an incompatible change safe.

## The runtime API must support independent RPC upgrades

The runtime interface consumed by RPC implementations must remain backwards and forwards compatible.
Older clients must work with newer runtimes, and newer clients must work with older runtimes through
mutually supported API versions. Breaking this boundary is highly disruptive: do not require a
coordinated RPC and runtime upgrade.

Follow the [versioning guide](types/GUIDE.md) when changing this interface. It owns the rules for
wire types, payload versions, conversions, capability selection, and compatibility verification.
Keep execution types separate from wire types so an internal refactor cannot accidentally change the
API.

## Cargo features are purely additive

Cargo features make optional code and dependencies available, saving compilation when that
capability is not needed. They must not select different behavior for existing code, change
defaults, or start services. Cargo unifies dependency features: another crate in the build can
enable a feature without the application's author choosing it. A feature named experimental is not
exempt from this rule.

Prefer a runtime check and explicit configuration to select behavior. For example, compiling support
for an optional service may make its implementation available, but only runtime opt-in may bind its
port, open its database, or start its background work. With the feature enabled and no opt-in, the
application must behave as it does with the feature disabled.

If selection must happen at compile time, use an explicit custom `--cfg`, not a Cargo feature, and
explain why a runtime check is insufficient. Register the cfg name in the workspace's `check-cfg`
configuration and propagate it deliberately into nested runtime builds where required. Preserve
existing compiler flags. Build scripts should inspect `CARGO_CFG_*`, not search `RUSTFLAGS` text for
a substring.

Neither features nor custom cfgs permit changing existing syscall or host-function contracts. Nodes
must execute the same deployed runtime with the same semantics regardless of local build options.
Consensus behavior must use runtime-controlled configuration, not machine-local settings.

Test feature-off, feature-on without opt-in, and feature-on with opt-in. Exercise both sides of
custom cfgs in CI; a successful default build says nothing about code it excludes. Group conditional
implementation behind module boundaries instead of scattering identical cfgs across imports and
items. Do not add unused forwarding features just because a dependency exposes them.

## Establish visibility through module hierarchy

Do not use scoped visibility modifiers such as `pub(crate)`, `pub(super)`, or `pub(in ...)`.
Structure module hierarchies properly and expose the intended interface through focused public
re-exports. Per-item modifiers require remembering the restriction on every type and item. A proper
hierarchy establishes the boundary once. Scoped modifiers also make it easy to work around a poorly
structured module tree instead of fixing it.

Keep implementation modules private. An item can be `pub` within a private module without exposing
the whole module outside the crate. Re-export the items that belong to the public interface. Do not
make an entire module public to expose one helper or make a test compile. Prefer an inherent method
when an operation belongs to an existing type rather than adding another free-standing export.

## Types and implementation

- Do not alias primitive types. `type Counter = u64` adds another name to remember without
  preventing a counter from being confused with any other `u64`. Use the primitive directly, or a
  newtype when distinguishing values or enforcing construction rules provides actual type safety.
- Reuse canonical types, constants, and helpers. Do not introduce a second representation or a new
  dependency just to convert to and from the type the surrounding code already uses.
- Keep one source of truth. If an enum variant already determines an operation, do not carry a
  second field describing that operation and add checks to keep them synchronized. Fix the
  representation.
- Reuse already-loaded state and existing transaction helpers. Avoid reading the same storage key
  twice or reimplementing a storage transaction around a helper that already provides one.
- Add abstractions and derives for concrete needs, not possible future callers. Keep fields private
  where they protect invariants; do not add accessors that merely expose every implementation
  detail. Preserve derives required by FRAME, serialization, storage, or an existing public
  interface.
- Keep dependencies in the workspace and inherit them from member crates. Preserve the runtime's
  `no_std` support and existing use of `alloc`; keep host-only dependencies out of runtime code.
- Prefer safe operations. Validate untrusted lengths, ranges, conversions, and alignment before
  using them. Malformed contract or runtime input must not panic the host. For unavoidable unsafe
  code, document the safety invariant and why it holds at the operation. A caller-upheld
  memory-safety precondition must not be hidden behind a safe function signature.
- Fix the underlying cause instead of suppressing a diagnostic or adding a fallback that hides an
  error. Follow the SDK's [style guide](../../../docs/contributor/STYLE_GUIDE.md) for panic proofs
  and safety justification. Keep error propagation and state changes consistent with the call
  boundary.

## Tests must exercise the contract behavior

Write new contract tests in Solidity and run them through all supported backends using the existing
Solc and Resolc harness. Existing Rust contract fixtures are legacy; do not add new ones. Rust
harness code may deploy Solidity fixtures and assert outcomes, but the contract scenario belongs in
Solidity.

Use the real execution path. A helper-only test does not prove that the contract can reach the
helper with the intended arguments, or that failure rolls back the right state. Assert observable
results and the invariants at risk: returned values and errors, state changes, balances, and
deposits. Cover failure and boundary cases, not just successful calls. Preserve documented backend
differences explicitly in the assertions. Do not weaken expectations to match the implementation
being tested.

Keep regression cases deterministic. Expensive searches for a suitable input belong in fixture
preparation, not in every test run. Preserve coverage for existing callers when adding interfaces;
use the versioning guide's checks for runtime API compatibility.

Report the command, configuration, and result that actually ran. Check that a filtered run selected
the intended tests. Skipping fixture compilation is useful for static checks, but is not evidence
that contract tests passed. Distinguish missing prerequisites and infrastructure failures from
source failures, and local validation from CI results.

## Performance and weights

Measure before claiming a performance improvement. Inspect repeated storage reads, allocations, and
copies on the affected path. Removing work is preferable to adding a cache or another representation
that must be kept consistent.

When changing metered work, check reference time, proof size, database reads and writes, and refund
accounting. Check empty inputs: a benchmark's fixed cost must not accidentally charge every call for
an optional operation it does not perform. Keep fixture setup outside the measured operation unless
that setup is part of the cost being modeled.

Regenerate affected weights when the measured work changes. Follow the [weight-generation
procedure](../../../docs/contributor/weight-generation.md). Do not claim existing weights cover a
changed benchmark merely because the tests pass.

## Documentation and review

Explain contracts and non-obvious invariants, not a history of how the change was developed. Follow
the [documentation guidelines](../../../docs/contributor/DOCUMENTATION_GUIDELINES.md). Keep code and
repository prose within the SDK's line-width conventions, and link to the document that owns a
procedure instead of copying it. Use plain engineering prose without mannered speech, em dashes,
or private provenance. Avoid ornate phrasing, stock transitions, and inflated claims. Say directly
what changed, why it matters, and what the reader needs to do.

Keep a change focused on its stated behavior. Split unrelated fixes so they can be reviewed and
backported independently.

## Prdocs

Handwrite the prdoc as a porting guide for downstream users and integrators. Do not use the bot to
generate it from the PR description or copy the description into it. Review notes explain an
implementation to reviewers; a prdoc must explain the released change to someone upgrading their
code or deployment.

Describe the observable change and who is affected. For changes requiring action, explain the old
and new behavior and give concrete migration steps: which APIs, configuration, or runtime
integration need updating, in what order, and why. Include a small before-and-after example when it
makes the migration clearer. State relevant compatibility constraints and upgrade requirements. If
no migration is needed, say so rather than leaving the reader to infer it.

Use the [SDK prdoc guide](../../../docs/contributor/prdoc.md) for the file format, audience values,
crate entries, version-bump rules, and validation commands. Its generation workflow does not apply
here: write the content by hand. List the affected published crates and assess their actual
compatibility impact. A new variant in a public exhaustive enum or a required method on a public
trait can break downstream code even if the change looks additive locally.

Keep the prdoc up to date as the implementation and scope change. Before requesting review or
merging, compare it with the final diff: update migration instructions, audiences, crate entries,
and version bumps, and remove stale claims. Verify examples against the resulting interface. The
prdoc must describe what will ship, not an earlier revision of the PR.

## Before review

Check the change against these rules and record the relevant results:

- The diff is scoped; new types, dependencies, derives, cfgs, and unsafe operations have concrete
  reasons, and module boundaries enforce the intended interface.
- Existing syscalls remain compatible. Runtime API changes follow the linked versioning procedure
  and preserve interoperability across versions.
- Feature combinations preserve defaults; new contract behavior has Solidity coverage across
  backends, including relevant failure cases.
- The applicable checks ran with the required fixtures and configurations. Missing coverage or
  blocked checks are identified, and performance claims have measurements.
- Documentation and affected weights are current. The PR has the required component label and a
  handwritten prdoc with accurate porting instructions matching the final diff, or qualifies for
  the documented `R0-no-crate-publish-required` exemption.

Use the SDK's [validation and formatting guidance](../../../docs/contributor/CONTRIBUTING.md) and
[Markdown checks](../../../docs/contributor/markdown_linting.md). Choose checks appropriate to the
change; a documentation-only edit does not need a runtime build.
