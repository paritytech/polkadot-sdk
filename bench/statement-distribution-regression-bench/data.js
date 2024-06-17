window.BENCHMARK_DATA = {
  "lastUpdate": 1718644057934,
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
      }
    ]
  }
}