# Versioning Guide

This document presents a guide to versioning of pallet-revive's runtime API functions. Specifically, how existing
un-versioned functions can be versioned and how versioned functions can be updated. It is written in a simple way which
can be followed either by you or your agent and could even be turned into a skill if you wanted to use it that way
(although we're not versioning the runtime APIs often enough for a skill to be too useful).

Versioning an un-versioned runtime API function or updating an existing runtime API function is a simple process but
it's quite mechanical and this document describes the steps in full which need to be taken to do it.

> [!NOTE]
> All of the paths provided in this document are relative to `polkadot-sdk/substrate/frame/revive`.

## Nomenclature

This section introduces a number of terms which will be used throughout this document and defines them once for the
purpose of allowing the document to flow in a more natural way.

<table>
  <thead>
    <tr>
      <th>Term</th>
      <th>Definition</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td><strong>Execution Type</strong></td>
      <td>
        A Rust type which is used by pallet-revive for its internal computation, storage, and usage and which should not
        be exposed through any of the runtime API functions.
      </td>
    </tr>
    <tr>
      <td><strong>Wire Type</strong></td>
      <td>
        A Rust type which is used by pallet-revive in its runtime API functions either somewhere in the input or in the
        output and never used internally in any way which isn't simple conversions.
      </td>
    </tr>
    <tr>
      <td><strong>Non-Primitive Types</strong></td>
      <td>
        This refers to the non-primitive types which can be found in the signature of runtime API functions either as
        arguments or as output. Types such as <code>H256</code>, <code>U256</code>, <code>String</code>,
        <code>Vec</code> are treated as primitive types while other types which are defined in pallet-revive are treated
        as non-primitive types. For example, when versioning the <code>eth_receipt_data</code> runtime API function, we
        treated the <code>ReceiptGasInfo</code> as a non-primitive type and treated the numeric types used within it as
        primitive types.
      </td>
    </tr>
  </tbody>
</table>

## Versioning an Unversioned Runtime API Function

Let's assume that we want to version a runtime API function called `${function-name}`.

### Setup

- Find the existing runtime API function defined in pallet-revive in `src/lib.rs` and collect the set of non-primitive
  types in its signature.

### Type Definitions

- If the `${function-name}` signature contains non-primitive types which are not already defined in the
  `pallet-revive-types` crate then define them with a `V1` postfix in either an existing module or a new file module in
  the `types/src/runtime_api/types` module. This step makes new type definitions for new wire type(s).
  - **Example:** When versioning the `block_hash` runtime API function there were no non-primitive types and therefore
    we did not need to define any new types in the `types/runtime_api/types` module.
  - **Example:** When versioning the `eth_receipt_data` runtime API function we found two primitive numeric types and a
    single non-primitive type which contained them: `ReceiptGasInfo`. Since that type was not already defined in the
    `types/runtime_api/types` we defined it in a new file module called `receipt` and named the type `ReceiptGasInfoV1`
    according to the rules here.
- If, as part of the step above, new types are defined then appropriate conversion traits need to be implemented for
  them in `pallet-revive`. These traits need to be implement right underneath their equivalent execution types.
  - If the wire type is used as an input type then add a conversion of `From<WireType> for ExecutionType`
  - If the wire type is used as an output type then add a conversion of `From<ExecutionType> for WireType`
  - If it’s being used as both then add both conversion implementations.
  - **Example:** After the `ReceiptGasInfoV1` type was added to the `pallet-revive-types` crate we added a
    `From<ReceiptGasInfo> for ReceiptGasInfoV1` implementation in the `src/evm/block_hash.rs` module underneath the type
    definition of the execution type directly. The direction of the conversion was selected in this way since
    `ReceiptGasInfoV1` is an output type. The reverse conversion was not implemented since it's never used anywhere as
    an input to a runtime API function.

### Payload Definitions

- Add a new file module in `types/src/runtime_api/payloads` which carries the same name as the runtime API function
  which is being versioned and wire it up into the `mod.rs` file. The contents of this file need to be exactly the
  following:
  - The Polkadot-sdk license header present in all of the other files present in this repo.
  - The required `use` statements needed for this file (including imports for common collections such as `Vec` from
    `alloc` since the `pallet-revive-types` crate can be compiled with `no-std`).
  - A struct with the following specifications:
    - Named `${function-name} InputPayloadV1` (we're using V1 here since this is the first version of that payload)
    - With named fields with field names matching the argument names of the existing unversioned `${function-name}`
      runtime API function. This may optionally be a no-field struct if `${function-name}` takes no arguments.
    - Derives `TypeInfo, Debug, Clone, Encode, Decode, PartialEq` with `PartialEq` being an optional derive if the types
      used for the fields can not satisfy `PartialEq`.
    - With the same fields as the unversioned runtime API function `${function-name}`.
    - With the same generics as the unversioned runtime API function `${function-name}` has in its arguments.
  - An enum with the following specifications:
    - Named `${function-name} VersionedInputPayload`
    - With a single variant named `V1` which uses tuple-fields with a single field of the type
      `${function-name} InputPayloadV1`.
    - Derives `TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto` with `PartialEq` being an optional
      derive if the types used for the fields can not satisfy `PartialEq`.
    - With the same generics as the unversioned runtime API function `${function-name}` has in its arguments.
  - A struct with the following specifications:
    - Named `${function-name} OutputPayloadV1` (we're using V1 here since this is the first version of that payload)
    - With named fields matching the intended return type's semantic meaning of the unversioned runtime API function
      `${function-name}`. This may optionally be a no-field struct if the runtime API function has no returns.
    - Derives `TypeInfo, Debug, Clone, Encode, Decode, PartialEq` with `PartialEq` being an optional derive if the types
      used for the fields can not satisfy `PartialEq`.
    - With the same generics as the unversioned runtime API function `${function-name}` has in its return type.
    - If the return type of `${function-name}` is an `Option<T>` then this struct needs to have a single named-field of
    type `Option<T>` (notice that we didn't just make it `T`). If the return type of `${function-name}` is
    `Result<T, E>` then this struct needs to have a single named-field of type `T`.
    <!--
    TODO: The above needs to change once/if we decide to version the error types but it's currently something that we do
    not do.
    -->
  - An enum with the following specifications:
    - Named `${function-name} VersionedOutputPayload`
    - With a single variant named `V1` which uses tuple-fields with a single field of the type
      `${function-name} OutputPayloadV1`.
    - Derives `TypeInfo, Debug, Clone, Encode, Decode, PartialEq, From, TryInto` with `PartialEq` being an optional
      derive if the types used for the fields can not satisfy `PartialEq`.
    - With the same generics as the unversioned runtime API function `${function-name}` has in its return type.

### Pallet Revive Execution Types

- Create a new module in `src/runtime_api` named `${function-name}` and wire it up to the `mod.rs` file. The contents of
  this file need to be exactly the following:
  - The Polkadot-sdk license header present in all of the other files present in this repo.
  - The required `use` statements needed for this file (including imports for common collections such as `Vec` from
    `alloc` since the `pallet-revive-types` crate can be compiled with `no-std`).
  - A struct with the following specifications:
    - Named `${function-name} InputPayload`.
    - With the same fields as the unversioned runtime API function `${function-name}` has as arguments but with the
      field types being the appropriate execution types.
    - With the same generics as the unversioned runtime API function `${function-name}` has in its inputs.
    - Never derive `Encode`, `Decode`, `TypeInfo`, `Serialize`, or `Deserialize` for this type since it's never meant to
      cross the wire or be serialized in any capacity.
    - **Example:** the execution type of the `trace_block` runtime API function input in
      `src/runtime_api/trace_block.rs` has two fields: a generic `Block` and the execution type `TracerType`. Notice
      that the type used in this definition is an execution type and not the `TracerTypeV1` wire type.
  - An implementation of `From<${function-name} VersionedInputPayload> for ${function-name} InputPayload`.
  - An implementation of `From<${function-name} InputPayloadVn> for ${function-name} InputPayload` for each version
    which is defined (only one in this case since this is a newly versioned runtime API)
  - A struct with the following specifications:
    - Named `${function-name} OutputPayload`.
    - With the same fields as the unversioned runtime API function `${function-name}` has as return type(s) but with the
      field types being the appropriate execution types.
    - With the same generics as the unversioned runtime API function `${function-name}` has in its return type(s).
    - Never derive `Encode`, `Decode`, `TypeInfo`, `Serialize`, or `Deserialize` for this type since it's never meant to
      cross the wire or be serialized in any capacity.
    - **Example:** the execution type of the `trace_block` runtime API function output in
      `src/runtime_api/trace_block.rs` has a single field of the type `Vec<(u32, Trace)>`. Notice that the type used in
      this definition is an execution type and not the `TraceV1` wire type.
  - An implementation of `From<${function-name} OutputPayload> for ${function-name} OutputPayloadVn` for each version
    which is defined (only one in this case since this is a newly versioned runtime API)
  - Never implement `From<${function-name} OutputPayload> for ${function-name} VersionedOutputPayload` since there is no
    way to tell what version the execution output needs to be converted into and therefore it's the responsibility of
    the caller to make that conscious decision on their own.

### Adding The New Runtime API

- Declare a new runtime API function in pallet-revive in `src/lib.rs` in the `decl_runtime_api` block with the following
  specifications:
  - Named `${function-name}_versioned`.
  - Carries a `#[api_version(2)]` attribute on it.
  - Carries no comments.
  - With a single argument called `input` of the type `${function-name} VersionedInputPayload`.
  - With a single return type `${function-name} VersionedOutputPayload`.
  - Added at the same relative position as its unversioned counter-part existed at (e.g., before function X and after
    function Y).
- Implement the new versioned runtime API function in pallet-revive in `src/lib.rs` in the
  `impl_runtime_apis_plus_revive_traits` block with the following specifications:
  - Named `${function-name}_versioned`.
  - With a single argument called `input` of the type
    `$crate::pallet_revive_types::runtime_api::${function-name} VersionedInputPayload`.
  - With a single return type `$crate::pallet_revive_types::runtime_api::${function-name} VersionedOutputPayload`.
  - First line of the function block is a `use $crate::pallet_revive_types::runtime_api::*;` to make the subsequent
    matching simpler.
  - Subsequent code in the implementation of this runtime API function looks like the following:

    ```rust
    // Getting the execution input type and a function for wrapping the output to be called at
    // the end of the execution.
    //
    // Note: the wrapper function converts the output into the same version as the input which
    // fulfills the invariant that a caller who provides a Vn input is guaranteed to get back a
    // Vn output or an error.
    let (input, output_wrapper): (
        _,
        Box<dyn Fn(FunctionNameOutputPayload) -> FunctionNameVersionedOutputPayload>,
    ) = match input {
        FunctionNameVersionedInputPayload::V1(payload) => (
            FunctionNameInputPayload::from(payload),
            Box::new(|output| FunctionNameVersionedOutputPayload::V1(output.into())),
        ),
    };

    // Some computation which is performed as part of the runtime API function.
    let output = perform_function_name(input.field1, input.field2);
    let output = FunctionNameOutputPayload { field1: output.field1 };

    // Converting the return type into the same version provided by the caller.
    output_wrapper(output)
    ```

  - In the same relative position as its unversioned counter-part existed at (e.g., before function X and after function
    Y).

### Deprecations

- Deprecate the old unversioned runtime API function `${function-name}` with a deprecation notice of
  `"Use the versioned equivalent ${function-name}_versioned if available on your runtime"`.
- Update the implementation of the old unversioned runtime API function `${function-name}` in pallet-revive `src/lib.rs`
  `impl_runtime_apis_plus_revive_traits` block such that it constructs V1 input, delegates to the new versioned runtime
  API function, then deconstructs V1 input in the following way:

  ```rust
  fn function_name(argument: $crate::ArgumentType) -> $crate::ReturnType {
      use $crate::pallet_revive_types::runtime_api::*;

      let input = FunctionNameVersionedInputPayload::from(FunctionNameInputPayloadV1 {
          argument: argument
      });
      let output = Self::function_name_versioned(input);
      FunctionNameOutputPayloadV1::try_from(output)
          .expect("v1 input must produce v1 output; qed")
          .output
  }
  ```

- If this runtime API function contained non-primitive types which needed to be defined in the `pallet-revive-types`
  crate at the beginning of this procedure then change all of the runtime API functions to use the new wire types which
  were defined.
  - **Example:** when versioning the `eth_receipt_data` runtime API function we defined a new wire-type:
    `ReceiptGasInfoV1`. Then, we changed the return type of the existing unversioned `eth_receipt_data` runtime API
    function to be the new `ReceiptGasInfoV1` therefore making the `ReceiptGasInfo` type truly internal to pallet-revive
    and unused anywhere in it's interface, not even in the older unversioned runtime API functions.
  - **Example:** when versioning the `trace_block` runtime API function we defined a number of wire-types, one of them
    was the `TraceV1` type. We replaced the return type of the unversioned `trace_block` runtime API function to be
    `TraceV1` and also replaced the return type of other runtime API functions which were not versioned in the same
    commit such as `trace_call_with_config`, `trace_call`, and `trace_tx` to use the new wire types we had defined
    (again, even though they were not versioned in that commit) in order to ensure that each wire type that we add
    completely replaces the existing execution type from the interface of pallet-revive even when it's not in the
    function we're currently versioning.

### State

At this point in the procedure, the state of the codebase should be as follows:

- The non-primitive types from the signature of `${function-name}` have been defined in the `pallet-revive-types` crate.
- The payload types for the inputs and outputs of the `${function-name}` runtime API function have been defined in the
  `pallet-revive-types` crate.
- The unversioned runtime API function `${function-name}` is deprecated with an appropriate deprecation message.
- The runtime API of pallet-revive no longer contains anywhere in its entire interface any of the non-primitive types
  which have had wire types defined for them in this procedure neither in the runtime API function we're versioning nor
  in other runtime API functions.
- The new versioned runtime API function has been declared and implemented with an `#[api_version(2)]`.
- The unversioned runtime API function delegates execution to the versioned runtime API function.
- The new runtime API function handles all versions its implemented for and guarantees that `Vn` input produces `Vn`
  output.

### ETH-RPC Integration

- Add type substitutions for all of the wire-types and the payload types added as part of this procedure to the
  `rpc/src/subxt_client.rs` file's versioning section in order to make the eth-rpc use the types we have defined rather
  than the types generated by subxt.
  - For the `${function-name} InputPayloadV1`, `${function-name} OutputPayloadV1`,
    `${function-name} VersionedInputPayload`, and `${function-name} VersionedOutputPayload` types.
  - If any non-primitive wire types were defined as part of this procedure into the `types/src/runtime_api/types` module
    then replacements for all of them must be added.
- If non-primitive wire types were defined as part of this procedure then ensure that the `pallet-revive-eth-rpc` does
  not use the old execution types in anyway.
  - **Example:** When versioning the `eth_receipt_data` runtime API function we introduced the `ReceiptGasInfoV1` wire
    type. We no longer want the eth-rpc to depend on the old execution type since it's no longer being exposed by any of
    the runtime API function we have. Therefore, we added a type substitution for the new `ReceiptGasInfoV1` and ensured
    that all of the appropriate parts of the `rpc/src/receipt_extractor.rs` used the `ReceiptGasInfoV1` wire type that
    is now being returned by pallet-revive.

### Cleanups

These are done in order to ensure that as we version runtime API functions we make execution types truly internal to
pallet-revive and ensure that they do not leak out in anyway. For all of the execution types which had wire types
defined for them as part of this procedure check:

- Does this execution type still need to be `pub` or could it be downgraded to a `pub(crate)`? If it can be downgraded
  then do it.
  - Example: when versioning the `eth_receipt_data` runtime API function we could not make the execution type
    `ReceiptGasInfo` a `pub(crate)` since it's being exposed by a public pallet function (not a runtime API function).
- Does this execution type still need its scale encoding derives or could they be removed? If they could be removed then
  remove them.
  - Example: when versioning the `eth_receipt_data` runtime API function we could not remove the scale derives from the
    `ReceiptGasInfo` type since it's being stored in storage.
  - Example: when versioning the `trace_block` runtime API function we were able to remove the scale derives from the
    `Trace` type (and all of the non-primitive types which are in this type graph) since this type is no longer returned
    from any runtime API function and is not being stored in storage.
- Does this execution type still need its serde derives or could they be removed? If they could be removed then remove
  them.
  - Example: when versioning the `trace_block` runtime API function we were able to remove the serde derives from the
    `Trace` type (and all of the non-primitive types which are in this type graph) since the new wire types handle all
    of the serde serialization and deserialization implementations.

<!--
TODO: Add the section for updating an already versioned runtime API function. I fist need to have the script which
created the type graph which is the main missing piece for this.
-->

## Commands

<table>
  <thead>
    <tr>
      <th>Action</th>
      <th>Command</th>
      <th>Why</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td>Check</td>
      <td>
        <code>SKIP_WASM_BUILD=1 SKIP_PALLET_REVIVE_FIXTURES=1 cargo clippy -p pallet-revive
        -p pallet-revive-eth-rpc -p revive-dev-runtime -p pallet-revive-types</code>
      </td>
      <td>
        The other packages added to this command are needed since some of the errors we get when versioning never appear
        in pallet-revive and might only appear in the consumer (e.g., the <code>pallet-revive-eth-rpc</code>) or in a
        runtime which implements the runtime API of pallet revive (e.g., <code>revive-dev-runtime</code>)
      </td>
    </tr>
    <tr>
      <td>Formatting</td>
      <td><code>cargo +nightly-2026-01-27 fmt --all</code></td>
      <td>LLMs get it wrong all the time</td>
    </tr>
  </tbody>
</table>

## Invariants

This section outlines various invariants and constraints which must be met when versioning a new runtime API function or
when updating an existing versioned runtime API function.

- All versioned runtime API functions have exactly one argument which is the versioned enum of all of its input versions
  and exactly one return type which is a versioned enum of all of its output versions. This applies even if we have a
  runtime API function which takes no arguments and returns nothing.
- Calling a runtime API function with a `Vn` input guarantees a `Vn` output on a successful call (or an error on
  fallible runtime API functions).
- The scale encoding of all of the V1 types must be identical to those used in the unversioned runtime API functions.
  E.g., `TraceV1` must be byte-by-byte identical to `Trace` such that encoding either allows us to decode as the other.
