window.BENCHMARK_DATA = {
  "lastUpdate": 1725280612228,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "statement-distribution-regression-bench": [
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a7097681b76bdaef21dcde9aec8c33205f480e44",
          "message": "[subsystem-benchmarks] Add statement-distribution benchmarks (#3863)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3748\n\nAdds a subsystem benchmark for statements-distribution subsystem.\n\nResults in CI (reference hw):\n```\n$ cargo bench -p polkadot-statement-distribution --bench statement-distribution-regression-bench --features subsystem-benchmarks\n\n[Sent to peers] standart_deviation 0.07%\n[Received from peers] standart_deviation 0.00%\n[statement-distribution] standart_deviation 0.97%\n[test-environment] standart_deviation 1.03%\n\nNetwork usage, KiB                     total   per block\nReceived from peers                1088.0000    108.8000\nSent to peers                      1238.1800    123.8180\n\nCPU usage, seconds                     total   per block\nstatement-distribution                0.3897      0.0390\ntest-environment                      0.4715      0.0472\n```",
          "timestamp": "2024-05-27T19:23:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a7097681b76bdaef21dcde9aec8c33205f480e44"
        },
        "date": 1716844401847,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92799999999993,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04608676030200003,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037103448994000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk",
            "email": "1728078+michalkucharczyk@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2d3a6932de35fc53da4e4b6bc195b1cc69550300",
          "message": "`sc-chain-spec`: deprecated code removed (#4410)\n\nThis PR removes deprecated code:\n- The `RuntimeGenesisConfig` generic type parameter in\n`GenericChainSpec` struct.\n- `ChainSpec::from_genesis` method allowing to create chain-spec using\nclosure providing runtime genesis struct\n- `GenesisSource::Factory` variant together with no longer needed\n`GenesisSource`'s generic parameter `G` (which was intended to be a\nruntime genesis struct).\n\n\nhttps://github.com/paritytech/polkadot-sdk/blob/17b56fae2d976a3df87f34076875de8c26da0355/substrate/client/chain-spec/src/chain_spec.rs#L559-L563",
          "timestamp": "2024-05-27T21:29:50Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2d3a6932de35fc53da4e4b6bc195b1cc69550300"
        },
        "date": 1716851734932,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91999999999993,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038638358854,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04772121519000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3bf283ff22224e7713cf0c1b9878e9137dc6dbf7",
          "message": "[subsytem-bench] Remove redundant banchmark_name param (#4540)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3601\n\nSince we print benchmark results manually, we don't need to save\nbenchmark_name anywhere, better just put the name inside `println!`.",
          "timestamp": "2024-05-28T08:51:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3bf283ff22224e7713cf0c1b9878e9137dc6dbf7"
        },
        "date": 1716888348464,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94199999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03838162524400001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04736182489,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6ed020037f4c2b6a6b542be6e5a15e86b0b7587b",
          "message": "[CI] Deny adding git deps (#4572)\n\nAdds a small CI check to match the existing Git deps agains a known-bad\nlist.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-05-28T11:23:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6ed020037f4c2b6a6b542be6e5a15e86b0b7587b"
        },
        "date": 1716901825824,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92399999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03954887485799999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04928162677200004,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bolaji Ahmad",
            "username": "bolajahmad",
            "email": "56865496+bolajahmad@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "650b124fd81f4a438c212cb010cc0a730bac5c2d",
          "message": "Improve On_demand_assigner events (#4339)\n\ntitle: Improving `on_demand_assigner` emitted events\n\ndoc:\n  - audience: Rutime User\ndescription: OnDemandOrderPlaced event that is useful for indexers to\nsave data related to on demand orders. Check [discussion\nhere](https://substrate.stackexchange.com/questions/11366/ondemandassignmentprovider-ondemandorderplaced-event-was-removed/11389#11389).\n\nCloses #4254 \n\ncrates: [ 'runtime-parachain]\n\n---------\n\nCo-authored-by: Maciej <maciej.zyszkiewicz@parity.io>",
          "timestamp": "2024-05-28T14:44:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/650b124fd81f4a438c212cb010cc0a730bac5c2d"
        },
        "date": 1716912815140,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94999999999993,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036255953064,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045810916002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2b1c606a338c80c5220c502c56a4b489f6d51488",
          "message": "parachain-inherent: Make `para_id` more prominent (#4555)\n\nThis should make it more obvious that at instantiation of the\n`MockValidationDataInherentDataProvider` the `para_id` needs to be\npassed.",
          "timestamp": "2024-05-28T16:08:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2b1c606a338c80c5220c502c56a4b489f6d51488"
        },
        "date": 1716919815120,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.902,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037057450898000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04731493461599998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d6cf147c1bda601e811bf5813b0d46ca1c8ad9b9",
          "message": "Filter workspace dependencies in the templates (#4599)\n\nThis detaches the templates from monorepo's workspace dependencies.\n\nCurrently the templates [re-use the monorepo's\ndependencies](https://github.com/paritytech/polkadot-sdk-minimal-template/blob/bd8afe66ec566d61f36b0e3d731145741a9e9e19/Cargo.toml#L45-L58),\nmost of which are not needed.\n\nThe simplest approach is to specify versions directly and not use\nworkspace dependencies in the templates.\n\nAnother approach would be to programmatically filter dependencies that\nare actually needed - but not sure if it's worth it, given that it would\ncomplicate the synchronization job.\n\ncc @kianenigma @gupnik",
          "timestamp": "2024-05-28T17:57:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d6cf147c1bda601e811bf5813b0d46ca1c8ad9b9"
        },
        "date": 1716925470609,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.95799999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03720920827200003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046185171588000014,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5f68c93039fce08d7f711025eddc5343b0272111",
          "message": "Moves runtime macro out of experimental flag (#4249)\n\nStep in https://github.com/paritytech/polkadot-sdk/issues/3688\n\nNow that the `runtime` macro (Construct Runtime V2) has been\nsuccessfully deployed on Westend, this PR moves it out of the\nexperimental feature flag and makes it generally available for runtime\ndevs.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-05-29T03:41:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5f68c93039fce08d7f711025eddc5343b0272111"
        },
        "date": 1716960271234,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03773168850800001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04668900673199998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "89604daa0f4244bc83782bd489918cfecb81a7d0",
          "message": "Add omni bencher & chain-spec-builder bins to release (#4557)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4354\n\nThis PR adds the steps to build and attach `frame-omni-bencher` and\n`chain-spec-builder` binaries to the release draft\n\n## TODO\n- [x] add also chain-spec-builder binary\n- [ ] ~~check/investigate Kian's comment: `chain spec builder. Ideally I\nwant it to match the version of the sp-genesis-builder crate`~~ see\n[comment](https://github.com/paritytech/polkadot-sdk/pull/4518#issuecomment-2134731355)\n- [ ] Backport to `polkadot-sdk@1.11` release, so we can use it for next\nfellows release: https://github.com/polkadot-fellows/runtimes/pull/324\n- [ ] Backport to `polkadot-sdk@1.12` release\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-05-29T05:50:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/89604daa0f4244bc83782bd489918cfecb81a7d0"
        },
        "date": 1716968237171,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92199999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.039233402288,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.049000835000000006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dfcfa4ab37819fddb4278eaac306adc0f194fd27",
          "message": "Publish `chain-spec-builder` (#4518)\n\nmarking it as release-able, attaching the same version number that is\nattached to other binaries such as `polkadot` and `polkadot-parachain`.\n\nI have more thoughts about the version number, though. The chain-spec\nbuilder is mainly a user of the `sp-genesis-builder` api. So the\nversioning should be such that it helps users know give a version of\n`sp-genesis-builder` in their runtime, which version of\n`chain-spec-builder` should they use?\n\nWith this, we can possibly alter the version number to always match\n`sp-genesis-builder`.\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/4352\n\n- [x] Add to release artifacts ~~similar to\nhttps://github.com/paritytech/polkadot-sdk/pull/4405~~ done here:\nhttps://github.com/paritytech/polkadot-sdk/pull/4557\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-05-29T08:34:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dfcfa4ab37819fddb4278eaac306adc0f194fd27"
        },
        "date": 1716978061194,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93999999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04819241767400001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037415698831999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Joshua Cheong",
            "username": "joshuacheong",
            "email": "jrc96@cantab.ac.uk"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "aa32faaebf64426becb2feeede347740eb7a3908",
          "message": "Update README.md (#4623)\n\nMinor edit to a broken link for Rust Docs on the README.md\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-05-29T10:11:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aa32faaebf64426becb2feeede347740eb7a3908"
        },
        "date": 1716984217218,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046360992714,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036463560912,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d5053ac4161b6e3f634a3ffb6df07637058e9f55",
          "message": "Change `XcmDryRunApi::dry_run_extrinsic` to take a call instead (#4621)\n\nFollow-up to the new `XcmDryRunApi` runtime API introduced in\nhttps://github.com/paritytech/polkadot-sdk/pull/3872.\n\nTaking an extrinsic means the frontend has to sign first to dry-run and\nonce again to submit.\nThis is bad UX which is solved by taking an `origin` and a `call`.\nThis also has the benefit of being able to dry-run as any account, since\nit needs no signature.\n\nThis is a breaking change since I changed `dry_run_extrinsic` to\n`dry_run_call`, however, this API is still only on testnets.\nThe crates are bumped accordingly.\n\nAs a part of this PR, I changed the name of the API from `XcmDryRunApi`\nto just `DryRunApi`, since it can be used for general dry-running :)\n\nStep towards https://github.com/paritytech/polkadot-sdk/issues/690.\n\nExample of calling the API with PAPI, not the best code, just testing :)\n\n```ts\n// We just build a call, the arguments make it look very big though.\nconst call = localApi.tx.XcmPallet.transfer_assets({\n  dest: XcmVersionedLocation.V4({ parents: 0, interior: XcmV4Junctions.X1(XcmV4Junction.Parachain(1000)) }),\n  beneficiary: XcmVersionedLocation.V4({ parents: 0, interior: XcmV4Junctions.X1(XcmV4Junction.AccountId32({ network: undefined, id: Binary.fromBytes(encodeAccount(account.address)) })) }),\n  weight_limit: XcmV3WeightLimit.Unlimited(),\n  assets: XcmVersionedAssets.V4([{\n    id: { parents: 0, interior: XcmV4Junctions.Here() },\n    fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n) }\n  ]),\n  fee_asset_item: 0,\n});\n// We call the API passing in a signed origin \nconst result = await localApi.apis.XcmDryRunApi.dry_run_call(\n  WestendRuntimeOriginCaller.system(DispatchRawOrigin.Signed(account.address)),\n  call.decodedCall\n);\nif (result.success && result.value.execution_result.success) {\n  // We find the forwarded XCM we want. The first one going to AssetHub in this case.\n  const xcmsToAssetHub = result.value.forwarded_xcms.find(([location, _]) => (\n    location.type === \"V4\" &&\n      location.value.parents === 0 &&\n      location.value.interior.type === \"X1\"\n      && location.value.interior.value.type === \"Parachain\"\n      && location.value.interior.value.value === 1000\n  ))!;\n\n  // We can even find the delivery fees for that forwarded XCM.\n  const deliveryFeesQuery = await localApi.apis.XcmPaymentApi.query_delivery_fees(xcmsToAssetHub[0], xcmsToAssetHub[1][0]);\n\n  if (deliveryFeesQuery.success) {\n    const amount = deliveryFeesQuery.value.type === \"V4\" && deliveryFeesQuery.value.value[0].fun.type === \"Fungible\" && deliveryFeesQuery.value.value[0].fun.value.valueOf() || 0n;\n    // We store them in state somewhere.\n    setDeliveryFees(formatAmount(BigInt(amount)));\n  }\n}\n```\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-29T19:57:17Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d5053ac4161b6e3f634a3ffb6df07637058e9f55"
        },
        "date": 1717019120720,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.05015039981800001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03972209547199998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4ab078d6754147ce731523292dd1882f8a7b5775",
          "message": "pallet-staking: Put tests behind `cfg(debug_assertions)` (#4620)\n\nOtherwise these tests are failing if you don't run with\n`debug_assertions` enabled, which happens if you run tests locally in\nrelease mode.",
          "timestamp": "2024-05-29T21:23:27Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4ab078d6754147ce731523292dd1882f8a7b5775"
        },
        "date": 1717024105623,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92799999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037273492600000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04662715349399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jeeyong Um",
            "username": "conr2d",
            "email": "conr2d@proton.me"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d539778c3cc4e0376769472fdad37856f4051dc5",
          "message": "Fix broken windows build (#4636)\n\nFixes #4625.\n\nSpecifically, the `cfg` attribute `windows` refers to the compile target\nand not the build environment, and in the case of cross-compilation, the\nbuild environment and target can differ. However, the line modified is\nrelated to documentation generation, so there should be no critical\nissue with this change.",
          "timestamp": "2024-05-30T10:44:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d539778c3cc4e0376769472fdad37856f4051dc5"
        },
        "date": 1717068487966,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93999999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037914941948000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.048059659000000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "78c24ec9e24ea04b2f8513b53a8d1246ff6b35ed",
          "message": "Adds ability to specify chain type in chain-spec-builder (#4542)\n\nCurrently, `chain-spec-builder` only creates a spec with `Live` chain\ntype. This PR adds the ability to specify it while keeping the same\ndefault.\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-05-31T02:09:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/78c24ec9e24ea04b2f8513b53a8d1246ff6b35ed"
        },
        "date": 1717127487760,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036708316296,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045880886494,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "71f4f5a80bb9ef00d651c62a58c6e8192d4d9707",
          "message": "Update `runtime_type` ref doc with the new \"Associated Type Bounds\" (#4624)\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-31T04:58:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/71f4f5a80bb9ef00d651c62a58c6e8192d4d9707"
        },
        "date": 1717137517623,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94599999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036550707258000005,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04615422804200001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Sandu",
            "username": "sandreim",
            "email": "54316454+sandreim@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0ae721970909efc3b2a049632c9c904d9fa4fed1",
          "message": "collator-protocol: remove `elastic-scaling-experimental` feature (#4595)\n\nValidators already have been upgraded so they could already receive the\nnew `CollationWithParentHeadData` response when fetching collation.\nHowever this is only sent by collators when the parachain has more than\n1 core is assigned.\n\nTODO:\n- [x] PRDoc\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-31T06:34:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0ae721970909efc3b2a049632c9c904d9fa4fed1"
        },
        "date": 1717143682292,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036584844534,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04587214198800002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8d8c0e13a7dc8d067367ac55fb142b12ac8a6d13",
          "message": "Use Unlicense for templates (#4628)\n\nAddresses\n[this](https://github.com/paritytech/polkadot-sdk/issues/3155#issuecomment-2134411391).",
          "timestamp": "2024-05-31T10:15:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8d8c0e13a7dc8d067367ac55fb142b12ac8a6d13"
        },
        "date": 1717157204108,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04626340717999999,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036585757284,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "fc6c31829fc2e24e11a02b6a2adec27bc5d8918f",
          "message": "Implement `XcmPaymentApi` and `DryRunApi` on all system parachains (#4634)\n\nDepends on https://github.com/paritytech/polkadot-sdk/pull/4621.\n\nImplemented the\n[`XcmPaymentApi`](https://github.com/paritytech/polkadot-sdk/pull/3607)\nand [`DryRunApi`](https://github.com/paritytech/polkadot-sdk/pull/3872)\non all system parachains.\n\nMore scenarios can be tested on both rococo and westend if all system\nparachains implement this APIs.\nThe objective is for all XCM-enabled runtimes to implement them.\nAfter demonstrating fee estimation in a UI on the testnets, come the\nfellowship runtimes.\n\nStep towards https://github.com/paritytech/polkadot-sdk/issues/690.",
          "timestamp": "2024-05-31T15:38:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fc6c31829fc2e24e11a02b6a2adec27bc5d8918f"
        },
        "date": 1717176384103,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93399999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036044289866,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04551121451799999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f81751e0ce56b0ef50b3a0b5aa0ff4fb16c9ea37",
          "message": "Better error for missing index in CRV2 (#4643)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/4552\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2024-06-02T18:39:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f81751e0ce56b0ef50b3a0b5aa0ff4fb16c9ea37"
        },
        "date": 1717360437391,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.90999999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04859474890200003,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.038919108541999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "tugy",
            "username": "tugytur",
            "email": "33746108+tugytur@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5779ec5b775f86fb86be02783ab5c02efbf307ca",
          "message": "update amforc westend and its parachain bootnodes (#4641)\n\nTested each bootnode with `--reserved-only --reserved-nodes`",
          "timestamp": "2024-06-02T20:11:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5779ec5b775f86fb86be02783ab5c02efbf307ca"
        },
        "date": 1717365502632,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93399999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036893041079999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04648694305799998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f66e693a6befef0956a3129254fbe568247c9c57",
          "message": "Add chain-spec-builder docker image (#4655)\n\nThis PR adds possibility to publish container images for the\n`chain-spec-builder` binary on the regular basis.\nRelated to: https://github.com/paritytech/release-engineering/issues/190",
          "timestamp": "2024-06-03T08:30:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f66e693a6befef0956a3129254fbe568247c9c57"
        },
        "date": 1717410103360,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92399999999992,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036313560432000014,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04609666627400002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6f228e7d220bb14c113dcc27c931590737f9d0ab",
          "message": " [ci] Delete unused flow (#4676)",
          "timestamp": "2024-06-03T12:22:06Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6f228e7d220bb14c113dcc27c931590737f9d0ab"
        },
        "date": 1717419589840,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91599999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037198549799999966,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04727594728000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Samusev",
            "username": "alvicsam",
            "email": "41779041+alvicsam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "cbe45121c9a7bb956101bf28e6bb23f0efd3cbbf",
          "message": "[ci] Increase timeout for check-runtime-migration workflow (#4674)\n\n`[check-runtime-migration` now takes more than 30 minutes. Quick fix\nwith increased timeout.",
          "timestamp": "2024-06-03T13:42:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cbe45121c9a7bb956101bf28e6bb23f0efd3cbbf"
        },
        "date": 1717428633046,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93399999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04679631053400001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03765909039600001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Miasojed",
            "username": "smiasojed",
            "email": "s.miasojed@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3e8416456a27197bd1bbb7fc8149d941d9167f4d",
          "message": "Add READ_ONLY flag to contract call function  (#4418)\n\nThis PR implements the `READ_ONLY` flag to be used as a `Callflag` in\nthe `call` function.\nThe flag indicates that the callee is restricted from modifying the\nstate during call execution.\nIt is equivalent to Ethereum's\n[STATICCALL](https://eips.ethereum.org/EIPS/eip-214).\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Andrew Jones <ascjones@gmail.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2024-06-04T07:58:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3e8416456a27197bd1bbb7fc8149d941d9167f4d"
        },
        "date": 1717489663961,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92399999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.0467510005,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03669614254399998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Gheorghe",
            "username": "alexggh",
            "email": "49718502+alexggh@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a09ec64d149b400de16f144ca02f0fa958d2bb13",
          "message": "Forward put_record requests to authorithy-discovery (#4683)\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-06-04T10:05:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a09ec64d149b400de16f144ca02f0fa958d2bb13"
        },
        "date": 1717501830981,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94399999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999999,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03747444226600001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04663998288800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9b76492302f7184ff00bd6141c9b4163611e9d45",
          "message": "Use `parachain_info` in cumulus-test-runtime (#4672)\n\nThis allows to use custom para_ids with cumulus-test-runtime. \n\nZombienet is patching the genesis entries for `ParachainInfo`. This did\nnot work with `test-parachain` because it was using the `test_pallet`\nfor historic reasons I guess.",
          "timestamp": "2024-06-04T13:32:27Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9b76492302f7184ff00bd6141c9b4163611e9d45"
        },
        "date": 1717514128851,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93199999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046964888312,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037159875908000004,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk",
            "email": "1728078+michalkucharczyk@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "42ddb5b06578f6c2b2fc469de5161035b04fc79a",
          "message": "`chain-spec`/presets reference docs added (#4678)\n\nAdded reference doc about:\n- the pallet genesis config and genesis build, \n- runtime `genesis-builder` API,\n- presets,\n- interacting with the `chain-spec-builder` tool\n\nI've added [minimal\nruntime](https://github.com/paritytech/polkadot-sdk/tree/mku-chain-spec-guide/docs/sdk/src/reference_docs/chain_spec_runtime)\nto demonstrate above topics.\n\nI also sneaked in some little improvement to `chain-spec-builder` which\nallows to parse output of the `list-presets` command.\n\n---------\n\nCo-authored-by: Alexandru Vasile <60601340+lexnv@users.noreply.github.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-06-04T18:04:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/42ddb5b06578f6c2b2fc469de5161035b04fc79a"
        },
        "date": 1717530729523,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91999999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03776602047199999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04679643760000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "georgepisaltu",
            "username": "georgepisaltu",
            "email": "52418509+georgepisaltu@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3977f389cce4a00fd7100f95262e0563622b9aa4",
          "message": "[Identity] Remove double encoding username signature payload (#4646)\n\nIn order to receive a username in `pallet-identity`, users have to,\namong other things, provide a signature of the desired username. Right\nnow, there is an [extra encoding\nstep](https://github.com/paritytech/polkadot-sdk/blob/4ab078d6754147ce731523292dd1882f8a7b5775/substrate/frame/identity/src/lib.rs#L1119)\nwhen generating the payload to sign.\n\nEncoding a `Vec` adds extra bytes related to the length, which changes\nthe payload. This is unnecessary and confusing as users expect the\npayload to sign to be just the username bytes. This PR fixes this issue\nby validating the signature directly against the username bytes.\n\n---------\n\nSigned-off-by: georgepisaltu <george.pisaltu@parity.io>",
          "timestamp": "2024-06-05T07:38:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3977f389cce4a00fd7100f95262e0563622b9aa4"
        },
        "date": 1717579583502,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03820287798800001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04891659056399998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8ffe22903a37f4dab2aa1b15ec899f2c38439f60",
          "message": "Update the `polkadot_builder` Dockerfile (#4638)\n\nThis Dockerfile seems outdated - it currently fails to build (on my\nmachine).\nI don't see it being built anywhere on CI.\n\nI did a couple of tweaks to make it build.\n\n---------\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2024-06-05T09:55:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8ffe22903a37f4dab2aa1b15ec899f2c38439f60"
        },
        "date": 1717587749001,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93799999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04677236130999999,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037789614564000006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk",
            "email": "1728078+michalkucharczyk@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f65beb7f7a66a79f4afd0a308bece2bd5c8ba780",
          "message": "chain-spec-doc: some minor fixes (#4700)\n\nsome minor text fixes.",
          "timestamp": "2024-06-05T11:52:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f65beb7f7a66a79f4afd0a308bece2bd5c8ba780"
        },
        "date": 1717594769897,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.936,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04598563817,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03636507818999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d2fd53645654d3b8e12cbf735b67b93078d70113",
          "message": "Unify dependency aliases (#4633)\n\nInherited workspace dependencies cannot be renamed by the crate using\nthem (see [1](https://github.com/rust-lang/cargo/issues/12546),\n[2](https://stackoverflow.com/questions/76792343/can-inherited-dependencies-in-rust-be-aliased-in-the-cargo-toml-file)).\nSince we want to use inherited workspace dependencies everywhere, we\nfirst need to unify all aliases that we use for a dependency throughout\nthe workspace.\nThe umbrella crate is currently excluded from this procedure, since it\nshould be able to export the crates by their original name without much\nhassle.\n\nFor example: one crate may alias `parity-scale-codec` to `codec`, while\nanother crate does not alias it at all. After this change, all crates\nhave to use `codec` as name. The problematic combinations were:\n- conflicting aliases: most crates aliases as `A` but some use `B`.\n- missing alias: most of the crates alias a dep but some dont.\n- superfluous alias: most crates dont alias a dep but some do.\n\nThe script that i used first determines whether most crates opted to\nalias a dependency or not. From that info it decides whether to use an\nalias or not. If it decided to use an alias, the most common one is used\neverywhere.\n\nTo reproduce, i used\n[this](https://github.com/ggwpez/substrate-scripts/blob/master/uniform-crate-alias.py)\npython script in combination with\n[this](https://github.com/ggwpez/zepter/blob/38ad10585fe98a5a86c1d2369738bc763a77057b/renames.json)\nerror output from Zepter.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-05T13:54:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d2fd53645654d3b8e12cbf735b67b93078d70113"
        },
        "date": 1717604877582,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92799999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.049103086846,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.038070867889999985,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2460cddf57660a88844d201f769eb17a7accce5a",
          "message": "fix build on MacOS: bump secp256k1 and secp256k1-sys to patched versions (#4709)\n\n`secp256k1 v0.28.0` and `secp256k1-sys v0.9.0` were yanked because\nbuilding them fails for `aarch64-apple-darwin` targets.\n\nUse the `secp256k1 v0.28.2` and `secp256k1-sys v0.9.2` patched versions\nthat build fine on ARM chipset MacOS.",
          "timestamp": "2024-06-05T18:14:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2460cddf57660a88844d201f769eb17a7accce5a"
        },
        "date": 1717618049053,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.96599999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037668238464,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04872799223399998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5fb4c40a3ea24ae3ab2bdfefb3f3a40badc2a583",
          "message": "[CI] Delete cargo-deny config (#4677)\n\nNobody seems to maintain this and the job is disabled since months. I\nthink unless the Security team wants to pick this up we delete it for\nnow.\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-06-06T14:48:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5fb4c40a3ea24ae3ab2bdfefb3f3a40badc2a583"
        },
        "date": 1717692213494,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04756312851199999,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037765511577999976,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "494448b7fed02e098fbf38bad517d9245b056d1d",
          "message": "Cleanup PVF artifact by cache limit and stale time (#4662)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/4324\nWe don't change but extend the existing cleanup strategy. \n- We still don't touch artifacts being stale less than 24h\n- First time we attempt pruning only when we hit cache limit (10 GB)\n- If somehow happened that after we hit 10 GB and least used artifact is\nstale less than 24h we don't remove it.\n\n---------\n\nCo-authored-by: s0me0ne-unkn0wn <48632512+s0me0ne-unkn0wn@users.noreply.github.com>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2024-06-06T19:22:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/494448b7fed02e098fbf38bad517d9245b056d1d"
        },
        "date": 1717705589185,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.035705928844000014,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04571031502800002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "batman",
            "username": "iammasterbrucewayne",
            "email": "iammasterbrucewayne@protonmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "426956f87cc91f94ce71e2ed74ca34d88766e1d8",
          "message": "Update the README to include a link to the Polkadot SDK Version Manager (#4718)\n\nAdds a link to the [Polkadot SDK Version\nManager](https://github.com/paritytech/psvm) since this tool is not well\nknown, but very useful for developers using the Polkadot SDK.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-06T20:06:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/426956f87cc91f94ce71e2ed74ca34d88766e1d8"
        },
        "date": 1717708173603,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92399999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036043076166,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04576122104200004,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "batman",
            "username": "iammasterbrucewayne",
            "email": "iammasterbrucewayne@protonmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "426956f87cc91f94ce71e2ed74ca34d88766e1d8",
          "message": "Update the README to include a link to the Polkadot SDK Version Manager (#4718)\n\nAdds a link to the [Polkadot SDK Version\nManager](https://github.com/paritytech/psvm) since this tool is not well\nknown, but very useful for developers using the Polkadot SDK.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-06T20:06:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/426956f87cc91f94ce71e2ed74ca34d88766e1d8"
        },
        "date": 1717710834025,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94599999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.048520994559999996,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037818383486,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d783ca9d9bfb42ae938f8d4ce9899b6aa3cc00c6",
          "message": "New reference doc for Custom RPC V2 (#4654)\n\nThanks for @xlc for the original seed info, I've just fixed it up a bit\nand added example links.\n\nI've moved the comparison between eth-rpc-api and frontier outside, as\nit is opinionation. I think the content there was good but should live\nin the README of the corresponding repos. No strong opinion, happy\neither way.\n\n---------\n\nCo-authored-by: Bryan Chen <xlchen1291@gmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-06-07T11:26:52Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d783ca9d9bfb42ae938f8d4ce9899b6aa3cc00c6"
        },
        "date": 1717767037907,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.9199999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04654903398600002,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03702263918200001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "48d875d0e60c6d5e4c0c901582cc8edfb76f2f42",
          "message": "Contracts:  update wasmi to 0.32 (#3679)\n\ntake over #2941 \n[Weights\ncompare](https://weights.tasty.limo/compare?unit=weight&ignore_errors=true&threshold=10&method=asymptotic&repo=polkadot-sdk&old=master&new=pg%2Fwasmi-to-v0.32.0-beta.7&path_pattern=substrate%2Fframe%2F**%2Fsrc%2Fweights.rs%2Cpolkadot%2Fruntime%2F*%2Fsrc%2Fweights%2F**%2F*.rs%2Cpolkadot%2Fbridges%2Fmodules%2F*%2Fsrc%2Fweights.rs%2Ccumulus%2F**%2Fweights%2F*.rs%2Ccumulus%2F**%2Fweights%2Fxcm%2F*.rs%2Ccumulus%2F**%2Fsrc%2Fweights.rs)\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2024-06-07T14:40:10Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/48d875d0e60c6d5e4c0c901582cc8edfb76f2f42"
        },
        "date": 1717777986318,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93199999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036048308450000024,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04548887233000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "07cfcf0b3c9df971c673162b9d16cb5c17fbe97d",
          "message": "frame/proc-macro: Refactor code for better readability (#4712)\n\nSmall refactoring PR to improve the readability of the proc macros.\n- small improvement in docs\n- use new `let Some(..) else` expression\n- removed extra indentations by early returns\n\nDiscovered during metadata v16 poc, extracted from:\nhttps://github.com/paritytech/polkadot-sdk/pull/4358\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>\nCo-authored-by: gupnik <mail.guptanikhil@gmail.com>",
          "timestamp": "2024-06-08T07:48:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/07cfcf0b3c9df971c673162b9d16cb5c17fbe97d"
        },
        "date": 1717839210102,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92599999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03589073433600001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04577856762999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "batman",
            "username": "iammasterbrucewayne",
            "email": "iammasterbrucewayne@protonmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "cdb297b15ad9c1d952c0501afaf6b764e5fd147c",
          "message": "Update README.md to move the PSVM link under a \"Tooling\" section under the \"Releases\" section (#4734)\n\nThis update implements the suggestion from @kianenigma mentioned in\nhttps://github.com/paritytech/polkadot-sdk/pull/4718#issuecomment-2153777367\n\nReplaces the \"Other useful resources and tooling\" section at the bottom\nwith a new (nicer) \"Tooling\" section just under the \"Releases\" section.",
          "timestamp": "2024-06-08T11:37:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cdb297b15ad9c1d952c0501afaf6b764e5fd147c"
        },
        "date": 1717853033436,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94199999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03567975879000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045630244508,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Gheorghe",
            "username": "alexggh",
            "email": "49718502+alexggh@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2869fd6aba61f429ea2c006c2aae8dd5405dc5aa",
          "message": "approval-voting: Add no shows debug information (#4726)\n\nAdd some debug logs to be able to identify the validators and parachains\nthat have most no-shows, this metric is valuable because it will help us\nidentify validators and parachains that regularly have this problem.\n\nFrom the validator_index we can then query the on-chain information and\nidentify the exact validator that is causing the no-shows.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-06-10T09:44:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2869fd6aba61f429ea2c006c2aae8dd5405dc5aa"
        },
        "date": 1718019207950,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94799999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038553830368,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04919857013399997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Gheorghe",
            "username": "alexggh",
            "email": "49718502+alexggh@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b65313e81465dd730e48d4ce00deb76922618375",
          "message": "Remove unncessary call remove_from_peers_set (#4742)\n\n... this is superfluous because set_reserved_peers implementation\nalready calls this method here:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/cdb297b15ad9c1d952c0501afaf6b764e5fd147c/substrate/client/network/src/protocol_controller.rs#L571,\nso the call just ends producing this warnings whenever we manipulate the\npeers set.\n\n```\nTrying to remove unknown reserved node 12D3KooWRCePWvHoBbz9PSkw4aogtdVqkVDhiwpcHZCqh4hdPTXC from SetId(3)\npeerset warnings (from different peers)\n```\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-06-10T12:54:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b65313e81465dd730e48d4ce00deb76922618375"
        },
        "date": 1718030376550,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.91599999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.039374579481999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.049619512533999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "96ab6869bafb06352b282576a6395aec8e9f2705",
          "message": "finalization: Skip tree route calculation if no forks present (#4721)\n\n## Issue\n\nCurrently, syncing parachains from scratch can lead to a very long\nfinalization time once they reach the tip of the chain. The problem is\nthat we try to finalize everything from 0 to the tip, which can be\nthousands or even millions of blocks.\n\nWe finalize sequentially and try to compute displaced branches during\nfinalization. So for every block on the way, we compute an expensive\ntree route.\n\n## Proposed Improvements\n\nIn this PR, I propose improvements that solve this situation:\n\n- **Skip tree route calculation if `leaves().len() == 1`:** This should\nbe enough for 90% of cases where there is only one leaf after sync.\n- **Optimize finalization for long distances:** It can happen that the\nparachain has imported some leaf and then receives a relay chain\nnotification with the finalized block. In that case, the previous\noptimization will not trigger. A second mechanism should ensure that we\ndo not need to compute the full tree route. If the finalization distance\nis long, we check the lowest common ancestor of all the leaves. If it is\nabove the to-be-finalized block, we know that there are no displaced\nleaves. This is fast because forks are short and close to the tip, so we\ncan leverage the header cache.\n\n## Alternative Approach\n\n- The problem was introduced in #3962. Reverting that PR is another\npossible strategy.\n- We could store for every fork where it begins, however sounds a bit\nmore involved to me.\n\n\nfixes #4614",
          "timestamp": "2024-06-11T13:02:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/96ab6869bafb06352b282576a6395aec8e9f2705"
        },
        "date": 1718117257038,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.90999999999994,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04859771971999999,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03761839483599999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "cheme",
            "username": "cheme",
            "email": "emericchevalier.pro@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ad8620922bd7c0477b25c7dfd6fc233641cb27ae",
          "message": "Append overlay optimization. (#1223)\n\nThis branch propose to avoid clones in append by storing offset and size\nin previous overlay depth.\nThat way on rollback we can just truncate and change size of existing\nvalue.\nTo avoid copy it also means that :\n\n- append on new overlay layer if there is an existing value: create a\nnew Append entry with previous offsets, and take memory of previous\noverlay value.\n- rollback on append: restore value by applying offsets and put it back\nin previous overlay value\n- commit on append: appended value overwrite previous value (is an empty\nvec as the memory was taken). offsets of commited layer are dropped, if\nthere is offset in previous overlay layer they are maintained.\n- set value (or remove) when append offsets are present: current\nappended value is moved back to previous overlay value with offset\napplied and current empty entry is overwrite (no offsets kept).\n\nThe modify mechanism is not needed anymore.\nThis branch lacks testing and break some existing genericity (bit of\nduplicated code), but good to have to check direction.\n\nGenerally I am not sure if it is worth or we just should favor\ndifferents directions (transients blob storage for instance), as the\ncurrent append mechanism is a bit tricky (having a variable length in\nfirst position means we sometime need to insert in front of a vector).\n\nFix #30.\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: EgorPopelyaev <egor@parity.io>\nCo-authored-by: Alexandru Vasile <60601340+lexnv@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: joe petrowski <25483142+joepetrowski@users.noreply.github.com>\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Bastian Köcher <info@kchr.de>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-06-11T22:15:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ad8620922bd7c0477b25c7dfd6fc233641cb27ae"
        },
        "date": 1718150533739,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94799999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04665232661799998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036094561670000004,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c4aa2ab642419e6751400a6aabaf5df611a4ea37",
          "message": "Hide `tuplex` dependency and re-export by macro (#4774)\n\nAddressing comment:\nhttps://github.com/paritytech/polkadot-sdk/pull/4102/files#r1635502496\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-06-12T14:38:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c4aa2ab642419e6751400a6aabaf5df611a4ea37"
        },
        "date": 1718209412896,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94199999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03635728951199999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04681912996800002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "eca1052ea1eddeede91da8f9f7452ea8b57e7942",
          "message": "Update the pallet guide in `sdk-docs` (#4735)\n\nAfter using this tutorial in PBA, there was a few areas to improve it.\nMoreover, I have:\n\n- Improve `your_first_pallet`, link it in README, improve the parent\n`guide` section.\n- Updated the templates page, in light of recent efforts related to in\nhttps://github.com/paritytech/polkadot-sdk/issues/3155\n- Added small ref docs about metadata, completed the one about native\nruntime, added one about host functions.\n- Remove a lot of unfinished stuff from sdk-docs\n- update diagram for `Hooks`",
          "timestamp": "2024-06-13T02:36:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/eca1052ea1eddeede91da8f9f7452ea8b57e7942"
        },
        "date": 1718253679341,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93999999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04846578633000001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.038858006131999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "988103d7578ad515b13c69578da1237b28fa9f36",
          "message": "Use aggregated types for `RuntimeFreezeReason` and better examples of `MaxFreezes` (#4615)\n\nThis PR aligns the settings for `MaxFreezes`, `RuntimeFreezeReason`, and\n`FreezeIdentifier`.\n\n#### Future work and improvements\nhttps://github.com/paritytech/polkadot-sdk/issues/2997 (remove\n`MaxFreezes` and `FreezeIdentifier`)",
          "timestamp": "2024-06-13T08:44:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/988103d7578ad515b13c69578da1237b28fa9f36"
        },
        "date": 1718275053427,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93999999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03604918651199999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045600787604000007,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7b6b783cd1a3953ef5fa6e53f3965b1454e3efc8",
          "message": "[Backport] Version bumps and prdoc reorg from 1.13.0 (#4784)\n\nThis PR backports regular version bumps and prdocs reordering from the\nrelease branch back to master",
          "timestamp": "2024-06-13T16:27:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7b6b783cd1a3953ef5fa6e53f3965b1454e3efc8"
        },
        "date": 1718302460443,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94199999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.049182193529999994,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03860062925800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7f7f5fa857502b6e3649081abb6b53c3512bfedb",
          "message": "`polkadot-parachain-bin`: small cosmetics and improvements (#4666)\n\nRelated to: https://github.com/paritytech/polkadot-sdk/issues/5\n\nA couple of cosmetics and improvements related to\n`polkadot-parachain-bin`:\n\n- Adding some convenience traits in order to avoid declaring long\nduplicate bounds\n- Specifically check if the runtime exposes `AuraApi` when executing\n`start_lookahead_aura_consensus()`\n- Some fixes for the `RelayChainCli`. Details in the commits description",
          "timestamp": "2024-06-14T06:29:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7f7f5fa857502b6e3649081abb6b53c3512bfedb"
        },
        "date": 1718353186855,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.95599999999999,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03648255093799999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046496361195999966,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Sandu",
            "username": "sandreim",
            "email": "54316454+sandreim@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ae0b3bf6733e7b9e18badb16128a6b25bef1923b",
          "message": "CheckWeight: account for extrinsic len as proof size (#4765)\n\nFix https://github.com/paritytech/polkadot-sdk/issues/4743 which allows\nus to remove the defensive limit on pov size in Cumulus after relay\nchain gets upgraded with these changes. Also add unit test to ensure\n`CheckWeight` - `StorageWeightReclaim` integration works.\n\nTODO:\n- [x] PRDoc\n- [x] Add a len to all the other tests in storage weight reclaim and\ncall `CheckWeight::pre_dispatch`\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-06-14T12:42:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ae0b3bf6733e7b9e18badb16128a6b25bef1923b"
        },
        "date": 1718371116474,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93199999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03663629541799998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046599785726,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Sandu",
            "username": "sandreim",
            "email": "54316454+sandreim@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ae0b3bf6733e7b9e18badb16128a6b25bef1923b",
          "message": "CheckWeight: account for extrinsic len as proof size (#4765)\n\nFix https://github.com/paritytech/polkadot-sdk/issues/4743 which allows\nus to remove the defensive limit on pov size in Cumulus after relay\nchain gets upgraded with these changes. Also add unit test to ensure\n`CheckWeight` - `StorageWeightReclaim` integration works.\n\nTODO:\n- [x] PRDoc\n- [x] Add a len to all the other tests in storage weight reclaim and\ncall `CheckWeight::pre_dispatch`\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-06-14T12:42:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ae0b3bf6733e7b9e18badb16128a6b25bef1923b"
        },
        "date": 1718375819287,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.95399999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046684688274000005,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036501723102,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2f643816d79a76155aec790a35b9b72a5d8bb726",
          "message": "add ref doc for logging practices in FRAME (#4768)",
          "timestamp": "2024-06-17T03:31:15Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2f643816d79a76155aec790a35b9b72a5d8bb726"
        },
        "date": 1718601418252,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94399999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03617864762400001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04624845210399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d91cbbd453c1d4553d7e3dc8753a2007fc4c5a67",
          "message": "Impl and use default config for pallet-staking in tests (#4797)",
          "timestamp": "2024-06-17T12:35:15Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d91cbbd453c1d4553d7e3dc8753a2007fc4c5a67"
        },
        "date": 1718631246335,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037332102544,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04715301060799998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tom Mi",
            "username": "hitchhooker",
            "email": "tommi@niemi.lol"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6cb3bd23910ec48ab37a3c95a6b03286ff2979bf",
          "message": "Ibp bootnodes for Kusama People (#6) (#4741)\n\n* fix rotko's pcollectives bootnode\n* Update people-kusama.json\n* Add Dwellir People Kusama bootnode\n* add Gatotech bootnodes to `people-kusama`\n* Add Dwellir People Kusama bootnode\n* Update Amforc bootnodes for Kusama and Polkadot (#4668)\n\n---------\n\nCo-authored-by: RadiumBlock <info@radiumblock.com>\nCo-authored-by: Jonathan Udd <jonathan@dwellir.com>\nCo-authored-by: Milos Kriz <milos_kriz@hotmail.com>\nCo-authored-by: tugy <33746108+tugytur@users.noreply.github.com>\nCo-authored-by: Kutsal Kaan Bilgin <kutsalbilgin@gmail.com>\nCo-authored-by: Petr Mensik <petr.mensik1@gmail.com>\nCo-authored-by: Tommi <tommi@romeblockchain>",
          "timestamp": "2024-06-17T15:11:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6cb3bd23910ec48ab37a3c95a6b03286ff2979bf"
        },
        "date": 1718644028933,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.95599999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03579089512799999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04572791349,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Florian Franzen",
            "username": "FlorianFranzen",
            "email": "Florian.Franzen@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5055294521021c0ffa1c449d6793ec9d264e5bd5",
          "message": "node-inspect: do not depend on rocksdb (#4783)\n\nThe crate `sc-cli` otherwise enables the `rocksdb` feature.",
          "timestamp": "2024-06-17T18:47:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5055294521021c0ffa1c449d6793ec9d264e5bd5"
        },
        "date": 1718656877919,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93799999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046547350976000025,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036266912992000015,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kantapat chankasem",
            "username": "tesol2y090",
            "email": "gliese090@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "55a13abcd2f67e7fdfc8843f5c4a54798e26a9df",
          "message": "remove pallet::getter usage from pallet-timestamp (#3374)\n\nthis pr is a part of #3326\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-17T22:30:13Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/55a13abcd2f67e7fdfc8843f5c4a54798e26a9df"
        },
        "date": 1718670244610,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.928,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999999,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03741549381200001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04744983627999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Sandu",
            "username": "sandreim",
            "email": "54316454+sandreim@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "1dc68de8eec934b3c7f35a330f869d1172943da4",
          "message": "glutton: also increase parachain block length (#4728)\n\nGlutton currently is useful mostly for stress testing relay chain\nvalidators. It is unusable for testing the collator networking and block\nannouncement and import scenarios. This PR resolves that by improving\nglutton pallet to also buff up the blocks, up to the runtime configured\n`BlockLength`.\n\n### How it works\nIncludes an additional inherent in each parachain block. The `garbage`\nargument passed to the inherent is filled with trash data. It's size is\ncomputed by applying the newly introduced `block_length` percentage to\nthe maximum block length for mandatory dispatch class. After\nhttps://github.com/paritytech/polkadot-sdk/pull/4765 is merged, the\nlength of inherent extrinsic will be added to the total block proof\nsize.\n\nThe remaining weight is burnt in `on_idle` as configured by the\n`storage` percentage parameter.\n\n\nTODO:\n- [x] PRDoc\n- [x] Readme update\n- [x] Add tests\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-06-18T08:57:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1dc68de8eec934b3c7f35a330f869d1172943da4"
        },
        "date": 1718704007105,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93999999999994,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04661187672999998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036037940166,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Javier Bullrich",
            "username": "Bullrich",
            "email": "javier@bullrich.dev"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6daa939bc7c3f26c693a876d5a4b7ea00c6b2d7f",
          "message": "Migrated commands to github actions (#4701)\n\nMigrated commands individually to work as GitHub actions with a\n[`workflow_dispatch`](https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#workflow_dispatch)\nevent.\n\nThis will not disable the command-bot yet, but it's the first step\nbefore disabling it.\n\n### Commands migrated\n- [x] bench-all\n- [x] bench-overhead\n- [x] bench\n- [x] fmt\n- [x] update-ui\n\nAlso created an action that will inform users about the new\ndocumentation when they comment `bot`.\n\n### Created documentation \nCreated a detailed documentation on how to use this action. Found the\ndocumentation\n[here](https://github.com/paritytech/polkadot-sdk/blob/bullrich/cmd-action/.github/commands-readme.md).\n\n---------\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>\nCo-authored-by: Przemek Rzad <przemek@parity.io>",
          "timestamp": "2024-06-18T13:12:03Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6daa939bc7c3f26c693a876d5a4b7ea00c6b2d7f"
        },
        "date": 1718719447540,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94799999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03678316981599999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046689112112000004,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Niklas Adolfsson",
            "username": "niklasad1",
            "email": "niklasadolfsson1@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6c857609a9425902d6dfe5445afb16c6b23ad86c",
          "message": "rpc server: add `health/readiness endpoint` (#4802)\n\nPrevious attempt https://github.com/paritytech/substrate/pull/14314\n\nClose #4443 \n\nIdeally, we should move /health and /health/readiness to the prometheus\nserver but because it's was quite easy to implement on the RPC server\nand that RPC server already exposes /health.\n\nManual tests on a polkadot node syncing:\n\n```bash\n➜ polkadot-sdk (na-fix-4443) ✗ curl -v localhost:9944/health\n* Host localhost:9944 was resolved.\n* IPv6: ::1\n* IPv4: 127.0.0.1\n*   Trying [::1]:9944...\n* connect to ::1 port 9944 from ::1 port 55024 failed: Connection refused\n*   Trying 127.0.0.1:9944...\n* Connected to localhost (127.0.0.1) port 9944\n> GET /health HTTP/1.1\n> Host: localhost:9944\n> User-Agent: curl/8.5.0\n> Accept: */*\n>\n< HTTP/1.1 200 OK\n< content-type: application/json; charset=utf-8\n< content-length: 53\n< date: Fri, 14 Jun 2024 16:12:23 GMT\n<\n* Connection #0 to host localhost left intact\n{\"peers\":0,\"isSyncing\":false,\"shouldHavePeers\":false}%\n➜ polkadot-sdk (na-fix-4443) ✗ curl -v localhost:9944/health/readiness\n* Host localhost:9944 was resolved.\n* IPv6: ::1\n* IPv4: 127.0.0.1\n*   Trying [::1]:9944...\n* connect to ::1 port 9944 from ::1 port 54328 failed: Connection refused\n*   Trying 127.0.0.1:9944...\n* Connected to localhost (127.0.0.1) port 9944\n> GET /health/readiness HTTP/1.1\n> Host: localhost:9944\n> User-Agent: curl/8.5.0\n> Accept: */*\n>\n< HTTP/1.1 500 Internal Server Error\n< content-type: application/json; charset=utf-8\n< content-length: 0\n< date: Fri, 14 Jun 2024 16:12:36 GMT\n<\n* Connection #0 to host localhost left intact\n```\n\n//cc @BulatSaif you may be interested in this..\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-19T16:20:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6c857609a9425902d6dfe5445afb16c6b23ad86c"
        },
        "date": 1718816605258,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91399999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.04008166170800002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.049761670103999976,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "74decbbdf22a7b109209448307563c6f3d62abac",
          "message": "Bump curve25519-dalek from 4.1.2 to 4.1.3 (#4824)\n\nBumps\n[curve25519-dalek](https://github.com/dalek-cryptography/curve25519-dalek)\nfrom 4.1.2 to 4.1.3.\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/5312a0311ec40df95be953eacfa8a11b9a34bc54\"><code>5312a03</code></a>\ncurve: Bump version to 4.1.3 (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/660\">#660</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/b4f9e4df92a4689fb59e312a21f940ba06ba7013\"><code>b4f9e4d</code></a>\nSECURITY: fix timing variability in backend/serial/u32/scalar.rs (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/661\">#661</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/415892acf1cdf9161bd6a4c99bc2f4cb8fae5e6a\"><code>415892a</code></a>\nSECURITY: fix timing variability in backend/serial/u64/scalar.rs (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/659\">#659</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/56bf398d0caed63ef1d1edfbd35eb5335132aba2\"><code>56bf398</code></a>\nUpdates license field to valid SPDX format (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/647\">#647</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/9252fa5c0d09054fed4ac4d649e63c40fad7abaf\"><code>9252fa5</code></a>\nMitigate check-cfg until MSRV 1.77 (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/652\">#652</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/1efe6a93b176c4389b78e81e52b2cf85d728aac6\"><code>1efe6a9</code></a>\nFix a minor typo in signing.rs (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/649\">#649</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/cc3421a22fa7ee1f557cbe9243b450da53bbe962\"><code>cc3421a</code></a>\nIndicate that the rand_core feature is required (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/641\">#641</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/858c4ca8ae03d33fe8b71b4504c4d3f5ff5b45c0\"><code>858c4ca</code></a>\nAddress new nightly clippy unnecessary qualifications (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/639\">#639</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/31ccb6705067d68782cb135e23c79b640a6a06ee\"><code>31ccb67</code></a>\nRemove platforms in favor using CARGO_CFG_TARGET_POINTER_WIDTH (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/636\">#636</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/19c7f4a5d5e577adc9cc65a837abef9ed7ebf0a4\"><code>19c7f4a</code></a>\nFix new nightly redundant import lint warns (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/638\">#638</a>)</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/compare/curve25519-4.1.2...curve25519-4.1.3\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=curve25519-dalek&package-manager=cargo&previous-version=4.1.2&new-version=4.1.3)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\nYou can disable automated security fix PRs for this repo from the\n[Security Alerts\npage](https://github.com/paritytech/polkadot-sdk/network/alerts).\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-06-20T08:56:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/74decbbdf22a7b109209448307563c6f3d62abac"
        },
        "date": 1718880703028,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.934,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.039354216877999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.049423600441999976,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a23abb17232107275089040a33ff38e6a801e648",
          "message": "Bump ws from 8.16.0 to 8.17.1 in /bridges/testing/framework/utils/generate_hex_encoded_call (#4825)\n\nBumps [ws](https://github.com/websockets/ws) from 8.16.0 to 8.17.1.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/websockets/ws/releases\">ws's\nreleases</a>.</em></p>\n<blockquote>\n<h2>8.17.1</h2>\n<h1>Bug fixes</h1>\n<ul>\n<li>Fixed a DoS vulnerability (<a\nhref=\"https://redirect.github.com/websockets/ws/issues/2231\">#2231</a>).</li>\n</ul>\n<p>A request with a number of headers exceeding\nthe[<code>server.maxHeadersCount</code>][]\nthreshold could be used to crash a ws server.</p>\n<pre lang=\"js\"><code>const http = require('http');\nconst WebSocket = require('ws');\n<p>const wss = new WebSocket.Server({ port: 0 }, function () {\nconst chars =\n&quot;!#$%&amp;'*+-.0123456789abcdefghijklmnopqrstuvwxyz^_`|~&quot;.split('');\nconst headers = {};\nlet count = 0;</p>\n<p>for (let i = 0; i &lt; chars.length; i++) {\nif (count === 2000) break;</p>\n<pre><code>for (let j = 0; j &amp;lt; chars.length; j++) {\n  const key = chars[i] + chars[j];\n  headers[key] = 'x';\n\n  if (++count === 2000) break;\n}\n</code></pre>\n<p>}</p>\n<p>headers.Connection = 'Upgrade';\nheaders.Upgrade = 'websocket';\nheaders['Sec-WebSocket-Key'] = 'dGhlIHNhbXBsZSBub25jZQ==';\nheaders['Sec-WebSocket-Version'] = '13';</p>\n<p>const request = http.request({\nheaders: headers,\nhost: '127.0.0.1',\nport: wss.address().port\n});</p>\n<p>request.end();\n});\n</code></pre></p>\n<p>The vulnerability was reported by <a\nhref=\"https://github.com/rrlapointe\">Ryan LaPointe</a> in <a\nhref=\"https://redirect.github.com/websockets/ws/issues/2230\">websockets/ws#2230</a>.</p>\n<p>In vulnerable versions of ws, the issue can be mitigated in the\nfollowing ways:</p>\n<ol>\n<li>Reduce the maximum allowed length of the request headers using the\n[<code>--max-http-header-size=size</code>][] and/or the\n[<code>maxHeaderSize</code>][] options so\nthat no more headers than the <code>server.maxHeadersCount</code> limit\ncan be sent.</li>\n</ol>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/3c56601092872f7d7566989f0e379271afd0e4a1\"><code>3c56601</code></a>\n[dist] 8.17.1</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/e55e5106f10fcbaac37cfa89759e4cc0d073a52c\"><code>e55e510</code></a>\n[security] Fix crash when the Upgrade header cannot be read (<a\nhref=\"https://redirect.github.com/websockets/ws/issues/2231\">#2231</a>)</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/6a00029edd924499f892aed8003cef1fa724cfe5\"><code>6a00029</code></a>\n[test] Increase code coverage</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/ddfe4a804d79e7788ab136290e609f91cf68423f\"><code>ddfe4a8</code></a>\n[perf] Reduce the amount of <code>crypto.randomFillSync()</code>\ncalls</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/b73b11828d166e9692a9bffe9c01a7e93bab04a8\"><code>b73b118</code></a>\n[dist] 8.17.0</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/29694a5905fa703e86667928e6bacac397469471\"><code>29694a5</code></a>\n[test] Use the <code>highWaterMark</code> variable</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/934c9d6b938b93c045cb13e5f7c19c27a8dd925a\"><code>934c9d6</code></a>\n[ci] Test on node 22</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/1817bac06e1204bfb578b8b3f4bafd0fa09623d0\"><code>1817bac</code></a>\n[ci] Do not test on node 21</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/96c9b3deddf56cacb2d756aaa918071e03cdbc42\"><code>96c9b3d</code></a>\n[major] Flip the default value of <code>allowSynchronousEvents</code>\n(<a\nhref=\"https://redirect.github.com/websockets/ws/issues/2221\">#2221</a>)</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/e5f32c7e1e6d3d19cd4a1fdec84890e154db30c1\"><code>e5f32c7</code></a>\n[fix] Emit at most one event per event loop iteration (<a\nhref=\"https://redirect.github.com/websockets/ws/issues/2218\">#2218</a>)</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/websockets/ws/compare/8.16.0...8.17.1\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=ws&package-manager=npm_and_yarn&previous-version=8.16.0&new-version=8.17.1)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\nYou can disable automated security fix PRs for this repo from the\n[Security Alerts\npage](https://github.com/paritytech/polkadot-sdk/network/alerts).\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-06-21T07:23:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a23abb17232107275089040a33ff38e6a801e648"
        },
        "date": 1718961151476,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03725007771200001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046161316461999974,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Samusev",
            "username": "alvicsam",
            "email": "41779041+alvicsam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b301218db8785c6d425ca9a9ef90daa80780f2ce",
          "message": "[ci] Change storage type for forklift in GHA (#4850)\n\nPR changes forklift authentication to gcs\n\ncc https://github.com/paritytech/ci_cd/issues/987",
          "timestamp": "2024-06-21T09:33:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b301218db8785c6d425ca9a9ef90daa80780f2ce"
        },
        "date": 1718970658084,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046078120548,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036115333002000004,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c4b3c1c6c6e492c4196e06fbba824a58e8119a3b",
          "message": "Bump time to fix compilation on latest nightly (#4862)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4748",
          "timestamp": "2024-06-21T15:05:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c4b3c1c6c6e492c4196e06fbba824a58e8119a3b"
        },
        "date": 1718989791144,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91199999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.04601231223,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.058433991220000016,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "812dbff17513cbd2aeb2ff9c41214711bd1c0004",
          "message": "Frame: `Consideration` trait generic over `Footprint` and indicates zero cost (#4596)\n\n`Consideration` trait generic over `Footprint` and indicates zero cost\nfor a give footprint.\n\n`Consideration` trait is generic over `Footprint` (currently defined\nover the type with the same name). This makes it possible to setup a\ncustom footprint (e.g. current number of proposals in the storage).\n\n`Consideration::new` and `Consideration::update` return an\n`Option<Self>` instead `Self`, this make it possible to indicate a no\ncost for a specific footprint (e.g. if current number of proposals in\nthe storage < max_proposal_count / 2 then no cost).\n\nThese cases need to be handled for\nhttps://github.com/paritytech/polkadot-sdk/pull/3151",
          "timestamp": "2024-06-22T13:54:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/812dbff17513cbd2aeb2ff9c41214711bd1c0004"
        },
        "date": 1719071953153,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92399999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.04598394609,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.05838000881799999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "girazoki",
            "username": "girazoki",
            "email": "gorka.irazoki@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f8feebc12736c04d60040e0f291615479f9951a5",
          "message": "Reinitialize should allow to override existing config in collationGeneration (#4833)\n\nCurrently the `Initialize` and `Reinitialize` messages in the\ncollationGeneration subsystem fail if:\n-  `Initialize` if there exists already another configuration and\n- `Reinitialize` if another configuration does not exist\n\nI propose to instead change the behaviour of `Reinitialize` to always\nset the config regardless of whether one exists or not.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2024-06-23T09:35:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f8feebc12736c04d60040e0f291615479f9951a5"
        },
        "date": 1719141545075,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92399999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038989935574,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04926476632599999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Nazar Mokrynskyi",
            "username": "nazar-pc",
            "email": "nazar@mokrynskyi.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "686aa233e67c619bcdbc8b758a9ddf92c3315cf1",
          "message": "Block import cleanups (#4842)\n\nI carried these things in a fork for a long time, I think wouldn't hurt\nto have it upstream.\n\nOriginally submitted as part of\nhttps://github.com/paritytech/polkadot-sdk/pull/1598 that went nowhere.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-23T11:36:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/686aa233e67c619bcdbc8b758a9ddf92c3315cf1"
        },
        "date": 1719149150209,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04867386897200002,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03836893484,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b7767168b7dd93964f40e8543b853097e3570621",
          "message": "Ensure earliest allowed block is at minimum the next block (#4823)\n\nWhen `min_enactment_period == 0` and `desired == At(n)` where `n` is\nsmaller than the current block number, the scheduling would fail. This\nhappened for example here:\nhttps://collectives.subsquare.io/fellowship/referenda/126\n\nTo ensure that this doesn't happen again, ensure that the earliest\nallowed block is at minimum the next block.",
          "timestamp": "2024-06-24T10:37:13Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b7767168b7dd93964f40e8543b853097e3570621"
        },
        "date": 1719227214772,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.926,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04727504998799999,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03695169342799999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5e62782d27a18d8c57da28617181c66cd57076b5",
          "message": "treasury pallet: remove unused config parameters (#4831)\n\nRemove unused config parameters `ApproveOrigin` and `OnSlash` from the\ntreasury pallet. Add `OnSlash` config parameter to the bounties and tips\npallets.\n\npart of https://github.com/paritytech/polkadot-sdk/issues/3800",
          "timestamp": "2024-06-24T12:31:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5e62782d27a18d8c57da28617181c66cd57076b5"
        },
        "date": 1719235851403,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.948,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.035751539126,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04554308903399998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dashangcun",
            "username": "dashangcun",
            "email": "907225865@qq.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "63e264446f6cabff06be72912eae902662dcb699",
          "message": "chore: remove repeat words (#4869)\n\nSigned-off-by: dashangcun <jchaodaohang@foxmail.com>\nCo-authored-by: dashangcun <jchaodaohang@foxmail.com>",
          "timestamp": "2024-06-24T15:00:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/63e264446f6cabff06be72912eae902662dcb699"
        },
        "date": 1719248903866,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.958,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.035781901483999984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04541261339399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "909bfc2d7c00a0fed7a5fd4e5292aa3fbe2299b6",
          "message": "[subsystem-bench] Trigger own assignments in approval-voting (#4772)",
          "timestamp": "2024-06-25T09:08:39Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/909bfc2d7c00a0fed7a5fd4e5292aa3fbe2299b6"
        },
        "date": 1719312982920,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92599999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.04200565245799999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04973421488800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3c213726cf165d8b1155d5151b9c548e879b5ff8",
          "message": "chain-spec-builder: Add support for `codeSubstitutes`  (#4685)\n\nWhile working on https://github.com/paritytech/polkadot-sdk/pull/4600 I\nfound that it would be nice if `chain-spec-builder` supported\n`codeSubstitutes`. After this PR is merged you can do:\n\n```\nchain-spec-builder add-code-substitute chain_spec.json my_runtime.compact.compressed.wasm 1234\n```\n\nIn addition, the `chain-spec-builder` was silently removing\n`relay_chain` and `para_id` fields when used on parachain chain-specs.\nThis is now fixed by providing a custom chain-spec extension that has\nthese fields marked as optional.",
          "timestamp": "2024-06-25T12:58:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3c213726cf165d8b1155d5151b9c548e879b5ff8"
        },
        "date": 1719323257335,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91599999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04898653255000002,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.039573446456000014,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "mail.guptanikhil@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2f3a1bf8736844272a7eb165780d9f283b19d5c0",
          "message": "Use real rust type for pallet alias in `runtime` macro (#4769)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/4723. Also,\ncloses https://github.com/paritytech/polkadot-sdk/issues/4622\n\nAs stated in the linked issue, this PR adds the ability to use a real\nrust type for pallet alias in the new `runtime` macro:\n```rust\n#[runtime::pallet_index(0)]\npub type System = frame_system::Pallet<Runtime>;\n```\n\nPlease note that the current syntax still continues to be supported.\n\nCC: @shawntabrizi @kianenigma\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-25T14:31:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2f3a1bf8736844272a7eb165780d9f283b19d5c0"
        },
        "date": 1719330306210,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94199999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04568269840600001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036910599694,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "20aecadbc7ed2e9fe3b8a7d345f1be301fc00ba0",
          "message": "[FRAME] Remove storage migration type (#3828)\n\nIntroduce migration type to remove data associated with a specific\nstorage of a pallet.\n\nBased on existing `RemovePallet` migration type.\n\nRequired for https://github.com/paritytech/polkadot-sdk/pull/3820\n\n---------\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-06-26T08:13:50Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/20aecadbc7ed2e9fe3b8a7d345f1be301fc00ba0"
        },
        "date": 1719392633866,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92999999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03809990176599999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04646798253,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Niklas Adolfsson",
            "username": "niklasad1",
            "email": "niklasadolfsson1@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7a2592e8458f8b3c5d9683eb02380a0f5959b5b3",
          "message": "rpc: upgrade jsonrpsee v0.23 (#4730)\n\nThis is PR updates jsonrpsee v0.23 which mainly changes:\n- Add `Extensions` which we now is using to get the connection id (used\nby the rpc spec v2 impl)\n- Update hyper to v1.0, http v1.0, soketto and related crates\n(hyper::service::make_service_fn is removed)\n- The subscription API for the client is modified to know why a\nsubscription was closed.\n\nFull changelog here:\nhttps://github.com/paritytech/jsonrpsee/releases/tag/v0.23.0\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-26T10:25:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7a2592e8458f8b3c5d9683eb02380a0f5959b5b3"
        },
        "date": 1719404216887,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94799999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.044861845360000034,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036134897732000015,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Lulu",
            "username": "Morganamilo",
            "email": "morgan@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7084463a49f2359dc2f378f5834c7252af02ed4d",
          "message": "Update parity publish (#4878)\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-06-26T12:20:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7084463a49f2359dc2f378f5834c7252af02ed4d"
        },
        "date": 1719411086492,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40599999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93199999999999,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03920275586800002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04902738136600001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "929a273ae1ba647628c4ba6e2f8737e58b596d6a",
          "message": "pallet assets: optional auto-increment for the asset ID (#4757)\n\nIntroduce an optional auto-increment setup for the IDs of new assets.\n\n---------\n\nCo-authored-by: joe petrowski <25483142+joepetrowski@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-26T16:36:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/929a273ae1ba647628c4ba6e2f8737e58b596d6a"
        },
        "date": 1719426729284,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91599999999993,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.044831557816,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036446774352,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d604e84ee71d685ac3b143b19976d0093b95a1e2",
          "message": "Ensure key ownership proof is optimal (#4699)\n\nEnsure that the key ownership proof doesn't contain duplicate or\nunneeded nodes.\n\nWe already have these checks for the bridge messages proof. Just making\nthem more generic and performing them also for the key ownership proof.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-06-27T11:17:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d604e84ee71d685ac3b143b19976d0093b95a1e2"
        },
        "date": 1719493646087,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93599999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045731850169999984,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03652288596999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dee18249742c4abbf81fcca62b40a868a394c3d4",
          "message": "BridgeHubs fresh weights for bridging pallets (#4891)\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-06-27T13:38:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dee18249742c4abbf81fcca62b40a868a394c3d4"
        },
        "date": 1719500542007,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93599999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045731850169999984,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03652288596999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dee18249742c4abbf81fcca62b40a868a394c3d4",
          "message": "BridgeHubs fresh weights for bridging pallets (#4891)\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-06-27T13:38:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dee18249742c4abbf81fcca62b40a868a394c3d4"
        },
        "date": 1719501739951,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93599999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045731850169999984,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03652288596999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Niklas Adolfsson",
            "username": "niklasad1",
            "email": "niklasadolfsson1@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "de41ae85ec600189c4621aaf9e58afc612f101f7",
          "message": "chore(deps): upgrade prometheous server to hyper v1 (#4898)\n\nPartly fixes\nhttps://github.com/paritytech/polkadot-sdk/pull/4890#discussion_r1655548633\n\nStill the offchain API needs to be updated to hyper v1.0 and I opened an\nissue for it, it's using low-level http body features that have been\nremoved",
          "timestamp": "2024-06-27T15:45:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/de41ae85ec600189c4621aaf9e58afc612f101f7"
        },
        "date": 1719509823774,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93799999999993,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037637074750000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046732126934000015,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "18a6a56cf35590062792a7122404a1ca09ab7fe8",
          "message": "Add `Runtime::OmniNode` variant to `polkadot-parachain` (#4805)\n\nAdding `Runtime::OmniNode` variant + small changes\n\n---------\n\nCo-authored-by: kianenigma <kian@parity.io>",
          "timestamp": "2024-06-28T06:02:30Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/18a6a56cf35590062792a7122404a1ca09ab7fe8"
        },
        "date": 1719560939442,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93599999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03522734362,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04529426500999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "016f394854a3fad6762913a0f208cece181c34fe",
          "message": "[Rococo<>Westend bridge] Allow any asset over the lane between the two Asset Hubs (#4888)\n\nOn Westend Asset Hub, we allow Rococo Asset Hub to act as reserve for\nany asset native to the Rococo or Ethereum ecosystems (practically\nproviding Westend access to Ethereum assets through double bridging:\nW<>R<>Eth).\n\nOn Rococo Asset Hub, we allow Westend Asset Hub to act as reserve for\nany asset native to the Westend ecosystem. We also allow Ethereum\ncontracts to act as reserves for the foreign assets identified by the\nsame respective contracts locations.\n\n- [x] add emulated tests for various assets (native, trust-based,\nforeign/bridged) going AHR -> AHW,\n- [x] add equivalent tests for the other direction AHW -> AHR.\n\nThis PR is a prerequisite to doing the same for Polkadot<>Kusama bridge.",
          "timestamp": "2024-06-28T09:09:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/016f394854a3fad6762913a0f208cece181c34fe"
        },
        "date": 1719568523178,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.954,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036429158684000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04637313299600002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "aaf0443591b134a0da217d575161872796e75059",
          "message": "network: Sync peerstore constants between libp2p and litep2p (#4906)\n\nCounterpart of: https://github.com/paritytech/polkadot-sdk/pull/4031\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-06-28T13:43:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aaf0443591b134a0da217d575161872796e75059"
        },
        "date": 1719588664547,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94999999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045780159236,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.035727389467999984,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "polka.dom",
            "username": "PolkadotDom",
            "email": "polkadotdom@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "333f4c78109345debb5cf6e8c8b3fe75a7bbe3fb",
          "message": "Remove getters from pallet-membership (#4840)\n\nAs per #3326 , removes pallet::getter macro usage from\npallet-membership. The syntax StorageItem::<T, I>::get() should be used\ninstead. Also converts some syntax to turbo and reimplements the removed\ngetters, following #223\n\ncc @muraca\n\n---------\n\nCo-authored-by: Dónal Murray <donalm@seadanda.dev>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-07-01T12:25:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/333f4c78109345debb5cf6e8c8b3fe75a7bbe3fb"
        },
        "date": 1719844512283,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92599999999993,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03590812205599999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04550532966400003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Yuri Volkov",
            "username": "mutantcornholio",
            "email": "0@mcornholio.ru"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "18228a9bdbe5b560a39a31810fdf2e3fba59a40d",
          "message": "prdoc upgrade (#4918)\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-01T14:54:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/18228a9bdbe5b560a39a31810fdf2e3fba59a40d"
        },
        "date": 1719852706700,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.91799999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04826996394199999,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.038401107400000016,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kazunobu Ndong",
            "username": "ndkazu",
            "email": "33208377+ndkazu@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "18ed309a37036db8429665f1e91fb24ab312e646",
          "message": "Pallet Name Customisation (#4806)\n\nAdded Instructions for pallet name customisation in the ReadMe\r\n\r\n---------\r\n\r\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\r\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\r\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-01T20:18:27Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/18ed309a37036db8429665f1e91fb24ab312e646"
        },
        "date": 1719870237946,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92999999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03869155290400002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04803751275400001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "98ce675a6bfafa145dd6be74c95d7768917392c1",
          "message": "bridge tests: send bridged assets from random parachain to bridged asset hub (#4870)\n\n- Send bridged WNDs: Penpal Rococo -> AH Rococo -> AH Westend\n- Send bridged ROCs: Penpal Westend -> AH Westend -> AH Rococo\n\nThe tests send both ROCs and WNDs, for each direction the native asset\nis only used to pay for the transport fees on the local AssetHub, and\nare not sent over the bridge.\n\nIncluding the native asset won't be necessary anymore once we get #4375.\n\n---------\n\nSigned-off-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-03T09:07:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/98ce675a6bfafa145dd6be74c95d7768917392c1"
        },
        "date": 1720000719493,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.944,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036622373096000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046756060619999984,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "98ce675a6bfafa145dd6be74c95d7768917392c1",
          "message": "bridge tests: send bridged assets from random parachain to bridged asset hub (#4870)\n\n- Send bridged WNDs: Penpal Rococo -> AH Rococo -> AH Westend\n- Send bridged ROCs: Penpal Westend -> AH Westend -> AH Rococo\n\nThe tests send both ROCs and WNDs, for each direction the native asset\nis only used to pay for the transport fees on the local AssetHub, and\nare not sent over the bridge.\n\nIncluding the native asset won't be necessary anymore once we get #4375.\n\n---------\n\nSigned-off-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-03T09:07:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/98ce675a6bfafa145dd6be74c95d7768917392c1"
        },
        "date": 1720005334091,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04798910721199998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03809756361800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "mail.guptanikhil@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e5791a56dcc35e308a80985cc3b6b7f2ed1eb6ec",
          "message": "Fixes warnings in `frame-support-procedural` crate (#4915)\n\nThis PR fixes the unused warnings in `frame-support-procedural` crate,\nraised by the latest stable rust release.",
          "timestamp": "2024-07-03T11:32:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e5791a56dcc35e308a80985cc3b6b7f2ed1eb6ec"
        },
        "date": 1720010805949,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04798910721199998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03809756361800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "mail.guptanikhil@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e5791a56dcc35e308a80985cc3b6b7f2ed1eb6ec",
          "message": "Fixes warnings in `frame-support-procedural` crate (#4915)\n\nThis PR fixes the unused warnings in `frame-support-procedural` crate,\nraised by the latest stable rust release.",
          "timestamp": "2024-07-03T11:32:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e5791a56dcc35e308a80985cc3b6b7f2ed1eb6ec"
        },
        "date": 1720012369060,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04798910721199998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03809756361800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b6f18232449b003d716a0d630f3da3a9dce669ab",
          "message": "[BEEFY] Add runtime support for reporting fork voting (#4522)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/4523\n\nExtracting part of https://github.com/paritytech/polkadot-sdk/pull/1903\n(credits to @Lederstrumpf for the high-level strategy), but also\nintroducing significant adjustments both to the approach and to the\ncode. The main adjustment is the fact that the `ForkVotingProof` accepts\nonly one vote, compared to the original version which accepted a\n`vec![]`. With this approach more calls are needed in order to report\nmultiple equivocated votes on the same commit, but it simplifies a lot\nthe checking logic. We can add support for reporting multiple signatures\nat once in the future.\n\nThere are 2 things that are missing in order to consider this issue\ndone, but I would propose to do them in a separate PR since this one is\nalready pretty big:\n- benchmarks/computing a weight for the new extrinsic (this wasn't\npresent in https://github.com/paritytech/polkadot-sdk/pull/1903 either)\n- exposing an API for generating the ancestry proof. I'm not sure if we\nshould do this in the Mmr pallet or in the Beefy pallet\n\nCo-authored-by: Robert Hambrock <roberthambrock@gmail.com>\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-07-03T13:44:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b6f18232449b003d716a0d630f3da3a9dce669ab"
        },
        "date": 1720020996692,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92199999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038320999138,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04904475088000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "282eaaa5f49749f68508752a993d6c79d64f6162",
          "message": "[Staking] Delegators can stake but stakers can't delegate (#4904)\n\nRelated: https://github.com/paritytech/polkadot-sdk/pull/4804.\nFixes the try state error in Westend:\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6564522.\nPasses here:\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6580393\n\n## Context\nCurrently in Kusama and Polkadot, an account can do both, directly\nstake, and join a pool.\n\nWith the migration of pools to `DelegateStake` (See\nhttps://github.com/paritytech/polkadot-sdk/pull/3905), the funds of pool\nmembers are locked in a different way than for direct stakers.\n- Pool member funds uses `holds`.\n- `pallet-staking` uses deprecated locks (analogous to freeze) which can\noverlap with holds.\n\nAn existing delegator can stake directly since pallet-staking only uses\nfree balance. But once an account becomes staker, we cannot allow them\nto be delegator as this risks an account to use already staked (frozen)\nfunds in pools.\n\nWhen an account gets into a situation where it is participating in both\npools and staking, it would no longer would be able to add any extra\nbond to the pool but they can still withdraw funds.\n\n## Changes\n- Add test for the above scenario.\n- Removes the assumption that a delegator cannot be a staker.",
          "timestamp": "2024-07-03T17:16:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/282eaaa5f49749f68508752a993d6c79d64f6162"
        },
        "date": 1720033833809,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.9439999999999,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036431150898,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04607055511,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Axay Sagathiya",
            "username": "axaysagathiya",
            "email": "axaysagathiya@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "51e98273d78ebd9c1f2b259c9d67e75780c0192a",
          "message": "rename the candidate backing message from `GetBackedCandidates` to `GetBackableCandidates` (#4921)\n\n**Backable Candidate**: If a candidate receives enough supporting\nStatements from the Parachain Validators currently assigned, that\ncandidate is considered backable.\n**Backed Candidate**: A Backable Candidate noted in a relay-chain block\n\n---\n\nWhen the candidate backing subsystem receives the `GetBackedCandidates`\nmessage, it sends back **backable** candidates, not **backed**\ncandidates. So we should rename this message to `GetBackableCandidates`\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-03T19:37:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/51e98273d78ebd9c1f2b259c9d67e75780c0192a"
        },
        "date": 1720042202199,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03817133221,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.047152782461999984,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "polka.dom",
            "username": "PolkadotDom",
            "email": "polkadotdom@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "924728cf19523f826a08e1c0eeee711ca3bb8ee7",
          "message": "Remove getter macro from pallet-insecure-randomness-collective-flip (#4839)\n\nAs per #3326, removes pallet::getter macro usage from the\npallet-insecure-randomness-collective-flip. The syntax `StorageItem::<T,\nI>::get()` should be used instead.\n\nExplicitly implements the getters that were removed as well, following\n#223\n\nAlso makes the storage values public and converts some syntax to turbo\n\ncc @muraca\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-03T20:53:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/924728cf19523f826a08e1c0eeee711ca3bb8ee7"
        },
        "date": 1720049310935,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037073152154000005,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046632329329999965,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e44f61af440316f4050f69df024fe964ffcd9346",
          "message": "Introduce basic slot-based collator (#4097)\n\nPart of #3168 \nOn top of #3568\n\n### Changes Overview\n- Introduces a new collator variant in\n`cumulus/client/consensus/aura/src/collators/slot_based/mod.rs`\n- Two tasks are part of that module, one for block building and one for\ncollation building and submission.\n- Introduces a new variant of `cumulus-test-runtime` which has 2s slot\nduration, used for zombienet testing\n- Zombienet tests for the new collator\n\n**Note:** This collator is considered experimental and should only be\nused for testing and exploration for now.\n\n### Comparison with `lookahead` collator\n- The new variant is slot based, meaning it waits for the next slot of\nthe parachain, then starts authoring\n- The search for potential parents remains mostly unchanged from\nlookahead\n- As anchor, we use the current best relay parent\n- In general, the new collator tends to be anchored to one relay parent\nearlier. `lookahead` generally waits for a new relay block to arrive\nbefore it attempts to build a block. This means the actual timing of\nparachain blocks depends on when the relay block has been authored and\nimported. With the slot-triggered approach we are authoring directly on\nthe slot boundary, were a new relay chain block has probably not yet\narrived.\n\n### Limitations\n- Overall, the current implementation focuses on the \"happy path\"\n- We assume that we want to collate close to the tip of the relay chain.\nIt would be useful however to have some kind of configurable drift, so\nthat we could lag behind a bit.\nhttps://github.com/paritytech/polkadot-sdk/issues/3965\n- The collation task is pretty dumb currently. It checks if we have\ncores scheduled and if yes, submits all the messages we have received\nfrom the block builder until we have something submitted for every core.\nIdeally we should do some extra checks, i.e. we do not need to submit if\nthe built block is already too old (build on a out of range relay\nparent) or was authored with a relay parent that is not an ancestor of\nthe relay block we are submitting at.\nhttps://github.com/paritytech/polkadot-sdk/issues/3966\n- There is no throttling, we assume that we can submit _velocity_ blocks\nevery relay chain block. There should be communication between the\ncollator task and block-builder task.\n- The parent search and ConsensusHook are not yet properly adjusted. The\nparent search makes assumptions about the pending candidate which no\nlonger hold. https://github.com/paritytech/polkadot-sdk/issues/3967\n- Custom triggers for block building not implemented.\n\n---------\n\nCo-authored-by: Davide Galassi <davxy@datawok.net>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-05T09:00:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e44f61af440316f4050f69df024fe964ffcd9346"
        },
        "date": 1720176663284,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92399999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036300145776000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04639367407399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Samusev",
            "username": "alvicsam",
            "email": "41779041+alvicsam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "299aacb56f4f11127b194d12692b00066e91ac92",
          "message": "[ci] Increase timeout for ci jobs (#4950)\n\nRelated to recent discussion. PR makes timeout less strict.\n\ncc https://github.com/paritytech/ci_cd/issues/996",
          "timestamp": "2024-07-05T11:13:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/299aacb56f4f11127b194d12692b00066e91ac92"
        },
        "date": 1720184354268,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94800000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03554854437599999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04550043995999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Nazar Mokrynskyi",
            "username": "nazar-pc",
            "email": "nazar@mokrynskyi.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "221eddc90cd1efc4fc3c822ce5ccf289272fb41d",
          "message": "Optimize finalization performance (#4922)\n\nThis PR largely fixes\nhttps://github.com/paritytech/polkadot-sdk/issues/4903 by addressing it\nfrom a few different directions.\n\nThe high-level observation is that complexity of finalization was\nunfortunately roughly `O(n^3)`. Not only\n`displaced_leaves_after_finalizing` was extremely inefficient on its\nown, especially when large ranges of blocks were involved, it was called\nonce upfront and then on every single block that was finalized over and\nover again.\n\nThe first commit refactores code adjacent to\n`displaced_leaves_after_finalizing` to optimize memory allocations. For\nexample things like `BTreeMap<_, Vec<_>>` were very bad in terms of\nnumber of allocations and after analyzing code paths was completely\nunnecessary and replaced with `Vec<(_, _)>`. In other places allocations\nof known size were not done upfront and some APIs required unnecessary\ncloning of vectors.\n\nI checked invariants and didn't find anything that was violated after\nrefactoring.\n\nSecond commit completely replaces `displaced_leaves_after_finalizing`\nimplementation with a much more efficient one. In my case with ~82k\nblocks and ~13k leaves it takes ~5.4s to finish\n`client.apply_finality()` now.\n\nThe idea is to avoid querying the same blocks over and over again as\nwell as introducing temporary local cache for blocks related to leaves\nabove block that is being finalized as well as local cache of the\nfinalized branch of the chain. I left some comments in the code and\nwrote tests that I belive should check all code invariants for\ncorrectness. `lowest_common_ancestor_multiblock` was removed as\nunnecessary and not great in terms of performance API, domain-specific\ncode should be written instead like done in\n`displaced_leaves_after_finalizing`.\n\nAfter these changes I noticed finalization is still horribly slow,\nturned out that even though `displaced_leaves_after_finalizing` was way\nfaster that before (probably order of magnitude), it was called for\nevery single of those 82k blocks :facepalm:\n\nThe quick hack I came up with in the third commit to handle this edge\ncase was to not call it when finalizing multiple blocks at once until\nthe very last moment. It works and allows to finish the whole\nfinalization in just 14 seconds (5.4+5.4 of which are two calls to\n`displaced_leaves_after_finalizing`). I'm really not happy with the fact\nthat `displaced_leaves_after_finalizing` is called twice, but much\nheavier refactoring would be necessary to get rid of second call.\n\n---\n\nNext steps:\n* assuming the changes are acceptable I'll write prdoc\n* https://github.com/paritytech/polkadot-sdk/pull/4920 or something\nsimilar in spirit should be implemented to unleash efficient parallelsm\nwith rayon in `displaced_leaves_after_finalizing`, which will allow to\nfurther (and significant!) scale its performance rather that being\nCPU-bound on a single core, also reading database sequentially should\nideally be avoided\n* someone should look into removal of the second\n`displaced_leaves_after_finalizing` call\n* further cleanups are possible if `undo_finalization` can be removed\n\n---\n\nPolkadot Address: 1vSxzbyz2cJREAuVWjhXUT1ds8vBzoxn2w4asNpusQKwjJd\n\n---------\n\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-07-05T19:02:18Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/221eddc90cd1efc4fc3c822ce5ccf289272fb41d"
        },
        "date": 1720210047629,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.984,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40999999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04589528185599999,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03599535505799999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Deepak Chaudhary",
            "username": "Aideepakchaudhary",
            "email": "54492415+Aideepakchaudhary@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d3cdfc4469ca9884403d52c94f2cb14bc62e6697",
          "message": "remove getter from babe pallet (#4912)\n\n### ISSUE\nLink to the issue:\nhttps://github.com/paritytech/polkadot-sdk/issues/3326\ncc @muraca \n\nDeliverables\n - [Deprecation] remove pallet::getter usage from all pallet-babe\n\n### Test Outcomes\n___\nSuccessful tests by running `cargo test -p pallet-babe --features\nruntime-benchmarks`\n\n\nrunning 32 tests\ntest\nmock::__pallet_staking_reward_curve_test_module::reward_curve_piece_count\n... ok\ntest mock::__construct_runtime_integrity_test::runtime_integrity_tests\n... ok\ntest mock::test_genesis_config_builds ... ok\n2024-06-28T17:02:11.158812Z ERROR runtime::storage: Corrupted state at\n`0x1cb6f36e027abb2091cfb5110ab5087f9aab0a5b63b359512deee557c9f4cf63`:\nError { cause: Some(Error { cause: None, desc: \"Could not decode\n`NextConfigDescriptor`, variant doesn't exist\" }), desc: \"Could not\ndecode `Option::Some(T)`\" }\n2024-06-28T17:02:11.159752Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::add_epoch_configurations_migration_works ... ok\ntest tests::author_vrf_output_for_secondary_vrf ... ok\ntest benchmarking::bench_check_equivocation_proof ... ok\n2024-06-28T17:02:11.160537Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::can_estimate_current_epoch_progress ... ok\ntest tests::author_vrf_output_for_primary ... ok\ntest tests::authority_index ... ok\n2024-06-28T17:02:11.162327Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::empty_randomness_is_correct ... ok\ntest tests::check_module ... ok\n2024-06-28T17:02:11.163492Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::current_slot_is_processed_on_initialization ... ok\ntest tests::can_enact_next_config ... ok\n2024-06-28T17:02:11.164987Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.165007Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::can_predict_next_epoch_change ... ok\ntest tests::first_block_epoch_zero_start ... ok\ntest tests::initial_values ... ok\n2024-06-28T17:02:11.168430Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.168685Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.170982Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.171220Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::only_root_can_enact_config_change ... ok\ntest tests::no_author_vrf_output_for_secondary_plain ... ok\ntest tests::can_fetch_current_and_next_epoch_data ... ok\n2024-06-28T17:02:11.172960Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::report_equivocation_has_valid_weight ... ok\n2024-06-28T17:02:11.173873Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.177084Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::report_equivocation_after_skipped_epochs_works ...\n2024-06-28T17:02:11.177694Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.177703Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.177925Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.177927Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\nok\n2024-06-28T17:02:11.179678Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.181446Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.183665Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.183874Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.185732Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.185951Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.189332Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.189559Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.189587Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::generate_equivocation_report_blob ... ok\ntest tests::disabled_validators_cannot_author_blocks - should panic ...\nok\n2024-06-28T17:02:11.190552Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.192279Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.194735Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.196136Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.197240Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::skipping_over_epochs_works ... ok\n2024-06-28T17:02:11.202783Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.202846Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.203029Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.205242Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::tracks_block_numbers_when_current_and_previous_epoch_started\n... ok\n2024-06-28T17:02:11.208965Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::report_equivocation_current_session_works ... ok\ntest tests::report_equivocation_invalid_key_owner_proof ... ok\n2024-06-28T17:02:11.216431Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.216855Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::report_equivocation_validate_unsigned_prevents_duplicates\n... ok\ntest tests::report_equivocation_invalid_equivocation_proof ... ok\ntest tests::valid_equivocation_reports_dont_pay_fees ... ok\ntest tests::report_equivocation_old_session_works ... ok\ntest\nmock::__pallet_staking_reward_curve_test_module::reward_curve_precision\n... ok\n\ntest result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered\nout; finished in 0.20s\n\n   Doc-tests pallet-babe\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered\nout; finished in 0.00s\n\n---\n\nPolkadot Address: 16htXkeVhfroBhL6nuqiwknfXKcT6WadJPZqEi2jRf9z4XPY",
          "timestamp": "2024-07-06T21:29:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d3cdfc4469ca9884403d52c94f2cb14bc62e6697"
        },
        "date": 1720304396008,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04537554765800001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.035449642738000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f7dd85d053dc44ee0a6851e7e507083f31b01bd3",
          "message": "Assets: can_decrease/increase for destroying asset is not successful (#3286)\n\nFunctions `can_decrease` and `can_increase` do not return successful\nconsequence results for assets undergoing destruction; instead, they\nreturn the `UnknownAsset` consequence variant.\n\nThis update aligns their behavior with similar functions, such as\n`reducible_balance`, `increase_balance`, `decrease_balance`, and `burn`,\nwhich return an `AssetNotLive` error for assets in the process of being\ndestroyed.",
          "timestamp": "2024-07-07T11:45:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f7dd85d053dc44ee0a6851e7e507083f31b01bd3"
        },
        "date": 1720359635847,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.89599999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.05373987694,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.04562849714000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e1460b5ee5f4490b428035aa4a72c1c99a262459",
          "message": "[Backport] Version bumps  and  prdocs reordering from 1.14.0 (#4955)\n\nThis PR backports regular version bumps and prdocs reordering from the\n1.14.0 release branch to master",
          "timestamp": "2024-07-08T07:40:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e1460b5ee5f4490b428035aa4a72c1c99a262459"
        },
        "date": 1720431628747,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92399999999991,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04678114826399998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03788009343999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7290042ad4975e9d42633f228e331f286397025e",
          "message": "Make `tracing::log` work in the runtime (#4863)\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-08T14:19:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7290042ad4975e9d42633f228e331f286397025e"
        },
        "date": 1720456441563,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94799999999992,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40799999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.047183627224000005,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03686724129200001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d4657f86208a13d8fcc7933018d4558c3a96f634",
          "message": "litep2p/peerstore: Fix bump last updated time (#4971)\n\nThis PR bumps the last time of a reputation update of a peer.\nDoing so ensures the peer remains in the peerstore for longer than 1\nhour.\n\nLibp2p updates the `last_updated` field as well.\n\nSmall summary for the peerstore:\n- A: when peers are reported the `last_updated` time is set to current\ntime (not done before this PR)\n- B: peers that were not updated for 1 hour are removed from the\npeerstore\n- the reputation of the peers is decaying to zero over time\n- peers are reported with a reputation change (positive or negative\ndepending on the behavior)\n\nBecause, (A) was not updating the `last_updated` time, we might lose the\nreputation of peers that are constantly updated after 1hour because of\n(B).\n\ncc @paritytech/networking\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-07-08T15:40:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d4657f86208a13d8fcc7933018d4558c3a96f634"
        },
        "date": 1720462479694,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.95599999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03756940747399999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04775867604999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Or Grinberg",
            "username": "orgr",
            "email": "or.grin@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "72dab6d897d9519881b619d466ef5be3c7df9a34",
          "message": "allow clear_origin in safe xcm builder (#4777)\n\nFixes #3770 \n\nAdded `clear_origin` as an allowed command after commands that load the\nholdings register, in the safe xcm builder.\n\nChecklist\n- [x] My PR includes a detailed description as outlined in the\n\"Description\" section above\n- [x] My PR follows the [labeling\nrequirements](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md#Process)\nof this project (at minimum one label for T required)\n- [x] I have made corresponding changes to the documentation (if\napplicable)\n- [x] I have added tests that prove my fix is effective or that my\nfeature works (if applicable)\n\n---------\n\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: gupnik <mail.guptanikhil@gmail.com>",
          "timestamp": "2024-07-09T03:40:39Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/72dab6d897d9519881b619d466ef5be3c7df9a34"
        },
        "date": 1720503227048,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.96199999999993,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036061638466000016,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04581154457000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "01e0fc23d8844461adcd4501815dac64f3a4986f",
          "message": "`polkadot-parachain` simplifications and deduplications (#4916)\n\n`polkadot-parachain` simplifications and deduplications\n\nDetails in the commit messages. Just copy-pasting the last commit\ndescription since it introduces the biggest changes:\n\n```\n    Implement a more structured way to define a node spec\n    \n    - use traits instead of bounds for `rpc_ext_builder()`,\n      `build_import_queue()`, `start_consensus()`\n    - add a `NodeSpec` trait for defining the specifications of a node\n    - deduplicate the code related to building a node's components /\n      starting a node\n```\n\nThe other changes are much smaller, most of them trivial and are\nisolated in separate commits.",
          "timestamp": "2024-07-09T08:30:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/01e0fc23d8844461adcd4501815dac64f3a4986f"
        },
        "date": 1720521030106,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.95999999999992,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036179848136,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046526736968,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9403a5d40214b0d223c87c8d7b13139672edfe95",
          "message": "Add `MAX_INSTRUCTIONS_TO_DECODE` to XCMv2 (#4978)\n\nIt was added to v4 and v3 but was missing from v2",
          "timestamp": "2024-07-09T13:49:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9403a5d40214b0d223c87c8d7b13139672edfe95"
        },
        "date": 1720535165416,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93199999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.04023065769799999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.05190449470800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "02e50adf7ba6cc65a9ef5c332b3e2974c8d23f48",
          "message": "Explain usage of `<T: Config>` in FRAME storage + Update parachain pallet template  (#4941)\n\nExplains one of the annoying parts of FRAME storage that we have seen\nmultiple times in PBA everyone gets stuck on.\n\nI have not updated the other two templates for now, and only reflected\nit in the parachain template. That can happen in a follow-up.\n\n- [x] Update possible answers in SE about the same topic.\n\n---------\n\nCo-authored-by: Serban Iorga <serban@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-10T16:46:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/02e50adf7ba6cc65a9ef5c332b3e2974c8d23f48"
        },
        "date": 1720636419703,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94199999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.035749216228,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045504692148000035,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Javier Bullrich",
            "username": "Bullrich",
            "email": "javier@bullrich.dev"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6dd777ffe62cff936e04a76134ccf07de9dee429",
          "message": "fixed cmd bot commenting not working (#5000)\n\nFixed the mentioned issue:\nhttps://github.com/paritytech/command-bot/issues/113#issuecomment-2222277552\n\nNow it will properly comment when the old bot gets triggered.",
          "timestamp": "2024-07-11T13:02:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6dd777ffe62cff936e04a76134ccf07de9dee429"
        },
        "date": 1720709351157,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.035446455206000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04530472039000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "53598b8ef5c17d4328dab47f1540bfa80649b1a0",
          "message": "Remove usage of `sp-std` on templates (#5001)\n\nFollowing PR for https://github.com/paritytech/polkadot-sdk/pull/4941\nthat removes usage of `sp-std` on templates\n\n`sp-std` crate was proposed to deprecate on\nhttps://github.com/paritytech/polkadot-sdk/issues/2101\n\n@kianenigma\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-07-11T23:35:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/53598b8ef5c17d4328dab47f1540bfa80649b1a0"
        },
        "date": 1720750306417,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.96399999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04500491831000002,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03531295812,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "1f8e44831d0a743b116dfb5948ea9f4756955962",
          "message": "Bridges V2 refactoring backport and `pallet_bridge_messages` simplifications (#4935)\n\n## Summary\n\nThis PR contains migrated code from the Bridges V2\n[branch](https://github.com/paritytech/polkadot-sdk/pull/4427) from the\nold `parity-bridges-common`\n[repo](https://github.com/paritytech/parity-bridges-common/tree/bridges-v2).\nEven though the PR looks large, it does not (or should not) contain any\nsignificant changes (also not relevant for audit).\nThis PR is a requirement for permissionless lanes, as they were\nimplemented on top of these changes.\n\n## TODO\n\n- [x] generate fresh weights for BridgeHubs\n- [x] run `polkadot-fellows` bridges zombienet tests with actual runtime\n1.2.5. or 1.2.6 to check compatibility\n- :ballot_box_with_check: working, checked with 1.2.8 fellows BridgeHubs\n- [x] run `polkadot-sdk` bridges zombienet tests\n  - :ballot_box_with_check: with old relayer in CI (1.6.5) \n- [x] run `polkadot-sdk` bridges zombienet tests (locally) - with the\nrelayer based on this branch -\nhttps://github.com/paritytech/parity-bridges-common/pull/3022\n- [x] check/fix relayer companion in bridges repo -\nhttps://github.com/paritytech/parity-bridges-common/pull/3022\n- [x] extract pruning stuff to separate PR\nhttps://github.com/paritytech/polkadot-sdk/pull/4944\n\nRelates to:\nhttps://github.com/paritytech/parity-bridges-common/issues/2976\nRelates to:\nhttps://github.com/paritytech/parity-bridges-common/issues/2451\n\n---------\n\nSigned-off-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Serban Iorga <serban@parity.io>\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-12T08:20:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1f8e44831d0a743b116dfb5948ea9f4756955962"
        },
        "date": 1720778820317,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92199999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03676741448599999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04712980672599997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d31285a1562318959a7b21dbfec95c2fd6f06d7a",
          "message": "[statement-distribution] Add metrics for distributed statements in V2 (#4554)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/4334",
          "timestamp": "2024-07-12T10:43:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d31285a1562318959a7b21dbfec95c2fd6f06d7a"
        },
        "date": 1720787498990,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93199999999993,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045887962572000014,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036439017446000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dharjeezy",
            "username": "dharjeezy",
            "email": "dharjeezy@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4aa29a41cf731b8181f03168240e8dedb2adfa7a",
          "message": "Try State Hook for Bounties (#4563)\n\nPart of: https://github.com/paritytech/polkadot-sdk/issues/239\n\nPolkadot address: 12GyGD3QhT4i2JJpNzvMf96sxxBLWymz4RdGCxRH5Rj5agKW\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-12T11:51:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4aa29a41cf731b8181f03168240e8dedb2adfa7a"
        },
        "date": 1720795151784,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.95199999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038520533797999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.049449457988000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d2dff5f1c3f705c5acdad040447822f92bb02891",
          "message": "network/tx: Ban peers with tx that fail to decode (#5002)\n\nA malicious peer can submit random bytes on transaction protocol.\nIn this case, the peer is not disconnected or reported back to the\npeerstore.\n\nThis PR ensures the peer's reputation is properly reported.\n\nDiscovered during testing:\n- https://github.com/paritytech/polkadot-sdk/pull/4977\n\n\ncc @paritytech/networking\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-07-15T08:31:06Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d2dff5f1c3f705c5acdad040447822f92bb02891"
        },
        "date": 1721039109640,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93799999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04883456351800005,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03827381267000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "291210aa0fafa97d9b924fe82d68c023bdb0a340",
          "message": "Use sp_runtime::traits::BadOrigin (#5011)\n\nIt says `Will be removed after July 2023` but that's not true 😃\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-15T10:45:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/291210aa0fafa97d9b924fe82d68c023bdb0a340"
        },
        "date": 1721046938322,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94999999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.035560535148,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04490776425000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7ecf3f757a5d6f622309cea7f788e8a547a5dce8",
          "message": "Remove most all usage of `sp-std` (#5010)\n\nThis should remove nearly all usage of `sp-std` except:\n- bridge and bridge-hubs\n- a few of frames re-export `sp-std`, keep them for now\n- there is a usage of `sp_std::Writer`, I don't have an idea how to move\nit\n\nPlease review proc-macro carefully. I'm not sure I'm doing it the right\nway.\n\nNote: need `/bot fmt`\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-15T13:50:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7ecf3f757a5d6f622309cea7f788e8a547a5dce8"
        },
        "date": 1721058816259,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93399999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04547377084000001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03561931226799999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c05b0b970942c2bc298f61be62cc2e2b5d46af19",
          "message": "Updated substrate-relay version for tests (#5017)\n\n## Testing\n\nBoth Bridges zombienet tests passed, e.g.:\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6698640\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6698641\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6700072\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6700073",
          "timestamp": "2024-07-15T21:10:03Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c05b0b970942c2bc298f61be62cc2e2b5d46af19"
        },
        "date": 1721084344484,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.962,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.035641428695999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04561570473800002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Niklas Adolfsson",
            "username": "niklasad1",
            "email": "niklasadolfsson1@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "926c1b6adcaa767f4887b5774ef1fdde75156dd9",
          "message": "rpc: add back rpc logger (#4952)\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-15T22:57:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/926c1b6adcaa767f4887b5774ef1fdde75156dd9"
        },
        "date": 1721090685801,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.95999999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03579584956200002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045396176844000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "79a3d6c294430a52f00563f8a5e59984680b889f",
          "message": "Adjust base value for statement-distribution regression tests (#5028)\n\nA baseline for the statement-distribution regression test was set only\nin the beginning and now we see that the actual values a bit lower.\n\n<img width=\"1001\" alt=\"image\"\nsrc=\"https://github.com/user-attachments/assets/40b06eec-e38f-43ad-b437-89eca502aa66\">\n\n\n[Source](https://paritytech.github.io/polkadot-sdk/bench/statement-distribution-regression-bench)",
          "timestamp": "2024-07-16T11:31:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/79a3d6c294430a52f00563f8a5e59984680b889f"
        },
        "date": 1721136341225,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94599999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03638986316399999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045953486223999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Samusev",
            "username": "alvicsam",
            "email": "41779041+alvicsam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "66baa2fb307fe72cb9ddc7c3be16ba57fcb2670a",
          "message": "[ci] Update forklift in CI image (#5032)\n\ncc https://github.com/paritytech/ci_cd/issues/939",
          "timestamp": "2024-07-16T14:48:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/66baa2fb307fe72cb9ddc7c3be16ba57fcb2670a"
        },
        "date": 1721146876162,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94599999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03638986316399999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045953486223999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "975e04bbb59b643362f918e8521f0cde5c27fbc8",
          "message": "Send PeerViewChange with high priority (#4755)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/577\n\n### Changed\n- `orchestra` updated to 0.4.0\n- `PeerViewChange` sent with high priority and should be processed first\nin a queue.\n- To count them in tests added tracker to TestSender and TestOverseer.\nIt acts more like a smoke test though.\n\n### Testing on Versi\n\nThe changes were tested on Versi with two objectives:\n1. Make sure the node functionality does not change.\n2. See how the changes affect performance.\n\nTest setup:\n- 2.5 hours for each case\n- 100 validators\n- 50 parachains\n- validatorsPerCore = 2\n- neededApprovals = 100\n- nDelayTranches = 89\n- relayVrfModuloSamples = 50\n\nDuring the test period, all nodes ran without any crashes, which\nsatisfies the first objective.\n\nTo estimate the change in performance we used ToF charts. The graphs\nshow that there are no spikes in the top as before. This proves that our\nhypothesis is correct.\n\n### Normalized charts with ToF\n\n![image](https://github.com/user-attachments/assets/0d49d0db-8302-4a8c-a557-501856805ff5)\n[Before](https://grafana.teleport.parity.io/goto/ZoR53ClSg?orgId=1)\n\n\n![image](https://github.com/user-attachments/assets/9cc73784-7e45-49d9-8212-152373c05880)\n[After](https://grafana.teleport.parity.io/goto/6ux5qC_IR?orgId=1)\n\n### Conclusion\n\nThe prioritization of subsystem messages reduces the ToF of the\nnetworking subsystem, which helps faster propagation of gossip messages.",
          "timestamp": "2024-07-16T17:56:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/975e04bbb59b643362f918e8521f0cde5c27fbc8"
        },
        "date": 1721161107243,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.41,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.91799999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046189688596000034,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03906811578199999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alin Dima",
            "username": "alindima",
            "email": "alin@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0db509263c18ff011dcb64af0b0e87f6f68a7c16",
          "message": "add elastic scaling MVP guide (#4663)\n\nResolves https://github.com/paritytech/polkadot-sdk/issues/4468\n\nGives instructions on how to enable elastic scaling MVP to parachain\nteams.\n\nStill a draft because it depends on further changes we make to the\nslot-based collator:\nhttps://github.com/paritytech/polkadot-sdk/pull/4097\n\nParachains cannot use this yet because the collator was not released and\nno relay chain network has been configured for elastic scaling yet",
          "timestamp": "2024-07-17T09:27:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0db509263c18ff011dcb64af0b0e87f6f68a7c16"
        },
        "date": 1721214930222,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.90399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038741478274000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04642110549199999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "739951991f14279a7dc05d42c29ccf57d3740a4c",
          "message": "Adjust release flows to use those with the new branch model (#5015)\n\nThis PR contains adjustments of the node release pipelines so that it\nwill be possible to use those to trigger release actions based on the\n`stable` branch.\n\nPreviously the whole pipeline of the flows from [creation of the\n`rc-tag`](https://github.com/paritytech/polkadot-sdk/blob/master/.github/workflows/release-10_rc-automation.yml)\n(v1.15.0-rc1, v1.15.0-rc2, etc) till [the release draft\ncreation](https://github.com/paritytech/polkadot-sdk/blob/master/.github/workflows/release-30_publish_release_draft.yml)\nwas triggered on push to the node release branch. As we had the node\nrelease branch and the crates release branch separately, it worked fine.\n\nFrom now on, as we are switching to the one branch approach, for the\nfirst iteration I would like to keep things simple to see how the new\nrelease process will work with both parts (crates and node) made from\none branch.\n\nChanges made: \n\n- The first step in the pipeline (rc-tag creation) will be triggered\nmanually instead of the push to the branch\n- The tag version will be set manually from the input instead of to be\ntaken from the branch name\n- Docker image will be additionally tagged as `stable`\n\n\n\nCloses: https://github.com/paritytech/release-engineering/issues/214",
          "timestamp": "2024-07-17T11:28:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/739951991f14279a7dc05d42c29ccf57d3740a4c"
        },
        "date": 1721224256393,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.926,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.048129754102,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.039631085871999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "1b6292bf7c71d56b793e98b651799f41bb0ef76b",
          "message": "Do not crash on block gap in `displaced_leaves_after_finalizing` (#4997)\n\nAfter the merge of #4922 we saw failing zombienet tests with the\nfollowing error:\n```\n2024-07-09 10:30:09 Error applying finality to block (0xb9e1d3d9cb2047fe61667e28a0963e0634a7b29781895bc9ca40c898027b4c09, 56685): UnknownBlock: Header was not found in the database: 0x0000000000000000000000000000000000000000000000000000000000000000    \n2024-07-09 10:30:09 GRANDPA voter error: could not complete a round on disk: UnknownBlock: Header was not found in the database: 0x0000000000000000000000000000000000000000000000000000000000000000    \n```\n\n[Example](https://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6662262)\n\nThe crashing situation is warp-sync related. After warp syncing, it can\nhappen that there are gaps in block ancestry where we don't have the\nheader. At the same time, the genesis hash is in the set of leaves. In\n`displaced_leaves_after_finalizing` we then iterate from the finalized\nblock backwards until we hit an unknown block, crashing the node.\n\nThis PR makes the detection of displaced branches resilient against\nunknown block in the finalized block chain.\n\ncc @nazar-pc (github won't let me request a review from you)\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-17T15:41:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1b6292bf7c71d56b793e98b651799f41bb0ef76b"
        },
        "date": 1721237537765,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999992,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04699550837200002,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03792932628800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b862b181ec507e1510dff6d78335b184b395d9b2",
          "message": "fix: Update libp2p-websocket to v0.42.2 to fix panics (#5040)\n\nThis release includes: https://github.com/libp2p/rust-libp2p/pull/5482\n\nWhich fixes substrate node crashing with libp2p trace:\n\n```\n 0: sp_panic_handler::set::{{closure}}\n   1: std::panicking::rust_panic_with_hook\n   2: std::panicking::begin_panic::{{closure}}\n   3: std::sys_common::backtrace::__rust_end_short_backtrace\n   4: std::panicking::begin_panic\n   5: <quicksink::SinkImpl<S,F,T,A,E> as futures_sink::Sink<A>>::poll_ready\n   6: <rw_stream_sink::RwStreamSink<S> as futures_io::if_std::AsyncWrite>::poll_write\n   7: <libp2p_noise::io::framed::NoiseFramed<T,S> as futures_sink::Sink<&alloc::vec::Vec<u8>>>::poll_ready\n   8: <libp2p_noise::io::Output<T> as futures_io::if_std::AsyncWrite>::poll_write\n   9: <yamux::frame::io::Io<T> as futures_sink::Sink<yamux::frame::Frame<()>>>::poll_ready\n  10: yamux::connection::Connection<T>::poll_next_inbound\n  11: <libp2p_yamux::Muxer<C> as libp2p_core::muxing::StreamMuxer>::poll\n  12: <libp2p_core::muxing::boxed::Wrap<T> as libp2p_core::muxing::StreamMuxer>::poll\n  13: <libp2p_core::muxing::boxed::Wrap<T> as libp2p_core::muxing::StreamMuxer>::poll\n  14: libp2p_swarm::connection::pool::task::new_for_established_connection::{{closure}}\n  15: <sc_service::task_manager::prometheus_future::PrometheusFuture<T> as core::future::future::Future>::poll\n  16: <futures_util::future::select::Select<A,B> as core::future::future::Future>::poll\n  17: <tracing_futures::Instrumented<T> as core::future::future::Future>::poll\n  18: std::panicking::try\n  19: tokio::runtime::task::harness::Harness<T,S>::poll\n  20: tokio::runtime::scheduler::multi_thread::worker::Context::run_task\n  21: tokio::runtime::scheduler::multi_thread::worker::Context::run\n  22: tokio::runtime::context::set_scheduler\n  23: tokio::runtime::context::runtime::enter_runtime\n  24: tokio::runtime::scheduler::multi_thread::worker::run\n  25: tokio::runtime::task::core::Core<T,S>::poll\n  26: tokio::runtime::task::harness::Harness<T,S>::poll\n  27: std::sys_common::backtrace::__rust_begin_short_backtrace\n  28: core::ops::function::FnOnce::call_once{{vtable.shim}}\n  29: std::sys::pal::unix::thread::Thread::new::thread_start\n  30: <unknown>\n  31: <unknown>\n\n\nThread 'tokio-runtime-worker' panicked at 'SinkImpl::poll_ready called after error.', /home/ubuntu/.cargo/registry/src/index.crates.io-6f17d22bba15001f/quicksink-0.1.2/src/lib.rs:158\n```\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4934\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-07-17T17:04:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b862b181ec507e1510dff6d78335b184b395d9b2"
        },
        "date": 1721243863018,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92399999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03823014000600001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.047894428045999984,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tom",
            "username": "senseless",
            "email": "tsenseless@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4bcdf8166972b63a70b618c90424b9f7d64b719b",
          "message": "Update the stake.plus bootnode addresses (#5039)\n\nUpdate the stake.plus bootnode addresses\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-17T21:36:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4bcdf8166972b63a70b618c90424b9f7d64b719b"
        },
        "date": 1721258984512,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.95199999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036294133834,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.044978426704000006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "25d9b59e6d0bf74637796a99d1adf84725468e0a",
          "message": "upgrade `wasm-bindgen` to 0.2.92 (#5056)\n\nThe rustc warns\n\n```\nThe package `wasm-bindgen v0.2.87` currently triggers the following future incompatibility lints:\n> warning: older versions of the `wasm-bindgen` crate will be incompatible with future versions of Rust; please update to `wasm-bindgen` v0.2.88\n>   |\n>   = warning: this was previously accepted by the compiler but is being phased out; it will become a hard error in a future release!\n>   = note: for more information, see issue #71871 <https://github.com/rust-lang/rust/issues/71871>\n>\n```",
          "timestamp": "2024-07-18T08:01:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/25d9b59e6d0bf74637796a99d1adf84725468e0a"
        },
        "date": 1721297269056,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94399999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037654848452,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04743398661399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d05cd9a55de64318f0ea16a47b21a0af4204c522",
          "message": "Bump enumn from 0.1.12 to 0.1.13 (#5061)\n\nBumps [enumn](https://github.com/dtolnay/enumn) from 0.1.12 to 0.1.13.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/dtolnay/enumn/releases\">enumn's\nreleases</a>.</em></p>\n<blockquote>\n<h2>0.1.13</h2>\n<ul>\n<li>Update proc-macro2 to fix caching issue when using a rustc-wrapper\nsuch as sccache</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/dtolnay/enumn/commit/de964e3ce0463f01dabb94c168d507254facfb86\"><code>de964e3</code></a>\nRelease 0.1.13</li>\n<li><a\nhref=\"https://github.com/dtolnay/enumn/commit/52dcafcb2ee193be8839dc0f96bfa0e151888645\"><code>52dcafc</code></a>\nPull in proc-macro2 sccache fix</li>\n<li><a\nhref=\"https://github.com/dtolnay/enumn/commit/ba2e288a83c5e62d1e29b993523ccf0528043ab0\"><code>ba2e288</code></a>\nTest docs.rs documentation build in CI</li>\n<li><a\nhref=\"https://github.com/dtolnay/enumn/commit/6f5a37e5a9dcdb75987552b44d7ebdbd7f0a2a93\"><code>6f5a37e</code></a>\nUpdate actions/checkout@v3 -&gt; v4</li>\n<li>See full diff in <a\nhref=\"https://github.com/dtolnay/enumn/compare/0.1.12...0.1.13\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=enumn&package-manager=cargo&previous-version=0.1.12&new-version=0.1.13)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-18T14:06:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d05cd9a55de64318f0ea16a47b21a0af4204c522"
        },
        "date": 1721317941965,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.948,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04493518522399998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03640216536600001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Juan Ignacio Rios",
            "username": "JuaniRios",
            "email": "54085674+JuaniRios@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ad9804f7b1e707d5144bcdc14b67c19db2975da8",
          "message": "Add `pub` to xcm::v4::PalletInfo (#4976)\n\nv3 PalletInfo had the fields public, but not v4. Any reason why?\nI need the PalletInfo fields public so I can read the values and do some\nlogic based on that at Polimec\n@franciscoaguirre \n\nIf this could be backported would be highly appreciated 🙏🏻\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-18T19:35:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ad9804f7b1e707d5144bcdc14b67c19db2975da8"
        },
        "date": 1721337731104,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.91399999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04681781941000002,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03862951663399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Özgün Özerk",
            "username": "ozgunozerk",
            "email": "ozgunozerk.elo@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f8f70b37562e3519401f8c1fada9a2c55589e0c6",
          "message": "relax `XcmFeeToAccount` trait bound on `AccountId` (#4959)\n\nFixes #4960 \n\nConfiguring `FeeManager` enforces the boundary `Into<[u8; 32]>` for the\n`AccountId` type.\n\nHere is how it works currently: \n\nConfiguration:\n```rust\n    type FeeManager = XcmFeeManagerFromComponents<\n        IsChildSystemParachain<primitives::Id>,\n        XcmFeeToAccount<Self::AssetTransactor, AccountId, TreasuryAccount>,\n    >;\n```\n\n`XcmToFeeAccount` struct:\n```rust\n/// A `HandleFee` implementation that simply deposits the fees into a specific on-chain\n/// `ReceiverAccount`.\n///\n/// It reuses the `AssetTransactor` configured on the XCM executor to deposit fee assets. If\n/// the `AssetTransactor` returns an error while calling `deposit_asset`, then a warning will be\n/// logged and the fee burned.\npub struct XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>(\n\tPhantomData<(AssetTransactor, AccountId, ReceiverAccount)>,\n);\n\nimpl<\n\t\tAssetTransactor: TransactAsset,\n\t\tAccountId: Clone + Into<[u8; 32]>,\n\t\tReceiverAccount: Get<AccountId>,\n\t> HandleFee for XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>\n{\n\tfn handle_fee(fee: Assets, context: Option<&XcmContext>, _reason: FeeReason) -> Assets {\n\t\tdeposit_or_burn_fee::<AssetTransactor, _>(fee, context, ReceiverAccount::get());\n\n\t\tAssets::new()\n\t}\n}\n```\n\n`deposit_or_burn_fee()` function:\n```rust\n/// Try to deposit the given fee in the specified account.\n/// Burns the fee in case of a failure.\npub fn deposit_or_burn_fee<AssetTransactor: TransactAsset, AccountId: Clone + Into<[u8; 32]>>(\n\tfee: Assets,\n\tcontext: Option<&XcmContext>,\n\treceiver: AccountId,\n) {\n\tlet dest = AccountId32 { network: None, id: receiver.into() }.into();\n\tfor asset in fee.into_inner() {\n\t\tif let Err(e) = AssetTransactor::deposit_asset(&asset, &dest, context) {\n\t\t\tlog::trace!(\n\t\t\t\ttarget: \"xcm::fees\",\n\t\t\t\t\"`AssetTransactor::deposit_asset` returned error: {:?}. Burning fee: {:?}. \\\n\t\t\t\tThey might be burned.\",\n\t\t\t\te, asset,\n\t\t\t);\n\t\t}\n\t}\n}\n```\n\n---\n\nIn order to use **another** `AccountId` type (for example, 20 byte\naddresses for compatibility with Ethereum or Bitcoin), one has to\nduplicate the code as the following (roughly changing every `32` to\n`20`):\n```rust\n/// A `HandleFee` implementation that simply deposits the fees into a specific on-chain\n/// `ReceiverAccount`.\n///\n/// It reuses the `AssetTransactor` configured on the XCM executor to deposit fee assets. If\n/// the `AssetTransactor` returns an error while calling `deposit_asset`, then a warning will be\n/// logged and the fee burned.\npub struct XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>(\n    PhantomData<(AssetTransactor, AccountId, ReceiverAccount)>,\n);\nimpl<\n        AssetTransactor: TransactAsset,\n        AccountId: Clone + Into<[u8; 20]>,\n        ReceiverAccount: Get<AccountId>,\n    > HandleFee for XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>\n{\n    fn handle_fee(fee: XcmAssets, context: Option<&XcmContext>, _reason: FeeReason) -> XcmAssets {\n        deposit_or_burn_fee::<AssetTransactor, _>(fee, context, ReceiverAccount::get());\n\n        XcmAssets::new()\n    }\n}\n\npub fn deposit_or_burn_fee<AssetTransactor: TransactAsset, AccountId: Clone + Into<[u8; 20]>>(\n    fee: XcmAssets,\n    context: Option<&XcmContext>,\n    receiver: AccountId,\n) {\n    let dest = AccountKey20 { network: None, key: receiver.into() }.into();\n    for asset in fee.into_inner() {\n        if let Err(e) = AssetTransactor::deposit_asset(&asset, &dest, context) {\n            log::trace!(\n                target: \"xcm::fees\",\n                \"`AssetTransactor::deposit_asset` returned error: {:?}. Burning fee: {:?}. \\\n                They might be burned.\",\n                e, asset,\n            );\n        }\n    }\n}\n```\n\n---\n\nThis results in code duplication, which can be avoided simply by\nrelaxing the trait enforced by `XcmFeeToAccount`.\n\nIn this PR, I propose to introduce a new trait called `IntoLocation` to\nbe able to express both `Into<[u8; 32]>` and `Into<[u8; 20]>` should be\naccepted (and every other `AccountId` type as long as they implement\nthis trait).\n\nCurrently, `deposit_or_burn_fee()` function converts the `receiver:\nAccountId` to a location. I think converting an account to `Location`\nshould not be the responsibility of `deposit_or_burn_fee()` function.\n\nThis trait also decouples the conversion of `AccountId` to `Location`,\nfrom `deposit_or_burn_fee()` function. And exposes `IntoLocation` trait.\nThus, allowing everyone to come up with their `AccountId` type and make\nit compatible for configuring `FeeManager`.\n\n---\n\nNote 1: if there is a better file/location to put `IntoLocation`, I'm\nall ears\n\nNote 2: making `deposit_or_burn_fee` or `XcmToFeeAccount` generic was\nnot possible from what I understood, due to Rust currently do not\nsupport a way to express the generic should implement either `trait A`\nor `trait B` (since the compiler cannot guarantee they won't overlap).\nIn this case, they are `Into<[u8; 32]>` and `Into<[u8; 20]>`.\nSee [this](https://github.com/rust-lang/rust/issues/20400) and\n[this](https://github.com/rust-lang/rfcs/pull/1672#issuecomment-262152934).\n\nNote 3: I should also submit a PR to `frontier` that implements\n`IntoLocation` for `AccountId20` if this PR gets accepted.\n\n\n### Summary \nthis new trait:\n- decouples the conversion of `AccountId` to `Location`, from\n`deposit_or_burn_fee()` function\n- makes `XcmFeeToAccount` accept every possible `AccountId` type as long\nas they they implement `IntoLocation`\n- backwards compatible\n- keeps the API simple and clean while making it less restrictive\n\n\n@franciscoaguirre and @gupnik are already aware of the issue, so tagging\nthem here for visibility.\n\n---------\n\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-19T11:09:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f8f70b37562e3519401f8c1fada9a2c55589e0c6"
        },
        "date": 1721393778321,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94799999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045345586748000016,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03660470014199999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "394ea70d2ad8d37b1a41854659d989e750758705",
          "message": "[NPoS] Some simple refactors to Delegate Staking (#4981)\n\n## Changes\n- `fn update_payee` is renamed to `fn set_payee` in the trait\n`StakingInterface` since there is also a call `Staking::update_payee`\nwhich does something different, ie used for migrating deprecated\n`Controller` accounts.\n- `set_payee` does not re-dispatch, only mutates ledger.\n- Fix rustdocs for `NominationPools::join`.\n- Add an implementation note about why we cannot allow existing stakers\nto join/bond_extra into the pool.",
          "timestamp": "2024-07-19T16:32:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/394ea70d2ad8d37b1a41854659d989e750758705"
        },
        "date": 1721414068038,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.91399999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03875564540400001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04869709657599999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d649746e840ead01898957329b5f63ddad6e032c",
          "message": "Implements `PoV` export and local validation (#4640)\n\nThis pull requests adds a new CLI flag to `polkadot-parachains`\n`--export-pov-to-path`. This CLI flag will instruct the node to export\nany `PoV` that it build locally to export to the given folder. Then\nthese `PoV` files can be validated using the introduced\n`cumulus-pov-validator`. The combination of export and validation can be\nused for debugging parachain validation issues that may happen on the\nrelay chain.",
          "timestamp": "2024-07-19T21:23:06Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d649746e840ead01898957329b5f63ddad6e032c"
        },
        "date": 1721431432194,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94199999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03815976861000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04753930286199998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Parth Mittal",
            "username": "mittal-parth",
            "email": "76661350+mittal-parth@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "59e3315f7fd74b95e08e6409d1b3f53f6c2123f4",
          "message": "Balances Pallet: Emit events when TI is updated in currency impl (#4936)\n\n# Description\n\nPreviously, in the `Currency` impl, the implementation of\n`pallet_balances` was not emitting any instances of `Issued` and\n`Rescinded` events, even though the `Fungible` equivalent was.\n\nThis PR adds the `Issued` and `Rescinded` events in appropriate places\nin `impl_currency` along with tests.\n\nCloses #4028 \n\npolkadot address: 5GsLutpKjbzsbTphebs9Uy4YK6gTN47MAaz6njPktidjR5cp\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2024-07-19T23:13:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/59e3315f7fd74b95e08e6409d1b3f53f6c2123f4"
        },
        "date": 1721437401008,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.96199999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037321793928,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045687180550000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9dcf6f12e87c479f7d32ff6ac7869b375ca06de3",
          "message": "beefy: Increment metric and add extra log details (#5075)\n\nThis PR increments the beefy metric wrt no peers to query justification\nfrom.\nThe metric is incremented when we submit a request to a known peer,\nhowever that peer failed to provide a valid response, and there are no\nfurther peers to query.\n\nWhile at it, add a few extra details to identify the number of active\npeers and cached peers, together with the request error\n\nPart of:\n- https://github.com/paritytech/polkadot-sdk/issues/4985\n- https://github.com/paritytech/polkadot-sdk/issues/4925\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-07-20T14:37:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9dcf6f12e87c479f7d32ff6ac7869b375ca06de3"
        },
        "date": 1721492778279,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92599999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036459110742000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04493508402799999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d1979d4be9c44d06c62aeea44d30f5f201d67b5e",
          "message": "Bump assert_cmd from 2.0.12 to 2.0.14 (#5070)\n\nBumps [assert_cmd](https://github.com/assert-rs/assert_cmd) from 2.0.12\nto 2.0.14.\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/assert-rs/assert_cmd/blob/master/CHANGELOG.md\">assert_cmd's\nchangelog</a>.</em></p>\n<blockquote>\n<h2>[2.0.14] - 2024-02-19</h2>\n<h3>Compatibility</h3>\n<ul>\n<li>MSRV is now 1.73.0</li>\n</ul>\n<h3>Features</h3>\n<ul>\n<li>Run using the cargo target runner</li>\n</ul>\n<h2>[2.0.13] - 2024-01-12</h2>\n<h3>Internal</h3>\n<ul>\n<li>Dependency update</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/9ebfc0b140847e926374a8cd0ad235ba93f119f9\"><code>9ebfc0b</code></a>\nchore: Release assert_cmd version 2.0.14</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/025c5f6dcb1ca3b0dfc24983e48aab123d093895\"><code>025c5f6</code></a>\ndocs: Update changelog</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/82b99c139882bdbc3d613094fb0eea944b05419c\"><code>82b99c1</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/assert-rs/assert_cmd/issues/193\">#193</a>\nfrom glehmann/cross</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/b3a290ce81873bb266457c6ea7f6d94124ba8ed5\"><code>b3a290c</code></a>\nfeat: add cargo runner support in order to work with cross</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/132db496f6e89454e33b13269b4bb9d42324ce7d\"><code>132db49</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/assert-rs/assert_cmd/issues/194\">#194</a>\nfrom assert-rs/renovate/rust-1.x</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/f1308abaf458e22511548bc7f3ddecc2bde579ed\"><code>f1308ab</code></a>\nchore(deps): update msrv to v1.73</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/9b0f20acd4868a00544b5e28a0fcbcad6689afdf\"><code>9b0f20a</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/assert-rs/assert_cmd/issues/192\">#192</a>\nfrom assert-rs/renovate/rust-1.x</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/07f4cdee717ea3b6f96ac7eb1eaeb4ed2253d6af\"><code>07f4cde</code></a>\nchore(deps): update msrv to v1.72</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/19da72b81c789f9c06817b99c1ecebfe7083dbfb\"><code>19da72b</code></a>\nchore: Release assert_cmd version 2.0.13</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/db5ee325aafffb5b8e042cda1cc946e36079302a\"><code>db5ee32</code></a>\ndocs: Update changelog</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/assert-rs/assert_cmd/compare/v2.0.12...v2.0.14\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=assert_cmd&package-manager=cargo&previous-version=2.0.12&new-version=2.0.14)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-20T23:26:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d1979d4be9c44d06c62aeea44d30f5f201d67b5e"
        },
        "date": 1721525896989,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04601067975399999,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037230720384,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "mail.guptanikhil@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d0d8e29197a783f3ea300569afc50244a280cafa",
          "message": "Fixes doc links for `procedural` crate (#5023)\n\nThis PR fixes the documentation for FRAME Macros when pointed from\n`polkadot_sdk_docs` crate. This is achieved by referring to the examples\nin the `procedural` crate, embedded via `docify`.\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-07-22T10:13:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d0d8e29197a783f3ea300569afc50244a280cafa"
        },
        "date": 1721649867026,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91599999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40799999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04859253769800001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.038442295767999984,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "612c1bd3d51c7638fd078b632c57d6bb177e04c6",
          "message": "Prepare PVFs if node is a validator in the next session (#4791)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/4324\n- On every active leaf candidate-validation subsystem checks if the node\nis the next session authority.\n- If it is, it fetches backed candidates and prepares unknown PVFs.\n- We limit number of PVFs per block to not overload subsystem.",
          "timestamp": "2024-07-22T16:35:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/612c1bd3d51c7638fd078b632c57d6bb177e04c6"
        },
        "date": 1721667931395,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.90799999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038994957084,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04966569385199998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "612c1bd3d51c7638fd078b632c57d6bb177e04c6",
          "message": "Prepare PVFs if node is a validator in the next session (#4791)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/4324\n- On every active leaf candidate-validation subsystem checks if the node\nis the next session authority.\n- If it is, it fetches backed candidates and prepares unknown PVFs.\n- We limit number of PVFs per block to not overload subsystem.",
          "timestamp": "2024-07-22T16:35:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/612c1bd3d51c7638fd078b632c57d6bb177e04c6"
        },
        "date": 1721672949765,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94199999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.05021944215800001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.040126686292,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ac98bc3fa158aa01faa3c0f95d10c8932affbcc2",
          "message": "Bump slotmap from 1.0.6 to 1.0.7 (#5096)\n\nBumps [slotmap](https://github.com/orlp/slotmap) from 1.0.6 to 1.0.7.\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/orlp/slotmap/blob/master/RELEASES.md\">slotmap's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>Version 1.0.7</h1>\n<ul>\n<li>Added <code>clone_from</code> implementations for all slot\nmaps.</li>\n<li>Added <code>try_insert_with_key</code> methods that accept a\nfallible closure.</li>\n<li>Improved performance of insertion and key hashing.</li>\n<li>Made <code>new_key_type</code> resistant to shadowing.</li>\n<li>Made iterators clonable regardless of item type clonability.</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/c905b6ced490551476cb7c37778eb8128bdea7ba\"><code>c905b6c</code></a>\nRelease 1.0.7.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/cdee6974d5d57fef62f862ffad9ffe668652d26f\"><code>cdee697</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/orlp/slotmap/issues/107\">#107</a> from\nwaywardmonkeys/minor-doc-tweaks</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/4456784a9bce360ec38005c9f4cbf4b4a6f92162\"><code>4456784</code></a>\nFix a typo and add some backticks.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/a7287a2caa80bb4a6d7a380a92a49d48535357a5\"><code>a7287a2</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/orlp/slotmap/issues/99\">#99</a> from\nchloekek/hash-u64</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/eeaf92e5b3627ac5a2035742c6e2818999ac3d0c\"><code>eeaf92e</code></a>\nProvide explicit impl Hash for KeyData</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/ce6e1e02bb2c2074d8d581e87ad9c2f72ce495c3\"><code>ce6e1e0</code></a>\nLint invalid_html_tags has been renamed, but not really necessary.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/941c39301211a03385e8d7915d0d31b6f6f5ecd5\"><code>941c393</code></a>\nFixed remaining references to global namespace in new_key_type\nmacro.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/cf7e44c05d777440687cfa0d439a31fdec50cc3a\"><code>cf7e44c</code></a>\nAdded utility module.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/5575afe1a31c634d5ab15d273ad8793f6711f8b1\"><code>5575afe</code></a>\nEnsure insert always has fast path.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/7220adc6fa9defb356699f3d96af736e9ef477b5\"><code>7220adc</code></a>\nCargo fmt and added test case for cloneable iterators.</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/orlp/slotmap/compare/v1.0.6...v1.0.7\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=slotmap&package-manager=cargo&previous-version=1.0.6&new-version=1.0.7)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-22T22:25:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ac98bc3fa158aa01faa3c0f95d10c8932affbcc2"
        },
        "date": 1721693632469,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92999999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.044988294938000016,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03622579330799999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "bbb8668672896b1952496b2dfe641a91defb2454",
          "message": "Replace all ansi_term crates (#2923)\n\nNow Polkadot-SDK is ansi_term free\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-23T15:54:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bbb8668672896b1952496b2dfe641a91defb2454"
        },
        "date": 1721752654261,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92599999999993,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04806456881199999,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03886797865600001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "polka.dom",
            "username": "PolkadotDom",
            "email": "polkadotdom@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9974a68ca16ddd4d7ee822edb9baf48e6b93906e",
          "message": "Remove pallet::getter macro from pallet-identity (#4586)\n\nAs per #3326, removes pallet::getter macro usage from the\npallet-identity. The syntax `StorageItem::<T, I>::get()` should be used\ninstead.\n\nAlso makes all storage values public\n\ncc @muraca\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-23T17:42:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9974a68ca16ddd4d7ee822edb9baf48e6b93906e"
        },
        "date": 1721759604252,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93799999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999999,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036226317056,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04507921291000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6b50637082bad5f8183876418059eb1327bbb2fd",
          "message": "hotfix: blockchain/backend: Skip genesis leaf to unblock syncing (#5103)\n\nThis PR effectively skips over cases where the blockchain reports the\ngenesis block as leaf.\n\nThe issue manifests as the blockchain getting stuck and not importing\nblocks after a while.\nAlthough the root-cause of why the blockchain reports the genesis as\nleaf is not scoped, this hot-fix is unblocking the new release.\n\nWhile at it, added some extra debug logs to identify issues more easily\nin the future.\n\n### Issue\n\n```\n2024-07-22 10:06:08.708 DEBUG tokio-runtime-worker db::blockchain: Checking for displaced leaves after finalization. leaves=[0xd62aea69664b74c55b7e79ab5855b117d213156a5e9ab05ad0737772aaf42c14, 0xb0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe] finalized_block_hash=0x8f8e…7f34 finalized_block_number=24148459\n2024-07-22 10:06:08.708 DEBUG tokio-runtime-worker db::blockchain: Handle displaced leaf 0xd62aea69664b74c55b7e79ab5855b117d213156a5e9ab05ad0737772aaf42c14 (elapsed 25.74µs) leaf_number=24148577\n2024-07-22 10:06:08.709 DEBUG tokio-runtime-worker db::blockchain: Leaf points to the finalized header 0xd62aea69664b74c55b7e79ab5855b117d213156a5e9ab05ad0737772aaf42c14, skipping for now (elapsed 70.72µs)\n\n\n// This is Kusama genesis\n2024-07-22 10:06:08.709 DEBUG tokio-runtime-worker db::blockchain: Handle displaced leaf 0xb0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe (elapsed 127.271µs) leaf_number=0\n2024-07-22 10:06:08.709 DEBUG tokio-runtime-worker db::blockchain: Skip more blocks until we get all blocks on finalized chain until the height of the parent block current_hash=0xb0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe current_num=0 finalized_num=24148458\n```\n\n### Before\n\n```\n2024-07-20 00:45:00.234  INFO tokio-runtime-worker substrate: ⚙️  Preparing  0.0 bps, target=#24116589 (50 peers), best: #24116498 (0xb846…8720), finalized #24116493 (0x50b6…2445), ⬇ 2.3MiB/s ⬆ 2.6kiB/s    \n   \n...\n\n2024-07-20 14:05:18.572  INFO tokio-runtime-worker substrate: ⚙️  Syncing  0.0 bps, target=#24124495 (51 peers), best: #24119976 (0x6970…aeb3), finalized #24119808 (0xd900…abe4), ⬇ 2.2MiB/s ⬆ 3.1kiB/s    \n2024-07-20 14:05:23.573  INFO tokio-runtime-worker substrate: ⚙️  Syncing  0.0 bps, target=#24124495 (51 peers), best: #24119976 (0x6970…aeb3), finalized #24119808 (0xd900…abe4), ⬇ 2.2MiB/s ⬆ 5.8kiB/s    \n```\n\n### After\n\n```\n2024-07-22 10:41:10.897 DEBUG tokio-runtime-worker db::blockchain: Handle displaced leaf 0x4e8cf3ff18e7d13ff7fec28f9fc8ce6eff5492ed8dc046e961b76dec5c0cfddf (elapsed 39.26µs) leaf_number=24150969\n2024-07-22 10:41:10.897 DEBUG tokio-runtime-worker db::blockchain: Leaf points to the finalized header 0x4e8cf3ff18e7d13ff7fec28f9fc8ce6eff5492ed8dc046e961b76dec5c0cfddf, skipping for now (elapsed 49.69µs)\n2024-07-22 10:41:10.897 DEBUG tokio-runtime-worker db::blockchain: Skip genesis block 0xb0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe reporterd as leaf (elapsed 54.57µs)\n2024-07-22 10:41:10.897 DEBUG tokio-runtime-worker db::blockchain: Finished with result DisplacedLeavesAfterFinalization { displaced_leaves: [], displaced_blocks: [] } (elapsed 58.78µs) finalized_block_hash=0x02b3…5338 finalized_block_number=24150967\n2024-07-22 10:41:12.357  INFO tokio-runtime-worker substrate: 🏆 Imported #24150970 (0x4e8c…fddf → 0x3637…56bb)\n2024-07-22 10:41:12.862  INFO tokio-runtime-worker substrate: 💤 Idle (50 peers), best: #24150970 (0x3637…56bb), finalized #24150967 (0x02b3…5338), ⬇ 2.0MiB/s ⬆ 804.7kiB/s\n2024-07-22 10:41:14.772 DEBUG tokio-runtime-worker db::blockchain: Checking for displaced leaves after finalization. leaves=[0x363763b16c23fc20a84f38f67014fa7ae6ba9c708fc074890016699e5ca756bb, 0xb0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe] finalized_block_hash=0xa1534a105b90e7036a18ac1c646cd2bd6c41c66cc055817f4f51209ab9070e5c finalized_block_number=24150968\n2024-07-22 10:41:14.772 DEBUG tokio-runtime-worker db::blockchain: Handle displaced leaf 0x363763b16c23fc20a84f38f67014fa7ae6ba9c708fc074890016699e5ca756bb (elapsed 62.48µs) leaf_number=24150970\n2024-07-22 10:41:14.772 DEBUG tokio-runtime-worker db::blockchain: Leaf points to the finalized header 0x363763b16c23fc20a84f38f67014fa7ae6ba9c708fc074890016699e5ca756bb, skipping for now (elapsed 71.76µs)\n2024-07-22 10:41:14.772 DEBUG tokio-runtime-worker db::blockchain: Skip genesis block 0xb0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe reporterd as leaf (elapsed 75.96µs)\n2024-07-22 10:41:14.772 DEBUG tokio-runtime-worker db::blockchain: Finished with result DisplacedLeavesAfterFinalization { displaced_leaves: [], displaced_blocks: [] } (elapsed 80.27µs) finalized_block_hash=0xa153…0e5c finalized_block_number=24150968\n2024-07-22 10:41:14.795 DEBUG tokio-runtime-worker db::blockchain: Checking for displaced leaves after finalization. leaves=[0x363763b16c23fc20a84f38f67014fa7ae6ba9c708fc074890016699e5ca756bb, 0xb0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe] finalized_block_hash=0xa1534a105b90e7036a18ac1c646cd2bd6c41c66cc055817f4f51209ab9070e5c finalized_block_number=24150968\n2024-07-22 10:41:14.795 DEBUG tokio-runtime-worker db::blockchain: Handle displaced leaf 0x363763b16c23fc20a84f38f67014fa7ae6ba9c708fc074890016699e5ca756bb (elapsed 39.67µs) leaf_number=24150970\n2024-07-22 10:41:14.795 DEBUG tokio-runtime-worker db::blockchain: Leaf points to the finalized header 0x363763b16c23fc20a84f38f67014fa7ae6ba9c708fc074890016699e5ca756bb, skipping for now (elapsed 50.3µs)\n2024-07-22 10:41:14.795 DEBUG tokio-runtime-worker db::blockchain: Skip genesis block 0xb0a8d493285c2df73290dfb7e61f870f17b41801197a149ca93654499ea3dafe reporterd as leaf (elapsed 54.52µs)\n2024-07-22 10:41:14.795 DEBUG tokio-runtime-worker db::blockchain: Finished with result DisplacedLeavesAfterFinalization { displaced_leaves: [], displaced_blocks: [] } (elapsed 58.66µs) finalized_block_hash=0xa153…0e5c finalized_block_number=24150968\n2024-07-22 10:41:17.863  INFO tokio-runtime-worker substrate: 💤 Idle (50 peers), best: #24150970 (0x3637…56bb), finalized #24150968 (0xa153…0e5c), ⬇ 1.2MiB/s ⬆ 815.0kiB/s\n2024-07-22 10:41:18.399  INFO tokio-runtime-worker substrate: 🏆 Imported #24150971 (0x3637…56bb → 0x4ee3…5f7c)\n```\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/5088\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-23T18:55:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6b50637082bad5f8183876418059eb1327bbb2fd"
        },
        "date": 1721768342904,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94799999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045175379390000006,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036331609424000016,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "604f56f03db847a90aa4fdb13be6b80482a4dcd6",
          "message": "Remove not-audited warning (#5114)\n\nPallet tx-pause and safe-mode are both audited, see: #4445\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-23T21:11:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/604f56f03db847a90aa4fdb13be6b80482a4dcd6"
        },
        "date": 1721777849983,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03643041962,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04535562995000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Guillaume Thiolliere",
            "username": "gui1117",
            "email": "gui.thiolliere@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8a96d07e5c0d9479f857b0c3053d3e68497f4430",
          "message": "pallet macro: do not generate try-runtime related code when frame-support doesn't have try-runtime. (#5099)\n\nStatus: Ready for review\n\nFix https://github.com/paritytech/polkadot-sdk/issues/5092\n\nIntroduce a new macro in frame-support which discard content if\n`try-runtime` is not enabled.\n\nUse this macro inside `frame-support-procedural` to generate code only\nwhen `frame-support` is compiled with `try-runtime`.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-24T10:02:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8a96d07e5c0d9479f857b0c3053d3e68497f4430"
        },
        "date": 1721822745554,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94799999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036245586804,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045573296392,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Parth Mittal",
            "username": "mittal-parth",
            "email": "76661350+mittal-parth@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "71109c5fa3f7f4446d859d2b53d1b90d79aa13a1",
          "message": "Remove `pallet-getter` usage from pallet-transaction-payment (#4970)\n\nAs per #3326, removes usage of the `pallet::getter` macro from the\n`transaction-payment` pallet. The syntax `StorageItem::<T, I>::get()`\nshould be used instead.\n\nAlso, adds public functions for compatibility.\n\nNOTE: The `Releases` enum has been made public to transition\n`StorageVersion` from `pub(super) type` to `pub type`.\n\ncc @muraca\n\npolkadot address: 5GsLutpKjbzsbTphebs9Uy4YK6gTN47MAaz6njPktidjR5cp\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-24T12:21:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/71109c5fa3f7f4446d859d2b53d1b90d79aa13a1"
        },
        "date": 1721830961456,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93999999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04872113321,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03911063676400002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a164639f7f1223634fb01cf38dab49622ab940ef",
          "message": "Bump paritytech/review-bot from 2.4.0 to 2.5.0 (#5057)\n\nBumps [paritytech/review-bot](https://github.com/paritytech/review-bot)\nfrom 2.4.0 to 2.5.0.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/paritytech/review-bot/releases\">paritytech/review-bot's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v2.5.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Upgraded dependencies of actions by <a\nhref=\"https://github.com/Bullrich\"><code>@​Bullrich</code></a> in <a\nhref=\"https://redirect.github.com/paritytech/review-bot/pull/120\">paritytech/review-bot#120</a></li>\n<li>Bump ws from 8.16.0 to 8.17.1 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/paritytech/review-bot/pull/124\">paritytech/review-bot#124</a></li>\n<li>Bump braces from 3.0.2 to 3.0.3 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/paritytech/review-bot/pull/125\">paritytech/review-bot#125</a></li>\n<li>Yarn &amp; Node.js upgrade by <a\nhref=\"https://github.com/mutantcornholio\"><code>@​mutantcornholio</code></a>\nin <a\nhref=\"https://redirect.github.com/paritytech/review-bot/pull/126\">paritytech/review-bot#126</a></li>\n<li>v2.5.0 by <a\nhref=\"https://github.com/mutantcornholio\"><code>@​mutantcornholio</code></a>\nin <a\nhref=\"https://redirect.github.com/paritytech/review-bot/pull/127\">paritytech/review-bot#127</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/paritytech/review-bot/compare/v2.4.1...v2.5.0\">https://github.com/paritytech/review-bot/compare/v2.4.1...v2.5.0</a></p>\n<h2>v2.4.1</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Bump undici from 5.26.3 to 5.28.3 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/paritytech/review-bot/pull/114\">paritytech/review-bot#114</a></li>\n<li>Add terminating dots in sentences. by <a\nhref=\"https://github.com/rzadp\"><code>@​rzadp</code></a> in <a\nhref=\"https://redirect.github.com/paritytech/review-bot/pull/116\">paritytech/review-bot#116</a></li>\n<li>Bump undici from 5.28.3 to 5.28.4 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/paritytech/review-bot/pull/117\">paritytech/review-bot#117</a></li>\n<li>Fix IdentityOf tuple introduced in v1.2.0 by <a\nhref=\"https://github.com/Bullrich\"><code>@​Bullrich</code></a> in <a\nhref=\"https://redirect.github.com/paritytech/review-bot/pull/119\">paritytech/review-bot#119</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a href=\"https://github.com/rzadp\"><code>@​rzadp</code></a> made\ntheir first contribution in <a\nhref=\"https://redirect.github.com/paritytech/review-bot/pull/116\">paritytech/review-bot#116</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/paritytech/review-bot/compare/v2.4.0...v2.4.1\">https://github.com/paritytech/review-bot/compare/v2.4.0...v2.4.1</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/paritytech/review-bot/commit/04d633ea6b1edb748974d192e55b168b870150e2\"><code>04d633e</code></a>\nv2.5.0 (<a\nhref=\"https://redirect.github.com/paritytech/review-bot/issues/127\">#127</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/review-bot/commit/6132aa29f3544fc49319d37d612354e8c2e759b3\"><code>6132aa2</code></a>\nYarn &amp; Node.js upgrade (<a\nhref=\"https://redirect.github.com/paritytech/review-bot/issues/126\">#126</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/review-bot/commit/8c7a2842e0074af1af5e64e4033be3d896f2f444\"><code>8c7a284</code></a>\nBump braces from 3.0.2 to 3.0.3 (<a\nhref=\"https://redirect.github.com/paritytech/review-bot/issues/125\">#125</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/review-bot/commit/625303e5cf93a079e0e3c1f73e7b3f6eeab24648\"><code>625303e</code></a>\nBump ws from 8.16.0 to 8.17.1 (<a\nhref=\"https://redirect.github.com/paritytech/review-bot/issues/124\">#124</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/review-bot/commit/8a67d67e39f16cd92bae100a02fe7f0b230a6e31\"><code>8a67d67</code></a>\nUpgraded dependencies of actions (<a\nhref=\"https://redirect.github.com/paritytech/review-bot/issues/120\">#120</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/review-bot/commit/29e944c422279d1d648428375fbcb2d1d48e2c10\"><code>29e944c</code></a>\nFix IdentityOf tuple introduced in v1.2.0 (<a\nhref=\"https://redirect.github.com/paritytech/review-bot/issues/119\">#119</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/review-bot/commit/6134083c1cb95c0d5e617230848051765f8e8c40\"><code>6134083</code></a>\nBump undici from 5.28.3 to 5.28.4 (<a\nhref=\"https://redirect.github.com/paritytech/review-bot/issues/117\">#117</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/review-bot/commit/876731de3f8c4ecebf1969baddd99a9fa84dd6ee\"><code>876731d</code></a>\nAdd terminating dots in sentences. (<a\nhref=\"https://redirect.github.com/paritytech/review-bot/issues/116\">#116</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/review-bot/commit/80d543ce060632fbcd2fa05a8148d23e1ace62b0\"><code>80d543c</code></a>\nBump undici from 5.26.3 to 5.28.3 (<a\nhref=\"https://redirect.github.com/paritytech/review-bot/issues/114\">#114</a>)</li>\n<li>See full diff in <a\nhref=\"https://github.com/paritytech/review-bot/compare/v2.4.0...v2.5.0\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=paritytech/review-bot&package-manager=github_actions&previous-version=2.4.0&new-version=2.5.0)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-24T14:13:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a164639f7f1223634fb01cf38dab49622ab940ef"
        },
        "date": 1721837146992,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03744839492999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046336853689999985,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "60d21e9c4314d5144c9ebdd4fd851ee5f06b0f0d",
          "message": "Introduce a workflow updating the wishlist leaderboards (#5085)\n\n- Closes https://github.com/paritytech/eng-automation/issues/11\n\nThe workflow periodically updates the leaderboards of the wishlist\nissues: https://github.com/paritytech/polkadot-sdk/issues/3900 and\nhttps://github.com/paritytech/polkadot-sdk/issues/3901\n\nThe code is adopted from\n[here](https://github.com/kianenigma/wishlist-tracker), with slight\nmodifications.\n\nPreviously, the score could be increased by the same person adding\ndifferent reactions. Also, some wishes have a score of 0 - even thought\nthere is a wish for them, because the author was not counted.\n\nNow, the score is a unique count of upvoters of the desired issue,\nupvoters of the wish comment, and the author of the wish comment.\n\nI changed the format to include the `Last updated:` at the bottom - it\nwill be automatically updated.",
          "timestamp": "2024-07-24T18:18:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/60d21e9c4314d5144c9ebdd4fd851ee5f06b0f0d"
        },
        "date": 1721851659866,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.90199999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.04067722138399999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.050130263882,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "btwiuse",
            "username": "btwiuse",
            "email": "54848194+btwiuse@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "240b374e052e0d000d24d8e88f9bd9092147646a",
          "message": "Fix misleading comment about RewardHandler in epm config (#3095)\n\nIn pallet_election_provider_multi_phase::Config, the effect of\n\n       type RewardHandler = ()\n\nis to mint rewards from the void, not \"nothing to do upon rewards\"\n\nCo-authored-by: navigaid <navigaid@gmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-24T21:03:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/240b374e052e0d000d24d8e88f9bd9092147646a"
        },
        "date": 1721863909968,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.96199999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038267107864000005,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.048701009150000016,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "de6733baedf5406c37393c20912536600d3ef6fa",
          "message": "Bridges improved tests and nits (#5128)\n\nThis PR adds `exporter_is_compatible_with_pallet_xcm_bridge_hub_router`,\nwhich ensures that our `pallet_xcm_bridge_hub` and\n`pallet_xcm_bridge_hub_router` are compatible when handling\n`ExportMessage`. Other changes are just small nits and cosmetics which\nmakes others stuff easier.\n\n---------\n\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>",
          "timestamp": "2024-07-25T06:56:18Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/de6733baedf5406c37393c20912536600d3ef6fa"
        },
        "date": 1721897111504,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92199999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046009328855999995,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037315117092000014,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Sandu",
            "username": "sandreim",
            "email": "54316454+sandreim@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "48afbe352507888934280bfac61918e2c36dcbbd",
          "message": "CandidateDescriptor: disable collator signature and collator id usage (#4665)\n\nCollator id and collator signature do not serve any useful purpose.\nRemoving the signature check from runtime but keeping the checks in the\nnode until the runtime is upgraded.\n\nTODO: \n- [x] PRDoc\n- [x] Add node feature for core index commitment enablement\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-07-25T09:44:06Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/48afbe352507888934280bfac61918e2c36dcbbd"
        },
        "date": 1721907769766,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92199999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.048897730384000006,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.038422513298000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dónal Murray",
            "username": "seadanda",
            "email": "donal.murray@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d6f7f495ad25d166b5c0381f67030669f178027a",
          "message": "Add people polkadot genesis chainspec (#5124)\n\nPublished as part of the fellowship\n[v1.2.6](https://github.com/polkadot-fellows/runtimes/releases/tag/v1.2.6)\nrelease and originally intentionally left out of the repo as the\nhardcoded system chains will soon be removed from the\n`polkadot-parachain`.\n\nAfter a conversation in\nhttps://github.com/paritytech/polkadot-sdk/issues/5112 it was pointed\nout by @josepot that there should be a single authoritative source for\nthese chainspecs. Since this is already the place for these it will\nserve until something more fitting can be worked out.",
          "timestamp": "2024-07-25T11:23:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d6f7f495ad25d166b5c0381f67030669f178027a"
        },
        "date": 1721913305695,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93599999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046626687558000014,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037143562129999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Popiak",
            "username": "apopiak",
            "email": "alexander.popiak@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8c61dbadb8bd65388b459153b533777c55c73aa5",
          "message": "Getting Started Script (#4879)\n\ncloses https://github.com/paritytech/polkadot-sdk/pull/4879\n\nProvide a fast and easy way for people to get started developing with\nPolkadot SDK.\n\nSets up a development environment (including Rust) and clones and builds\nthe minimal template.\n\nPolkadot address: 16xyKzix34WZ4um8C3xzMdjKhrAQe9DjCf4KxuHsmTjdcEd\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-25T13:23:28Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8c61dbadb8bd65388b459153b533777c55c73aa5"
        },
        "date": 1721921219551,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92599999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038538068688,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04869390424,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3b9c9098924037615f3e5c942831e267468057d4",
          "message": "Dependabot: Group all CI dependencies (#5145)\n\nDependabot is going a bit crazy lately and spamming up a lot of merge\nrequests. Going to group all the CI deps into one and reducing the\nfrequency to weekly.\nMaybe we can do some more aggressive batching for the Rust deps as well.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-25T15:03:03Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3b9c9098924037615f3e5c942831e267468057d4"
        },
        "date": 1721926392149,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.95199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037673723308,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04633766094000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "18db502172bdf438f086cd5964c646b318b8ad37",
          "message": "enhancing solochain template (#5143)\n\nSince we have a minimal template, I propose enhancing the solochain\ntemplate, which would be easier for startup projects.\n\n- Sync separates `api`, `configs`, and `benchmarks` from the parachain\ntemplate\n- introducing `frame-metadata-hash-extension`\n- Some style update\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-07-25T17:20:15Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/18db502172bdf438f086cd5964c646b318b8ad37"
        },
        "date": 1721934480485,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94599999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.04107351004600001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04940388669000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "326342fe63297668d74fc69095090e53d43a2b0a",
          "message": "Upgrade time crate, fix compilation on Rust 1.80 (#5149)\n\nThis will fix the compilation on the latest Rust 1.80.0\n\nPS. There are tons of new warnings about feature gates and annotations,\nit would be nice you guys to investigate them",
          "timestamp": "2024-07-26T10:24:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/326342fe63297668d74fc69095090e53d43a2b0a"
        },
        "date": 1721991398602,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.90799999999992,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.04573430456399999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.05690635620200001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "200632144624d6a73fed400219761da58379cb3c",
          "message": "Umbrella crate: Add polkadot-sdk-frame/?runtime (#5151)\n\nThis should make it possible to use the umbrella crate alone for\ntemplates/*/runtime crate of the repo",
          "timestamp": "2024-07-26T11:47:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/200632144624d6a73fed400219761da58379cb3c"
        },
        "date": 1721996397979,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.91599999999993,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04513687381600001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036472933740000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "200632144624d6a73fed400219761da58379cb3c",
          "message": "Umbrella crate: Add polkadot-sdk-frame/?runtime (#5151)\n\nThis should make it possible to use the umbrella crate alone for\ntemplates/*/runtime crate of the repo",
          "timestamp": "2024-07-26T11:47:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/200632144624d6a73fed400219761da58379cb3c"
        },
        "date": 1722001090726,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92799999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03687695882000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04526167456799999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5dc0670a85d575480e0840204c20a5771cd8f7d3",
          "message": "BEEFY: Disarm finality notifications to prevent pinning (#5129)\n\nThis should prevent excessive pinning of blocks while we are waiting for\nthe block ancestry to be downloaded after gap sync.\nWe spawn a new task that gets polled to transform finality notifications\ninto an unpinned counterpart. Before this PR, finality notifications\nwere kept in the notification channel. This led to pinning cache\noverflows.\n\nfixes #4389\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-26T14:33:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5dc0670a85d575480e0840204c20a5771cd8f7d3"
        },
        "date": 1722006230736,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92199999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.044763566549999974,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036447095839999985,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Guillaume Thiolliere",
            "username": "gui1117",
            "email": "gui.thiolliere@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "72509375d17ce3d78ee3794792cabcea5e560a15",
          "message": "Fix warnings for rust 1.80 (#5150)\n\nFix warnings for rust 1.80",
          "timestamp": "2024-07-26T17:27:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/72509375d17ce3d78ee3794792cabcea5e560a15"
        },
        "date": 1722017757194,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04561630370799999,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03682698459600001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sergej Sakac",
            "username": "Szegoo",
            "email": "73715684+Szegoo@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c39cc333b677da6e4f18ee836a38bf01130ca221",
          "message": "Fix region nonfungible implementation (#5067)\n\nThe problem with the current implementation is that minting will cause\nthe region coremask to be set to `Coremask::complete` regardless of the\nactual coremask.\n\nThis PR fixes that.\n\nMore details about the nonfungible implementation can be found here:\nhttps://github.com/paritytech/polkadot-sdk/pull/3455\n\n---------\n\nCo-authored-by: Dónal Murray <donalm@seadanda.dev>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-26T17:49:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c39cc333b677da6e4f18ee836a38bf01130ca221"
        },
        "date": 1722023975288,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.962,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045335929889999994,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036285020147999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d3d1542c1d387408c141f9a1a8168e32435a4be9",
          "message": "Replace homepage in all TOML files (#5118)\n\nA bit of a controversial move, but a good preparation for even further\nreducing the traffic on outdated content of `substrate.io`. Current\nstatus:\n\n<img width=\"728\" alt=\"Screenshot 2024-07-15 at 11 32 48\"\nsrc=\"https://github.com/user-attachments/assets/df33b164-0ce7-4ac4-bc97-a64485f12571\">\n\nPreviously, I was in favor of changing the domain of the rust-docs to\nsomething like `polkadot-sdk.parity.io` or similar, but I think the\ncurrent format is pretty standard and has a higher chance of staying put\nover the course of time:\n\n`<org-name>.github.io/<repo-name>` ->\n`https://paritytech.github.io/polkadot-sdk/`\n\npart of https://github.com/paritytech/eng-automation/issues/10",
          "timestamp": "2024-07-26T23:20:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d3d1542c1d387408c141f9a1a8168e32435a4be9"
        },
        "date": 1722044526971,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92799999999994,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04577076319199997,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037062420913999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Guillaume Thiolliere",
            "username": "gui1117",
            "email": "gui.thiolliere@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "10b8039a53ddaa10ca37fe70c64e549c649a22dc",
          "message": "Re: fix warnings with latest rust (#5161)\n\nI made mistake on previous PR\nhttps://github.com/paritytech/polkadot-sdk/pull/5150. It disabled all\nunexpected cfgs instead of just allowing `substrate_runtime` condition.\n\nIn this PR: unexpected cfgs other than `substrate_runtime` are still\nchecked. and some warnings appear about them",
          "timestamp": "2024-07-28T07:10:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/10b8039a53ddaa10ca37fe70c64e549c649a22dc"
        },
        "date": 1722158491844,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.89399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.040023583684000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04742970789,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "André Silva",
            "username": "andresilva",
            "email": "123550+andresilva@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dc4047c7d05f593757328394d1accf0eb382d709",
          "message": "grandpa: handle error from SelectChain::finality_target (#5153)\n\nFix https://github.com/paritytech/polkadot-sdk/issues/3487.\n\n---------\n\nCo-authored-by: Dmitry Lavrenov <39522748+dmitrylavrenov@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <info@kchr.de>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-28T20:03:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dc4047c7d05f593757328394d1accf0eb382d709"
        },
        "date": 1722204858487,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93199999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.044923026972000006,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036619518356,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Gheorghe",
            "username": "alexggh",
            "email": "49718502+alexggh@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0636ffdc3dfea52e90102403527ff99d2f2d6e7c",
          "message": "[2 / 5] Make approval-distribution logic runnable on a separate thread (#4845)\n\nThis is part of the work to further optimize the approval subsystems, if\nyou want to understand the full context start with reading\nhttps://github.com/paritytech/polkadot-sdk/pull/4849#issue-2364261568,\n\n# Description\n\nThis PR contain changes to make possible the run of multiple instances\nof approval-distribution, so that we can parallelise the work. This does\nnot contain any functional changes it just decouples the subsystem from\nthe subsystem Context and introduces more specific trait dependencies\nfor each function instead of all of them requiring a context.\n\nIt does not have any dependency of the follow PRs, so it can be merged\nindependently of them.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-07-29T10:09:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0636ffdc3dfea52e90102403527ff99d2f2d6e7c"
        },
        "date": 1722255030033,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92199999999994,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04511175016799998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03643301948999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4def82e7ff6cfacee9e33f53e20f25c10ce6b9e9",
          "message": "Bump serde_json from 1.0.120 to 1.0.121 in the known_good_semver group (#5169)\n\nBumps the known_good_semver group with 1 update:\n[serde_json](https://github.com/serde-rs/json).\n\nUpdates `serde_json` from 1.0.120 to 1.0.121\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/serde-rs/json/releases\">serde_json's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v1.0.121</h2>\n<ul>\n<li>Optimize position search in error path (<a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1160\">#1160</a>,\nthanks <a\nhref=\"https://github.com/purplesyringa\"><code>@​purplesyringa</code></a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/eca2658a22cb39952783cb6914eb18242659f66a\"><code>eca2658</code></a>\nRelease 1.0.121</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/b0d678cfb473386830d559b6ab255d9e21ba39c5\"><code>b0d678c</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1160\">#1160</a>\nfrom iex-rs/efficient-position</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/b1edc7d13f72880fd0ac569403a409e5f7961d5f\"><code>b1edc7d</code></a>\nOptimize position search in error path</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/40dd7f5e862436f02471fe076f3486c55e472bc2\"><code>40dd7f5</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1159\">#1159</a>\nfrom iex-rs/fix-recursion</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/6a306e6ee9f47f3b37088217ffe3ebe9bbb54e5a\"><code>6a306e6</code></a>\nMove call to tri! out of check_recursion!</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/3f1c6de4af28b1f6c5100da323f2bffaf7c2083f\"><code>3f1c6de</code></a>\nIgnore byte_char_slices clippy lint in test</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/3fd6f5f49dc1c732d9b1d7dfece4f02c0d440d39\"><code>3fd6f5f</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1153\">#1153</a>\nfrom dpathakj/master</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/fcb5e83e44abe0f9c27c755a240a6ad56312c090\"><code>fcb5e83</code></a>\nCorrect documentation URL for Value's Index impl.</li>\n<li>See full diff in <a\nhref=\"https://github.com/serde-rs/json/compare/v1.0.120...v1.0.121\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=serde_json&package-manager=cargo&previous-version=1.0.120&new-version=1.0.121)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore <dependency name> major version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's major version (unless you unignore this specific\ndependency's major version or upgrade to it yourself)\n- `@dependabot ignore <dependency name> minor version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's minor version (unless you unignore this specific\ndependency's minor version or upgrade to it yourself)\n- `@dependabot ignore <dependency name>` will close this group update PR\nand stop Dependabot creating any more for the specific dependency\n(unless you unignore this specific dependency or upgrade to it yourself)\n- `@dependabot unignore <dependency name>` will remove all of the ignore\nconditions of the specified dependency\n- `@dependabot unignore <dependency name> <ignore condition>` will\nremove the ignore condition of the specified dependency and ignore\nconditions\n\n\n</details>\n\n---------\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2024-07-29T14:29:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4def82e7ff6cfacee9e33f53e20f25c10ce6b9e9"
        },
        "date": 1722271082883,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.942,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03927133414200002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04933511857800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Yuri Volkov",
            "username": "mutantcornholio",
            "email": "0@mcornholio.ru"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "71cb378f14e110607b3fa568803003bf331d7fdf",
          "message": "Review-bot@2.6.0 (#5177)",
          "timestamp": "2024-07-29T19:09:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/71cb378f14e110607b3fa568803003bf331d7fdf"
        },
        "date": 1722286716012,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93599999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036629370878,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045726413072,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Pankaj",
            "username": "Polkaverse",
            "email": "pankajchaudhary172@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "839ead3441b4012cbf6451af5b19a923bcbc379b",
          "message": "Remove pallet::getter usage from proxy (#4963)\n\nISSUE\nLink to the issue:\nhttps://github.com/paritytech/polkadot-sdk/issues/3326\n\nDeliverables\n\n[Deprecation] remove pallet::getter usage from pallet-proxy\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-29T22:12:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/839ead3441b4012cbf6451af5b19a923bcbc379b"
        },
        "date": 1722297751227,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94799999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.047862193694,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.038024580797999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "07544295e45faedbd2bd6f903b76653527e0b6cf",
          "message": "[subsystem-benchmark] Update availability-distribution-regression-bench baseline after recent subsystem changes (#5180)",
          "timestamp": "2024-07-30T08:54:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/07544295e45faedbd2bd6f903b76653527e0b6cf"
        },
        "date": 1722341072788,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037384287506,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04677198309600002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "686ee99c7c5de56d1416d47e4db4f3a2420c6c82",
          "message": "[CI] Cache try-runtime check (#5179)\n\nAdds a snapshot step to the try-runtime check that tries to download a\ncached snapshot.\nThe cache is valid for the current day and is otherwise re-created.\n\nCheck is now only limited by build time and docker startup.\n\n![Screenshot 2024-07-30 at 02 02\n58](https://github.com/user-attachments/assets/0773e9b9-4a52-4572-a891-74b9d725ba70)\n\n![Screenshot 2024-07-30 at 02 02\n20](https://github.com/user-attachments/assets/4685ef17-a04c-4bdc-9d61-311d0010f71c)\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-30T12:29:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/686ee99c7c5de56d1416d47e4db4f3a2420c6c82"
        },
        "date": 1722349490455,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.96,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03715819613799999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04697318362199998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Gonçalo Pestana",
            "username": "gpestana",
            "email": "g6pestana@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "03c45b910331214c5b4f9cc88244b684e8a97e42",
          "message": "pallet-timestamp: `UnixTime::now` implementation logs error only if called at genesis (#5055)\n\nThis PR reverts the removal of an [`if`\nstatement](https://github.com/paritytech/polkadot-sdk/commit/7ecf3f757a5d6f622309cea7f788e8a547a5dce8#diff-8bf31ba8d9ebd6377983fd7ecc7f4e41cb1478a600db1a15a578d1ae0e8ed435L370)\nmerged recently, which affected test output verbosity of several pallets\n(e.g. staking, EPM, and potentially others).\n\nMore generally, the `UnixTime::now` implementation of the timestamp\npallet should log an error *only* when called at the genesis block.",
          "timestamp": "2024-07-30T15:03:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/03c45b910331214c5b4f9cc88244b684e8a97e42"
        },
        "date": 1722358888949,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92199999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038059570496000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.047704718782000013,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Parth Mittal",
            "username": "mittal-parth",
            "email": "76661350+mittal-parth@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7fbfc7e0cf22ba0b5340b2ce09e17fe7072c9b70",
          "message": "Remove `pallet::getter` usage from the pallet-balances (#4967)\n\nAs per #3326, removes usage of the `pallet::getter` macro from the\nbalances pallet. The syntax `StorageItem::<T, I>::get()` should be used\ninstead.\n\nAlso, adds public functions for compatibility.\n\ncc @muraca\n\npolkadot address: 5GsLutpKjbzsbTphebs9Uy4YK6gTN47MAaz6njPktidjR5cp\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-30T18:10:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7fbfc7e0cf22ba0b5340b2ce09e17fe7072c9b70"
        },
        "date": 1722369533178,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94799999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999999,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03879543903599999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04902315715800003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Guillaume Thiolliere",
            "username": "gui1117",
            "email": "gui.thiolliere@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "39daa61eb3a0c7395f96cad5c0a30c4cfc2ecfe9",
          "message": "Run UI tests in CI for some other crates (#5167)\n\nThe test name is `test-frame-ui` I don't know if I can also change it to\n`test-ui` without breaking other stuff. So I kept the name unchanged.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-31T08:45:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/39daa61eb3a0c7395f96cad5c0a30c4cfc2ecfe9"
        },
        "date": 1722422501268,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93399999999993,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03791378063199998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04819008556600002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7d0aa89653d5073081a949eca1de2ca2d42a9e98",
          "message": "litep2p/discovery: Publish authority records with external addresses only (#5176)\n\nThis PR reduces the occurrences for identified observed addresses.\n\nLitep2p discovers its external addresses by inspecting the\n`IdentifyInfo::ObservedAddress` field reported by other peers.\nAfter we get 5 confirmations of the same external observed address (the\naddress the peer dialed to reach us), the address is reported through\nthe network layer.\n\nThe PR effectively changes this from 5 to 2.\nThis has a subtle implication on freshly started nodes for the\nauthority-discovery discussed below.\n\nThe PR also makes the authority discovery a bit more robust by not\npublishing records if the node doesn't have addresses yet to report.\nThis aims to fix a scenario where:\n- the litep2p node has started, it has some pending observed addresses\nbut less than 5\n- the authorit-discovery publishes a record, but at this time the node\ndoesn't have any addresses discovered and the record is published\nwithout addresses -> this means other nodes will not be able to reach us\n\nNext Steps\n- [ ] versi testing\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/5147\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-31T10:20:28Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7d0aa89653d5073081a949eca1de2ca2d42a9e98"
        },
        "date": 1722428076161,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037758680522000015,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.047256836590000025,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6a5b6e03bfc8d0c6f5f05f3180313902c15aee84",
          "message": "Adjust sync templates flow to use new release branch (#5182)\n\nAs the release branch name changed starting from this release, this PR\nadds it to the sync templates flow so that checkout step worked\nproperly.\n\n---------\n\nCo-authored-by: rzadp <roopert7@gmail.com>",
          "timestamp": "2024-08-01T08:43:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6a5b6e03bfc8d0c6f5f05f3180313902c15aee84"
        },
        "date": 1722509021101,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92799999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.039202942485999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04831305812599997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8ccb6b33c564da038de2af987d4e8d347f32e9c7",
          "message": "Add an adapter for configuring AssetExchanger (#5130)\n\nAdded a new adapter to xcm-builder, the `SingleAssetExchangeAdapter`.\nThis adapter makes it easy to use `pallet-asset-conversion` for\nconfiguring the `AssetExchanger` XCM config item.\n\nI also took the liberty of adding a new function to the `AssetExchange`\ntrait, with the following signature:\n\n```rust\nfn quote_exchange_price(give: &Assets, want: &Assets, maximal: bool) -> Option<Assets>;\n```\n\nThe signature is meant to be fairly symmetric to that of\n`exchange_asset`.\nThe way they interact can be seen in the doc comment for it in the\n`AssetExchange` trait.\n\nThis is a breaking change but is needed for\nhttps://github.com/paritytech/polkadot-sdk/pull/5131.\nAnother idea is to create a new trait for this but that would require\nsetting it in the XCM config which is also breaking.\n\nOld PR: https://github.com/paritytech/polkadot-sdk/pull/4375.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-08-02T12:24:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8ccb6b33c564da038de2af987d4e8d347f32e9c7"
        },
        "date": 1722608517914,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04571169707799998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03687248358800002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ce6938ae92b77b54aa367e6d367a4d490dede7c4",
          "message": "rpc: Enable ChainSpec for polkadot-parachain (#5205)\n\nThis PR enables the `chainSpec_v1` class for the polkadot-parachian. \nThe chainSpec is part of the rpc-v2 which is spec-ed at:\nhttps://github.com/paritytech/json-rpc-interface-spec/blob/main/src/api/chainSpec.md.\n\nThis also paves the way for enabling a future `chainSpec_unstable_spec`\non all nodes.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/5191\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-08-02T15:09:13Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ce6938ae92b77b54aa367e6d367a4d490dede7c4"
        },
        "date": 1722618529285,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92999999999992,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04642195958599998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037832334008,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2abd03ef330c8b55e73755a7ef4b43baf1451657",
          "message": "beefy: Tolerate pruned state on runtime API call (#5197)\n\nWhile working on #5129 I noticed that after warp sync, nodes would\nprint:\n```\n2024-07-29 17:59:23.898 ERROR ⋮beefy: 🥩 Error: ConsensusReset. Restarting voter.    \n```\n\nAfter some debugging I found that we enter the following loop:\n1. Wait for beefy pallet to be available: Pallet is detected available\ndirectly after warp sync since we are at the tip.\n2. Wait for headers from tip to beefy genesis to be available: During\nthis time we don't process finality notifications, since we later want\nto inspect all the headers for authority set changes.\n3. Gap sync finishes, route to beefy genesis is available.\n4. The worker starts acting, tries to fetch beefy genesis block. It\nfails, since we are acting on old finality notifications where the state\nis already pruned.\n5. Whole beefy subsystem is being restarted, loading the state from db\nagain and iterating a lot of headers.\n\nThis already happened before #5129.",
          "timestamp": "2024-08-05T07:40:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2abd03ef330c8b55e73755a7ef4b43baf1451657"
        },
        "date": 1722850827133,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.89999999999992,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.05139142230000001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.04084992188800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "035211d707d0a74a2a768fd658160721f09d5b44",
          "message": "Remove unused feature gated code from the minimal template (#5237)\n\n- Progresses https://github.com/paritytech/polkadot-sdk/issues/5226\n\nThere is no actual `try-runtime` or `runtime-benchmarks` functionality\nin the minimal template at the moment.",
          "timestamp": "2024-08-05T11:48:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/035211d707d0a74a2a768fd658160721f09d5b44"
        },
        "date": 1722865688407,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036298280706000025,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045745884142000016,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Miasojed",
            "username": "smiasojed",
            "email": "s.miasojed@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "11fdff18e8cddcdee90b0e2c3e35a2b0124de4de",
          "message": "[pallet_contracts] Increase the weight of the deposit_event host function to limit the memory used by events. (#4973)\n\nThis PR updates the weight of the `deposit_event` host function by\nadding\na fixed ref_time of 60,000 picoseconds per byte. Given a block time of 2\nseconds\nand this specified ref_time, the total allocation size is 32MB.\n\n---------\n\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2024-08-06T15:12:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/11fdff18e8cddcdee90b0e2c3e35a2b0124de4de"
        },
        "date": 1722964146287,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94199999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03726748550800002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04587051390599999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "291c082cbbb0c838c886f38040e54424c55d9618",
          "message": "Improve Pallet UI doc test (#5264)\n\nTest currently failing, therefore improving to include a file from the\nsame crate to not trip up the caching.\n\nR0 silent since this is only modifying unpublished crates.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2024-08-06T18:04:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/291c082cbbb0c838c886f38040e54424c55d9618"
        },
        "date": 1722974365106,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93399999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03702615041400001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045834972454000006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ron",
            "username": "yrong",
            "email": "yrong1997@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "efdc1e9b1615c5502ed63ffc9683d99af6397263",
          "message": "Snowbridge on Westend (#5074)\n\n### Context\n\nSince Rococo is now deprecated, we need another testnet to detect\nbleeding-edge changes to Substrate, Polkadot, & BEEFY consensus\nprotocols that could brick the bridge.\n\nIt's the mirror PR of https://github.com/Snowfork/polkadot-sdk/pull/157\nwhich has reviewed by Snowbridge team internally.\n\nSynced with @acatangiu about that in channel\nhttps://matrix.to/#/!gxqZwOyvhLstCgPJHO:matrix.parity.io/$N0CvTfDSl3cOQLEJeZBh-wlKJUXx7EDHAuNN5HuYHY4?via=matrix.parity.io&via=parity.io&via=matrix.org\n\n---------\n\nCo-authored-by: Clara van Staden <claravanstaden64@gmail.com>",
          "timestamp": "2024-08-07T09:49:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/efdc1e9b1615c5502ed63ffc9683d99af6397263"
        },
        "date": 1723031732100,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93799999999993,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036410715538,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04582053834800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0fb6e3c51eb37575f33ac7f06b6350c0aab4f0c7",
          "message": "Umbrella crate: exclude chain-specific crates (#5173)\n\nUses custom metadata to exclude chain-specific crates.  \nThe only concern is that devs who want to use chain-specific crates,\nstill need to select matching versions numbers. Could possibly be\naddresses with chain-specific umbrella crates, but currently it should\nbe possible to use [psvm](https://github.com/paritytech/psvm).\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-08-07T14:30:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0fb6e3c51eb37575f33ac7f06b6350c0aab4f0c7"
        },
        "date": 1723047948876,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.95199999999991,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03714147479600001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04706038380599999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Maksym H",
            "username": "mordamax",
            "email": "1177472+mordamax@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "711d91aaabb9687cd26904443c23706cd4dbd1ee",
          "message": "frame-omni-bencher short checks (#5268)\n\n- Part of https://github.com/paritytech/ci_cd/issues/1006\n- Closes: https://github.com/paritytech/ci_cd/issues/1010\n- Related: https://github.com/paritytech/polkadot-sdk/pull/4405\n\n- Possibly affecting how frame-omni-bencher works on different runtimes:\nhttps://github.com/paritytech/polkadot-sdk/pull/5083\n\nCurrently works in parallel with gitlab short benchmarks. \nTriggered only by adding `GHA-migration` label to assure smooth\ntransition (kind of feature-flag).\nLater when tested on random PRs we'll remove the gitlab and turn on by\ndefault these tests\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-08-07T17:03:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/711d91aaabb9687cd26904443c23706cd4dbd1ee"
        },
        "date": 1723057724827,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93199999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038549133253999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.048609769338000014,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "joe petrowski",
            "username": "joepetrowski",
            "email": "25483142+joepetrowski@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "eb0a9e593fb6a0f2bbfdb75602a51f4923995529",
          "message": "Fix Weight Annotation (#5275)\n\nhttps://github.com/paritytech/polkadot-sdk/pull/4527/files#r1706673828",
          "timestamp": "2024-08-08T08:47:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/eb0a9e593fb6a0f2bbfdb75602a51f4923995529"
        },
        "date": 1723113880763,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94399999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037049332418,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04615549656999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Samusev",
            "username": "alvicsam",
            "email": "41779041+alvicsam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "12539e7a931e82a040e74c84e413baa712ecd638",
          "message": "[ci] Add test-linux-stable jobs GHA (#4897)\n\nPR adds github-action for jobs test-linux-stable-oldkernel.\nPR waits the latest release of forklift.\n\ncc https://github.com/paritytech/ci_cd/issues/939\ncc https://github.com/paritytech/ci_cd/issues/1006\n\n---------\n\nCo-authored-by: Maksym H <1177472+mordamax@users.noreply.github.com>",
          "timestamp": "2024-08-08T15:20:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/12539e7a931e82a040e74c84e413baa712ecd638"
        },
        "date": 1723138461795,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036835195034000005,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046310132088000014,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2993b0008e2ec4040be91868bf5f48a892508c3a",
          "message": "Add stable release tag as an input parameter (#5282)\n\nThis PR adds the possibility to set the docker stable release tag as an\ninput parameter to the produced docker images, so that it matches with\nthe release version",
          "timestamp": "2024-08-09T08:01:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2993b0008e2ec4040be91868bf5f48a892508c3a"
        },
        "date": 1723197473193,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91799999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038096964627999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04718024213000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "87280eb52537a355c67c9b9cee8ff1f97d02d67a",
          "message": "Synchronize templates through PRs, instead of pushes (#5291)\n\nDespite what we had in the [original\nrequest](https://github.com/paritytech/polkadot-sdk/issues/3155#issuecomment-1979037109),\nI'm proposing a change to open a PR to the destination template\nrepositories instead of pushing the code.\n\nThis will give it a chance to run through the destination CI before\nmaking changes, and to set stricter branch protection in the destination\nrepos.",
          "timestamp": "2024-08-09T11:03:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/87280eb52537a355c67c9b9cee8ff1f97d02d67a"
        },
        "date": 1723208274722,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037343891114000015,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04606659208599998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Samusev",
            "username": "alvicsam",
            "email": "41779041+alvicsam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b1a9ad4d387e8e75932e4bc8950c4535f4c82119",
          "message": "[ci] Move checks to GHA (#5289)\n\nCloses https://github.com/paritytech/ci_cd/issues/1012",
          "timestamp": "2024-08-09T13:05:18Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b1a9ad4d387e8e75932e4bc8950c4535f4c82119"
        },
        "date": 1723215548516,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03651846625000002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045533297612,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "149c70938f2b29f8d92ba1cc952aeb63d4084e27",
          "message": "Add missing features in templates' node packages (#5294)\n\nCorrects the issue we had\n[here](https://github.com/paritytech/polkadot-sdk-parachain-template/pull/10),\nin which `cargo build --release` worked but `cargo build --package\nparachain-template-node --release` failed with missing features.\n\nThe command has been added to CI to make sure it works, but at the same\nwe're changing it in the readme to just `cargo build --release` for\nsimplification.\n\nLabeling silent because those packages are un-published as part of the\nregular release process.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Shawn Tabrizi <shawntabrizi@gmail.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-08-09T16:11:50Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/149c70938f2b29f8d92ba1cc952aeb63d4084e27"
        },
        "date": 1723227151697,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.942,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037226186744000016,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04699253603599998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "jserrat",
            "username": "Jpserrat",
            "email": "35823283+Jpserrat@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ebcbca3ff606b22b5eb81bcbfaa9309752d64dde",
          "message": "xcm-executor: allow deposit of multiple assets if at least one of them satisfies ED (#4460)\n\nCloses #4242\n\nXCM programs that deposit assets to some new (empty) account will now\nsucceed if at least one of the deposited assets satisfies ED. Before\nthis change, the requirement was that the _first_ asset had to satisfy\nED, but assets order can be changed during reanchoring so it is not\nreliable.\n\nWith this PR, ordering doesn't matter, any one(s) of them can satisfy ED\nfor the whole deposit to work.\n\nKusama address: FkB6QEo8VnV3oifugNj5NeVG3Mvq1zFbrUu4P5YwRoe5mQN\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-08-12T08:40:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ebcbca3ff606b22b5eb81bcbfaa9309752d64dde"
        },
        "date": 1723459113286,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92799999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03780464119200001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04602941030200001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dónal Murray",
            "username": "seadanda",
            "email": "donal.murray@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "1f49358db0033e57a790eac6daccc45beba81863",
          "message": "Fix favicon link to fix CI (#5319)\n\nThe polkadot.network website was recently refreshed and the\n`favicon-32x32.png` was removed. It was linked in some docs and so the\ndocs have been updated to point to a working favicon on the new website.\n\nPreviously the lychee link checker was failing on all PRs.",
          "timestamp": "2024-08-12T10:44:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1f49358db0033e57a790eac6daccc45beba81863"
        },
        "date": 1723466207232,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94599999999991,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045672315325999975,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037399839324,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alin Dima",
            "username": "alindima",
            "email": "alin@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "fc906d5d0fb7796ef54ba670101cf37b0aad6794",
          "message": "fix av-distribution Jaeger spans mem leak (#5321)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/5258",
          "timestamp": "2024-08-12T13:56:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fc906d5d0fb7796ef54ba670101cf37b0aad6794"
        },
        "date": 1723478212067,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93799999999993,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036756936788000016,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046113830606000016,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk",
            "email": "1728078+michalkucharczyk@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b52cfc2605362b53a1a570c3c1b41d15481d0990",
          "message": "chain-spec: minor clarification on the genesis config patch (#5324)\n\nAdded minor clarification on the genesis config patch\n([link](https://substrate.stackexchange.com/questions/11813/in-the-genesis-config-what-does-the-patch-key-do/11825#11825))\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-08-12T15:58:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b52cfc2605362b53a1a570c3c1b41d15481d0990"
        },
        "date": 1723485149650,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045633596585999976,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036894423572,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "eskimor",
            "username": "eskimor",
            "email": "eskimor@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "819a5818284a96a5a5bd65ce67e69bab860d4534",
          "message": "Bump authoring duration for async backing to 2s. (#5195)\n\nShould be safe on all production network. \n\nI noticed that Paseo needs to be updated, it is lacking behind in a\ncouple of things.\n\nExecution environment parameters should be updated to those of Polkadot:\n\n```\n[\n      {\n        MaxMemoryPages: 8,192\n      }\n      {\n        PvfExecTimeout: [\n          Backing\n          2,500\n        ]\n      }\n      {\n        PvfExecTimeout: [\n          Approval\n          15,000\n        ]\n      }\n    ]\n  ]\n  ```\n\n---------\n\nCo-authored-by: eskimor <eskimor@no-such-url.com>",
          "timestamp": "2024-08-12T20:34:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/819a5818284a96a5a5bd65ce67e69bab860d4534"
        },
        "date": 1723501591975,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94999999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03731461479,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04860362933000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c5f6b700bd1f3fd6a7fa40405987782bdecec636",
          "message": "Bump libp2p-identity from 0.2.8 to 0.2.9 (#5232)\n\nBumps [libp2p-identity](https://github.com/libp2p/rust-libp2p) from\n0.2.8 to 0.2.9.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/libp2p/rust-libp2p/releases\">libp2p-identity's\nreleases</a>.</em></p>\n<blockquote>\n<h2>libp2p-v0.53.2</h2>\n<p>See individual <a\nhref=\"https://github.com/libp2p/rust-libp2p/blob/HEAD/CHANGELOG.md\">changelogs</a>\nfor details.</p>\n<h2>libp2p-v0.53.1</h2>\n<p>See individual <a\nhref=\"https://github.com/libp2p/rust-libp2p/blob/HEAD/CHANGELOG.md\">changelogs</a>\nfor details.</p>\n<h2>libp2p-v0.53.0</h2>\n<p>The most ergonomic version of rust-libp2p yet!</p>\n<p>We've been busy again, with over <a\nhref=\"https://github.com/libp2p/rust-libp2p/compare/libp2p-v0.52.0...master\">250</a>\nPRs being merged into <code>master</code> since <code>v0.52.0</code>\n(excluding dependency updates).</p>\n<h2>Backwards-compatible features</h2>\n<p>Numerous improvements landed as patch releases since the\n<code>v0.52.0</code> release, for example a new, type-safe <a\nhref=\"https://redirect.github.com/libp2p/rust-libp2p/pull/4120\"><code>SwarmBuilder</code></a>\nthat also encompasses the most common transport protocols:</p>\n<pre lang=\"rust\"><code>let mut swarm =\nlibp2p::SwarmBuilder::with_new_identity()\n    .with_tokio()\n    .with_tcp(\n        tcp::Config::default().port_reuse(true).nodelay(true),\n        noise::Config::new,\n        yamux::Config::default,\n    )?\n    .with_quic()\n    .with_dns()?\n    .with_relay_client(noise::Config::new, yamux::Config::default)?\n    .with_behaviour(|keypair, relay_client| Behaviour {\n        relay_client,\n        ping: ping::Behaviour::default(),\n        dcutr: dcutr::Behaviour::new(keypair.public().to_peer_id()),\n    })?\n    .build();\n</code></pre>\n<p>The new builder makes heavy use of the type-system to guide you\ntowards a correct composition of all transports. For example, it is\nimportant to compose the DNS transport as a wrapper around all other\ntransports but before the relay transport. Luckily, you no longer need\nto worry about these details as the builder takes care of that for you!\nHave a look yourself if you dare <a\nhref=\"https://github.com/libp2p/rust-libp2p/tree/master/libp2p/src/builder\">here</a>\nbut be warned, the internals are a bit wild :)</p>\n<p>Some more features that we were able to ship in <code>v0.52.X</code>\npatch-releases include:</p>\n<ul>\n<li><a\nhref=\"https://redirect.github.com/libp2p/rust-libp2p/pull/4325\">stable\nQUIC implementation</a></li>\n<li>for rust-libp2p compiled to WASM running in the browser\n<ul>\n<li><a\nhref=\"https://redirect.github.com/libp2p/rust-libp2p/pull/4015\">WebTransport\nsupport</a></li>\n<li><a\nhref=\"https://redirect.github.com/libp2p/rust-libp2p/pull/4248\">WebRTC\nsupport</a></li>\n</ul>\n</li>\n<li><a\nhref=\"https://redirect.github.com/libp2p/rust-libp2p/pull/4156\">UPnP\nimplementation to automatically configure port-forwarding with ones\ngateway</a></li>\n<li><a\nhref=\"https://redirect.github.com/libp2p/rust-libp2p/pull/4281\">option\nto limit connections based on available memory</a></li>\n</ul>\n<p>We always try to ship as many features as possible in a\nbackwards-compatible way to get them to you faster. Often times, these\ncome with deprecations to give you a heads-up about what will change in\na future version. We advise updating to each intermediate version rather\nthan skipping directly to the most recent one, to avoid missing any\ncrucial deprecation warnings. We highly recommend you stay up-to-date\nwith the latest version to make upgrades as smooth as possible.</p>\n<p>Some improvments we unfortunately cannot ship in a way that Rust\nconsiders a non-breaking change but with every release, we attempt to\nsmoothen the way for future upgrades.</p>\n<h2><code>#[non_exhaustive]</code> on key enums</h2>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li>See full diff in <a\nhref=\"https://github.com/libp2p/rust-libp2p/commits\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=libp2p-identity&package-manager=cargo&previous-version=0.2.8&new-version=0.2.9)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-08-13T08:28:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c5f6b700bd1f3fd6a7fa40405987782bdecec636"
        },
        "date": 1723544886588,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94999999999999,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038580207192000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046507679204,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0d7bd6badce2d53bb332fc48d4a32e828267cf7e",
          "message": "Small nits found accidentally along the way (#5341)",
          "timestamp": "2024-08-13T11:11:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d7bd6badce2d53bb332fc48d4a32e828267cf7e"
        },
        "date": 1723557648927,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94799999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.039177281598,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.048507676321999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "055eb5377da43eaced23647ed4348a816bfeb8f4",
          "message": "StorageWeightReclaim: set to node pov size if higher (#5281)\n\nThis PR adds an additional defensive check to the reclaim SE. \n\nSince it can happen that we miss some storage accesses on other SEs\npre-dispatch, we should double check\nthat the bookkeeping of the runtime stays ahead of the node-side\npov-size.\n\nIf we discover a mismatch and the node-side pov-size is indeed higher,\nwe should set the runtime bookkeeping to the node-side value. In cases\nsuch as #5229, we would stop including extrinsics and not run `on_idle`\nat least.\n\ncc @gui1117\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-08-13T19:57:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/055eb5377da43eaced23647ed4348a816bfeb8f4"
        },
        "date": 1723585919578,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94399999999992,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.048060402122,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.039089991254,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jeeyong Um",
            "username": "conr2d",
            "email": "conr2d@proton.me"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0cd577ba1c4995500eb3ed10330d93402177a53b",
          "message": "Minor clean up (#5284)\n\nThis PR performs minor code cleanup to reduce verbosity. Since the\ncompiler has already optimized out indirect calls in the existing code,\nthese changes improve readability but do not affect performance.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-08-13T21:54:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0cd577ba1c4995500eb3ed10330d93402177a53b"
        },
        "date": 1723594102896,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92399999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03685395366000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045106309534000015,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d944ac2f25d267b443631f891aa68a834dc24af0",
          "message": "Stop running the wishlist workflow on forks (#5297)\n\nAddresses\nhttps://github.com/paritytech/polkadot-sdk/pull/5085#issuecomment-2277231072",
          "timestamp": "2024-08-14T08:46:03Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d944ac2f25d267b443631f891aa68a834dc24af0"
        },
        "date": 1723632954357,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91599999999993,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036978560892,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04607091616000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e4f8a6de49b37af3c58d9414ba81f7002c911551",
          "message": "[tests] dedup test code, add more tests, improve naming and docs (#5338)\n\nThis is mostly tests cleanup:\n- uses helper macro for generating teleport tests,\n- adds missing treasury tests,\n- improves naming and docs for transfer tests.\n\n- [x] does not need a PRDOC\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-08-14T10:58:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e4f8a6de49b37af3c58d9414ba81f7002c911551"
        },
        "date": 1723644174583,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94799999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03611195109599999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045200273572,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Gheorghe",
            "username": "alexggh",
            "email": "49718502+alexggh@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "05a8ba662f0afdd4a4e6e2f4e61e4ca2458d666c",
          "message": "Fix OurViewChange small race (#5356)\n\nAlways queue OurViewChange event before we send view changes to our\npeers, because otherwise we risk the peers sending us a message that can\nbe processed by our subsystems before OurViewChange.\n\nNormally, this is not really a problem because the latency of the\nViewChange we send to our peers is way higher that our subsystem\nprocessing OurViewChange, however on testnets like versi where CPU is\nsometimes overcommitted this race gets triggered occasionally, so let's\nfix it by sending the messages in the right order.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-08-14T13:55:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/05a8ba662f0afdd4a4e6e2f4e61e4ca2458d666c"
        },
        "date": 1723651945306,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94199999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037889271546000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04813190521199999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "53f42749ca1a84a9b391388eadbc3a98004708ec",
          "message": "Upgrade accidentally downgraded deps (#5365)",
          "timestamp": "2024-08-14T20:09:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/53f42749ca1a84a9b391388eadbc3a98004708ec"
        },
        "date": 1723673004864,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92599999999992,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03669287501599999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.044672917006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ebf4f8d2d590f41817d5d38b2d9b5812a46f2342",
          "message": "[Pools] fix derivation of pool account (#4999)\n\ncloses https://github.com/paritytech-secops/srlabs_findings/issues/408.\nThis fixes how ProxyDelegator accounts are derived but may cause issues\nin Westend since it would use the old derivative accounts. Does not\naffect Polkadot/Kusama as this pallet is not deployed to them yet.\n\n---------\n\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>",
          "timestamp": "2024-08-15T00:10:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ebf4f8d2d590f41817d5d38b2d9b5812a46f2342"
        },
        "date": 1723687412250,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94399999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036502698539999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04518643322399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e91f1463884946fa1c11b1994fc6bb121de57091",
          "message": "Bump trie-db from 0.29.0 to 0.29.1 (#5231)\n\nBumps [trie-db](https://github.com/paritytech/trie) from 0.29.0 to\n0.29.1.\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/paritytech/trie/commit/48fcfa99c439949f55d29762e35f8793113edc91\"><code>48fcfa9</code></a>\nmemory-db: update parity-util-mem (<a\nhref=\"https://redirect.github.com/paritytech/trie/issues/166\">#166</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/trie/commit/02b030a24bc60d46ed7f156888bdbbed6498b216\"><code>02b030a</code></a>\nPrepare trie-db 0.24.0 release (<a\nhref=\"https://redirect.github.com/paritytech/trie/issues/163\">#163</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/trie/commit/aff1cbac8f03e8dc7533263b374dc0fcd17444ad\"><code>aff1cba</code></a>\nIntroduce trie level cache &amp; recorder (<a\nhref=\"https://redirect.github.com/paritytech/trie/issues/157\">#157</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/trie/commit/aa3168d6de01793e71ebd906d3a82ae4b363db59\"><code>aa3168d</code></a>\nBump actions/checkout from 2 to 3 (<a\nhref=\"https://redirect.github.com/paritytech/trie/issues/160\">#160</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/trie/commit/d597275768f4796417c7fc9f8ad64f9b26be14d8\"><code>d597275</code></a>\nAdd GHA to dependabot and CODEOWNERS (<a\nhref=\"https://redirect.github.com/paritytech/trie/issues/159\">#159</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/trie/commit/5c9267c1133000aa41a5983d8acd6d0968ab8032\"><code>5c9267c</code></a>\ntest prefix seek more precisely (<a\nhref=\"https://redirect.github.com/paritytech/trie/issues/158\">#158</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/trie/commit/f64e1b0a8ced1b4b574d2b705202bf790d4394e4\"><code>f64e1b0</code></a>\nDo not check for root in <code>TrieDB</code> and <code>TrieDBMut</code>\nconstructors (<a\nhref=\"https://redirect.github.com/paritytech/trie/issues/155\">#155</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/trie/commit/8d5b8675fcc8ecc8648206d08f2e4c06ab489593\"><code>8d5b867</code></a>\nUpdate dependencies. (<a\nhref=\"https://redirect.github.com/paritytech/trie/issues/154\">#154</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/trie/commit/fac100cbf49c197c49d102f12040bccbfa38827e\"><code>fac100c</code></a>\nAdding support for eip-1186 proofs (<a\nhref=\"https://redirect.github.com/paritytech/trie/issues/146\">#146</a>)</li>\n<li><a\nhref=\"https://github.com/paritytech/trie/commit/2e1541e44989f24cec5dbe3081c7cecf00d8b509\"><code>2e1541e</code></a>\nFix hex trace output (<a\nhref=\"https://redirect.github.com/paritytech/trie/issues/153\">#153</a>)</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/paritytech/trie/compare/trie-db-v0.29.0...reference-trie-v0.29.1\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=trie-db&package-manager=cargo&previous-version=0.29.0&new-version=0.29.1)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-08-15T09:21:18Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e91f1463884946fa1c11b1994fc6bb121de57091"
        },
        "date": 1723719531371,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.914,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.039663730251999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04967757327399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Loïs",
            "username": "SailorSnoW",
            "email": "49660929+SailorSnoW@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "069f8a6a374108affa4e240685c7504a1fbd1397",
          "message": "fix visibility for `pallet_nfts` types used as call arguments (#3634)\n\nfix #3631 \n\nTypes which are impacted and fixed here are `ItemTip`,\n`PriceWithDirection`, `PreSignedMint`, `PreSignedAttributes`.\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-08-15T12:33:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/069f8a6a374108affa4e240685c7504a1fbd1397"
        },
        "date": 1723732442958,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93999999999993,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40799999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03683079788399998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046231039751999985,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b5029eb4fd6c7ffd8164b2fe12b71bad0c59c9f2",
          "message": "Update links in the documentation (#5175)\n\n- Where applicable, use a regular [`reference`] instead of\n`../../../reference/index.html`.\n- Typos.\n- Update a link to `polkadot-evm` which has moved out of the monorepo.\n- ~~The link specification for `chain_spec_builder` is invalid~~\n(actually it was valid) - it works fine without it.\n\nPart of https://github.com/paritytech/eng-automation/issues/10",
          "timestamp": "2024-08-15T14:55:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b5029eb4fd6c7ffd8164b2fe12b71bad0c59c9f2"
        },
        "date": 1723741058455,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04693787008800001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037908758351999985,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "843c4db78f0ca260376c5a3abf78c95782ebdc64",
          "message": "Update Readme of the `polkadot` crate (#5326)\n\n- Typos.\n- Those telemetry links like https://telemetry.polkadot.io/#list/Kusama\ndidn't seem to properly point to a proper list (anymore?) - updated\nthem.\n- Also looks like it was trying to use rust-style linking instead of\nmarkdown linking, changed that.\n- Relative links do not work on crates.io - updated to absolute,\nsimilarly as some already existing links, such as contribution\nguidelines.",
          "timestamp": "2024-08-15T17:40:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/843c4db78f0ca260376c5a3abf78c95782ebdc64"
        },
        "date": 1723753133602,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93199999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.049266022667999995,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.038919617421999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Gheorghe",
            "username": "alexggh",
            "email": "49718502+alexggh@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4780e3d07ff23a49e8f0a508138f83eb6e0d36c6",
          "message": "approval-distribution: Fix handling of conclude (#5375)\n\nAfter\n\nhttps://github.com/paritytech/polkadot-sdk/commit/0636ffdc3dfea52e90102403527ff99d2f2d6e7c\napproval-distribution did not terminate anymore if Conclude signal was\nreceived.\n\nThis should have been caught by the subsystem tests, but it wasn't\nbecause the subsystem is also exiting on error when the channels are\ndropped so the test overseer was dropped which made the susbystem exit\nand masked the problem.\n\nThis pr fixes both the test and the subsystem.\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-08-16T07:38:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4780e3d07ff23a49e8f0a508138f83eb6e0d36c6"
        },
        "date": 1723800798978,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91999999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04549660588600002,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03638250723999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "74267881e765a01b1c7b3114c21b80dbe7686940",
          "message": "Remove redundant minimal template workspace (#5330)\n\nThis removes the workspace of the minimal template, which (I think) is\nredundant. The other two templates do not have such a workspace.\n\nThe synchronized template created [it's own\nworkspace](https://github.com/paritytech/polkadot-sdk-minimal-template/blob/master/Cargo.toml)\nanyway, and the new readme replaced the old docs contained in `lib.rs`.\n\nCloses\nhttps://github.com/paritytech/polkadot-sdk-minimal-template/issues/11\n\nSilent because the crate was private.",
          "timestamp": "2024-08-16T10:29:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/74267881e765a01b1c7b3114c21b80dbe7686940"
        },
        "date": 1723811047028,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92799999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036593513714,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04570100952400002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Guillaume Thiolliere",
            "username": "gui1117",
            "email": "gui.thiolliere@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "fd522b818693965b6ec9e4cfdb4182c114d6105c",
          "message": "Fix doc: start_destroy doesn't need asset to be frozen (#5204)\n\nFix https://github.com/paritytech/polkadot-sdk/issues/5184\n\n`owner` can set himself as a `freezer` and freeze the asset so\nrequirement is not really needed. And requirement is not implemented.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-08-16T14:09:06Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fd522b818693965b6ec9e4cfdb4182c114d6105c"
        },
        "date": 1723824532472,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92199999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03662657437399999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04530761559600002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Nazar Mokrynskyi",
            "username": "nazar-pc",
            "email": "nazar@mokrynskyi.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "feac7a521092c599d47df3e49084e6bff732c7db",
          "message": "Replace unnecessary `&mut self` with `&self` in `BlockImport::import_block()` (#5339)\n\nThere was no need for it to be `&mut self` since block import can happen\nconcurrently for different blocks and in many cases it was `&mut Arc<dyn\nBlockImport>` anyway :man_shrugging:\n\nSimilar in nature to\nhttps://github.com/paritytech/polkadot-sdk/pull/4844",
          "timestamp": "2024-08-18T05:23:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/feac7a521092c599d47df3e49084e6bff732c7db"
        },
        "date": 1723965800804,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.90799999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.04117345799200001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.049563087898000006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3fe22d17c4904c5ae016034b6ec2c335464b4165",
          "message": "binary-merkle-tree: Do not spam test output (#5376)\n\nThe CI isn't happy with the amount of output:\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/7035621/raw\n\n---------\n\nCo-authored-by: Shawn Tabrizi <shawntabrizi@gmail.com>",
          "timestamp": "2024-08-18T19:41:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3fe22d17c4904c5ae016034b6ec2c335464b4165"
        },
        "date": 1724017175054,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94599999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037045164278,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04562181083199999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "946afaabd8244f1256f3aecff75e23c02937bd38",
          "message": "Fix publishing  of the`chain-spec-builder` image (#5387)\n\nThis PR fixes the issue with the publishing flow of the\n`chain-speck-builder` image\nCloses: https://github.com/paritytech/release-engineering/issues/219",
          "timestamp": "2024-08-19T15:47:50Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/946afaabd8244f1256f3aecff75e23c02937bd38"
        },
        "date": 1724089480714,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40799999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94599999999993,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045302192516000005,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036483008308000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Gheorghe",
            "username": "alexggh",
            "email": "49718502+alexggh@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f239abac93b8354fec1441e39727b0706b489415",
          "message": "approval-distribution: Fix preallocation of ApprovalEntries (#5411)\n\nWe preallocated the approvals field in the ApprovalEntry by up to a\nfactor of two in the worse conditions, since we can't have more than 6\napprovals and candidates.len() will return 20 if you have just the 20th\nbit set.\nThis adds to a lot of wasted memory because we have an ApprovalEntry for\neach assignment we received\n\nThis was discovered while running rust jemalloc-profiling with the steps\nfrom here: https://www.magiroux.com/rust-jemalloc-profiling/\n\nJust with this optimisation approvals subsystem-benchmark memory usage\non the worst case scenario is reduced from 6.1GiB to 2.4 GiB, even cpu\nusage of approval-distribution decreases by 4-5%.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-08-20T11:19:30Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f239abac93b8354fec1441e39727b0706b489415"
        },
        "date": 1724160224747,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045341186731999994,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037242634610000006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "73e2316adad1582a2113301e4f2938d68ca10974",
          "message": "Fix mmr zombienet test (#5417)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/4309\n\nIf a new block is generated between these 2 lines:\n\n```\n  const proof = await apis[nodeName].rpc.mmr.generateProof([1, 9, 20]);\n\n  const root = await apis[nodeName].rpc.mmr.root()\n```\n\nwe will try to verify a proof for the previous block with the mmr root\nat the current block. Which will fail.\n\nSo we generate the proof and get the mmr root at block 21 for\nconsistency.",
          "timestamp": "2024-08-20T13:02:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/73e2316adad1582a2113301e4f2938d68ca10974"
        },
        "date": 1724165924154,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94199999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.047068780549999995,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03854741795599999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Javier Bullrich",
            "username": "Bullrich",
            "email": "javier@bullrich.dev"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "717bbb24c40717e70d2e3b648bcd372559c71bd2",
          "message": "Migrated `docs` scripts to GitHub actions (#5345)\n\nMigrated the following scripts to GHA\n- test-doc\n- test-rustdoc\n- build-rustdoc\n- build-implementers-guide\n- publish-rustdoc (only runs when `master` is modified)\n\nResolves paritytech/ci_cd#1016\n\n---\n\nSome questions I have:\n- Should I remove the equivalent scripts from the `gitlab-ci` files?\n\n---------\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2024-08-20T15:16:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/717bbb24c40717e70d2e3b648bcd372559c71bd2"
        },
        "date": 1724173821820,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.91999999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03773395667400001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04621429934400003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8b17f0f4c697793e0080836f175596a5ac2a0b3a",
          "message": "peerstore: Clarify peer report warnings (#5407)\n\nThis PR aims to make the logging from the peer store a bit more clear.\n\nIn the past, we aggressively produced warning logs from the peer store\ncomponent, even in cases where the reputation change was not malicious.\nThis has led to an extensive number of logs, as well to node operator\nconfusion.\n\nIn this PR, we produce a warning message if:\n- The peer crosses the banned threshold for the first time. This is the\nactual reason of a ban\n- The peer misbehaves again while being banned. This may happen during a\nbatch peer report\n\ncc @paritytech/networking \n\nPart of: https://github.com/paritytech/polkadot-sdk/issues/5379.\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Dmitry Markin <dmitry@markin.tech>",
          "timestamp": "2024-08-21T15:50:50Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8b17f0f4c697793e0080836f175596a5ac2a0b3a"
        },
        "date": 1724262391119,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93999999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03759913480199999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04584928843800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dónal Murray",
            "username": "seadanda",
            "email": "donal.murray@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dce789ddd28ea179e42a00f2817b3e5d713bf6f6",
          "message": "Add the Polkadot Coretime chain-spec (#5436)\n\nAdd the Polkadot Coretime chain-spec to the directory with the other\nsystem chain-specs.\n\nThis is the chain-spec used at genesis and for which the genesis head\ndata was generated.\n\nIt is also included in the assets for fellowship [release\nv1.3.0](https://github.com/polkadot-fellows/runtimes/releases/tag/v1.3.0)",
          "timestamp": "2024-08-22T06:37:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dce789ddd28ea179e42a00f2817b3e5d713bf6f6"
        },
        "date": 1724315484079,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93599999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.047794641311999994,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03817367199,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e600b74ce586c462c36074856f62008e1c145318",
          "message": "[Backport] Version bumps and prdoc reorgs from stable2407-1 (#5374)\n\nThis PR backports regular version bumps and `prdoc` reorganisation from\nthe `stable2407` release branch to master",
          "timestamp": "2024-08-22T11:57:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e600b74ce586c462c36074856f62008e1c145318"
        },
        "date": 1724334861713,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.90599999999993,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03771571599600001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045653777903999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "eskimor",
            "username": "eskimor",
            "email": "eskimor@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b2ec017c0e5e49f3cbf782a5255bb0f9e88bd6c1",
          "message": "Don't disconnect on invalid imports. (#5392)\n\nThere are numerous reasons for invalid imports, most of them would\nlikely be caused by bugs. On the other side, dispute distribution\nhandles all connections fairly, thus there is little harm in keeping a\nproblematic connection open.\n\n---------\n\nCo-authored-by: eskimor <eskimor@no-such-url.com>\nCo-authored-by: ordian <write@reusable.software>",
          "timestamp": "2024-08-22T14:15:18Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b2ec017c0e5e49f3cbf782a5255bb0f9e88bd6c1"
        },
        "date": 1724343468763,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93999999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045450697662,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036876070192,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Gustavo Gonzalez",
            "username": "ggonzalez94",
            "email": "ggonzalezsomer@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4ffccac4fe4d2294dfa63b20155f9c6052c69574",
          "message": "Update OpenZeppelin template documentation (#5398)\n\n# Description\nUpdates `template.rs` to reflect the two OZ templates available and a\nshort description\n\n# Checklist\n\n* [x] My PR includes a detailed description as outlined in the\n\"Description\" and its two subsections above.\n* [x] My PR follows the [labeling requirements](CONTRIBUTING.md#Process)\nof this project (at minimum one label for `T`\n  required)\n* External contributors: ask maintainers to put the right label on your\nPR.\n* [x] I have made corresponding changes to the documentation (if\napplicable)\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Shawn Tabrizi <shawntabrizi@gmail.com>",
          "timestamp": "2024-08-23T08:17:28Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4ffccac4fe4d2294dfa63b20155f9c6052c69574"
        },
        "date": 1724407885180,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40799999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92599999999995,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037112153086,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04519638542199999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Yuri Volkov",
            "username": "mutantcornholio",
            "email": "0@mcornholio.ru"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b3c2a25b73bb4854f26204068f0aec3e8577196c",
          "message": "Moving `Find FAIL-CI` check to GHA (#5377)",
          "timestamp": "2024-08-23T14:03:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b3c2a25b73bb4854f26204068f0aec3e8577196c"
        },
        "date": 1724428900516,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92399999999994,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.048009230978,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03943656596600001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "José Molina Colmenero",
            "username": "Moliholy",
            "email": "jose@blockdeep.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "475432f462450c3ca29b48066482765c87420ad3",
          "message": "pallet-collator-selection: correctly register weight in `new_session` (#5430)\n\nThe `pallet-collator-selection` is not correctly using the weight for\nthe\n[new_session](https://github.com/blockdeep/pallet-collator-staking/blob/main/src/benchmarking.rs#L350-L353)\nfunction.\n\nThe first parameter is the removed candidates, and the second one the\noriginal number of candidates before the removal, but both values are\nswapped.",
          "timestamp": "2024-08-24T22:38:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/475432f462450c3ca29b48066482765c87420ad3"
        },
        "date": 1724546302274,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93999999999994,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04498353714000002,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036768433547999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Lech Głowiak",
            "username": "LGLO",
            "email": "LGLO@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "178e699c7d9a9f399040e290943dd13873772c68",
          "message": "Skip slot before creating inherent data providers during major sync (#5344)\n\n# Description\n\nMoves `create_inherent_data_provider` after checking if major sync is in\nprogress.\n\n## Integration\n\nChange is internal to sc-consensus-slots. It should be no-op unless\nsomeone is using fork of this SDK.\n\n## Review Notes\n\nMotivation for this change is to avoid calling\n`create_inherent_data_providers` if it's result is going to be discarded\nanyway during major sync. This has potential to speed up node operations\nduring major sync by not calling possibly expensive\n`create_inherent_data_provider`.\n\nTODO: labels T0-node D0-simple\nTODO: there is no tests for `Slots`, should I add one for this case?\n\n# Checklist\n\n* [x] My PR includes a detailed description as outlined in the\n\"Description\" and its two subsections above.\n* [x] My PR follows the [labeling requirements](CONTRIBUTING.md#Process)\nof this project (at minimum one label for `T`\n  required)\n* External contributors: ask maintainers to put the right label on your\nPR.\n* [ ] I have made corresponding changes to the documentation (if\napplicable)\n* [ ] I have added tests that prove my fix is effective or that my\nfeature works (if applicable)",
          "timestamp": "2024-08-25T22:30:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/178e699c7d9a9f399040e290943dd13873772c68"
        },
        "date": 1724632008189,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92599999999993,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04554700970999998,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03678659144600001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3cbefaf197b5a5633b61d724f1b9a92fb7790ebb",
          "message": "Add build options to the srtool build step (#4956)\n\nThis PR adds possibility to set BUILD_OPTIONS to the \"Srtool Build\" step\nin the release pipeline while building runtimes.\n\nColses: https://github.com/paritytech/release-engineering/issues/213\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: EgorPopelyaev <EgorPopelyaev@users.noreply.github.com>",
          "timestamp": "2024-08-26T14:44:03Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3cbefaf197b5a5633b61d724f1b9a92fb7790ebb"
        },
        "date": 1724685650451,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.94799999999998,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.044928072862000006,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.036288010787999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Nazar Mokrynskyi",
            "username": "nazar-pc",
            "email": "nazar@mokrynskyi.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dd1aaa4713b93607804bed8adeaed6f98f3e5aef",
          "message": "Sync status refactoring (#5450)\n\nAs I was looking at the coupling between `SyncingEngine`,\n`SyncingStrategy` and individual strategies I noticed a few things that\nwere unused, redundant or awkward.\n\nThe awkward change comes from\nhttps://github.com/paritytech/substrate/pull/13700 where\n`num_connected_peers` property was added to `SyncStatus` struct just so\nit can be rendered in the informer. While convenient, the property\ndidn't really belong there and was annoyingly set to `0` in some\nstrategies and to `num_peers` in others. I have replaced that with a\nproperty on `SyncingService` that already stored necessary information\ninternally.\n\nAlso `ExtendedPeerInfo` didn't have a working `Clone` implementation due\nto lack of perfect derive in Rust and while I ended up not using it in\nthe refactoring, I included fixed implementation for it in this PR\nanyway.\n\nWhile these changes are not strictly necessary for\nhttps://github.com/paritytech/polkadot-sdk/issues/5333, they do reduce\ncoupling of syncing engine with syncing strategy, which I thought is a\ngood thing.\n\nReviewing individual commits will be the easiest as usual.\n\n---------\n\nCo-authored-by: Dmitry Markin <dmitry@markin.tech>",
          "timestamp": "2024-08-26T15:37:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dd1aaa4713b93607804bed8adeaed6f98f3e5aef"
        },
        "date": 1724694314589,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92599999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037972241566,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04704506029399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b34d4a083f71245794604fd9fa6464e714f958b1",
          "message": "[CI] Fix SemVer check base commit (#5361)\n\nAfter seeing some cases of reported changes that did not happen by the\nmerge request proposer (like\nhttps://github.com/paritytech/polkadot-sdk/pull/5339), it became clear\nthat [this](https://github.com/orgs/community/discussions/59677) is\nprobably the issue.\nThe base commit of the SemVer check CI is currently using the *latest*\nmaster commit, instead of the master commit at the time when the MR was\ncreated.\n\nTrying to get the correct base commit now. For this to be debugged, i\nhave to wait until another MR is merged into master.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-08-26T19:53:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b34d4a083f71245794604fd9fa6464e714f958b1"
        },
        "date": 1724708900399,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.94599999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03830250692200002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.046268075630000016,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Liu-Cheng Xu",
            "username": "liuchengxu",
            "email": "xuliuchengxlc@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6ecbde331ead4600536df2fba912a868ebc06625",
          "message": "Only log the propagating transactions when they are not empty (#5424)\n\nThis can make the log cleaner, especially when you specify `--log\nsync=debug`.",
          "timestamp": "2024-08-27T00:14:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6ecbde331ead4600536df2fba912a868ebc06625"
        },
        "date": 1724724687854,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999998,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.92199999999991,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046959880297999995,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03856465291800001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7e7c33453eeb14f47c6c4d0f98cc982e485edc77",
          "message": "frame-omni-bencher maintenance (#5466)\n\nChanges:\n- Set default level to `Info` again. Seems like a dependency update set\nit to something higher.\n- Fix docs to not use `--locked` since we rely on dependency bumps via\ncargo.\n- Add README with rust docs.\n- Fix bug where the node ignored `--heap-pages` argument.\n\nYou can test the `--heap-pages` bug by running this command on master\nand then on this branch. Note that it should fail because of the very\nlow heap pages arg:\n`cargo run --release --bin polkadot --features=runtime-benchmarks --\nbenchmark pallet --chain=dev --steps=10 --repeat=30\n--wasm-execution=compiled --heap-pages=8 --pallet=frame-system\n--extrinsic=\"*\"`\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: ggwpez <ggwpez@users.noreply.github.com>",
          "timestamp": "2024-08-27T10:05:15Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7e7c33453eeb14f47c6c4d0f98cc982e485edc77"
        },
        "date": 1724759886357,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40599999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93799999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.045741226266,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03757363849399998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Frazz",
            "username": "Sudo-Whodo",
            "email": "59382025+Sudo-Whodo@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7a2c5375fa4b950f9518a47c87d92bd611b1bfdc",
          "message": "Adding stkd bootnodes (#5470)\n\nOpening this PR to add our bootnodes for the IBP. These nodes are\nlocated in Santiago Chile, we own and manage the underlying hardware. If\nyou need any more information please let me know.\n\n\nCommands to test:\n\n```\n./polkadot --tmp --name \"testing-bootnode\" --chain kusama --reserved-only --reserved-nodes \"/dns/kusama.bootnode.stkd.io/tcp/30633/wss/p2p/12D3KooWJHhnF64TXSmyxNkhPkXAHtYNRy86LuvGQu1LTi5vrJCL\" --no-hardware-benchmarks\n\n./polkadot --tmp --name \"testing-bootnode\" --chain paseo --reserved-only --reserved-nodes \"/dns/paseo.bootnode.stkd.io/tcp/30633/wss/p2p/12D3KooWMdND5nwfCs5M2rfp5kyRo41BGDgD8V67rVRaB3acgZ53\" --no-hardware-benchmarks\n\n./polkadot --tmp --name \"testing-bootnode\" --chain polkadot --reserved-only --reserved-nodes \"/dns/polkadot.bootnode.stkd.io/tcp/30633/wss/p2p/12D3KooWEymrFRHz6c17YP3FAyd8kXS5gMRLgkW4U77ZJD2ZNCLZ\" --no-hardware-benchmarks\n\n./polkadot --tmp --name \"testing-bootnode\" --chain westend --reserved-only --reserved-nodes \"/dns/westend.bootnode.stkd.io/tcp/30633/wss/p2p/12D3KooWHaQKkJiTPqeNgqDcW7dfYgJxYwT8YqJMtTkueSu6378V\" --no-hardware-benchmarks\n```",
          "timestamp": "2024-08-27T16:23:17Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7a2c5375fa4b950f9518a47c87d92bd611b1bfdc"
        },
        "date": 1724782587805,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.91999999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037099808570000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045331881766,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "s0me0ne-unkn0wn",
            "username": "s0me0ne-unkn0wn",
            "email": "48632512+s0me0ne-unkn0wn@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f90bfa6ada211ef1bb55861ffbb7588b15ec21df",
          "message": "Add feature to allow Aura collator to use full PoV size (#5393)\n\nThis PR introduces a feature that allows to optionally enable using the\nfull PoV size.\n\nTechnically, we're ready to enable it by default, but as corresponding\nruntime changes have not been propagated to the system parachain\nruntimes yet, doing so could put them at risk. On the other hand, there\nare teams that could benefit from it right now, and it makes no sense\nfor them to wait for the fellowship release and everything.\n\n---------\n\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2024-08-27T18:22:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f90bfa6ada211ef1bb55861ffbb7588b15ec21df"
        },
        "date": 1724790069474,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.964,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036945338147999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04511969883999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Guillaume Thiolliere",
            "username": "gui1117",
            "email": "gui.thiolliere@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "97fa922c85976523f755acdd104fdc77ed63dae9",
          "message": "Refactor verbose test (#5506)\n\nA test is triggering a log error. But is correct and successful. This is\na refactor without triggering the log error.",
          "timestamp": "2024-08-28T14:42:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/97fa922c85976523f755acdd104fdc77ed63dae9"
        },
        "date": 1724863293474,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92399999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03706647469000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04586632426599998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Samusev",
            "username": "alvicsam",
            "email": "41779041+alvicsam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a67d6232687ab5131816b96808cda22ecb516798",
          "message": "[ci] Migrate checks to GHA (#5511)\n\nPR migrates jobs `quick-benchmarks`, `cargo-clippy`, `check-try-runtime`\nand `check-core-crypto-features` from Gitlab to GitHub\n\ncc https://github.com/paritytech/ci_cd/issues/1006\n\n---------\n\nCo-authored-by: Maksym H <1177472+mordamax@users.noreply.github.com>",
          "timestamp": "2024-08-29T09:17:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a67d6232687ab5131816b96808cda22ecb516798"
        },
        "date": 1724930079193,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93399999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046426137018,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03745865253,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dharjeezy",
            "username": "dharjeezy",
            "email": "dharjeezy@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "61bfcb846c2582e9fdf5068f91a20c107d6f030e",
          "message": "Add new try-state check invariant for nomination-pools (points >= stake) (#5465)\n\ncloses https://github.com/paritytech/polkadot-sdk/issues/5448\n\n---------\n\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: Ankan <10196091+Ank4n@users.noreply.github.com>",
          "timestamp": "2024-08-29T12:28:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/61bfcb846c2582e9fdf5068f91a20c107d6f030e"
        },
        "date": 1724941859276,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.92999999999992,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04569659666199997,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.037114517218,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Samusev",
            "username": "alvicsam",
            "email": "41779041+alvicsam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f7504cec1689850f2c93176fe81667d650217e1c",
          "message": "[ci] Move check-runtime-migration to GHA (#5519)\n\nPR moves rococo and wococo check-runtime-migration jobs to GHA\n\ncc https://github.com/paritytech/ci_cd/issues/1006",
          "timestamp": "2024-08-29T14:05:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f7504cec1689850f2c93176fe81667d650217e1c"
        },
        "date": 1724947643170,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93599999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03832411197999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.049717713508,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Sandu",
            "username": "sandreim",
            "email": "54316454+sandreim@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "09035a7d5d14fc3f2df3db304cd0fcc8fc9ed27b",
          "message": "Polkadot Primitives v8  (#5525)\n\nAs Runtime release 1.3.0 includes all of the remaining staging\nprimitives and APIs we can now release primitives version 8.\nNo other changes other than renaming/moving done here.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-08-30T10:39:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/09035a7d5d14fc3f2df3db304cd0fcc8fc9ed27b"
        },
        "date": 1725021450173,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.96599999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40799999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.03662134025199998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04556814210399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Maksym H",
            "username": "mordamax",
            "email": "1177472+mordamax@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d34f6878a337f6211a4708560e7e669d9bd7c1d6",
          "message": "fix cmd bot PR context (#5531)\n\n- restore update-ui.sh (accidentally removed with bunch of bash 😅\n- fix empty context and pushing to dev branch (supporting forks)\ntested fork here: https://github.com/paritytech-stg/polkadot-sdk/pull/45",
          "timestamp": "2024-08-30T14:05:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d34f6878a337f6211a4708560e7e669d9bd7c1d6"
        },
        "date": 1725029682224,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.96199999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.41399999999997,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.036815952868,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.045588305691999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jan-Jan",
            "username": "Jan-Jan",
            "email": "Jan-Jan@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "824e1cfa62d91635cbe3db13b2839690e3635d49",
          "message": "'remainder' instead of reminder && explicit instruction to clone (#5535)\n\n# Description\n\nTrivial doc fixes:\n\n* Replace the word `reminder` with `remainder` so that the English\nmatches the code intent.\n* Explicit instruct the reader to `clone`.\n\n## Review Notes\n\n* Trivial\n\nCo-authored-by: Jan-Jan <111935+Jan-Jan@users.noreply.github.com>",
          "timestamp": "2024-08-30T19:46:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/824e1cfa62d91635cbe3db13b2839690e3635d49"
        },
        "date": 1725054595999,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93599999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.39999999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04546816276599996,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03670466654,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ea80adfdbc477c54d1cda9f6e911f917acc3af17",
          "message": "Bump the known_good_semver group across 1 directory with 5 updates (#5460)\n\nBumps the known_good_semver group with 4 updates in the / directory:\n[quote](https://github.com/dtolnay/quote),\n[serde](https://github.com/serde-rs/serde),\n[serde_json](https://github.com/serde-rs/json) and\n[syn](https://github.com/dtolnay/syn).\n\nUpdates `quote` from 1.0.36 to 1.0.37\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/dtolnay/quote/releases\">quote's\nreleases</a>.</em></p>\n<blockquote>\n<h2>1.0.37</h2>\n<ul>\n<li>Implement ToTokens for CStr and CString (<a\nhref=\"https://redirect.github.com/dtolnay/quote/issues/283\">#283</a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/b1ebffa035363a430862e033aa3268e8cb17affa\"><code>b1ebffa</code></a>\nRelease 1.0.37</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/43acd77961424b3cb5035688f74d14d556eefe90\"><code>43acd77</code></a>\nDelete unneeded use of <code>ref</code></li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/9382c2182ea10f8e0f90d1e5f15ca3f20a777dff\"><code>9382c21</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/dtolnay/quote/issues/283\">#283</a>\nfrom dtolnay/cstr</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/6ac432877bbfe43892677e32af7e3f0e28b6333e\"><code>6ac4328</code></a>\nAdd C string tests</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/9fb0591a17893eea81260351c6eb431e1fd83524\"><code>9fb0591</code></a>\nImplement ToTokens for CStr and CString</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/ba7a9d08c9acba8ae97926dcc18822b20441c0fa\"><code>ba7a9d0</code></a>\nOrganize test imports</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/aa9970f9838a5b6dd5438c662921470f873e2b3a\"><code>aa9970f</code></a>\nInline the macro that generates primitive impls</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/ba411091c98c311526774adde73e724448836337\"><code>ba41109</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/dtolnay/quote/issues/282\">#282</a>\nfrom dtolnay/tokens</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/c77340a4c6869690ad7b40069e8ca1cb90e4abb8\"><code>c77340a</code></a>\nConsistently use 'tokens' as the name of the &amp;mut TokenStream\narg</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/a4a0abf12fa0137eca5aaa74fe88ca6694e78746\"><code>a4a0abf</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/dtolnay/quote/issues/281\">#281</a>\nfrom dtolnay/char</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/dtolnay/quote/compare/1.0.36...1.0.37\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `serde` from 1.0.206 to 1.0.209\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/serde-rs/serde/releases\">serde's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v1.0.209</h2>\n<ul>\n<li>Fix deserialization of empty structs and empty tuples inside of\nuntagged enums (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2805\">#2805</a>,\nthanks <a\nhref=\"https://github.com/Mingun\"><code>@​Mingun</code></a>)</li>\n</ul>\n<h2>v1.0.208</h2>\n<ul>\n<li>Support serializing and deserializing unit structs in a\n<code>flatten</code> field (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2802\">#2802</a>,\nthanks <a\nhref=\"https://github.com/jonhoo\"><code>@​jonhoo</code></a>)</li>\n</ul>\n<h2>v1.0.207</h2>\n<ul>\n<li>Improve interactions between <code>flatten</code> attribute and\n<code>skip_serializing</code>/<code>skip_deserializing</code> (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2795\">#2795</a>,\nthanks <a\nhref=\"https://github.com/Mingun\"><code>@​Mingun</code></a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/30752ac4ffdaa284606eda34055ad185e28c5499\"><code>30752ac</code></a>\nRelease 1.0.209</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/b84e6ca4f5fef69b3de985c586a07b1246f3eb9a\"><code>b84e6ca</code></a>\nImprove wording of PR 2805 comments</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/87a2fb0f1a2774ea5bb20c0ed988b9ba57fc8166\"><code>87a2fb0</code></a>\nWrap comments from PR 2805 to 80 columns</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/9eaf7b9824f2082c50d17ad22b786322dc283a61\"><code>9eaf7b9</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2805\">#2805</a>\nfrom Mingun/untagged-tests</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/7bde100237875d4f435de5ad90074b0479c37486\"><code>7bde100</code></a>\nReplace MapRefDeserializer with value::MapDeserializer</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/da7fc795ee654252effa232a62a5a1e6d4f551ee\"><code>da7fc79</code></a>\nFix deserialization of empty struct variant in untagged enums</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/4c5fec1363d363f995375426f72db11c28f357c1\"><code>4c5fec1</code></a>\nTest special cases that reaches SeqRefDeserializer::deserialize_any\nlen==0 co...</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/6588b0ad3777f7ad930d68ab4b9ec5b9c25398e0\"><code>6588b0a</code></a>\nCover Content::Seq case in VariantRefDeserializer::struct_variant</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/0093f74cfee5ee3239514a7aad5fb44843eddcdd\"><code>0093f74</code></a>\nSplit test newtype_enum into four tests for each variant</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/171c6da57af712cfcf01c6c124b14cabfca364ba\"><code>171c6da</code></a>\nComplete coverage of\nContentRefDeserializer::deserialize_newtype_struct</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/serde-rs/serde/compare/v1.0.206...v1.0.209\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `serde_derive` from 1.0.206 to 1.0.209\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/serde-rs/serde/releases\">serde_derive's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v1.0.209</h2>\n<ul>\n<li>Fix deserialization of empty structs and empty tuples inside of\nuntagged enums (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2805\">#2805</a>,\nthanks <a\nhref=\"https://github.com/Mingun\"><code>@​Mingun</code></a>)</li>\n</ul>\n<h2>v1.0.208</h2>\n<ul>\n<li>Support serializing and deserializing unit structs in a\n<code>flatten</code> field (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2802\">#2802</a>,\nthanks <a\nhref=\"https://github.com/jonhoo\"><code>@​jonhoo</code></a>)</li>\n</ul>\n<h2>v1.0.207</h2>\n<ul>\n<li>Improve interactions between <code>flatten</code> attribute and\n<code>skip_serializing</code>/<code>skip_deserializing</code> (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2795\">#2795</a>,\nthanks <a\nhref=\"https://github.com/Mingun\"><code>@​Mingun</code></a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/30752ac4ffdaa284606eda34055ad185e28c5499\"><code>30752ac</code></a>\nRelease 1.0.209</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/b84e6ca4f5fef69b3de985c586a07b1246f3eb9a\"><code>b84e6ca</code></a>\nImprove wording of PR 2805 comments</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/87a2fb0f1a2774ea5bb20c0ed988b9ba57fc8166\"><code>87a2fb0</code></a>\nWrap comments from PR 2805 to 80 columns</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/9eaf7b9824f2082c50d17ad22b786322dc283a61\"><code>9eaf7b9</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2805\">#2805</a>\nfrom Mingun/untagged-tests</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/7bde100237875d4f435de5ad90074b0479c37486\"><code>7bde100</code></a>\nReplace MapRefDeserializer with value::MapDeserializer</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/da7fc795ee654252effa232a62a5a1e6d4f551ee\"><code>da7fc79</code></a>\nFix deserialization of empty struct variant in untagged enums</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/4c5fec1363d363f995375426f72db11c28f357c1\"><code>4c5fec1</code></a>\nTest special cases that reaches SeqRefDeserializer::deserialize_any\nlen==0 co...</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/6588b0ad3777f7ad930d68ab4b9ec5b9c25398e0\"><code>6588b0a</code></a>\nCover Content::Seq case in VariantRefDeserializer::struct_variant</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/0093f74cfee5ee3239514a7aad5fb44843eddcdd\"><code>0093f74</code></a>\nSplit test newtype_enum into four tests for each variant</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/171c6da57af712cfcf01c6c124b14cabfca364ba\"><code>171c6da</code></a>\nComplete coverage of\nContentRefDeserializer::deserialize_newtype_struct</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/serde-rs/serde/compare/v1.0.206...v1.0.209\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `serde_json` from 1.0.124 to 1.0.127\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/serde-rs/json/releases\">serde_json's\nreleases</a>.</em></p>\n<blockquote>\n<h2>1.0.127</h2>\n<ul>\n<li>Add more removal methods to OccupiedEntry (<a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1179\">#1179</a>,\nthanks <a\nhref=\"https://github.com/GREsau\"><code>@​GREsau</code></a>)</li>\n</ul>\n<h2>1.0.126</h2>\n<ul>\n<li>Improve string parsing on targets that use 32-bit pointers but also\nhave fast 64-bit integer arithmetic, such as\naarch64-unknown-linux-gnu_ilp32 and x86_64-unknown-linux-gnux32 (<a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1182\">#1182</a>,\nthanks <a href=\"https://github.com/CryZe\"><code>@​CryZe</code></a>)</li>\n</ul>\n<h2>1.0.125</h2>\n<ul>\n<li>Speed up \\uXXXX parsing and improve handling of unpaired surrogates\nwhen deserializing to bytes (<a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1172\">#1172</a>,\n<a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1175\">#1175</a>,\nthanks <a\nhref=\"https://github.com/purplesyringa\"><code>@​purplesyringa</code></a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/5ebf65cc480f90714c94f82099ca9161d80cbb10\"><code>5ebf65c</code></a>\nRelease 1.0.127</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/f287a3b1a93ecb1a11cee31cb638bd9523a58add\"><code>f287a3b</code></a>\nMerge pull request 1179 from GREsau/patch-1</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/ec980b02774abbff12fd3e26b0a1582eb14dcef7\"><code>ec980b0</code></a>\nRelease 1.0.126</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/e6282b0c479947805a33c7f167b1d19dd4c7ad4f\"><code>e6282b0</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1184\">#1184</a>\nfrom serde-rs/fastarithmetic</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/ffc4a43453029cdc5603cfe3ef08414488fd45de\"><code>ffc4a43</code></a>\nImprove cfg names for fast arithmetic</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/4b1048d0ecc4d326d6657531689513f182a4f850\"><code>4b1048d</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1183\">#1183</a>\nfrom serde-rs/arithmetic</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/f268173a9fb1f5f8a80f47af62b564525cf33764\"><code>f268173</code></a>\nUnify chunk size choice between float and string parsing</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/fec03769743c3f0ceb6b5b56d91321fdc856dff2\"><code>fec0376</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1182\">#1182</a>\nfrom CryZe/chunk-64bit</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/3d837e1cc4a0f1df56ba6645c3b6d144768b5d9d\"><code>3d837e1</code></a>\nEnsure the SWAR chunks are 64-bit in more cases</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/11fc61c7af7b59ea80fb2ef7d78db94465dfbd54\"><code>11fc61c</code></a>\nAdd <code>OccupiedEntry::shift_remove()</code> and\n<code>swap_remove()</code></li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/serde-rs/json/compare/v1.0.124...1.0.127\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `syn` from 2.0.61 to 2.0.65\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/dtolnay/syn/releases\">syn's\nreleases</a>.</em></p>\n<blockquote>\n<h2>2.0.65</h2>\n<ul>\n<li>Optimize the implementation of <code>Fold</code> to compile faster\n(<a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1666\">#1666</a>, <a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1667\">#1667</a>, <a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1668\">#1668</a>)</li>\n</ul>\n<h2>2.0.64</h2>\n<ul>\n<li>Support using ParseBuffer across <code>catch_unwind</code> (<a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1646\">#1646</a>)</li>\n<li>Validate that the expression in a let-else ends in brace as required\nby rustc (<a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1648\">#1648</a>, <a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1649\">#1649</a>)</li>\n<li>Legalize invalid const generic arguments by wrapping in braces (<a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1654\">#1654</a>, <a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1655\">#1655</a>)</li>\n<li>Fix some expression precedence edge cases involving\n<code>break</code> and <code>return</code> in loop headers (<a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1656\">#1656</a>)</li>\n<li>Always print closure bodies with a brace when the closure has an\nexplicit return type (<a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1658\">#1658</a>)</li>\n<li>Automatically insert necessary parentheses in ToTokens for Expr when\nrequired by expression precedence (<a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1659\">#1659</a>)</li>\n<li>Support struct literal syntax in match guard expressions (<a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1662\">#1662</a>)</li>\n</ul>\n<h2>2.0.63</h2>\n<ul>\n<li>Parse and print long if-else-if chains without reliance on deep\nrecursion to avoid overflowing stack (<a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1644\">#1644</a>, <a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1645\">#1645</a>)</li>\n</ul>\n<h2>2.0.62</h2>\n<ul>\n<li>Reject invalid unparenthesized range and comparison operator\nexpressions (<a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1642\">#1642</a>, <a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1643\">#1643</a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/dtolnay/syn/commit/9f2371eefa6f681b53e4d74458d86dd41673227f\"><code>9f2371e</code></a>\nRelease 2.0.65</li>\n<li><a\nhref=\"https://github.com/dtolnay/syn/commit/4cd181325f3488c47866f15966977682be610da1\"><code>4cd1813</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1668\">#1668</a>\nfrom dtolnay/foldhelper</li>\n<li><a\nhref=\"https://github.com/dtolnay/syn/commit/ed54092bcea6798ab0b5ed7aca6755f8918fc79e\"><code>ed54092</code></a>\nEliminate gen::helper module</li>\n<li><a\nhref=\"https://github.com/dtolnay/syn/commit/eacc8ab1b98b590df3ce9462510fd755cddf6762\"><code>eacc8ab</code></a>\nEliminate FoldHelper trait</li>\n<li><a\nhref=\"https://github.com/dtolnay/syn/commit/6e20bb8d7799d0f4c34c144e80b3bd1b6e9afd27\"><code>6e20bb8</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1667\">#1667</a>\nfrom dtolnay/punctuatedfold</li>\n<li><a\nhref=\"https://github.com/dtolnay/syn/commit/9d95cab6d332d08903538d5ce3d6e47c1598912e\"><code>9d95cab</code></a>\nOptimize punctuated::fold</li>\n<li><a\nhref=\"https://github.com/dtolnay/syn/commit/82ffe86c2b721b9985edb6f368e7366bd202bc5b\"><code>82ffe86</code></a>\nMove Punctuated fold helper to punctuated module</li>\n<li><a\nhref=\"https://github.com/dtolnay/syn/commit/3dfacc1538f655d33c5c8037b14669149bcd81cd\"><code>3dfacc1</code></a>\nIgnore manual_map clippy lint</li>\n<li><a\nhref=\"https://github.com/dtolnay/syn/commit/7273aa77aa09ee2562b279a5d9495a212d9c0876\"><code>7273aa7</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/dtolnay/syn/issues/1666\">#1666</a>\nfrom dtolnay/foldhelper</li>\n<li><a\nhref=\"https://github.com/dtolnay/syn/commit/8124c0eb99e11cae036d2c967f91f0c456c50368\"><code>8124c0e</code></a>\nGenerate fewer monomorphizations in Fold</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/dtolnay/syn/compare/2.0.61...2.0.65\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore <dependency name> major version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's major version (unless you unignore this specific\ndependency's major version or upgrade to it yourself)\n- `@dependabot ignore <dependency name> minor version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's minor version (unless you unignore this specific\ndependency's minor version or upgrade to it yourself)\n- `@dependabot ignore <dependency name>` will close this group update PR\nand stop Dependabot creating any more for the specific dependency\n(unless you unignore this specific dependency or upgrade to it yourself)\n- `@dependabot unignore <dependency name>` will remove all of the ignore\nconditions of the specified dependency\n- `@dependabot unignore <dependency name> <ignore condition>` will\nremove the ignore condition of the specified dependency and ignore\nconditions\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-08-30T22:45:15Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ea80adfdbc477c54d1cda9f6e911f917acc3af17"
        },
        "date": 1725064999865,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.90599999999996,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.038369474308,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04631456604400004,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Maksym H",
            "username": "mordamax",
            "email": "1177472+mordamax@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b7d5f15aede020d65b2b9634e858dac863c0520a",
          "message": "Update cmd.yml (#5536)\n\nTiny fix for subweight diff in /cmd",
          "timestamp": "2024-08-31T11:13:52Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b7d5f15aede020d65b2b9634e858dac863c0520a"
        },
        "date": 1725110022433,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40199999999997,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.93599999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.04804965213800001,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.039167046632,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Gheorghe",
            "username": "alexggh",
            "email": "49718502+alexggh@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6b854acc69cd64f7c0e6cdb606e741e630e45032",
          "message": "[3 / 5] Move crypto checks in the approval-distribution (#4928)\n\n# Prerequisite \nThis is part of the work to further optimize the approval subsystems, if\nyou want to understand the full context start with reading\nhttps://github.com/paritytech/polkadot-sdk/pull/4849#issue-2364261568,\n\n# Description\nThis PR contain changes, so that the crypto checks are performed by the\napproval-distribution subsystem instead of the approval-voting one. The\nbenefit for these, is twofold:\n1. Approval-distribution won't have to wait every single time for the\napproval-voting to finish its job, so the work gets to be pipelined\nbetween approval-distribution and approval-voting.\n\n2. By running in parallel multiple instances of approval-distribution as\ndescribed here\nhttps://github.com/paritytech/polkadot-sdk/pull/4849#issue-2364261568,\nthis significant body of work gets to run in parallel.\n\n## Changes:\n1. When approval-voting send `ApprovalDistributionMessage::NewBlocks` it\nneeds to pass the core_index and candidate_hash of the candidates.\n2. ApprovalDistribution needs to use `RuntimeInfo` to be able to fetch\nthe SessionInfo from the runtime.\n3. Move `approval-voting` logic that checks VRF assignment into\n`approval-distribution`\n4. Move `approval-voting` logic that checks vote is correctly signed\ninto `approval-distribution`\n5. Plumb `approval-distribution` and `approval-voting` tests to support\nthe new logic.\n\n## Benefits\nEven without parallelisation the gains are significant, for example on\nmy machine if we run approval subsystem bench for 500 validators and 100\ncores and trigger all 89 tranches of assignments and approvals, the\nsystem won't fall behind anymore because of late processing of messages.\n```\nBefore change\nChain selection approved  after 11500 ms hash=0x0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a\n\nAfter change\n\nChain selection approved  after 5500 ms hash=0x0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a\n```\n\n## TODO:\n- [x] Run on versi.\n- [x] Update parachain host documentation.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-09-02T09:05:03Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6b854acc69cd64f7c0e6cdb606e741e630e45032"
        },
        "date": 1725274382466,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 106.40399999999995,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 127.91399999999994,
            "unit": "KiB"
          },
          {
            "name": "statement-distribution",
            "value": 0.037595121780000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.04608136284999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5291412e159d3b99c64d5f7f969dbde39d715769",
          "message": "Swaps for XCM delivery fees (#5131)\n\n# Context\n\nFees can already be paid in other assets locally thanks to the Trader\nimplementations we have.\nThis doesn't work when sending messages because delivery fees go through\na different mechanism altogether.\nThe idea is to fix this leveraging the `AssetExchanger` config item\nthat's able to turn the asset the user wants to pay fees in into the\nasset the router expects for delivery fees.\n\n# Main addition\n\nAn adapter was needed to use `pallet-asset-conversion` for exchanging\nassets in XCM.\nThis was created in\nhttps://github.com/paritytech/polkadot-sdk/pull/5130.\n\nThe XCM executor was modified to use `AssetExchanger` (when available)\nto swap assets to pay for delivery fees.\n\n## Limitations\n\nWe can only pay for delivery fees in different assets in intermediate\nhops. We can't pay in different assets locally. The first hop will\nalways need the native token of the chain (or whatever is specified in\nthe `XcmRouter`).\nThis is a byproduct of using the `BuyExecution` instruction to know\nwhich asset should be used for delivery fee payment.\nSince this instruction is not present when executing an XCM locally, we\nare left with this limitation.\nTo illustrate this limitation, I'll show two scenarios. All chains\ninvolved have pools.\n\n### Scenario 1\n\nParachain A --> Parachain B\n\nHere, parachain A can use any asset in a pool with its native asset to\npay for local execution fees.\nHowever, as of now we can't use those for local delivery fees.\nThis means transfers from A to B need some amount of A's native token to\npay for delivery fees.\n\n### Scenario 2\n\nParachain A --> Parachain C --> Parachain B\n\nHere, Parachain C's remote delivery fees can be paid with any asset in a\npool with its native asset.\nThis allows a reserve asset transfer between A and B with C as the\nreserve to only need A's native token at the starting hop.\nAfter that, it could all be pool assets.\n\n## Future work\n\nThe fact that delivery fees go through a totally different mechanism\nresults in a lot of bugs and pain points.\nUnfortunately, this is not so easy to solve in a backwards compatible\nmanner.\nDelivery fees will be integrated into the language in future XCM\nversions, following\nhttps://github.com/polkadot-fellows/xcm-format/pull/53.\n\nOld PR: https://github.com/paritytech/polkadot-sdk/pull/4375.",
          "timestamp": "2024-09-02T10:47:13Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5291412e159d3b99c64d5f7f969dbde39d715769"
        },
        "date": 1725280582976,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 127.93399999999991,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 106.40599999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.046216483624000014,
            "unit": "seconds"
          },
          {
            "name": "statement-distribution",
            "value": 0.03774881882199999,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}