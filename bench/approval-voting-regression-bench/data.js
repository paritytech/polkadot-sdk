window.BENCHMARK_DATA = {
  "lastUpdate": 1714129078739,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "approval-voting-regression-bench": [
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
          "id": "3380e21cd92690c2066f686164a954ba7cd17244",
          "message": "Use default branch of `psvm` when synchronizing templates (#4240)\n\nWe cannot lock to a specific version of `psvm`, because we will need to\nkeep it up-to-date - each release currently requires a change in `psvm`\nsuch as [this one](https://github.com/paritytech/psvm/pull/2/files).\n\nThere is no `stable` branch in `psvm` repo or anything so using the\ndefault branch.",
          "timestamp": "2024-04-22T16:34:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3380e21cd92690c2066f686164a954ba7cd17244"
        },
        "date": 1713807753741,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63548.46,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.115701152709936,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.713580235960006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0335909815501774,
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
          "id": "bd9287f766bded2022036a63d12fb86a2f7174a0",
          "message": "wasm-builder: Make it easier to build a WASM binary (#4177)\n\nBasically combines all the recommended calls into one\n`build_using_defaults()` call or `init_with_defaults()` when there are\nsome custom changes required.",
          "timestamp": "2024-04-22T19:28:27Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bd9287f766bded2022036a63d12fb86a2f7174a0"
        },
        "date": 1713819780244,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63542.530000000006,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52938.09999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.7648006330202373,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.260694753859994,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.841950453610078,
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
          "id": "84c294c3821baf8b81693ce6e5615b9e157b5303",
          "message": "[testnets] remove XCM SafeCallFilter for chains using Weights::v3 (#4199)\n\nWeights::v3 also accounts for PoV weight so we no longer need the\nSafeCallFilter. All calls are allowed as long as they \"fit in the\nblock\".",
          "timestamp": "2024-04-22T22:10:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/84c294c3821baf8b81693ce6e5615b9e157b5303"
        },
        "date": 1713828841439,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63553.490000000005,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52945.40000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.973300304849985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.903687700000238,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.496413773119922,
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
          "id": "7f1646eb3837bfa53fb1cb8eabd7a0e1026469b8",
          "message": "Add `validate_xcm_nesting` to the `ParentAsUmp` and `ChildParachainRouter` (#4236)\n\nThis PR:\n- moves `validate_xcm_nesting` from `XcmpQueue` into the `VersionedXcm`\n- adds `validate_xcm_nesting` to the `ParentAsUmp`\n- adds `validate_xcm_nesting` to the `ChildParachainRouter`\n\n\nBased on discussion\n[here](https://github.com/paritytech/polkadot-sdk/pull/4186#discussion_r1571344270)\nand/or\n[here](https://github.com/paritytech/polkadot-sdk/pull/4186#discussion_r1572076666)\nand/or [here]()\n\n## Question/TODO\n\n- [x] To the\n[comment](https://github.com/paritytech/polkadot-sdk/pull/4186#discussion_r1572072295)\n- Why was `validate_xcm_nesting` added just to the `XcmpQueue` router\nand nowhere else? What kind of problem `MAX_XCM_DECODE_DEPTH` is\nsolving? (see\n[comment](https://github.com/paritytech/polkadot-sdk/pull/4236#discussion_r1574605191))",
          "timestamp": "2024-04-23T08:38:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7f1646eb3837bfa53fb1cb8eabd7a0e1026469b8"
        },
        "date": 1713867196476,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63547.780000000006,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.292572380559976,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.902055862849888,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.148137250190042,
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
          "id": "118cd6f922acc9c4b3938645cd34098275d41c93",
          "message": "Ensure outbound XCMs are decodable with limits + add `EnsureDecodableXcm` router (for testing purposes) (#4186)\n\nThis PR:\n- adds `EnsureDecodableXcm` (testing) router that attempts to *encode*\nand *decode* passed XCM `message` to ensure that the receiving side will\nbe able to decode, at least with the same XCM version.\n- fixes `pallet_xcm` / `pallet_xcm_benchmarks` assets data generation\n\nRelates to investigation of\nhttps://substrate.stackexchange.com/questions/11288 and missing fix\nhttps://github.com/paritytech/polkadot-sdk/pull/2129 which did not get\ninto the fellows 1.1.X release.\n\n## Questions/TODOs\n\n- [x] fix XCM benchmarks, which produces undecodable data - new router\ncatched at least two cases\n  - `BoundedVec exceeds its limit`\n  - `Fungible asset of zero amount is not allowed`  \n- [x] do we need to add `sort` to the `prepend_with` as we did for\nreanchor [here](https://github.com/paritytech/polkadot-sdk/pull/2129)?\n@serban300 (**created separate/follow-up PR**:\nhttps://github.com/paritytech/polkadot-sdk/pull/4235)\n- [x] We added decoding check to `XcmpQueue` -> `validate_xcm_nesting`,\nwhy not to added to the `ParentAsUmp` or `ChildParachainRouter`?\n@franciscoaguirre (**created separate/follow-up PR**:\nhttps://github.com/paritytech/polkadot-sdk/pull/4236)\n- [ ] `SendController::send_blob` replace `VersionedXcm::<()>::decode(`\nwith `VersionedXcm::<()>::decode_with_depth_limit(MAX_XCM_DECODE_DEPTH,\ndata)` ?\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-04-23T11:40:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/118cd6f922acc9c4b3938645cd34098275d41c93"
        },
        "date": 1713877568236,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63545.259999999995,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.460571035949991,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.99426986811998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.982703402650149,
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
          "id": "eda5e5c31f9bffafd6afd6d14fb95001a10dba9a",
          "message": "Fix Stuck Collator Funds (#4229)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/4206\n\nIn #1340 one of the storage types was changed from `Candidates` to\n`CandidateList`. Since the actual key includes the hash of this value,\nall of the candidates stored here are (a) \"missing\" and (b) unable to\nunreserve their candidacy bond.\n\nThis migration kills the storage values and refunds the deposit held for\neach candidate.\n\n---------\n\nSigned-off-by: georgepisaltu <george.pisaltu@parity.io>\nCo-authored-by: georgepisaltu <52418509+georgepisaltu@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: georgepisaltu <george.pisaltu@parity.io>",
          "timestamp": "2024-04-23T12:53:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/eda5e5c31f9bffafd6afd6d14fb95001a10dba9a"
        },
        "date": 1713882048731,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.55,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.171994607919993,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.915962875349962,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0689954355900957,
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
          "id": "ffbce2a817ec2e7c8b7ce49f7ed6794584f19667",
          "message": "pallet_broker: Let `start_sales` calculate and request the correct core count (#4221)",
          "timestamp": "2024-04-23T15:37:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ffbce2a817ec2e7c8b7ce49f7ed6794584f19667"
        },
        "date": 1713892035366,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.10000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.949043325160032,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.407199724850006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9184529612400936,
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
          "id": "9a0049d0da59b8b842f64fae441b34dba3408430",
          "message": "Plumbing to increase pvf workers configuration based on chain id (#4252)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/4126 we want\nto safely increase the execute_workers_max_num gradually from chain to\nchain and assess if there are any negative impacts.\n\nThis PR performs the necessary plumbing to be able to increase it based\non the chain id, it increase the number of execution workers from 2 to 4\non test network but lives kusama and polkadot unchanged until we gather\nmore data.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-24T06:15:39Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9a0049d0da59b8b842f64fae441b34dba3408430"
        },
        "date": 1713944341225,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63547.89,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.8,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.1493058733800927,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.276675537590025,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.805114786339908,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Kalankhodzhaev",
            "username": "kalaninja",
            "email": "kalansoft@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c594b10a803e218f63c1bd97d2b27454efb4e852",
          "message": "Remove unnecessary cloning (#4263)\n\nSeems like Externalities already [return a\nvector](https://github.com/paritytech/polkadot-sdk/blob/ffbce2a817ec2e7c8b7ce49f7ed6794584f19667/substrate/primitives/externalities/src/lib.rs#L86),\nso calling `to_vec` on a vector just results in an unneeded copying.\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-04-24T09:30:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c594b10a803e218f63c1bd97d2b27454efb4e852"
        },
        "date": 1713953792965,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63549.409999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.7,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.8353009554801347,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.856177245070033,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.397069171060004,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Kalankhodzhaev",
            "username": "kalaninja",
            "email": "kalansoft@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c594b10a803e218f63c1bd97d2b27454efb4e852",
          "message": "Remove unnecessary cloning (#4263)\n\nSeems like Externalities already [return a\nvector](https://github.com/paritytech/polkadot-sdk/blob/ffbce2a817ec2e7c8b7ce49f7ed6794584f19667/substrate/primitives/externalities/src/lib.rs#L86),\nso calling `to_vec` on a vector just results in an unneeded copying.\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-04-24T09:30:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c594b10a803e218f63c1bd97d2b27454efb4e852"
        },
        "date": 1713956713508,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63545.829999999994,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.995785177580109,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.020310992390042,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.48648768921989,
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
          "id": "4f3d43a0c4e75caf73c1034a85590f81a9ae3809",
          "message": "Revert `execute_blob` and `send_blob` (#4266)\n\nRevert \"pallet-xcm: Deprecate `execute` and `send` in favor of\n`execute_blob` and `send_blob` (#3749)\"\n\nThis reverts commit feee773d15d5237765b520b03854d46652181de5.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Javier Bullrich <javier@bullrich.dev>",
          "timestamp": "2024-04-24T15:49:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4f3d43a0c4e75caf73c1034a85590f81a9ae3809"
        },
        "date": 1713975617529,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63541.6,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.8124763528901857,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.321571008640008,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.776342704840073,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a633e954f3b88697aa797d9792e8a5b5cf310b7e",
          "message": "Bridge: make some headers submissions free (#4102)\n\nsupersedes https://github.com/paritytech/parity-bridges-common/pull/2873\n\nDraft because of couple of TODOs:\n- [x] fix remaining TODOs;\n- [x] double check that all changes from\nhttps://github.com/paritytech/parity-bridges-common/pull/2873 are\ncorrectly ported;\n- [x] create a separate PR (on top of that one or a follow up?) for\nhttps://github.com/paritytech/polkadot-sdk/tree/sv-try-new-bridge-fees;\n- [x] fix compilation issues (haven't checked, but there should be\nmany).\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-04-25T05:26:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a633e954f3b88697aa797d9792e8a5b5cf310b7e"
        },
        "date": 1714028234524,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63548.42,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.2,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.952581344790132,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.005963712880008,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.558739944510005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7e68b2b8da9caf634ff4f6c6d96d2d7914c44fb7",
          "message": "Bridge: added free headers submission support to the substrate-relay (#4157)\n\nOriginal PR:\nhttps://github.com/paritytech/parity-bridges-common/pull/2884. Since\nchain-specific code lives in the `parity-bridges-common` repo, some\nparts of original PR will require another PR\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-04-25T07:20:17Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7e68b2b8da9caf634ff4f6c6d96d2d7914c44fb7"
        },
        "date": 1714034875340,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63550.37000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.681745372810052,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.0874637672599565,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0183881767801024,
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
          "id": "077041788070eddc6f3c1043b9cb6146585b1469",
          "message": "[XCM] Treat recursion limit error as transient in the MQ (#4202)\n\nChanges:\n- Add new error variant `ProcessMessageError::StackLimitReached` and\ntreat XCM error `ExceedsStackLimit` as such.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-04-25T09:01:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/077041788070eddc6f3c1043b9cb6146585b1469"
        },
        "date": 1714041097066,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.030000000006,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.029960095840012,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9907601420701004,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.597790111489985,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Liam Aharon",
            "username": "liamaharon",
            "email": "liam.aharon@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ff2b178206f9952c3337638659450c67fd700e7e",
          "message": "remote-externalities: retry get child keys query (#4280)",
          "timestamp": "2024-04-25T12:01:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ff2b178206f9952c3337638659450c67fd700e7e"
        },
        "date": 1714052022179,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63543.95,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.984516114180022,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6014633912001477,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.608732732140043,
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
          "id": "c9923cd7feb9e7c6337f0942abd3279468df5559",
          "message": "rename fragment_tree folder to fragment_chain (#4294)\n\nMakes https://github.com/paritytech/polkadot-sdk/pull/4035 easier to\nreview",
          "timestamp": "2024-04-25T13:52:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c9923cd7feb9e7c6337f0942abd3279468df5559"
        },
        "date": 1714058632489,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63554.06999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.151423785680008,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.747454451830055,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.11348499832016,
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
          "id": "dd5b06e622c6c5c301a1554286ec1f4995c7daca",
          "message": "[subsystem-benchmarks] Log standart deviation for subsystem-benchmarks (#4285)\n\nShould help us to understand more what's happening between individual\nruns and possibly adjust the number of runs",
          "timestamp": "2024-04-25T15:06:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dd5b06e622c6c5c301a1554286ec1f4995c7daca"
        },
        "date": 1714062819511,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63551.020000000004,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.851173004009993,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.458606011900036,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.498016354730205,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Noah Jelich",
            "username": "njelich",
            "email": "12912633+njelich@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8f8c49deffe56567ba5cde0e1047de15b660bb0e",
          "message": "Fix bad links (#4231)\n\nThe solochain template links to parachain template instead of solochain.",
          "timestamp": "2024-04-26T07:03:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8f8c49deffe56567ba5cde0e1047de15b660bb0e"
        },
        "date": 1714118784396,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52936.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.170000000006,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.083771634440125,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.072660863429943,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.883378100330038,
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
          "id": "e8f7c81db66abb40802c582c22041aa63c78ddff",
          "message": "[balances] Safeguard against consumer ref underflow (#3865)\n\nThere are some accounts that do not have a consumer ref while having a\nreserve.\nThis adds a fail-safe mechanism to trigger in the case that\n`does_consume` is true, but the assumption of `consumer>0` is not.\n\nThis should prevent those accounts from loosing balance and the TI from\ngetting messed up even more, but is not an \"ideal\" fix. TBH an ideal fix\nis not possible, since on-chain data is in an invalid state.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-26T08:16:03Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e8f7c81db66abb40802c582c22041aa63c78ddff"
        },
        "date": 1714124623493,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63547.4,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.3101875874600886,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.4809040582500455,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.033078273380081,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c66d8a84687f5d68c0192122aa513b4b340794ca",
          "message": "Bump bridges relay version + uncomment bridges zombeinet tests (#4289)\n\nTODOs:\n- [x] wait and see if test `1` works;\n- [x] ~think of whether we need remaining tests.~ I think we should keep\nit - will try to revive and update it",
          "timestamp": "2024-04-26T09:24:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c66d8a84687f5d68c0192122aa513b4b340794ca"
        },
        "date": 1714129052884,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63545.29,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.912297464300147,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2215600110300797,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.343937647910001,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}