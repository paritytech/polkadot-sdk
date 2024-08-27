window.BENCHMARK_DATA = {
  "lastUpdate": 1724781720081,
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
      },
      {
        "commit": {
          "author": {
            "name": "gui",
            "username": "thiolliere",
            "email": "gui.thiolliere@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "97f74253387ee43e30c25fd970b5ae4cc1a722d7",
          "message": "Try state: log errors instead of loggin the number of error and discarding them (#4265)\n\nCurrently we discard errors content\nWe should at least log it.\n\nCode now is more similar to what is written in try_on_runtime_upgrade.\n\nlabel should be R0\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Javier Bullrich <javier@bullrich.dev>",
          "timestamp": "2024-04-26T12:27:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/97f74253387ee43e30c25fd970b5ae4cc1a722d7"
        },
        "date": 1714139638695,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52945.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.43999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.1441507093600807,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.235885624789934,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.77721540583996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tsvetomir Dimitrov",
            "username": "tdimitrov",
            "email": "tsvetomir@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "988e30f102b155ab68d664d62ac5c73da171659a",
          "message": "Implementation of the new validator disabling strategy (#2226)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/1966,\nhttps://github.com/paritytech/polkadot-sdk/issues/1963 and\nhttps://github.com/paritytech/polkadot-sdk/issues/1962.\n\nDisabling strategy specification\n[here](https://github.com/paritytech/polkadot-sdk/pull/2955). (Updated\n13/02/2024)\n\nImplements:\n* validator disabling for a whole era instead of just a session\n* no more than 1/3 of the validators in the active set are disabled\nRemoves:\n* `DisableStrategy` enum - now each validator committing an offence is\ndisabled.\n* New era is not forced if too many validators are disabled.\n\nBefore this PR not all offenders were disabled. A decision was made\nbased on [`enum\nDisableStrategy`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/primitives/staking/src/offence.rs#L54).\nSome offenders were disabled for a whole era, some just for a session,\nsome were not disabled at all.\n\nThis PR changes the disabling behaviour. Now a validator committing an\noffense is disabled immediately till the end of the current era.\n\nSome implementation notes:\n* `OffendingValidators` in pallet session keeps all offenders (this is\nnot changed). However its type is changed from `Vec<(u32, bool)>` to\n`Vec<u32>`. The reason is simple - each offender is getting disabled so\nthe bool doesn't make sense anymore.\n* When a validator is disabled it is first added to\n`OffendingValidators` and then to `DisabledValidators`. This is done in\n[`add_offending_validator`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/frame/staking/src/slashing.rs#L325)\nfrom staking pallet.\n* In\n[`rotate_session`](https://github.com/paritytech/polkadot-sdk/blob/bdbe98297032e21a553bf191c530690b1d591405/substrate/frame/session/src/lib.rs#L623)\nthe `end_session` also calls\n[`end_era`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/frame/staking/src/pallet/impls.rs#L490)\nwhen an era ends. In this case `OffendingValidators` are cleared\n**(1)**.\n* Then in\n[`rotate_session`](https://github.com/paritytech/polkadot-sdk/blob/bdbe98297032e21a553bf191c530690b1d591405/substrate/frame/session/src/lib.rs#L623)\n`DisabledValidators` are cleared **(2)**\n* And finally (still in `rotate_session`) a call to\n[`start_session`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/frame/staking/src/pallet/impls.rs#L430)\nrepopulates the disabled validators **(3)**.\n* The reason for this complication is that session pallet knows nothing\nabut eras. To overcome this on each new session the disabled list is\nrepopulated (points 2 and 3). Staking pallet knows when a new era starts\nso with point 1 it ensures that the offenders list is cleared.\n\n---------\n\nCo-authored-by: ordian <noreply@reusable.software>\nCo-authored-by: ordian <write@reusable.software>\nCo-authored-by: Maciej <maciej.zyszkiewicz@parity.io>\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: command-bot <>\nCo-authored-by: Ankan <10196091+Ank4n@users.noreply.github.com>",
          "timestamp": "2024-04-26T13:28:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/988e30f102b155ab68d664d62ac5c73da171659a"
        },
        "date": 1714143970510,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.969999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.7,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.517011993659924,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.952060624670001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9219918873501807,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "antiyro",
            "username": "antiyro",
            "email": "74653697+antiyro@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2a497d297575947b613fe0f3bbac9273a48fd6b0",
          "message": "fix(seal): shameless fix on sealing typo (#4304)",
          "timestamp": "2024-04-26T16:23:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2a497d297575947b613fe0f3bbac9273a48fd6b0"
        },
        "date": 1714151706848,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.969999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.7,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.517011993659924,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.952060624670001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9219918873501807,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "antiyro",
            "username": "antiyro",
            "email": "74653697+antiyro@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2a497d297575947b613fe0f3bbac9273a48fd6b0",
          "message": "fix(seal): shameless fix on sealing typo (#4304)",
          "timestamp": "2024-04-26T16:23:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2a497d297575947b613fe0f3bbac9273a48fd6b0"
        },
        "date": 1714153799713,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63554.759999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.8,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.3914274698899955,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.932334918190048,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1972823707301616,
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
          "id": "73b9a8391fa0b18308fa35f905e31cec77f5618f",
          "message": "[Staking] Runtime api if era rewards are pending to be claimed (#4301)\n\ncloses https://github.com/paritytech/polkadot-sdk/issues/426.\nrelated to https://github.com/paritytech/polkadot-sdk/pull/1189.\n\nWould help offchain programs to query if there are unclaimed pages of\nrewards for a given era.\n\nThe logic could look like below\n\n```js\n// loop as long as all era pages are claimed.\nwhile (api.call.stakingApi.pendingRewards(era, validator_stash)) {\n  api.tx.staking.payout_stakers(validator_stash, era)\n}\n```",
          "timestamp": "2024-04-28T12:35:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/73b9a8391fa0b18308fa35f905e31cec77f5618f"
        },
        "date": 1714313471528,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63544.81000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.917013953590134,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.296032320599975,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6498849975701217,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Squirrel",
            "username": "gilescope",
            "email": "gilescope@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "954150f3b5fdb7d07d1ed01b138e2025245bb227",
          "message": "remove unnessisary use statements due to 2021 core prelude (#4183)\n\nSome traits are already included in the 2021 prelude and so shouldn't be\nneeded to use explicitly:\n\nuse `convert::TryFrom`, `convert::TryInto`, and `iter::FromIterator` are\nremoved.\n\n( https://doc.rust-lang.org/core/prelude/rust_2021/ )\n\nNo breaking changes or change of functionality, so I think no PR doc is\nneeded in this case.\n\n(Motivation: Removes some references to `sp-std`)",
          "timestamp": "2024-04-28T15:29:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/954150f3b5fdb7d07d1ed01b138e2025245bb227"
        },
        "date": 1714323513319,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63547.02999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.8,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.573588295469973,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.191013673789955,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.359205705610195,
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
          "id": "92a348f57deed44789511df73d3fbbbcb58d98cb",
          "message": "Bump snow from 0.9.3 to 0.9.6 (#4061)\n\nBumps [snow](https://github.com/mcginty/snow) from 0.9.3 to 0.9.6.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/mcginty/snow/releases\">snow's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v0.9.6</h2>\n<ul>\n<li>Validate invalid PSK positions when building a Noise protocol.</li>\n<li>Raise errors in various typos/mistakes in Noise patterns when\nparsing.</li>\n<li>Deprecate the <code>sodiumoxide</code> backend, as that crate is no\nlonger maintained. We may eventually migrate it to a maintaned version\nof the crate, but for now it's best to warn users.</li>\n<li>Set a hard limit in <code>read_message()</code> in transport mode to\n65535 to be fully compliant with the Noise specification.</li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/mcginty/snow/compare/v0.9.5...v0.9.6\">https://github.com/mcginty/snow/compare/v0.9.5...v0.9.6</a></p>\n<h2>v0.9.5</h2>\n<p>This is a security release that fixes a logic flaw in decryption in\n<code>TransportState</code> (i.e. the stateful one), where the nonce\ncould increase even when decryption failed, which can cause a desync\nbetween the sender and receiver, opening this up as a denial of service\nvector if the attacker has the ability to inject packets in the channel\nNoise is talking over.</p>\n<p>More details can be found in the advisory: <a\nhref=\"https://github.com/mcginty/snow/security/advisories/GHSA-7g9j-g5jg-3vv3\">https://github.com/mcginty/snow/security/advisories/GHSA-7g9j-g5jg-3vv3</a></p>\n<p>All users are encouraged to update.</p>\n<h2>v0.9.4</h2>\n<p>This is a dependency version bump release because a couple of\nimportant dependencies released new versions that needed a\n<code>Cargo.toml</code> bump:</p>\n<ul>\n<li><code>ring</code> 0.17</li>\n<li><code>pqcrypto-kyber</code> 0.8</li>\n<li><code>aes-gcm</code> 0.10</li>\n<li><code>chacha20poly1305</code> 0.10</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/a4be73faa042c5967f39662aa66919f774831a9a\"><code>a4be73f</code></a>\nmeta: v0.9.6 release</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/9e53dcf5bbea869b5e3e9ed26866d683906bc848\"><code>9e53dcf</code></a>\nTransportState: limit read_message size to 65535</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/faf05609e19f4106cd47b78123415dfeb9330861\"><code>faf0560</code></a>\nDeprecate sodiumoxide resolver</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/308a24d23da13cb01a173f0ec23f140898801fb9\"><code>308a24d</code></a>\nAdd warnings about multiple calls to same method in Builder</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/f280991ae408685d72e098545314f2be160e57f9\"><code>f280991</code></a>\nError when extraneous parameters are included in string to parse</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/dbdcc4803aae6e5d9910163a7d52e0df8def4310\"><code>dbdcc48</code></a>\nError on duplicate modifiers in parameter string</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/8b1a819c93ceae98f9ba0a1be192fa61fdec78c2\"><code>8b1a819</code></a>\nValidate PSK index in pattern to avoid panic</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/74e30cf591d6d89c8a1670ee713ecc4e9607e38f\"><code>74e30cf</code></a>\nmeta: v0.9.5 release</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/12e8ae55547ae297d5f70599e5c884ea891303eb\"><code>12e8ae5</code></a>\nStateful nonce desync fix</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/02c26b7551cb7e221792a9d3d3a94730e6a34e8a\"><code>02c26b7</code></a>\nRemove clap from simple example</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/mcginty/snow/compare/v0.9.3...v0.9.6\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=snow&package-manager=cargo&previous-version=0.9.3&new-version=0.9.6)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\nYou can disable automated security fix PRs for this repo from the\n[Security Alerts\npage](https://github.com/paritytech/polkadot-sdk/network/alerts).\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-04-28T16:36:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/92a348f57deed44789511df73d3fbbbcb58d98cb"
        },
        "date": 1714327359091,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52936.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63541.27,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.602611301369866,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.044108083460036,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9723941646101464,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tin Chung",
            "username": "chungquantin",
            "email": "56880684+chungquantin@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f34d8e3cf033e2a22a41b505c437972a5dc83d78",
          "message": "Remove hard-coded indices from pallet-xcm tests (#4248)\n\n# ISSUE\n- Link to issue: https://github.com/paritytech/polkadot-sdk/issues/4237\n\n# DESCRIPTION\nRemove all ModuleError with hard-coded indices to pallet Error. For\nexample:\n```rs\nErr(DispatchError::Module(ModuleError {\n\tindex: 4,\n\terror: [2, 0, 0, 0],\n\tmessage: Some(\"Filtered\")\n}))\n```\nTo \n```rs\nlet expected_result = Err(crate::Error::<Test>::Filtered.into());\nassert_eq!(result, expected_result);\n```\n# TEST OUTCOME\n```\ntest result: ok. 74 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s\n```\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-29T07:13:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f34d8e3cf033e2a22a41b505c437972a5dc83d78"
        },
        "date": 1714380483675,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63548.65000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.09999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.225093080390157,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.384155654220022,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.977357872389947,
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
          "id": "0031d49d1ec083c62a4e2b5bf594b7f45f84ab0d",
          "message": "[Staking] Not allow reap stash for virtual stakers (#4311)\n\nRelated to https://github.com/paritytech/polkadot-sdk/pull/3905.\n\nSince virtual stakers does not have a real balance, they should not be\nallowed to be reaped.\n\nThe proper reaping for agents slashed will be done in a separate PR.",
          "timestamp": "2024-04-29T15:55:45Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0031d49d1ec083c62a4e2b5bf594b7f45f84ab0d"
        },
        "date": 1714411492071,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63557.02999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.464848092050062,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.8619582053599455,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.511399431040174,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Shawn Tabrizi",
            "username": "shawntabrizi",
            "email": "shawntabrizi@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4875ea11aeef4f3fc7d724940e5ffb703830619b",
          "message": "Refactor XCM Simulator Example (#4220)\n\nThis PR does a \"developer experience\" refactor of the XCM Simulator\nExample.\n\nI was looking for existing code / documentation where developers could\nbetter learn about working with and configuring XCM.\n\nThe XCM Simulator was a natural starting point due to the fact that it\ncan emulate end to end XCM scenarios, without needing to spawn multiple\nreal chains.\n\nHowever, the XCM Simulator Example was just 3 giant files with a ton of\nconfigurations, runtime, pallets, and tests mashed together.\n\nThis PR breaks down the XCM Simulator Example in a way that I believe is\nmore approachable by a new developer who is looking to navigate the\nvarious components of the end to end example, and modify it themselves.\n\nThe basic structure is:\n\n- xcm simulator example\n    - lib (tries to only use the xcm simulator macros)\n    - tests\n    - relay-chain\n        - mod (basic runtime that developers should be familiar with)\n        - xcm-config\n            - mod (contains the `XcmConfig` type\n            - various files for each custom configuration  \n    - parachain\n        - mock_msg_queue (custom pallet for simulator example)\n        - mod (basic runtime that developers should be familiar with)\n        - xcm-config\n            - mod (contains the `XcmConfig` type\n            - various files for each custom configuration\n\nI would like to add more documentation to this too, but I think this is\na first step to be accepted which will affect how documentation is added\nto the example\n\n---------\n\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-29T21:22:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4875ea11aeef4f3fc7d724940e5ffb703830619b"
        },
        "date": 1714431076131,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63541.07000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.790374835590022,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.318418332620022,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.7899848660101525,
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
          "id": "31dc8bb1de9a73c57863c4698ea23559ef729f67",
          "message": "Improvements in minimal template (#4119)\n\nThis PR makes a few improvements in the docs for the minimal template.\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-30T05:39:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/31dc8bb1de9a73c57863c4698ea23559ef729f67"
        },
        "date": 1714458844257,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63543.29,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.010989594540124,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3309497785101003,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.4801723087899985,
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
          "id": "c973fe86f8c668462186c95655a58fda04508e9a",
          "message": "Contracts: revert reverted changes from 4266 (#4277)\n\nrevert some reverted changes from #4266",
          "timestamp": "2024-04-30T14:29:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c973fe86f8c668462186c95655a58fda04508e9a"
        },
        "date": 1714492540637,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.72000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.248916427840134,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.3258713197000755,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.639183161030006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Maciej",
            "username": "Overkillus",
            "email": "maciej.zyszkiewicz@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6d392c7eea496e0874a9ea37f4a8ea447ebc330e",
          "message": "Statement Distribution Per Peer Rate Limit (#3444)\n\n- [x] Drop requests from a PeerID that is already being served by us.\n- [x] Don't sent requests to a PeerID if we already are requesting\nsomething from them at that moment (prioritise other requests or wait).\n- [x] Tests\n- [ ] ~~Add a small rep update for unsolicited requests (same peer\nrequest)~~ not included in original PR due to potential issues with\nnodes slowly updating\n- [x] Add a metric to track the amount of dropped requests due to peer\nrate limiting\n- [x] Add a metric to track how many time a node reaches the max\nparallel requests limit in v2+\n\nHelps with but does not close yet:\nhttps://github.com/paritytech-secops/srlabs_findings/issues/303",
          "timestamp": "2024-05-01T17:17:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6d392c7eea496e0874a9ea37f4a8ea447ebc330e"
        },
        "date": 1714589132553,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.420000000006,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.589795507319994,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.116679865599979,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.309011316340095,
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
          "id": "e5a93fbcd4a6acec7ab83865708e5c5df3534a7b",
          "message": "HRMP - set `DefaultChannelSizeAndCapacityWithSystem` with dynamic values according to the `ActiveConfig` (#4332)\n\n## Summary\nThis PR enhances the capability to set\n`DefaultChannelSizeAndCapacityWithSystem` for HRMP. Currently, all\ntestnets (Rococo, Westend) have a hard-coded value set as 'half of the\nmaximum' determined by the live `ActiveConfig`. While this approach\nappears satisfactory, potential issues could arise if the live\n`ActiveConfig` are adjusted below these hard-coded values, necessitating\na new runtime release with updated values. Additionally, hard-coded\nvalues have consequences, such as Rococo's benchmarks not functioning:\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6082656.\n\n## Solution\nThe proposed solution here is to utilize\n`ActiveConfigHrmpChannelSizeAndCapacityRatio`, which reads the current\n`ActiveConfig` and calculates `DefaultChannelSizeAndCapacityWithSystem`,\nfor example, \"half of the maximum\" based on live data. This way,\nwhenever `ActiveConfig` is modified,\n`ActiveConfigHrmpChannelSizeAndCapacityRatio` automatically returns\nadjusted values with the appropriate ratio. Thus, manual adjustments and\nnew runtime releases become unnecessary.\n\n\nRelates to a comment/discussion:\nhttps://github.com/paritytech/polkadot-sdk/pull/3721/files#r1541001420\nRelates to a comment/discussion:\nhttps://github.com/paritytech/polkadot-sdk/pull/3721/files#r1549291588\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-01T20:01:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e5a93fbcd4a6acec7ab83865708e5c5df3534a7b"
        },
        "date": 1714598868859,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63540.84000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.09999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.1158913670701347,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.218438773569974,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.799201386750028,
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
          "id": "14c4afc5382dcb18095af5c090e2aa1af65ecae6",
          "message": "[Backport] Version bumps and reorg prdocs from 1.11.0 (#4336)\n\nThis PR backports version bumps and reorganization of the `prdocs` from\n`1.11.0` release branch back to master",
          "timestamp": "2024-05-02T07:21:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/14c4afc5382dcb18095af5c090e2aa1af65ecae6"
        },
        "date": 1714639868568,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63547.78999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.3,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.6460106499399405,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.25104776159003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3935174199001787,
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
          "id": "171bedc2b319e18d51a7b510d8bd4cfd2e645c31",
          "message": "Bridge: ignore client errors when calling recently added `*_free_headers_interval` methods (#4350)\n\nsee https://github.com/paritytech/parity-bridges-common/issues/2974 : we\nstill need to support unupgraded chains (BHK and BHP) in relay\n\nWe may need to revert this change when all chains are upgraded",
          "timestamp": "2024-05-02T10:02:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/171bedc2b319e18d51a7b510d8bd4cfd2e645c31"
        },
        "date": 1714647360193,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63539.619999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52933.5,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.3350431958601434,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.528457082589954,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.155113249340108,
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
          "id": "877617c44629e84ee36ac7194f4fe00fe3fa0b71",
          "message": "cargo: Update experimental litep2p to latest version (#4344)\n\nThis PR updates the litep2p crate to the latest version.\n\nThis fixes the build for developers that want to perform `cargo update`\non all their dependencies:\nhttps://github.com/paritytech/polkadot-sdk/pull/4343, by porting the\nlatest changes.\n\nThe peer records were introduced to litep2p to be able to distinguish\nand update peers with outdated records.\nIt is going to be properly used in substrate via:\nhttps://github.com/paritytech/polkadot-sdk/pull/3786, however that is\npending the commit to merge on litep2p master:\nhttps://github.com/paritytech/litep2p/pull/96.\n\nCloses: https://github.com/paritytech/polkadot-sdk/pull/4343\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-05-02T10:54:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/877617c44629e84ee36ac7194f4fe00fe3fa0b71"
        },
        "date": 1714653078538,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63549.36,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.750741698809943,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.146267871579989,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.7203813428501618,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Qinxuan Chen",
            "username": "koushiro",
            "email": "koushiro.cqx@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4e0b3abbd696c809aebd8d5e64a671abf843087f",
          "message": "deps: update jsonrpsee to v0.22.5 (#4330)\n\nuse `server-core` feature instead of `server` feature when defining the\nrpc api",
          "timestamp": "2024-05-02T12:25:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4e0b3abbd696c809aebd8d5e64a671abf843087f"
        },
        "date": 1714657991672,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63551.1,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.141575628700015,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.813329087629892,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0584330719201267,
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
          "id": "30a1972ee5608afa22cd5b72339acb59bb51b0f3",
          "message": "More `xcm::v4` cleanup and `xcm_fee_payment_runtime_api::XcmPaymentApi` nits (#4355)\n\nThis PR:\n- changes `xcm::v4` to `xcm::prelude` imports for coretime stuff\n- changes `query_acceptable_payment_assets` /\n`query_weight_to_asset_fee` implementations to be more resilient to the\nXCM version change\n- adds `xcm_fee_payment_runtime_api::XcmPaymentApi` to the\nAssetHubRococo/Westend exposing a native token as acceptable payment\nasset\n\nContinuation of: https://github.com/paritytech/polkadot-sdk/pull/3607\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4297\n\n## Possible follow-ups\n\n- [ ] add all sufficient assets (`Assets`, `ForeignAssets`) as\nacceptable payment assets ?",
          "timestamp": "2024-05-02T14:08:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/30a1972ee5608afa22cd5b72339acb59bb51b0f3"
        },
        "date": 1714664111724,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63540.909999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.507767096989978,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.18690846672989,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3372831581402367,
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
          "id": "6580101ef3d5c36e1d84a820136fb87f398b04a3",
          "message": "Deprecate `NativeElseWasmExecutor` (#4329)\n\nThe native executor is deprecated and downstream users should stop using\nit.",
          "timestamp": "2024-05-02T15:19:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6580101ef3d5c36e1d84a820136fb87f398b04a3"
        },
        "date": 1714668637461,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63553.030000000006,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.352515512110115,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.737282359900052,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.481066659380165,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kris Bitney",
            "username": "krisbitney",
            "email": "kris@dorg.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a9aeabe923dae63ab76ab290951cb9183c51f59c",
          "message": "Allow for 0 existential deposit in benchmarks for `pallet_staking`, `pallet_session`, and `pallet_balances` (#4346)\n\nThis PR ensures non-zero values are available in benchmarks for\n`pallet_staking`, `pallet_session`, and `pallet_balances` where required\nfor them to run.\n\nThis small change makes it possible to run the benchmarks for\n`pallet_staking`, `pallet_session`, and `pallet_balances` in a runtime\nfor which existential deposit is set to 0.\n\nThe benchmarks for `pallet_staking` and `pallet_session` will still fail\nin runtimes that use `U128CurrencyToVote`, but that is easy to work\naround by creating a new `CurrencyToVote` implementation for\nbenchmarking.\n\nThe changes are implemented by checking if existential deposit equals 0\nand using 1 if so.\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-02T20:16:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a9aeabe923dae63ab76ab290951cb9183c51f59c"
        },
        "date": 1714686037084,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63553.030000000006,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.352515512110115,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.737282359900052,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.481066659380165,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kris Bitney",
            "username": "krisbitney",
            "email": "kris@dorg.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a9aeabe923dae63ab76ab290951cb9183c51f59c",
          "message": "Allow for 0 existential deposit in benchmarks for `pallet_staking`, `pallet_session`, and `pallet_balances` (#4346)\n\nThis PR ensures non-zero values are available in benchmarks for\n`pallet_staking`, `pallet_session`, and `pallet_balances` where required\nfor them to run.\n\nThis small change makes it possible to run the benchmarks for\n`pallet_staking`, `pallet_session`, and `pallet_balances` in a runtime\nfor which existential deposit is set to 0.\n\nThe benchmarks for `pallet_staking` and `pallet_session` will still fail\nin runtimes that use `U128CurrencyToVote`, but that is easy to work\naround by creating a new `CurrencyToVote` implementation for\nbenchmarking.\n\nThe changes are implemented by checking if existential deposit equals 0\nand using 1 if so.\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-02T20:16:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a9aeabe923dae63ab76ab290951cb9183c51f59c"
        },
        "date": 1714688133154,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.869999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.3,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.238943065889993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4086536865300965,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.65474114762001,
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
          "id": "ad72cd8d481008ddc38bdd67a3ca3434901dd795",
          "message": "[WIP][CI] Add more GHA jobs (#4270)\n\ncc https://github.com/paritytech/ci_cd/issues/939",
          "timestamp": "2024-05-03T10:43:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ad72cd8d481008ddc38bdd67a3ca3434901dd795"
        },
        "date": 1714738238499,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63549.31,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.7,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.433170125599913,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.996437076970114,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.251428405340177,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kris Bitney",
            "username": "krisbitney",
            "email": "kris@dorg.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "519862334feec5c72709afbf595ed61ddfb2298a",
          "message": "Fix: dust unbonded for zero existential deposit (#4364)\n\nWhen a staker unbonds and withdraws, it is possible that their stash\nwill contain less currency than the existential deposit. If that\nhappens, their stash is reaped. But if the existential deposit is zero,\nthe reap is not triggered. This PR adjusts `pallet_staking` to reap a\nstash in the special case that the stash value is zero and the\nexistential deposit is zero.\n\nThis change is important for blockchains built on substrate that require\nan existential deposit of zero, becuase it conserves valued storage\nspace.\n\nThere are two places in which ledgers are checked to determine if their\nvalue is less than the existential deposit and they should be reaped: in\nthe methods `do_withdraw_unbonded` and `reap_stash`. When the check is\nmade, the condition `ledger_total == 0` is also checked. If\n`ledger_total` is zero, then it must be below any existential deposit\ngreater than zero and equal to an existential deposit of 0.\n\nI added a new test for each method to confirm the change behaves as\nexpected.\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/4340\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-03T12:31:45Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/519862334feec5c72709afbf595ed61ddfb2298a"
        },
        "date": 1714744731427,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63539.8,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.115136607060132,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.712722248160047,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.027563684640188,
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
          "id": "4c09a0631334c3e021a30aa49a503815aa047b29",
          "message": "State trie migration on asset-hub westend and collectives westend (#4185)",
          "timestamp": "2024-05-03T14:04:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4c09a0631334c3e021a30aa49a503815aa047b29"
        },
        "date": 1714750291333,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63542.48,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.947577491499999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.428531070030215,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.6140283230100065,
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
          "id": "f45847b8b10db9cea664d0ce90083947c3843b10",
          "message": "Add validate field to prdoc (#4368)",
          "timestamp": "2024-05-03T15:07:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f45847b8b10db9cea664d0ce90083947c3843b10"
        },
        "date": 1714756323992,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63544.079999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52938.3,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.9777332244200965,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.962579782649982,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.551998903559989,
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
          "id": "c0234becc185f88445dd63105b6f363c9e5990ce",
          "message": "Publish `polkadot-sdk-frame`  crate (#4370)",
          "timestamp": "2024-05-04T05:36:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c0234becc185f88445dd63105b6f363c9e5990ce"
        },
        "date": 1714806231111,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.069999999985,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.032233782080004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2852326877801703,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.403643804229962,
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
          "id": "73c89d308fefcedfc3619f0273e13b6623766b81",
          "message": "Introduces `TypeWithDefault<T, D: Get<T>>` (#4034)\n\nNeeded for: https://github.com/polkadot-fellows/runtimes/issues/248\n\nThis PR introduces a new type `TypeWithDefault<T, D: Get<T>>` to be able\nto provide a custom default for any type. This can, then, be used to\nprovide the nonce type that returns the current block number as the\ndefault, to avoid replay of immortal transactions.\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-05-06T03:59:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/73c89d308fefcedfc3619f0273e13b6623766b81"
        },
        "date": 1714973910563,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63546.06000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 8.080053513599918,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.632035481910163,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.501562840889955,
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
          "id": "6e3059a873b78c7a4e2da18b4c298847713140ee",
          "message": "Upgrade a few deps (#4381)\n\nSplit from #4374\n\nThis PR helps to reduce dependencies and align versions, which would\nhelp to move them to workspace dep",
          "timestamp": "2024-05-06T11:49:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6e3059a873b78c7a4e2da18b4c298847713140ee"
        },
        "date": 1715001877714,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63548.79,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.90000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.180909964369988,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.621565220630034,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4189322805300955,
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
          "id": "e434176e0867d17336301388b46a6796b366a976",
          "message": "Improve Create release draft workflow + templates for the free notes and docker images sections in the notes (#4371)\n\nThis PR has the following changes:\n\n- New templates for the free notes and docker images sections in the\nrelease notes. There is going to be a section for the manual additions\nto the release notes + a section with the links to the docker images for\n`polkadot` and `polkadot-parachain` binaries at the end of the release\ndraft.\n- Fix for matrix section in the Create release draft flow (adds the\nrelease environment variable)\n- Reduction of the message which is posted to the announcement chats, as\nthe current one with the full release notes text is too big.",
          "timestamp": "2024-05-06T14:39:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e434176e0867d17336301388b46a6796b366a976"
        },
        "date": 1715011732310,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63554.66000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.3336728504301254,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.5705820393000405,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.087911451330054,
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
          "id": "b709dccd063507d468db3e10f491bb60cd80ac64",
          "message": "Add support for versioned notification for HRMP pallet (#4281)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4003 (please\nsee for the problem description)\n\n## TODO\n- [x] add more tests covering `WrapVersion` corner cases (e.g. para has\nlower version, ...)\n- [x] regenerate benchmarks `runtime_parachains::hrmp` (fix for Rococo\nis here: https://github.com/paritytech/polkadot-sdk/pull/4332)\n\n## Questions / possible improvements\n- [ ] A `WrapVersion` implementation for `pallet_xcm` initiates version\ndiscovery with\n[note_unknown_version](https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/xcm/pallet-xcm/src/lib.rs#L2527C5-L2527C25),\nthere is possibility to avoid this overhead in this HRMP case to create\nnew `WrapVersion` adapter for `pallet_xcm` which would not use\n`note_unknown_version`. Is it worth to do it or not?\n- [ ] There's a possibility to decouple XCM functionality from the HRMP\npallet, allowing any relay chain to generate its own notifications. This\napproach wouldn't restrict notifications solely to the XCM. However,\nit's uncertain whether it's worthwhile or desirable to do so? It means\nmaking HRMP pallet more generic. E.g. hiding HRMP notifications behind\nsome trait:\n\t```\n\ttrait HrmpNotifications {\n\n\t\tfn on_channel_open_request(\n\t\t\tsender: ParaId,\n\t\t\tproposed_max_capacity: u32,\n\t\t\tproposed_max_message_size: u32) -> primitives::DownwardMessage;\n\nfn on_channel_accepted(recipient: ParaId) ->\nprimitives::DownwardMessage;\n\nfn on_channel_closing(initiator: ParaId, sender: ParaId, recipient:\nParaId) -> primitives::DownwardMessage;\n\t}\n\t```\nand then we could have whatever adapter, `impl HrmpNotifications for\nVersionedXcmHrmpNotifications {...}`,\n\t```\n\timpl parachains_hrmp::Config for Runtime {\n\t..\n\t\ttype HrmpNotifications = VersionedXcmHrmpNotifications;\n\t..\n\t}\n\t```\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-07T08:33:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b709dccd063507d468db3e10f491bb60cd80ac64"
        },
        "date": 1715076245838,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63550.86,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.59999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 8.022393158519925,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.723369890899981,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6416040401402165,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "jimdssd",
            "username": "jimdssd",
            "email": "wqq1479791@163.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "29c8130bab0ed8216f48e47a78c602e7f0c5c1f2",
          "message": "chore: fix typos (#4395)",
          "timestamp": "2024-05-07T10:23:27Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/29c8130bab0ed8216f48e47a78c602e7f0c5c1f2"
        },
        "date": 1715082867280,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63549.91000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.8422964905701624,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.281030101940011,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.787595838069935,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Evgeny Snitko",
            "username": "AndWeHaveAPlan",
            "email": "evgeny@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "1c8595adb89b7b6ac443e9a1caf0b20a6e1231a5",
          "message": "Code coverage preparations (#4387)\n\nAdded manual jobs for code coverage (triggered via `codecov-start` job):\n - **codecov-start** - initialize Codecov report for commit/pr\n- **test-linux-stable-codecov** - perform `nextest run` and upload\ncoverage data parts\n- **codecov-finish** - finalize uploading of data parts and generate\nCodecov report\n\nCoverage requires code to be built with `-C instrument-coverage` which\ncauses build errors (e .g. ```error[E0275]: overflow evaluating the\nrequirement `<mock::Test as pallet::Config>::KeyOwnerProof == _\\` ```,\nseems like related to\n[2641](https://github.com/paritytech/polkadot-sdk/issues/2641)) and\nunstable tests behavior\n([example](https://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6004731)).\nThis is where we'll nee the developers assistance\n\nclosing [[polkadot-sdk] Add code coverage\n#902](https://github.com/paritytech/ci_cd/issues/902)",
          "timestamp": "2024-05-07T15:14:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1c8595adb89b7b6ac443e9a1caf0b20a6e1231a5"
        },
        "date": 1715096716627,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63543.57000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.5,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.819302342120174,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.345501352530052,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.770008094350001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tsvetomir Dimitrov",
            "username": "tdimitrov",
            "email": "tsvetomir@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b6dcd1b65436a6f7c087ad659617a9caf29a233a",
          "message": "Update prdoc for 2226 (#4401)\n\nMention that offenders are no longer chilled and suggest node operators\nand nominators to monitor their nodes/nominees closely.\n\n---------\n\nCo-authored-by: Maciej <maciej.zyszkiewicz@parity.io>",
          "timestamp": "2024-05-07T19:18:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b6dcd1b65436a6f7c087ad659617a9caf29a233a"
        },
        "date": 1715114773510,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63545.05,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.751099255790104,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.244758526120053,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.45252866074019,
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
          "id": "c91c13b9c1d6eaf12d89dbf088c1e16b25261822",
          "message": "Generate XCM weights for coretimes (#4396)\n\nAddressing comment:\nhttps://github.com/paritytech/polkadot-sdk/pull/3455#issuecomment-2094829076\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-07T21:50:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c91c13b9c1d6eaf12d89dbf088c1e16b25261822"
        },
        "date": 1715121365215,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63547.67999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.255527307599957,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.820853044140153,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.793941075689942,
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
          "id": "c91c13b9c1d6eaf12d89dbf088c1e16b25261822",
          "message": "Generate XCM weights for coretimes (#4396)\n\nAddressing comment:\nhttps://github.com/paritytech/polkadot-sdk/pull/3455#issuecomment-2094829076\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-07T21:50:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c91c13b9c1d6eaf12d89dbf088c1e16b25261822"
        },
        "date": 1715123875379,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.62000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.263894010299913,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.8759899162901195,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.874875719489987,
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
          "id": "17b56fae2d976a3df87f34076875de8c26da0355",
          "message": "Bridge: check bridge GRANDPA pallet call limits from signed extension (#4385)\n\nsilent, because it'll be deployed with the\nhttps://github.com/paritytech/polkadot-sdk/pull/4102, where this code\nhas been introduced\n\nI've planned originally to avoid doing that check in the runtime code,\nbecause it **may be** checked offchain. But actually, the check is quite\ncheap and we could do that onchain too.",
          "timestamp": "2024-05-08T08:26:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/17b56fae2d976a3df87f34076875de8c26da0355"
        },
        "date": 1715159218774,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.95,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.802010429180001,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.284466665599918,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.850940871960126,
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
          "id": "17b56fae2d976a3df87f34076875de8c26da0355",
          "message": "Bridge: check bridge GRANDPA pallet call limits from signed extension (#4385)\n\nsilent, because it'll be deployed with the\nhttps://github.com/paritytech/polkadot-sdk/pull/4102, where this code\nhas been introduced\n\nI've planned originally to avoid doing that check in the runtime code,\nbecause it **may be** checked offchain. But actually, the check is quite\ncheap and we could do that onchain too.",
          "timestamp": "2024-05-08T08:26:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/17b56fae2d976a3df87f34076875de8c26da0355"
        },
        "date": 1715161958887,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63543.590000000004,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.3,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.284690175330036,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.814637812119976,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.8629673604801598,
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
          "id": "37b1544b51aeba183350d4c8d76987c32e6c9ca7",
          "message": "Adds benchmarking and try-runtime support in frame crate (#4406)",
          "timestamp": "2024-05-08T11:50:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/37b1544b51aeba183350d4c8d76987c32e6c9ca7"
        },
        "date": 1715174948896,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63552.96000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.3,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 11.50274803591998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 4.01244826689031,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.68367593888994,
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
          "id": "6fdb522ded3813f43a539964af78d5fc6d9f1e97",
          "message": "Add semver CI check  (#4279)\n\nThis checks changed files against API surface changes against what the\nprdoc says.\n\nIt will error if the detected semver change is greater than the one\nlisted in the prdoc. It will also error if any crates were touched but\nnot mentioned in the prdoc.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-05-08T16:17:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6fdb522ded3813f43a539964af78d5fc6d9f1e97"
        },
        "date": 1715190662482,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63548.94,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.963092178420013,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.236361867680031,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6726475538101853,
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
          "id": "d37719da022879b4e2ef7947f5c9d2187f666ae7",
          "message": "rpc: add option to `whitelist ips` in rate limiting (#3701)\n\nThis PR adds two new CLI options to disable rate limiting for certain ip\naddresses and whether to trust \"proxy header\".\nAfter going back in forth I decided to use ip addr instead host because\nwe don't want rely on the host header which can be spoofed but another\nsolution is to resolve the ip addr from the socket to host name.\n\nExample:\n\n```bash\n$ polkadot --rpc-rate-limit 10 --rpc-rate-limit-whitelisted-ips 127.0.0.1/8 --rpc-rate-limit-trust-proxy-headers\n```\n\nThe ip addr is read from the HTTP proxy headers `Forwarded`,\n`X-Forwarded-For` `X-Real-IP` if `--rpc-rate-limit-trust-proxy-headers`\nis enabled if that is not enabled or the headers are not found then the\nip address is read from the socket.\n\n//cc @BulatSaif can you test this and give some feedback on it?",
          "timestamp": "2024-05-09T07:23:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d37719da022879b4e2ef7947f5c9d2187f666ae7"
        },
        "date": 1715245028558,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63555.380000000005,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52947.2,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.691800476000141,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.295124224940112,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.903209441329993,
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
          "id": "657df04cd901559cc6e33a8dfe70395bddb079f2",
          "message": "Fixes `frame-support` reference in `try_decode_entire_state` (#4425)",
          "timestamp": "2024-05-10T09:19:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/657df04cd901559cc6e33a8dfe70395bddb079f2"
        },
        "date": 1715338359729,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63546.95,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52937.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.62155968260004,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.117522617739997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6434177220401884,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Maciej",
            "username": "Overkillus",
            "email": "maciej.zyszkiewicz@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "00440779d42b754292783612fc0f7e99d7cde2d2",
          "message": "Disabling Strategy Implementers Guide (#2955)\n\nCloses #1961",
          "timestamp": "2024-05-10T12:31:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/00440779d42b754292783612fc0f7e99d7cde2d2"
        },
        "date": 1715347669796,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63546.95,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52937.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.62155968260004,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.117522617739997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6434177220401884,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Maciej",
            "username": "Overkillus",
            "email": "maciej.zyszkiewicz@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "00440779d42b754292783612fc0f7e99d7cde2d2",
          "message": "Disabling Strategy Implementers Guide (#2955)\n\nCloses #1961",
          "timestamp": "2024-05-10T12:31:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/00440779d42b754292783612fc0f7e99d7cde2d2"
        },
        "date": 1715349417780,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63546.95,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52937.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.62155968260004,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.117522617739997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6434177220401884,
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
          "id": "a993513c9c54313bc3c7093563d2f4ff6fe42ae2",
          "message": "Add docs to request_core_count (#4423)\n\nThe fact that this takes two sessions to come into effect is not\nobvious. Just added some docs to explain that.\n\nAlso tidied up uses of \"broker chain\" -> \"coretime chain\"",
          "timestamp": "2024-05-10T19:41:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a993513c9c54313bc3c7093563d2f4ff6fe42ae2"
        },
        "date": 1715375380269,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63552.12000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.1985245119301036,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.96179448921998,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.384794801320022,
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
          "id": "32deb605a09adf28ba30319b06a4197a2d048ef7",
          "message": "Remove `pallet::getter` usage from authority-discovery pallet (#4091)\n\nAs per #3326, removes pallet::getter usage from the pallet\nauthority-discovery. The syntax `StorageItem::<T, I>::get()` should be\nused instead.\n\ncc @muraca\n\n---------\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-10T21:28:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/32deb605a09adf28ba30319b06a4197a2d048ef7"
        },
        "date": 1715381942699,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.81999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.09999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.764223260750265,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.036223898689968,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.299465111939956,
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
          "id": "9e0e5fcd0a814ab30d15b3f8920c8d9ab3970e11",
          "message": "xcm-emlator: Use `BlockNumberFor` instead of `parachains_common::BlockNumber=u32` (#4434)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4428",
          "timestamp": "2024-05-12T15:16:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9e0e5fcd0a814ab30d15b3f8920c8d9ab3970e11"
        },
        "date": 1715534384622,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63547.999999999985,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.3,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.904712378400082,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.905309556839994,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.53526453698008,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dastan",
            "username": "dastansam",
            "email": "88332432+dastansam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "efc2132fa2ece419d36af03c935b3c2c60440eb5",
          "message": "migrations: `take()`should consume read and write operation weight (#4302)\n\n#### Problem\n`take()` consumes only 1 read worth of weight in\n`single-block-migrations` example, while `take()`\n[is](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/support/src/storage/unhashed.rs#L63)\n`get() + kill()`, i.e should be 1 read + 1 write. I think this could\nmislead developers who follow this example to write their migrations\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-12T22:35:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/efc2132fa2ece419d36af03c935b3c2c60440eb5"
        },
        "date": 1715555914802,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63548.78999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.623665051299954,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.253141216560081,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4393584506501176,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dastan",
            "username": "dastansam",
            "email": "88332432+dastansam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "efc2132fa2ece419d36af03c935b3c2c60440eb5",
          "message": "migrations: `take()`should consume read and write operation weight (#4302)\n\n#### Problem\n`take()` consumes only 1 read worth of weight in\n`single-block-migrations` example, while `take()`\n[is](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/support/src/storage/unhashed.rs#L63)\n`get() + kill()`, i.e should be 1 read + 1 write. I think this could\nmislead developers who follow this example to write their migrations\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-12T22:35:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/efc2132fa2ece419d36af03c935b3c2c60440eb5"
        },
        "date": 1715558748528,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63539.7,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.53407298781997,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.234906157319992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3119468829001333,
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
          "id": "477a120893ecd35f5e4f808cba10525424b5711d",
          "message": "[ci] Add forklift to GHA ARC (#4372)\n\nPR adds forklift settings and forklift to test-github-actions\n\ncc https://github.com/paritytech/ci_cd/issues/939",
          "timestamp": "2024-05-13T09:54:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/477a120893ecd35f5e4f808cba10525424b5711d"
        },
        "date": 1715596503300,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52945.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63549.69,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.55501807833011,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.5006567842001592,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.855586049150079,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tsvetomir Dimitrov",
            "username": "tdimitrov",
            "email": "tsvetomir@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "fb7362f67e3ac345073b203e029bcb561822f09c",
          "message": "Bump `proc-macro-crate` to the latest version (#4409)\n\nThis PR bumps `proc-macro-crate` to the latest version.\n\nIn order to test a runtime from\nhttps://github.com/polkadot-fellows/runtimes/ with the latest version of\npolkadot-sdk one needs to use `cargo vendor` to extract all runtime\ndependencies, patch them by hand and then build the runtime.\n\nHowever at the moment 'vendored' builds fail due to\nhttps://github.com/bkchr/proc-macro-crate/issues/48. To fix this\n`proc-macro-crate` should be updated to version `3.0.1` or higher.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-13T14:58:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fb7362f67e3ac345073b203e029bcb561822f09c"
        },
        "date": 1715619065137,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63551.02999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.8,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.799721153960038,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.372491497599954,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.495429803080202,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Éloïs",
            "username": "librelois",
            "email": "c@elo.tf"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "594c3ed5750bc7ab97f82fb8387f82661eca1cc4",
          "message": " improve MockValidationDataInherentDataProvider to support async backing (#4442)\n\nSupport async backing in `--dev` mode\n\nThis PR improve the relay mock `MockValidationDataInherentDataProvider`\nto mach expectations of async backing runtimes.\n\n* Add para_head in the mock relay proof\n* Add relay slot in the mock relay proof \n\nfix https://github.com/paritytech/polkadot-sdk/issues/4437",
          "timestamp": "2024-05-13T22:03:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/594c3ed5750bc7ab97f82fb8387f82661eca1cc4"
        },
        "date": 1715645065334,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63543.55,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.0999899565601146,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.163690392279949,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.899207171680029,
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
          "id": "115c2477eb287df55107cd95594100ba395ed239",
          "message": "Bridge: use *-uri CLI arguments when starting relayer (#4451)\n\n`*-host` and `*-port` are obsolete and we'll hopefully remove them in\nthe future (already WIP for Rococo <> Westend relayer)",
          "timestamp": "2024-05-14T08:34:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/115c2477eb287df55107cd95594100ba395ed239"
        },
        "date": 1715682911659,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63547.240000000005,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 12.615147477179944,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 9.82655496778997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 4.57723289372031,
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
          "id": "6487ac1ede14b5785be1429655ed8c387d82be9a",
          "message": "Remove pallet::getter usage from the bounties and child-bounties pallets (#4392)\n\nAs per #3326, removes pallet::getter usage from the bounties and\nchild-bounties pallets. The syntax `StorageItem::<T, I>::get()` should\nbe used instead.\n\nChanges to one pallet involved changes in the other, so I figured it'd\nbe best to combine these two.\n\ncc @muraca\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-16T09:01:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6487ac1ede14b5785be1429655ed8c387d82be9a"
        },
        "date": 1715855466563,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52945.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63551.340000000004,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.916470267209914,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.8684474593001164,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.531943889440084,
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
          "id": "4adfa37d14c0d81f09071687afb270ecdd5c2076",
          "message": "[Runtime] Bound XCMP queue (#3952)\n\nRe-applying #2302 after increasing the `MaxPageSize`.  \n\nRemove `without_storage_info` from the XCMP queue pallet. Part of\nhttps://github.com/paritytech/polkadot-sdk/issues/323\n\nChanges:\n- Limit the number of messages and signals a HRMP channel can have at\nmost.\n- Limit the number of HRML channels.\n\nA No-OP migration is put in place to ensure that all `BoundedVec`s still\ndecode and not truncate after upgrade. The storage version is thereby\nbumped to 5 to have our tooling remind us to deploy that migration.\n\n## Integration\n\nIf you see this error in your try-runtime-cli:  \n```pre\nMax message size for channel is too large. This means that the V5 migration can be front-run and an\nattacker could place a large message just right before the migration to make other messages un-decodable.\nPlease either increase `MaxPageSize` or decrease the `max_message_size` for this channel. Channel max:\n102400, MaxPageSize: 65535\n```\n\nThen increase the `MaxPageSize` of the `cumulus_pallet_xcmp_queue` to\nsomething like this:\n```rust\ntype MaxPageSize = ConstU32<{ 103 * 1024 }>;\n```\n\nThere is currently no easy way for on-chain governance to adjust the\nHRMP max message size of all channels, but it could be done:\nhttps://github.com/paritytech/polkadot-sdk/issues/3145.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>",
          "timestamp": "2024-05-16T10:43:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4adfa37d14c0d81f09071687afb270ecdd5c2076"
        },
        "date": 1715861452235,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.95000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.883740833400016,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.8430086689601333,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.534753632750014,
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
          "id": "04f88f5b03038acbdeb7475f543baf6b06d64f74",
          "message": "Remove pallet::getter usage from the democracy pallet (#4472)\n\nAs per #3326, removes usage of the pallet::getter macro from the\ndemocracy pallet. The syntax `StorageItem::<T, I>::get()` should be used\ninstead.\n\ncc @muraca",
          "timestamp": "2024-05-16T13:53:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/04f88f5b03038acbdeb7475f543baf6b06d64f74"
        },
        "date": 1715869162783,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63545.380000000005,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.9512555055800798,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.679352646599984,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.015975438669988,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Clara van Staden",
            "username": "claravanstaden",
            "email": "claravanstaden64@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "943eb46ed54c2fcd9fab693b86ef59ce18c0f792",
          "message": "Snowbridge - Ethereum Client - Reject finalized updates without a sync committee in next store period (#4478)\n\nWhile syncing Ethereum consensus updates to the Snowbridge Ethereum\nlight client, the syncing process stalled due to error\n`InvalidSyncCommitteeUpdate` when importing the next sync committee for\nperiod `1087`.\n\nThis bug manifested specifically because our light client checkpoint is\na few weeks old (submitted to governance weeks ago) and had to catchup\nuntil a recent block. Since then, we have done thorough testing of the\ncatchup sync process.\n\n### Symptoms\n- Import next sync committee for period `1086` (essentially period\n`1087`). Light client store period = `1086`.\n- Import header in period `1087`. Light client store period = `1087`.\nThe current and next sync committee is not updated, and is now in an\noutdated state. (current sync committee = `1086` and current sync\ncommittee = `1087`, where it should be current sync committee = `1087`\nand current sync committee = `None`)\n- Import next sync committee for period `1087` (essentially period\n`1088`) fails because the expected next sync committee's roots don't\nmatch.\n\n### Bug\nThe bug here is that the current and next sync committee's didn't\nhandover when an update in the next period was received.\n\n### Fix\nThere are two possible fixes here:\n1. Correctly handover sync committees when a header in the next period\nis received.\n2. Reject updates in the next period until the next sync committee\nperiod is known.\n\nWe opted for solution 2, which is more conservative and requires less\nchanges.\n\n### Polkadot-sdk versions\nThis fix should be backported in polkadot-sdk versions 1.7 and up.\n\nSnowfork PR: https://github.com/Snowfork/polkadot-sdk/pull/145\n\n---------\n\nCo-authored-by: Vincent Geddes <117534+vgeddes@users.noreply.github.com>",
          "timestamp": "2024-05-16T13:54:28Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/943eb46ed54c2fcd9fab693b86ef59ce18c0f792"
        },
        "date": 1715873180565,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63540.95,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52935.7,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.0448210039900285,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.015152266050129,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.693810640819994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jesse Chejieh",
            "username": "Doordashcon",
            "email": "jesse.chejieh@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d5fe478e4fe2d62b0800888ae77b00ff0ba28b28",
          "message": "Adds `MaxRank` Config in `pallet-core-fellowship` (#3393)\n\nresolves #3315\n\n---------\n\nCo-authored-by: doordashcon <jessechejieh@doordashcon.local>\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-16T16:22:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d5fe478e4fe2d62b0800888ae77b00ff0ba28b28"
        },
        "date": 1715882135216,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63548.619999999995,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.693737322719967,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.5704612115601386,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.102180436060063,
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
          "id": "f86f2131fe0066cf9009cb909e843da664b3df98",
          "message": "Contracts: remove kitchensink dynamic parameters (#4489)\n\nUsing Dynamic Parameters for contracts seems like a bad idea for now.\n\nGiven that we have benchmarks for each host function (in addition to our\nextrinsics), parameter storage reads will be counted multiple times. We\nwill work on updates to the benchmarking framework to mitigate this\nissue in future iterations.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-17T05:52:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f86f2131fe0066cf9009cb909e843da664b3df98"
        },
        "date": 1715930376264,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63547.079999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.8,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.0425426843700105,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.648408949679999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0101022399201094,
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
          "id": "2c48b9ddb0a5de4499d4ed699b79eacc354f016a",
          "message": "Bridge: fixed relayer version metric value (#4492)\n\nBefore relayer crates have been moved + merged, the `MetricsParams` type\nhas been created from a `substrate-relay` crate (binary) and hence it\nhas been setting the `substrate_relay_build_info` metic value properly -\nto the binary version. Now it is created from the\n`substrate-relay-helper` crate, which has the fixed (it isn't published)\nversion `0.1.0`, so our relay provides incorrect metric value. This\n'breaks' our monitoring tools - we see that all relayers have that\nincorrect version, which is not cool.\n\nThe idea is to have a global static variable (shame on me) that is\ninitialized by the binary during initialization like we do with the\nlogger initialization already. Was considering some alternative options:\n- adding a separate argument to every relayer subcommand and propagating\nit to the `MetricsParams::new()` causes a lot of changes and introduces\neven more noise to the binary code, which is supposed to be as small as\npossible in the new design. But I could do that if team thinks it is\nbetter;\n- adding a `structopt(skip) pub relayer_version: RelayerVersion`\nargument to all subcommand params won't work, because it will be\ninitialized by default and `RelayerVersion` needs to reside in some util\ncrate (not the binary), so it'll have the wrong value again.",
          "timestamp": "2024-05-17T08:00:39Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2c48b9ddb0a5de4499d4ed699b79eacc354f016a"
        },
        "date": 1715938417392,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63539.14,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.17742462257996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.323629257230145,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.544828279240034,
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
          "id": "2e36f571e5c9486819b85561d12fa4001018e953",
          "message": "Allow pool to be destroyed with an extra (erroneous) consumer reference on the pool account (#4503)\n\naddresses https://github.com/paritytech/polkadot-sdk/issues/4440 (will\nclose once we have this in prod runtimes).\nrelated: https://github.com/paritytech/polkadot-sdk/issues/2037.\n\nAn extra consumer reference is preventing pools to be destroyed. When a\npool is ready to be destroyed, we\ncan safely clear the consumer references if any. Notably, I only check\nfor one extra consumer reference since that is a known bug. Anything\nmore indicates possibly another issue and we probably don't want to\nsilently absorb those errors as well.\n\nAfter this change, pools with extra consumer reference should be able to\ndestroy normally.",
          "timestamp": "2024-05-17T12:09:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2e36f571e5c9486819b85561d12fa4001018e953"
        },
        "date": 1715952964943,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63544.920000000006,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.8,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.769997565739951,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.086218854210026,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0088377940700957,
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
          "id": "a90d324d5b3252033e00a96d9f9f4890b1cfc982",
          "message": "Contracts: Remove topics for internal events (#4510)",
          "timestamp": "2024-05-17T13:47:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a90d324d5b3252033e00a96d9f9f4890b1cfc982"
        },
        "date": 1715960534836,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63542.579999999994,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.046118530730043,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.597563301780033,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9756738932400904,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "jimwfs",
            "username": "jimwfs",
            "email": "wqq1479787@163.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "247358a86f874bfa109575dd086a6478dbc96eb4",
          "message": "chore: fix typos (#4515)\n\nCo-authored-by: jimwfs <169986508+jimwfs@users.noreply.github.com>",
          "timestamp": "2024-05-19T15:31:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/247358a86f874bfa109575dd086a6478dbc96eb4"
        },
        "date": 1716137999707,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63547.85,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.089215772390016,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.164442324029972,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.293660103249934,
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
          "id": "e7b6d7dffd6459174f02598bd8b84fe4b1cb6e72",
          "message": "`remote-externalities`: `rpc_child_get_keys` to use paged scraping (#4512)\n\nReplace usage of deprecated\n`substrate_rpc_client::ChildStateApi::storage_keys` with\n`substrate_rpc_client::ChildStateApi::storage_keys_paged`.\n\nRequired for successful scraping of Aleph Zero state.",
          "timestamp": "2024-05-19T17:53:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e7b6d7dffd6459174f02598bd8b84fe4b1cb6e72"
        },
        "date": 1716146707452,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63542.42,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.0756305048901917,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.22913772824999,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.808103166350001,
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
          "id": "313fe0f9a277f27a4228634f0fb15a1c3fa21271",
          "message": "Remove usage of the pallet::getter macro from pallet-fast-unstake (#4514)\n\nAs per #3326, removes pallet::getter macro usage from\npallet-fast-unstake. The syntax `StorageItem::<T, I>::get()` should be\nused instead.\n\ncc @muraca\n\n---------\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-05-20T06:36:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/313fe0f9a277f27a4228634f0fb15a1c3fa21271"
        },
        "date": 1716192326386,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63547.829999999994,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.547934758850094,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4239887044501196,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.857821811039999,
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
          "id": "278486f9bf7db06c174203f098eec2f91839757a",
          "message": "Remove the prospective-parachains subsystem from collators (#4471)\n\nImplements https://github.com/paritytech/polkadot-sdk/issues/4429\n\nCollators only need to maintain the implicit view for the paraid they\nare collating on.\nIn this case, bypass prospective-parachains entirely. It's still useful\nto use the GetMinimumRelayParents message from prospective-parachains\nfor validators, because the data is already present there.\n\nThis enables us to entirely remove the subsystem from collators, which\nconsumed resources needlessly\n\nAims to resolve https://github.com/paritytech/polkadot-sdk/issues/4167 \n\nTODO:\n- [x] fix unit tests",
          "timestamp": "2024-05-21T08:14:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/278486f9bf7db06c174203f098eec2f91839757a"
        },
        "date": 1716284635180,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63540.590000000004,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.3545758959401804,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.299857497449985,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.616803574690006,
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
          "id": "d54feeb101b3779422323224c8e1ac43d3a1fafa",
          "message": "Fixed RPC subscriptions leak when subscription stream is finished (#4533)\n\ncloses https://github.com/paritytech/parity-bridges-common/issues/3000\n\nRecently we've changed our bridge configuration for Rococo <> Westend\nand our new relayer has started to submit transactions every ~ `30`\nseconds. Eventually, it switches itself into limbo state, where it can't\nsubmit more transactions - all `author_submitAndWatchExtrinsic` calls\nare failing with the following error: `ERROR bridge Failed to send\ntransaction to BridgeHubRococo node: Call(ErrorObject { code:\nServerError(-32006), message: \"Too many subscriptions on the\nconnection\", data: Some(RawValue(\"Exceeded max limit of 1024\")) })`.\n\nSome links for those who want to explore:\n- server side (node) has a strict limit on a number of active\nsubscriptions. It fails to open a new subscription if this limit is hit:\nhttps://github.com/paritytech/jsonrpsee/blob/a4533966b997e83632509ad97eea010fc7c3efc0/server/src/middleware/rpc/layer/rpc_service.rs#L122-L132.\nThe limit is set to `1024` by default;\n- internally this limit is a semaphore with `limit` permits:\nhttps://github.com/paritytech/jsonrpsee/blob/a4533966b997e83632509ad97eea010fc7c3efc0/core/src/server/subscription.rs#L461-L485;\n- semaphore permit is acquired in the first link;\n- the permit is \"returned\" when the `SubscriptionSink` is dropped:\nhttps://github.com/paritytech/jsonrpsee/blob/a4533966b997e83632509ad97eea010fc7c3efc0/core/src/server/subscription.rs#L310-L325;\n- the `SubscriptionSink` is dropped when [this `polkadot-sdk`\nfunction](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L58-L94)\nreturns. In other words - when the connection is closed, the stream is\nfinished or internal subscription buffer limit is hit;\n- the subscription has the internal buffer, so sending an item contains\nof two steps: [reading an item from the underlying\nstream](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L125-L141)\nand [sending it over the\nconnection](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L111-L116);\n- when the underlying stream is finished, the `inner_pipe_from_stream`\nwants to ensure that all items are sent to the subscriber. So it: [waits\nuntil the current send operation\ncompletes](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L146-L148)\nand then [send all remaining items from the internal\nbuffer](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L150-L155).\nOnce it is done, the function returns, the `SubscriptionSink` is\ndropped, semaphore permit is dropped and we are ready to accept new\nsubscriptions;\n- unfortunately, the code just calls the `pending_fut.await.is_err()` to\nensure that [the current send operation\ncompletes](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L146-L148).\nBut if there are no current send operation (which is normal), then the\n`pending_fut` is set to terminated future and the `await` never\ncompletes. Hence, no return from the function, no drop of\n`SubscriptionSink`, no drop of semaphore permit, no new subscriptions\nallowed (once number of susbcriptions hits the limit.\n\nI've illustrated the issue with small test - you may ensure that if e.g.\nthe stream is initially empty, the\n`subscription_is_dropped_when_stream_is_empty` will hang because\n`pipe_from_stream` never exits.",
          "timestamp": "2024-05-21T10:41:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d54feeb101b3779422323224c8e1ac43d3a1fafa"
        },
        "date": 1716294903886,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63553.780000000006,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.8,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 11.071536160509979,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.693405060809855,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.926619192420109,
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
          "id": "e0e1f2d6278885d1ffebe3263315089e48572a26",
          "message": "Bridge: added force_set_pallet_state call to pallet-bridge-grandpa (#4465)\n\ncloses https://github.com/paritytech/parity-bridges-common/issues/2963\n\nSee issue above for rationale\nI've been thinking about adding similar calls to other pallets, but:\n- for parachains pallet I haven't been able to think of a case when we\nwill need that given how long referendum takes. I.e. if storage proof\nformat changes and we want to unstuck the bridge, it'll take a large a\nfew weeks to sync a single parachain header, then another weeks for\nanother and etc.\n- for messages pallet I've made the similar call initially, but it just\nchanges a storage key (`OutboundLanes` and/or `InboundLanes`), so\nthere's no any logic here and it may be simply done using\n`system.set_storage`.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-21T13:46:06Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e0e1f2d6278885d1ffebe3263315089e48572a26"
        },
        "date": 1716306175823,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63550.52999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.928453114420087,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.995840572450031,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.537354433450137,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dmitry Markin",
            "username": "dmitry-markin",
            "email": "dmitry@markin.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d05786ffb5523c334f10d16870c2e73674661a52",
          "message": "Replace `Multiaddr` & related types with substrate-specific types (#4198)\n\nThis PR introduces custom types / substrate wrappers for `Multiaddr`,\n`multiaddr::Protocol`, `Multihash`, `ed25519::*` and supplementary types\nlike errors and iterators.\n\nThis is needed to unblock `libp2p` upgrade PR\nhttps://github.com/paritytech/polkadot-sdk/pull/1631 after\nhttps://github.com/paritytech/polkadot-sdk/pull/2944 was merged.\n`libp2p` and `litep2p` currently depend on different versions of\n`multiaddr` crate, and introduction of this \"common ground\" types is\nneeded to support independent version upgrades of `multiaddr` and\ndependent crates in `libp2p` & `litep2p`.\n\nWhile being just convenient to not tie versions of `libp2p` & `litep2p`\ndependencies together, it's currently not even possible to keep `libp2p`\n& `litep2p` dependencies updated to the same versions as `multiaddr` in\n`libp2p` depends on `libp2p-identity` that we can't include as a\ndependency of `litep2p`, which has it's own `PeerId` type. In the\nfuture, to keep things updated on `litep2p` side, we will likely need to\nfork `multiaddr` and make it use `litep2p` `PeerId` as a payload of\n`/p2p/...` protocol.\n\nWith these changes, common code in substrate uses these custom types,\nand `litep2p` & `libp2p` backends use corresponding libraries types.",
          "timestamp": "2024-05-21T16:10:10Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d05786ffb5523c334f10d16870c2e73674661a52"
        },
        "date": 1716313867064,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63548.22000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.90000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.1532320208801203,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.245917500669933,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.114284166720072,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Javier Viola",
            "username": "pepoviola",
            "email": "363911+pepoviola@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ec46106c33f2220d16a9dc7ad604d564d42ee009",
          "message": "chore: bump zombienet version (#4535)\n\nThis version includes the latest release of pjs/api\n(https://github.com/polkadot-js/api/releases/tag/v11.1.1).\nThx!",
          "timestamp": "2024-05-21T21:33:18Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ec46106c33f2220d16a9dc7ad604d564d42ee009"
        },
        "date": 1716332496659,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.240000000005,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.637186603939912,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.290151388159984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.380804310180065,
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
          "id": "c7cb1f25d1f8bb1a922d466e39ee935f5f027266",
          "message": "Add Extra Check in Primary Username Setter (#4534)",
          "timestamp": "2024-05-22T07:21:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c7cb1f25d1f8bb1a922d466e39ee935f5f027266"
        },
        "date": 1716368108343,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63537.97000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.8140629319002803,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.88439057956005,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.111667285029984,
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
          "id": "b06306c42c969eaa0b828413dd03dc3b7a844976",
          "message": "[Backport] Version bumps and prdocs reordering from 1.12.0 (#4538)\n\nThis PR backports version bumps and reorganisation of the prdoc files\nfrom the `1.12.0` release branch back to `master`",
          "timestamp": "2024-05-22T11:29:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b06306c42c969eaa0b828413dd03dc3b7a844976"
        },
        "date": 1716378862093,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63542.06,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.9311402595700615,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.582094827230055,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.942705505540021,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Riko",
            "username": "fasteater",
            "email": "49999458+fasteater@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ad54bc36c1b2ce9517d023f2df9d6bdec9ca64e1",
          "message": "fixed link (#4539)",
          "timestamp": "2024-05-22T11:55:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ad54bc36c1b2ce9517d023f2df9d6bdec9ca64e1"
        },
        "date": 1716384313369,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.18000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.59999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.094307171400074,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.1784341272500045,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.616803602759903,
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
          "id": "8949856d840c7f97c0c0c58a3786ccce5519a8fe",
          "message": "Refactor Nomination Pool to support multiple staking strategies (#3905)\n\nThird and final PR in the set, closes\nhttps://github.com/paritytech/polkadot-sdk/issues/454.\n\nOriginal PR: https://github.com/paritytech/polkadot-sdk/pull/2680\n\n## Precursors:\n- https://github.com/paritytech/polkadot-sdk/pull/3889.\n- https://github.com/paritytech/polkadot-sdk/pull/3904.\n\n## Follow up issues/improvements\n- https://github.com/paritytech/polkadot-sdk/issues/4404\n\nOverall changes are documented here (lot more visual 😍):\nhttps://hackmd.io/@ak0n/454-np-governance\n\n## Summary of various roles 🤯\n### Pallet Staking\n**Nominator**: An account that directly stakes on `pallet-staking` and\nnominates a set of validators.\n**Stakers**: Common term for nominators and validators.\nVirtual Stakers: Same as stakers, but they are keyless accounts and\ntheir locks are managed by a pallet external to `pallet-staking`.\n\n### Pallet Delegated Staking\n**Agent**: An account that receives delegation from other accounts\n(delegators) and stakes on their behalf. They are also Virtual Stakers\nin `pallet-staking` where `pallet-delegated-staking` manages its locks.\n**Delegator**: An account that delegates some funds to an agent.\n\n### Pallet Nomination Pools\n**Pool account**: Keyless account of a pool where funds are pooled.\nMembers pledge their funds towards the pools. These are going to become\n`Agent` accounts in `pallet-delegated-staking`.\n**Pool Members**: They are individual members of the pool who\ncontributed funds to it. They are also `Delegator` in\n`pallet-delegated-staking`.\n\n## Changes\n### Multiple Stake strategies\n\n**TransferStake**: The current nomination pool logic can be considered a\nstaking strategy where delegators transfer funds to pool and stake. In\nthis scenario, funds are locked in pool account, and users lose the\ncontrol of their funds.\n\n**DelegateStake**: With this PR, we introduce a new staking strategy\nwhere individual delegators delegate fund to pool. `Delegate` implies\nfunds are locked in delegator account itself. Important thing to note\nis, pool does not have funds of its own, but it has authorization from\nits members to use these funds for staking.\n\nWe extract out all the interaction of pool with staking interface into a\nnew trait `StakeStrategy`. This is the logic that varies between the\nabove two staking strategies. We use the trait `StakeStrategy` to\nimplement above two strategies: `TransferStake` and `DelegateStake`.\n\n### NominationPool\nConsumes an implementation of `StakeStrategy` instead of\n`StakingInterface`. I have renamed it from `Staking` to `StakeAdapter`\nto clarify the difference from the earlier used trait.\n\nTo enable delegation based staking in pool, Nomination pool can be\nconfigured as:\n```\ntype StakeAdapter = pallet_nomination_pools::adapter::DelegateStake<Self, DelegatedStaking>;\n```\n\nNote that with the following configuration, the changes in the PR are\nno-op.\n```\ntype StakeAdapter = pallet_nomination_pools::adapter::TransferStake<Self, Staking>;\n```\n\n## Deployment roadmap\nPlan to enable this only in Westend. In production runtimes, we can keep\npool to use `TransferStake` which will be no functional change.\n\nOnce we have a full audit, we can enable this in Kusama followed by\nPolkadot.\n\n## TODO\n- [x] Runtime level (Westend) migration for existing nomination pools.\n- [x] Permissionless call/ pallet::tasks for claiming delegator funds.\n- [x] Add/update benches.\n- [x] Migration tests.\n- [x] Storage flag to mark `DelegateStake` migration and integrity\nchecks to not allow `TransferStake` for migrated runtimes.\n\n---------\n\nSigned-off-by: Matteo Muraca <mmuraca247@gmail.com>\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nSigned-off-by: Adrian Catangiu <adrian@parity.io>\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nSigned-off-by: divdeploy <chenguangxue@outlook.com>\nSigned-off-by: dependabot[bot] <support@github.com>\nSigned-off-by: hongkuang <liurenhong@outlook.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: gemini132 <164285545+gemini132@users.noreply.github.com>\nCo-authored-by: Matteo Muraca <56828990+muraca@users.noreply.github.com>\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Alexandru Gheorghe <49718502+alexggh@users.noreply.github.com>\nCo-authored-by: Alessandro Siniscalchi <asiniscalchi@gmail.com>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>\nCo-authored-by: Ross Bulat <ross@parity.io>\nCo-authored-by: Serban Iorga <serban@parity.io>\nCo-authored-by: s0me0ne-unkn0wn <48632512+s0me0ne-unkn0wn@users.noreply.github.com>\nCo-authored-by: Sam Johnson <sam@durosoft.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: Alexandru Vasile <60601340+lexnv@users.noreply.github.com>\nCo-authored-by: Niklas Adolfsson <niklasadolfsson1@gmail.com>\nCo-authored-by: Dastan <88332432+dastansam@users.noreply.github.com>\nCo-authored-by: Clara van Staden <claravanstaden64@gmail.com>\nCo-authored-by: Ron <yrong1997@gmail.com>\nCo-authored-by: Vincent Geddes <vincent@snowfork.com>\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>\nCo-authored-by: Michal Kucharczyk <1728078+michalkucharczyk@users.noreply.github.com>\nCo-authored-by: Dino Pačandi <3002868+Dinonard@users.noreply.github.com>\nCo-authored-by: Andrei Eres <eresav@me.com>\nCo-authored-by: Alin Dima <alin@parity.io>\nCo-authored-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <info@kchr.de>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: gupnik <nikhilgupta.iitk@gmail.com>\nCo-authored-by: Vladimir Istyufeev <vladimir@parity.io>\nCo-authored-by: Lulu <morgan@parity.io>\nCo-authored-by: Juan Girini <juangirini@gmail.com>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>\nCo-authored-by: Shawn Tabrizi <shawntabrizi@gmail.com>\nCo-authored-by: Kutsal Kaan Bilgin <kutsalbilgin@gmail.com>\nCo-authored-by: Ermal Kaleci <ermalkaleci@gmail.com>\nCo-authored-by: ordian <write@reusable.software>\nCo-authored-by: divdeploy <166095818+divdeploy@users.noreply.github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\nCo-authored-by: Sergej Sakac <73715684+Szegoo@users.noreply.github.com>\nCo-authored-by: Squirrel <gilescope@gmail.com>\nCo-authored-by: HongKuang <166261675+HongKuang@users.noreply.github.com>\nCo-authored-by: Tsvetomir Dimitrov <tsvetomir@parity.io>\nCo-authored-by: Egor_P <egor@parity.io>\nCo-authored-by: Aaro Altonen <48052676+altonen@users.noreply.github.com>\nCo-authored-by: Dmitry Markin <dmitry@markin.tech>\nCo-authored-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Léa Narzis <78718413+lean-apple@users.noreply.github.com>\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: georgepisaltu <52418509+georgepisaltu@users.noreply.github.com>\nCo-authored-by: command-bot <>\nCo-authored-by: PG Herveou <pgherveou@gmail.com>\nCo-authored-by: jimwfs <wqq1479787@163.com>\nCo-authored-by: jimwfs <169986508+jimwfs@users.noreply.github.com>\nCo-authored-by: polka.dom <polkadotdom@gmail.com>",
          "timestamp": "2024-05-22T19:26:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8949856d840c7f97c0c0c58a3786ccce5519a8fe"
        },
        "date": 1716412020021,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63549.580000000016,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.7,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.479979130130054,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.108544063310095,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.861040778459952,
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
          "id": "fd161917108a14e791c1444ea1c767e9f6134bdf",
          "message": "Fix README.md Logo URL (#4546)\n\nThis one also works and it is easier.",
          "timestamp": "2024-05-23T08:03:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fd161917108a14e791c1444ea1c767e9f6134bdf"
        },
        "date": 1716456929999,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63550.630000000005,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.063409805489913,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.555947471349903,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3171491323201314,
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
          "id": "493ba5e2a144140d5018647b25744b0aac854ffd",
          "message": "Contracts: Rework host fn benchmarks (#4233)\n\nfix https://github.com/paritytech/polkadot-sdk/issues/4163\n\nThis PR does the following:\nUpdate to pallet-contracts-proc-macro: \n- Parse #[cfg] so we can add a dummy noop host function for benchmark.\n- Generate BenchEnv::<host_fn> so we can call host functions directly in\nthe benchmark.\n- Add the weight of the noop host function before calling the host\nfunction itself\n\nUpdate benchmarks:\n- Update all host function benchmark, a host function benchmark now\nsimply call the host function, instead of invoking the function n times\nfrom within a contract.\n- Refactor RuntimeCosts & Schedule, for most host functions, we can now\nuse the generated weight function directly instead of computing the diff\nwith the cost! macro\n\n```rust\n// Before\n#[benchmark(pov_mode = Measured)]\nfn seal_input(r: Linear<0, API_BENCHMARK_RUNS>) {\n    let code = WasmModule::<T>::from(ModuleDefinition {\n        memory: Some(ImportedMemory::max::<T>()),\n        imported_functions: vec![ImportedFunction {\n            module: \"seal0\",\n            name: \"seal_input\",\n            params: vec![ValueType::I32, ValueType::I32],\n            return_type: None,\n        }],\n        data_segments: vec![DataSegment { offset: 0, value: 0u32.to_le_bytes().to_vec() }],\n        call_body: Some(body::repeated(\n            r,\n            &[\n                Instruction::I32Const(4), // ptr where to store output\n                Instruction::I32Const(0), // ptr to length\n                Instruction::Call(0),\n            ],\n        )),\n        ..Default::default()\n    });\n\n    call_builder!(func, code);\n\n    let res;\n    #[block]\n    {\n        res = func.call();\n    }\n    assert_eq!(res.did_revert(), false);\n}\n```\n\n```rust\n// After\nfn seal_input(n: Linear<0, { code::max_pages::<T>() * 64 * 1024 - 4 }>) {\n    let mut setup = CallSetup::<T>::default();\n    let (mut ext, _) = setup.ext();\n    let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![42u8; n as usize]);\n    let mut memory = memory!(n.to_le_bytes(), vec![0u8; n as usize],);\n    let result;\n    #[block]\n    {\n        result = BenchEnv::seal0_input(&mut runtime, &mut memory, 4, 0)\n    }\n    assert_ok!(result);\n    assert_eq!(&memory[4..], &vec![42u8; n as usize]);\n}\n``` \n\n[Weights\ncompare](https://weights.tasty.limo/compare?unit=weight&ignore_errors=true&threshold=10&method=asymptotic&repo=polkadot-sdk&old=master&new=pg%2Frework-host-benchs&path_pattern=substrate%2Fframe%2Fcontracts%2Fsrc%2Fweights.rs%2Cpolkadot%2Fruntime%2F*%2Fsrc%2Fweights%2F**%2F*.rs%2Cpolkadot%2Fbridges%2Fmodules%2F*%2Fsrc%2Fweights.rs%2Ccumulus%2F**%2Fweights%2F*.rs%2Ccumulus%2F**%2Fweights%2Fxcm%2F*.rs%2Ccumulus%2F**%2Fsrc%2Fweights.rs)\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2024-05-23T11:17:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/493ba5e2a144140d5018647b25744b0aac854ffd"
        },
        "date": 1716464310909,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63548.14999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.8,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.917019355370109,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.907696877509972,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.37850907923002,
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
          "id": "03bbc17e92d1d04b6b4b9aef7669c403d08bc28c",
          "message": "Define `OpaqueValue` (#4550)\n\nDefine `OpaqueValue` and use it instead of\n`grandpa::OpaqueKeyOwnershipProof` and `beefy:OpaqueKeyOwnershipProof`\n\nRelated to\nhttps://github.com/paritytech/polkadot-sdk/pull/4522#discussion_r1608278279\n\nWe'll need to introduce a runtime API method that calls the\n`report_fork_voting_unsigned()` extrinsic. This method will need to\nreceive the ancestry proof as a paramater. I'm still not sure, but there\nis a chance that we'll send the ancestry proof as an opaque type.\n\nSo let's introduce this `OpaqueValue`. We can already use it to replace\n`grandpa::OpaqueKeyOwnershipProof` and `beefy:OpaqueKeyOwnershipProof`\nand maybe we'll need it for the ancestry proof as well.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-23T12:38:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/03bbc17e92d1d04b6b4b9aef7669c403d08bc28c"
        },
        "date": 1716473233605,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63550.27,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.787258560450051,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.407313604100002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.795471634730151,
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
          "id": "48d4f654612a67787426de426e462bd40f6f70f6",
          "message": "Mention new XCM docs in sdk docs (#4558)\n\nThe XCM docs were pretty much moved to the new rust docs format in\nhttps://github.com/paritytech/polkadot-sdk/pull/2633, with the addition\nof the XCM cookbook, which I plan to add more examples to shortly.\n\nThese docs were not mentioned in the polkadot-sdk rust docs, this PR\njust mentions them there, so people can actually find them.",
          "timestamp": "2024-05-23T21:04:41Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/48d4f654612a67787426de426e462bd40f6f70f6"
        },
        "date": 1716503335473,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63551.17999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.59999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.8321321595801514,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.853666724259945,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.400572957699922,
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
          "id": "700d5910580fdc17a0737925d4fe2472eb265f82",
          "message": "Use polkadot-ckb-merkle-mountain-range dependency (#4562)\n\nWe need to use the `polkadot-ckb-merkle-mountain-range` dependency\npublished on `crates.io` in order to unblock the release of the\n`sp-mmr-primitives` crate",
          "timestamp": "2024-05-24T07:43:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/700d5910580fdc17a0737925d4fe2472eb265f82"
        },
        "date": 1716542019623,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63547.2,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.59999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.230676135610021,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.955009803420065,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0941640435500823,
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
          "id": "ef144b1a88c6478e5d6dac945ffe12053f05d96a",
          "message": "Attempt to avoid specifying `BlockHashCount` for different `mocking::{MockBlock, MockBlockU32, MockBlockU128}` (#4543)\n\nWhile doing some migration/rebase I came in to the situation, where I\nneeded to change `mocking::MockBlock` to `mocking::MockBlockU32`:\n```\n#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]\nimpl frame_system::Config for TestRuntime {\n\ttype Block = frame_system::mocking::MockBlockU32<TestRuntime>;\n\ttype AccountData = pallet_balances::AccountData<ThisChainBalance>;\n}\n```\nBut actual `TestDefaultConfig` for `frame_system` is using `ConstU64`\nfor `type BlockHashCount = frame_support::traits::ConstU64<10>;`\n[here](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/system/src/lib.rs#L303).\nBecause of this, it force me to specify and add override for `type\nBlockHashCount = ConstU32<10>`.\n\nThis PR tries to fix this with `TestBlockHashCount` implementation for\n`TestDefaultConfig` which supports `u32`, `u64` and `u128` as a\n`BlockNumber`.\n\n### How to simulate error\nJust by removing `type BlockHashCount = ConstU32<250>;`\n[here](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/multisig/src/tests.rs#L44)\n```\n:~/parity/olkadot-sdk$ cargo test -p pallet-multisig\n   Compiling pallet-multisig v28.0.0 (/home/bparity/parity/aaa/polkadot-sdk/substrate/frame/multisig)\nerror[E0277]: the trait bound `ConstU64<10>: frame_support::traits::Get<u32>` is not satisfied\n   --> substrate/frame/multisig/src/tests.rs:41:1\n    |\n41  | #[derive_impl(frame_system::config_preludes::TestDefaultConfig)]\n    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the trait `frame_support::traits::Get<u32>` is not implemented for `ConstU64<10>`\n    |\n    = help: the following other types implement trait `frame_support::traits::Get<T>`:\n              <ConstU64<T> as frame_support::traits::Get<u64>>\n              <ConstU64<T> as frame_support::traits::Get<std::option::Option<u64>>>\nnote: required by a bound in `frame_system::Config::BlockHashCount`\n   --> /home/bparity/parity/aaa/polkadot-sdk/substrate/frame/system/src/lib.rs:535:24\n    |\n535 |         type BlockHashCount: Get<BlockNumberFor<Self>>;\n    |                              ^^^^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `Config::BlockHashCount`\n    = note: this error originates in the attribute macro `derive_impl` which comes from the expansion of the macro `frame_support::macro_magic::forward_tokens_verbatim` (in Nightly builds, run with -Z macro-backtrace for more info)\n\nFor more information about this error, try `rustc --explain E0277`.\nerror: could not compile `pallet-multisig` (lib test) due to 1 previous error \n```\n\n\n\n\n## For reviewers:\n\n(If there is a better solution, please let me know!)\n\nThe first commit contains actual attempt to fix the problem:\nhttps://github.com/paritytech/polkadot-sdk/commit/3c5499e539f2218503fbd6ce9be085b03c31ee13.\nThe second commit is just removal of `BlockHashCount` from all other\nplaces where not needed by default.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/1657\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-24T10:01:10Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ef144b1a88c6478e5d6dac945ffe12053f05d96a"
        },
        "date": 1716551086315,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63544.719999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52938.40000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.614414799830021,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.317904105969944,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.325840641310068,
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
          "id": "49bd6a6e94b8b6f4ef3497e930cfb493b8ec0fd0",
          "message": "Remove litep2p git dependency (#4560)\n\n@serban300 could you please do the same for the MMR crate? Am not sure\nwhat commit was released since there are no release tags in the repo.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-05-24T11:55:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/49bd6a6e94b8b6f4ef3497e930cfb493b8ec0fd0"
        },
        "date": 1716557186936,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63540.68000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.40000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.053455031159954,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.394019798470196,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.591572782869983,
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
          "id": "f469fbfb0a44c4e223488b07ec641ca02b2fb8f1",
          "message": "availability-recovery: bump chunk fetch threshold to 1MB for Polkadot and 4MB for Kusama + testnets (#4399)\n\nDoing this change ensures that we minimize the CPU usage we spend in\nreed-solomon by only doing the re-encoding into chunks if PoV size is\nless than 4MB (which means all PoVs right now)\n \nBased on susbystem benchmark results we concluded that it is safe to\nbump this number higher. At worst case scenario the network pressure for\na backing group of 5 is around 25% of the network bandwidth in hw specs.\n\nAssuming 6s block times (max_candidate_depth 3) and needed_approvals 30\nthe amount of bandwidth usage of a backing group used would hover above\n`30 * 4 * 3 = 360MB` per relay chain block. Given a backing group of 5\nthat gives 72MB per block per validator -> 12 MB/s.\n\n<details>\n<summary>Reality check on Kusama PoV sizes (click for chart)</summary>\n<br>\n<img width=\"697\" alt=\"Screenshot 2024-05-07 at 14 30 38\"\nsrc=\"https://github.com/paritytech/polkadot-sdk/assets/54316454/bfed32d4-8623-48b0-9ec0-8b95dd2a9d8c\">\n</details>\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-05-24T14:14:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f469fbfb0a44c4e223488b07ec641ca02b2fb8f1"
        },
        "date": 1716562383773,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63548.30000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.2,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.1797398424300054,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.393357452410045,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.129949339759937,
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
          "id": "e192b764971f99975e876380f9ebbf2c08f0c17d",
          "message": "Avoid using `xcm::v4` and use latest instead for AssetHub benchmarks (#4567)",
          "timestamp": "2024-05-24T20:59:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e192b764971f99975e876380f9ebbf2c08f0c17d"
        },
        "date": 1716589582995,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.15,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.62145459936006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.994705851280081,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.056886001400048,
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
          "id": "9201f9abbe0b63abbeabc1f6e6799cca030c8c46",
          "message": "Deprecate XCMv2 (#4131)\n\nMarked XCMv2 as deprecated now that we have XCMv4.\nIt will be removed sometime around June 2024.\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-05-27T06:12:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9201f9abbe0b63abbeabc1f6e6799cca030c8c46"
        },
        "date": 1716795776808,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63548.04,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52945.3,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.849268507330008,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.157376561160122,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.363124233650007,
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
          "id": "e0edb062e55e80cf21490fb140e4bbc3b7d7c89d",
          "message": "Add release version to commits and branch names of template synchronization job (#4353)\n\nJust to have some information what is the release number that was used\nto push a particular commit or PR in the templates repositories.",
          "timestamp": "2024-05-27T08:42:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e0edb062e55e80cf21490fb140e4bbc3b7d7c89d"
        },
        "date": 1716801361179,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63544.52999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.956743409090064,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.377071841110026,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2348093536701086,
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
          "id": "2352982717edc8976b55525274b1f9c9aa01aadd",
          "message": "Make markdown lint CI job pass (#4593)\n\nWas constantly failing, so here a fix.",
          "timestamp": "2024-05-27T09:39:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2352982717edc8976b55525274b1f9c9aa01aadd"
        },
        "date": 1716810432720,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.76000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.9301640552101595,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.0080454660000076,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.637758977209918,
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
          "id": "ce3e9b7c7099034e8ee30e4c7c912e3ed068bf8a",
          "message": "network: Update litep2p to v0.5.0 (#4570)\n\n## [0.5.0] - 2023-05-24\n\nThis is a small patch release that makes the `FindNode` command a bit\nmore robst:\n\n- The `FindNode` command now retains the K (replication factor) best\nresults.\n- The `FindNode` command has been updated to handle errors and\nunexpected states without panicking.\n\n### Changed\n\n- kad: Refactor FindNode query, keep K best results and add tests\n([#114](https://github.com/paritytech/litep2p/pull/114))\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-05-27T13:55:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ce3e9b7c7099034e8ee30e4c7c912e3ed068bf8a"
        },
        "date": 1716824135577,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63544.64,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.8,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.924206964580189,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.456111503100015,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.120952072129963,
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
          "id": "70dd67a5d129745da6a05bce958824504a4c9d83",
          "message": "check-weight: Disable total pov size check for mandatory extrinsics (#4571)\n\nSo in some pallets we like\n[here](https://github.com/paritytech/polkadot-sdk/blob/5dc522d02fe0b53be1517f8b8979176e489a388b/substrate/frame/session/src/lib.rs#L556)\nwe use `max_block` as return value for `on_initialize` (ideally we would\nnot).\n\nThis means the block is already full when we try to apply the inherents,\nwhich lead to the error seen in #4559 because we are unable to include\nthe required inherents. This was not erroring before #4326 because we\nwere running into this branch:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/e4b89cc50c8d17868d6c8b122f2e156d678c7525/substrate/frame/system/src/extensions/check_weight.rs#L222-L224\n\nThe inherents are of `DispatchClass::Mandatory` and therefore have a\n`reserved` value of `None` in all runtimes I have inspected. So they\nwill always pass the normal check.\n\nSo in this PR I adjust the `check_combined_proof_size` to return an\nearly `Ok(())` for mandatory extrinsics.\n\nIf we agree on this PR I will backport it to the 1.12.0 branch.\n\ncloses #4559\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-27T17:12:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/70dd67a5d129745da6a05bce958824504a4c9d83"
        },
        "date": 1716835364126,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63547.06000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.101972947849985,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.799852743510039,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.080497561800088,
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
          "id": "a7097681b76bdaef21dcde9aec8c33205f480e44",
          "message": "[subsystem-benchmarks] Add statement-distribution benchmarks (#3863)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3748\n\nAdds a subsystem benchmark for statements-distribution subsystem.\n\nResults in CI (reference hw):\n```\n$ cargo bench -p polkadot-statement-distribution --bench statement-distribution-regression-bench --features subsystem-benchmarks\n\n[Sent to peers] standart_deviation 0.07%\n[Received from peers] standart_deviation 0.00%\n[statement-distribution] standart_deviation 0.97%\n[test-environment] standart_deviation 1.03%\n\nNetwork usage, KiB                     total   per block\nReceived from peers                1088.0000    108.8000\nSent to peers                      1238.1800    123.8180\n\nCPU usage, seconds                     total   per block\nstatement-distribution                0.3897      0.0390\ntest-environment                      0.4715      0.0472\n```",
          "timestamp": "2024-05-27T19:23:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a7097681b76bdaef21dcde9aec8c33205f480e44"
        },
        "date": 1716843498474,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.619999999995,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.90519452377008,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.98492819191019,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.199175188529946,
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
        "date": 1716850837344,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63551.55,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.214670852160019,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.788088199590057,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.287202355520223,
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
          "id": "523e62560eb5d9a36ea75851f2fb15b9d7993f01",
          "message": "Add availability-recovery from systematic chunks (#1644)\n\n**Don't look at the commit history, it's confusing, as this branch is\nbased on another branch that was merged**\n\nFixes #598 \nAlso implements [RFC\n#47](https://github.com/polkadot-fellows/RFCs/pull/47)\n\n## Description\n\n- Availability-recovery now first attempts to request the systematic\nchunks for large POVs (which are the first ~n/3 chunks, which can\nrecover the full data without doing the costly reed-solomon decoding\nprocess). This has a fallback of recovering from all chunks, if for some\nreason the process fails. Additionally, backers are also used as a\nbackup for requesting the systematic chunks if the assigned validator is\nnot offering the chunk (each backer is only used for one systematic\nchunk, to not overload them).\n- Quite obviously, recovering from systematic chunks is much faster than\nrecovering from regular chunks (4000% faster as measured on my apple M2\nPro).\n- Introduces a `ValidatorIndex` -> `ChunkIndex` mapping which is\ndifferent for every core, in order to avoid only querying the first n/3\nvalidators over and over again in the same session. The mapping is the\none described in RFC 47.\n- The mapping is feature-gated by the [NodeFeatures runtime\nAPI](https://github.com/paritytech/polkadot-sdk/pull/2177) so that it\ncan only be enabled via a governance call once a sufficient majority of\nvalidators have upgraded their client. If the feature is not enabled,\nthe mapping will be the identity mapping and backwards-compatibility\nwill be preserved.\n- Adds a new chunk request protocol version (v2), which adds the\nChunkIndex to the response. This may or may not be checked against the\nexpected chunk index. For av-distribution and systematic recovery, this\nwill be checked, but for regular recovery, no. This is backwards\ncompatible. First, a v2 request is attempted. If that fails during\nprotocol negotiation, v1 is used.\n- Systematic recovery is only attempted during approval-voting, where we\nhave easy access to the core_index. For disputes and collator\npov_recovery, regular chunk requests are used, just as before.\n\n## Performance results\n\nSome results from subsystem-bench:\n\nwith regular chunk recovery: CPU usage per block 39.82s\nwith recovery from backers: CPU usage per block 16.03s\nwith systematic recovery: CPU usage per block 19.07s\n\nEnd-to-end results here:\nhttps://github.com/paritytech/polkadot-sdk/issues/598#issuecomment-1792007099\n\n#### TODO:\n\n- [x] [RFC #47](https://github.com/polkadot-fellows/RFCs/pull/47)\n- [x] merge https://github.com/paritytech/polkadot-sdk/pull/2177 and\nrebase on top of those changes\n- [x] merge https://github.com/paritytech/polkadot-sdk/pull/2771 and\nrebase\n- [x] add tests\n- [x] preliminary performance measure on Versi: see\nhttps://github.com/paritytech/polkadot-sdk/issues/598#issuecomment-1792007099\n- [x] Rewrite the implementer's guide documentation\n- [x] https://github.com/paritytech/polkadot-sdk/pull/3065 \n- [x] https://github.com/paritytech/zombienet/issues/1705 and fix\nzombienet tests\n- [x] security audit\n- [x] final versi test and performance measure\n\n---------\n\nSigned-off-by: alindima <alin@parity.io>\nCo-authored-by: Javier Viola <javier@parity.io>",
          "timestamp": "2024-05-28T08:15:50Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/523e62560eb5d9a36ea75851f2fb15b9d7993f01"
        },
        "date": 1716887622288,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.47000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.30752719854011,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.824472529999935,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.641507654750183,
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
        "date": 1716900925347,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.46,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.282869301339998,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.764633096330009,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.5715553056102465,
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
        "date": 1716912182038,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63552.659999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.7,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.714736212550081,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.368842663190083,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.894941225920233,
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
        "date": 1716918905127,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63544.68000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.7,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.721262702840079,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.20890958172,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1903572560101097,
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
        "date": 1716924572455,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63544.06000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52938.5,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.56989565830003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.662920402650277,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.939336311630119,
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
        "date": 1716959370014,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63543.9,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.59999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.315381373699834,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.088963145210085,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.010867965479934,
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
        "date": 1716967337494,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63550.11,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.771492682640057,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 4.021292192630169,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.541920819439959,
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
        "date": 1716977160708,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63551.079999999994,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.230803501570014,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.258283207810135,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.698717999579893,
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
        "date": 1716983196665,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63547,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.2157152798201984,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.1885301086699815,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.468995513410032,
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
        "date": 1717018213933,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63554.53999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.127901098799928,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.61366667835996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4876330171001824,
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
        "date": 1717023212223,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63550.409999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.693484378820008,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.171930262580127,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.189827579339986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "drskalman",
            "username": "drskalman",
            "email": "35698397+drskalman@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "bcab07a8c63687a148f19883688c50a9fa603091",
          "message": "Beefy client generic on aduthority Id (#1816)\n\nRevived version of https://github.com/paritytech/substrate/pull/13311 .\nExcept Signature is not generic and is dictated by AuthorityId.\n\n---------\n\nCo-authored-by: Davide Galassi <davxy@datawok.net>\nCo-authored-by: Robert Hambrock <roberthambrock@gmail.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-05-30T09:31:39Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bcab07a8c63687a148f19883688c50a9fa603091"
        },
        "date": 1717067589249,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63540.16000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.3139209798701486,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.383507485549979,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.960458793990032,
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
        "date": 1717126591136,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63548.3,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.959758548790157,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.208205212680014,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.904960204020003,
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
        "date": 1717137096257,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63551.29,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.2,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.922651667429987,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.283268073970016,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.99362123320011,
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
        "date": 1717142779539,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63547.7,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.679317829949907,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.192351853430073,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1723129015200624,
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
        "date": 1717156299614,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63548.08999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 8.038045056569993,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.348325275789893,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.844264029350183,
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
        "date": 1717175484487,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.8,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.2,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 8.054261614340039,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6903176934502717,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.606826697999894,
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
        "date": 1717360587146,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63541.61,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.7,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.876313380980187,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.809050838639948,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.337600516870129,
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
        "date": 1717364608470,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63547.04,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.934420523650077,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.191018483569914,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6323600010301353,
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
        "date": 1717409271306,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63554.17999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.9018704276302065,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.854449073079984,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.247959443749998,
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
          "id": "73ac7375a5421bbc142bef232ab23d221ead64c2",
          "message": "Fix umbrella CI check and fix the C&P message (#4670)",
          "timestamp": "2024-06-03T11:04:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/73ac7375a5421bbc142bef232ab23d221ead64c2"
        },
        "date": 1717418697715,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63549.219999999994,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.822866798620051,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.351183175950036,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.291119199760076,
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
        "date": 1717427732838,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63548.15,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.362536065780023,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.74139156273,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.309956572010175,
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
          "id": "09de7f157e30f3b9fa2880d298144cb251dd5958",
          "message": "Format the README.md files (#4688)\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-06-04T07:45:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/09de7f157e30f3b9fa2880d298144cb251dd5958"
        },
        "date": 1717488763949,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63543.08,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.531541389369995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1140137705500814,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.062571443189954,
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
        "date": 1717500927755,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63543.71000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.417209189649824,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.973542470919945,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.031371674170141,
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
        "date": 1717513226398,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.479999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.218046809720012,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.707519350640036,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2672337660501554,
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
        "date": 1717529828281,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63546.02,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52938.40000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.299292141040168,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.734819543859954,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.288513080389859,
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
        "date": 1717578686068,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63546.33,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.3,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.0527278344899385,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0941771376700773,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.471448659539963,
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
        "date": 1717586845392,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.5,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.543783748959896,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.29719559575022,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.279748065420044,
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
        "date": 1717593833328,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63548.52,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.59999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.252302674100088,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.952537704200036,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0270463027301315,
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
        "date": 1717603981720,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63548.54,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.830785874930056,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.330266330239999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.342014902300138,
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
        "date": 1717617146560,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63549.17,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.8,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.1586879473401157,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.161637523669978,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.717921984570115,
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
        "date": 1717691314609,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63545.2,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.4988494454702446,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.548063181189974,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.890217099679903,
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
        "date": 1717704689661,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63539.340000000004,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.8,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.0147633474300837,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.924857882679956,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.241718071089954,
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
        "date": 1717707268105,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63544.83,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.232204521820007,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.578427781110125,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2468090918501202,
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
        "date": 1717709933493,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.65,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.2,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.157378245189986,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.504827208150042,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2073581635202695,
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
          "id": "9dfe0fee74ce1e4b7f99c1a5122b635aa43a1e5f",
          "message": "Fix occupied core handling (#4691)\n\nCo-authored-by: eskimor <eskimor@no-such-url.com>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2024-06-07T10:50:30Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9dfe0fee74ce1e4b7f99c1a5122b635aa43a1e5f"
        },
        "date": 1717759026321,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63545.45,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.5550583428701747,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.07676580231996,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.739985368550002,
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
        "date": 1717766136214,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52945.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63550.5,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.968444493780028,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.375475116179963,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0435649992801803,
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
        "date": 1717777087538,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.759999999995,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.011561057319993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.375985634040159,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.408637095880056,
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
        "date": 1717838307587,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63552.37000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.3,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.323517483519922,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.993900192190175,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.930093513160021,
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
        "date": 1717852135152,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63545.96,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.28747961285996,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.876859750159946,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9370818035402304,
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
        "date": 1718017943100,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63549.420000000006,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.198643846029954,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.651033059570064,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.212166007400122,
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
        "date": 1718029478099,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.659999999996,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.963621416450162,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.292298246970091,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.857725576589925,
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
        "date": 1718116820033,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63548.32000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.59999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.539462959250038,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.091391929449873,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.444230754370083,
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
        "date": 1718149634327,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63547.2,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.59999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.765494947829927,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.190097293810223,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.138671873559974,
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
        "date": 1718208521720,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63546.65,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.90000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.407019395190151,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.5192890061,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.176769134619965,
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
        "date": 1718252775092,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52934.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63539.96000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.898188210150001,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.318749699889889,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.037390468950111,
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
        "date": 1718274153802,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52936.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63542.21,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.001744056569985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.109566849930148,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.360414324940006,
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
        "date": 1718301569898,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.02,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.2674886628901385,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.155648623550102,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.811336555650005,
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
        "date": 1718352308466,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52945,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63550.840000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.349105097360183,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.935911757529917,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.380523744189955,
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
          "id": "977254ccb1afca975780987ff9f19f356e99378f",
          "message": "Bridges - changes for Bridges V2 - relay client part (#4494)\n\nContains mainly changes/nits/refactors related to the relayer code\n(`client-substrate` and `lib-substrate-relay`) migrated from the Bridges\nV2 [branch](https://github.com/paritytech/polkadot-sdk/pull/4427).\n\nRelates to:\nhttps://github.com/paritytech/parity-bridges-common/issues/2976\nCompanion: https://github.com/paritytech/parity-bridges-common/pull/2988\n\n\n## TODO\n- [x] fix comments\n\n## Questions\n- [x] Do we need more testing for client V2 stuff? If so, how/what is\nthe ultimate test? @svyatonik\n- [x] check\n[comment](https://github.com/paritytech/polkadot-sdk/pull/4494#issuecomment-2117181144)\nfor more testing\n\n---------\n\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>\nCo-authored-by: Serban Iorga <serban@parity.io>",
          "timestamp": "2024-06-14T11:30:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/977254ccb1afca975780987ff9f19f356e99378f"
        },
        "date": 1718370217741,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63554.609999999986,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.526871195140175,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.686127097840047,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.39121992768012,
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
        "date": 1718374918214,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.9,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.2,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.5818529633801846,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.78037923621992,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.213121540560044,
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
        "date": 1718600517277,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63541.11000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.121080589649987,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.617194020469928,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1930408402001706,
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
        "date": 1718630342208,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.21000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.2,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.087246896820188,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.0411818510100135,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.556652814509956,
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
        "date": 1718643137446,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63552.409999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.5,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 8.097260994689977,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.7968603961201937,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.714941321930064,
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
        "date": 1718655974207,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63554.39,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.5,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.62921363210991,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.14774842490007,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.8046545736901636,
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
        "date": 1718668988715,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63546.81,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.238794083030057,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.289278355740054,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.826018924,
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
        "date": 1718703192132,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.72000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.874306870289952,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.315425670490116,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.991006914380159,
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
        "date": 1718718520252,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63544.56999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.70097270847,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.201733687410012,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.21615149713017,
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
        "date": 1718815707011,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63553.119999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.90000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.863898759150036,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.564267642539999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 4.110561763360249,
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
        "date": 1718879801649,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63549.619999999995,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.110723864519997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.218383392750134,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.545777927650034,
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
        "date": 1718960088522,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63543.83999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.5,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.288932295959976,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.8361939580099875,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.996202384440138,
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
        "date": 1718969766979,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63543.93000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52937.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.501351819750123,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.695427293180238,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.753536444880015,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dmitry Markin",
            "username": "dmitry-markin",
            "email": "dmitry@markin.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3b3a1d2b99512aa3bb52a2af6fe6adc8c63ac984",
          "message": "sc-network-types: implement `From<IpAddr> for Multiaddr` (#4855)\n\nAdd `From` implementation used by downstream project.\n\nRef.\nhttps://github.com/paritytech/polkadot-sdk/pull/4198#discussion_r1648676102\n\nCC @nazar-pc",
          "timestamp": "2024-06-21T14:38:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3b3a1d2b99512aa3bb52a2af6fe6adc8c63ac984"
        },
        "date": 1718983166450,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63545.54,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52936.8,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.241604231780078,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.187141284289983,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.718201077269976,
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
        "date": 1718988888942,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63552.770000000004,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.40000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 10.000618212120017,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 5.0319300388002794,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 12.500398158309943,
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
        "date": 1719071058627,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63550.87999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 12.142237593610112,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 9.638549754010011,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 4.83201469806029,
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
        "date": 1719140642898,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63542.37000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.708812270940026,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.201256159110041,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.297835754270108,
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
        "date": 1719148222833,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63542.409999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52936.59999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.297929376860174,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.179568960989949,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.607189222469868,
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
          "id": "7df94a469e02e1d553bd4050b0e91870d6a4c31b",
          "message": "Dont publish example pallets (#4861)\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-06-24T09:16:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7df94a469e02e1d553bd4050b0e91870d6a4c31b"
        },
        "date": 1719226313958,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63548.55000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.851494898030021,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.260658322589915,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0272067420301116,
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
        "date": 1719234951665,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63544.569999999985,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.948956350090043,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.378985339300037,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0932228218401407,
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
        "date": 1719247998191,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63535.329999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.59999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.822717705420068,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.25891932744993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9982344279001203,
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
        "date": 1719312078277,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63847.82000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52938.3,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.849407058520227,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.77000638471994,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.185767882629937,
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
        "date": 1719322350607,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52946,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63791.32000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.0661164511600867,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.973518079930001,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.554348648850095,
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
        "date": 1719329405919,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63840.17999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.7,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 11.384610949530009,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.407066608819997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 4.034281785070384,
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
        "date": 1719391776672,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63790.62999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.080231756710039,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.74150943266985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1750376122001325,
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
        "date": 1719403319840,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63787.92999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.847870962689943,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.521280193419999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.956825667490209,
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
        "date": 1719410133447,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63805.42,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.166100785310004,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.39160927436004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.348505258390227,
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
        "date": 1719425832619,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63786.27,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.473217807059948,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.8887178844701515,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.835994400470007,
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
        "date": 1719492744862,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63798.530000000006,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.616864213319865,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0183668817602296,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.92817120592005,
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
        "date": 1719499641863,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63798.530000000006,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.616864213319865,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0183668817602296,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.92817120592005,
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
        "date": 1719500839064,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63798.530000000006,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.616864213319865,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0183668817602296,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.92817120592005,
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
        "date": 1719508924597,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63823.35,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.90000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.6064868152801663,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.750918739249974,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.347348968209982,
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
        "date": 1719560035791,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63806.72000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.90000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.129595558990192,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.086575974659942,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.792857457609909,
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
        "date": 1719567625266,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63849.740000000005,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 8.309552342189985,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.026197997690101,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.8928016980501754,
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
        "date": 1719588037709,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52949.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63828.89,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.116056651830101,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.008501508239863,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.17539683521016,
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
        "date": 1719844564370,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63797.73,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.7,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.60845281101998,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.960109685819968,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0942155780101643,
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
        "date": 1719851788043,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63807.619999999995,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.316156019210089,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.495429709099956,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4135794316302444,
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
        "date": 1719869339051,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63833.29999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.877469235229958,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.929156631030223,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.26009080095996,
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
          "id": "62b955e98d36e4cfce680fddc967e3821b83c994",
          "message": "Fix markdown lint step (#4933)\n\nCI required markdown step seems to start failing after\nhttps://github.com/paritytech/polkadot-sdk/pull/4806\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-07-03T08:03:41Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/62b955e98d36e4cfce680fddc967e3821b83c994"
        },
        "date": 1719999826142,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63825.990000000005,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.7953943783599255,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.6556158168498,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6514709665902645,
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
        "date": 1720004433338,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63825.26000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.362603871530029,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.903096430520078,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.630073009030224,
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
        "date": 1720009904870,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63825.26000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.362603871530029,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.903096430520078,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.630073009030224,
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
        "date": 1720011472496,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63825.26000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.362603871530029,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.903096430520078,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.630073009030224,
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
        "date": 1720020189787,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63834.41000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.3,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.579303944939971,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4918129898803256,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.696830566120042,
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
        "date": 1720032968282,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63814.990000000005,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.327691369400013,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.437999542350026,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.476866682110213,
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
        "date": 1720041301046,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52945.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63833.35,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.504063605880017,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.5248846990000957,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.705471040969994,
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
        "date": 1720048409522,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63799.07000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.2115715579201476,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.089647752979954,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.82332818880995,
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
        "date": 1720175756232,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63817.219999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.2,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.531029578620021,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.305262425979981,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4898957250102236,
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
        "date": 1720183452581,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63805.530000000006,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.8,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.958595820240054,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.065678096419978,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.147542972970156,
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
        "date": 1720209224704,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63816.130000000005,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52948.8,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.2503177489501596,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.216577049810096,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.012829667190058,
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
        "date": 1720303495387,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52947.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63805.909999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.548641876829922,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.871728116460059,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.99002942807018,
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
        "date": 1720358740029,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63837.729999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52938,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.9416755993002384,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.37718658348014,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.229155979800066,
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
        "date": 1720430365807,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63813.01000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.90000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.37559214227021,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.23045238561989,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.389962483969967,
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
        "date": 1720455540944,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63806.4,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.2,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.491269379810073,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3872929645502543,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.267455673890009,
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
        "date": 1720461572460,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63790.83,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52936.5,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.94395738975017,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.747212939219979,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.497530255079953,
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
        "date": 1720502343844,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63797.659999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.539204766300012,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.838206757860023,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9940491189501874,
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
        "date": 1720520125420,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63778.3,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.001450311909932,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1312336581301765,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.763205755850091,
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
          "id": "2f0e5a61b7739ff0f41ba124653b45ce269dee7e",
          "message": "add notices to the implementer's guide docs that changed for elastic scaling (#4983)\n\nThe update is tracked by:\nhttps://github.com/paritytech/polkadot-sdk/issues/3699\n\nHowever, this is not worth doing at this point since it will change in\nthe future for phase 2 of the implementation.\n\nStill, it's useful to let people know that the information is not the\nmost up to date.",
          "timestamp": "2024-07-09T11:36:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2f0e5a61b7739ff0f41ba124653b45ce269dee7e"
        },
        "date": 1720534271513,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63841.829999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.09999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.6779079421002647,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.8598901578800495,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.67828677357984,
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
        "date": 1720635520001,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63778.3,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.59999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.798143439370081,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.563388876439948,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.931295102820137,
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
        "date": 1720708439920,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52936.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63788.079999999994,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.080430591060052,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.921338179329922,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.181919748320156,
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
        "date": 1720749407588,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63814.030000000006,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.444424940880001,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.759988797190024,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6208916435103,
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
        "date": 1720777917155,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63802.63999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.90000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.291477753360213,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.278815974539976,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.021457435020016,
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
        "date": 1720786608885,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63806.159999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.25519561439001,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.938801651089904,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.208922351020159,
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
        "date": 1720794274248,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63844.659999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 11.019592862200101,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 8.195193040709963,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.805535842470197,
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
        "date": 1721038210349,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63836.92,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52935.2,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.621676971550206,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.896605642439994,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.713485432480052,
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
        "date": 1721046035135,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63827.869999999995,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.422220804950044,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6499207378401977,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.8629335734799835,
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
        "date": 1721057908403,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63836.759999999995,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.92653722859004,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.85518998440997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6045671559401784,
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
        "date": 1721083465007,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63787.19999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.003356535520007,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.767090341999971,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.997048049330104,
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
        "date": 1721089790802,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63791.9,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.647870674790042,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9397447906501935,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.92533255148008,
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
        "date": 1721135436461,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63806.500000000015,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.351279163750166,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.022369424229963,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.401474525830025,
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
        "date": 1721145976626,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63806.500000000015,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.351279163750166,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.022369424229963,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.401474525830025,
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
        "date": 1721160202471,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63816.490000000005,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.230015051270083,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.30039404746021,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.461340937269975,
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
        "date": 1721213951999,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63820.520000000004,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.45653077590004,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.174285653119862,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3229169266701923,
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
        "date": 1721223358211,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63812.240000000005,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.365078473320015,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.666732895079994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4640759002201853,
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
        "date": 1721236641924,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63820.4,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.5316135840902083,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.71534643122998,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.312224713799967,
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
        "date": 1721242969283,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63810.90000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.444063901080284,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.668501487509994,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.521036370600061,
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
        "date": 1721258085182,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63818.04,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.726470375109983,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4485462737602064,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.232552758729977,
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
        "date": 1721296363692,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63801.85,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52946.7,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.822086165830118,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.064041547480168,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.14934858894001,
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
          "id": "e2d3b8b1f374ceeb6b4af5b9ef9a216bba44e4ed",
          "message": "Bump the known_good_semver group with 8 updates (#5060)\n\nBumps the known_good_semver group with 8 updates:\n\n| Package | From | To |\n| --- | --- | --- |\n| [clap](https://github.com/clap-rs/clap) | `4.5.3` | `4.5.9` |\n| [log](https://github.com/rust-lang/log) | `0.4.21` | `0.4.22` |\n| [paste](https://github.com/dtolnay/paste) | `1.0.14` | `1.0.15` |\n| [quote](https://github.com/dtolnay/quote) | `1.0.35` | `1.0.36` |\n| [serde](https://github.com/serde-rs/serde) | `1.0.197` | `1.0.204` |\n| [serde_derive](https://github.com/serde-rs/serde) | `1.0.197` |\n`1.0.204` |\n| [serde_json](https://github.com/serde-rs/json) | `1.0.114` | `1.0.120`\n|\n| [serde_yaml](https://github.com/dtolnay/serde-yaml) | `0.9.33` |\n`0.9.34+deprecated` |\n\nUpdates `clap` from 4.5.3 to 4.5.9\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/clap-rs/clap/releases\">clap's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v4.5.9</h2>\n<h2>[4.5.9] - 2024-07-09</h2>\n<h3>Fixes</h3>\n<ul>\n<li><em>(error)</em> When defining a custom help flag, be sure to\nsuggest it like we do the built-in one</li>\n</ul>\n<h2>v4.5.8</h2>\n<h2>[4.5.8] - 2024-06-28</h2>\n<h3>Fixes</h3>\n<ul>\n<li>Reduce extra flushes</li>\n</ul>\n<h2>v4.5.7</h2>\n<h2>[4.5.7] - 2024-06-10</h2>\n<h3>Fixes</h3>\n<ul>\n<li>Clean up error message when too few arguments for\n<code>num_args</code></li>\n</ul>\n<h2>v4.5.6</h2>\n<h2>[4.5.6] - 2024-06-06</h2>\n<h2>v4.5.4</h2>\n<h2>[4.5.4] - 2024-03-25</h2>\n<h3>Fixes</h3>\n<ul>\n<li><em>(derive)</em> Allow non-literal <code>#[arg(id)]</code>\nattributes again</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/clap-rs/clap/blob/master/CHANGELOG.md\">clap's\nchangelog</a>.</em></p>\n<blockquote>\n<h2>[4.5.9] - 2024-07-09</h2>\n<h3>Fixes</h3>\n<ul>\n<li><em>(error)</em> When defining a custom help flag, be sure to\nsuggest it like we do the built-in one</li>\n</ul>\n<h2>[4.5.8] - 2024-06-28</h2>\n<h3>Fixes</h3>\n<ul>\n<li>Reduce extra flushes</li>\n</ul>\n<h2>[4.5.7] - 2024-06-10</h2>\n<h3>Fixes</h3>\n<ul>\n<li>Clean up error message when too few arguments for\n<code>num_args</code></li>\n</ul>\n<h2>[4.5.6] - 2024-06-06</h2>\n<h2>[4.5.5] - 2024-06-06</h2>\n<h3>Fixes</h3>\n<ul>\n<li>Allow <code>exclusive</code> to override\n<code>required_unless_present</code>,\n<code>required_unless_present_any</code>,\n<code>required_unless_present_all</code></li>\n</ul>\n<h2>[4.5.4] - 2024-03-25</h2>\n<h3>Fixes</h3>\n<ul>\n<li><em>(derive)</em> Allow non-literal <code>#[arg(id)]</code>\nattributes again</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/clap-rs/clap/commit/43e73682835653ac48f32cc786514553d697c693\"><code>43e7368</code></a>\nchore: Release</li>\n<li><a\nhref=\"https://github.com/clap-rs/clap/commit/f00dafa690479e562ef22c3ed82f17726213ee32\"><code>f00dafa</code></a>\ndocs: Update changelog</li>\n<li><a\nhref=\"https://github.com/clap-rs/clap/commit/da1093a4f4cd1abba7de0b86f39319ab86913420\"><code>da1093a</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/clap-rs/clap/issues/5574\">#5574</a>\nfrom zanieb/zb/try-help-custom</li>\n<li><a\nhref=\"https://github.com/clap-rs/clap/commit/2eb842cc3bc1fb5f04156efa816003ef803d5254\"><code>2eb842c</code></a>\nfeat: Show user defined help flags in hints</li>\n<li><a\nhref=\"https://github.com/clap-rs/clap/commit/b24deb101f7e12660b8b19d6b3979df87ffe065d\"><code>b24deb1</code></a>\ntest: Add coverage for help flag hints</li>\n<li><a\nhref=\"https://github.com/clap-rs/clap/commit/866d7d14d33a3ef1f010222f004815b5cd8c15ef\"><code>866d7d1</code></a>\nchore(deps): Update compatible (dev) (<a\nhref=\"https://redirect.github.com/clap-rs/clap/issues/5560\">#5560</a>)</li>\n<li><a\nhref=\"https://github.com/clap-rs/clap/commit/d14bbc95317eb87a115f56c455bdab6ba19342ff\"><code>d14bbc9</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/clap-rs/clap/issues/5567\">#5567</a>\nfrom epage/c</li>\n<li><a\nhref=\"https://github.com/clap-rs/clap/commit/5448020b188899601d641a2684833073aba0a669\"><code>5448020</code></a>\nfix: Install shells for CI</li>\n<li><a\nhref=\"https://github.com/clap-rs/clap/commit/1c5a625ad0303e2407c8ab83ea7d37795e69a3a5\"><code>1c5a625</code></a>\nfix: Fix wrong <code>cfg(linux)</code></li>\n<li><a\nhref=\"https://github.com/clap-rs/clap/commit/2d2d1f498731d2ab70e8f15fed3765a856d52732\"><code>2d2d1f4</code></a>\nchore: Bump completest</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/clap-rs/clap/compare/clap_complete-v4.5.3...v4.5.9\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `log` from 0.4.21 to 0.4.22\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/rust-lang/log/blob/master/CHANGELOG.md\">log's\nchangelog</a>.</em></p>\n<blockquote>\n<h2>[0.4.22] - 2024-06-27</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Add some clarifications to the library docs by <a\nhref=\"https://github.com/KodrAus\"><code>@​KodrAus</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/620\">rust-lang/log#620</a></li>\n<li>Add links to <code>colog</code> crate by <a\nhref=\"https://github.com/chrivers\"><code>@​chrivers</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/621\">rust-lang/log#621</a></li>\n<li>adding line_number test + updating some testing infrastructure by <a\nhref=\"https://github.com/DIvkov575\"><code>@​DIvkov575</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/619\">rust-lang/log#619</a></li>\n<li>Clarify the actual set of functions that can race in _racy variants\nby <a href=\"https://github.com/KodrAus\"><code>@​KodrAus</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/623\">rust-lang/log#623</a></li>\n<li>Replace deprecated std::sync::atomic::spin_loop_hint() by <a\nhref=\"https://github.com/Catamantaloedis\"><code>@​Catamantaloedis</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/625\">rust-lang/log#625</a></li>\n<li>Check usage of max_level features by <a\nhref=\"https://github.com/Thomasdezeeuw\"><code>@​Thomasdezeeuw</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/627\">rust-lang/log#627</a></li>\n<li>Remove unneeded import by <a\nhref=\"https://github.com/Thomasdezeeuw\"><code>@​Thomasdezeeuw</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/628\">rust-lang/log#628</a></li>\n<li>Loosen orderings for logger initialization in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/632\">rust-lang/log#632</a>.\nOriginally by <a\nhref=\"https://github.com/pwoolcoc\"><code>@​pwoolcoc</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/599\">rust-lang/log#599</a></li>\n<li>Use Location::caller() for file and line info in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/633\">rust-lang/log#633</a>.\nOriginally by <a\nhref=\"https://github.com/Cassy343\"><code>@​Cassy343</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/520\">rust-lang/log#520</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a href=\"https://github.com/chrivers\"><code>@​chrivers</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/621\">rust-lang/log#621</a></li>\n<li><a href=\"https://github.com/DIvkov575\"><code>@​DIvkov575</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/619\">rust-lang/log#619</a></li>\n<li><a\nhref=\"https://github.com/Catamantaloedis\"><code>@​Catamantaloedis</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/rust-lang/log/pull/625\">rust-lang/log#625</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/rust-lang/log/compare/0.4.21...0.4.22\">https://github.com/rust-lang/log/compare/0.4.21...0.4.22</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/rust-lang/log/commit/d5ba2cfee9b3b4ca1fcad911b7f59dc79eeee022\"><code>d5ba2cf</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/rust-lang/log/issues/634\">#634</a>\nfrom rust-lang/cargo/0.4.22</li>\n<li><a\nhref=\"https://github.com/rust-lang/log/commit/d1a8306aadb88d56b74c73cdce4ef0153fb549cb\"><code>d1a8306</code></a>\nprepare for 0.4.22 release</li>\n<li><a\nhref=\"https://github.com/rust-lang/log/commit/46894ef229483bbabd30a806c474417fc034559c\"><code>46894ef</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/rust-lang/log/issues/633\">#633</a>\nfrom rust-lang/feat/panic-info</li>\n<li><a\nhref=\"https://github.com/rust-lang/log/commit/e0d389c9cadd91363f2fec52bd30f9585168a89f\"><code>e0d389c</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/rust-lang/log/issues/632\">#632</a>\nfrom rust-lang/feat/loosen-atomics</li>\n<li><a\nhref=\"https://github.com/rust-lang/log/commit/c9e5e13e9b02ec80e784c6fe4deacdc8f3194fca\"><code>c9e5e13</code></a>\nuse Location::caller() for file and line info</li>\n<li><a\nhref=\"https://github.com/rust-lang/log/commit/507b672660288f0223edb6353d34f8733fa0a2f4\"><code>507b672</code></a>\nloosen orderings for logger initialization</li>\n<li><a\nhref=\"https://github.com/rust-lang/log/commit/c879b011a8ac662545adf9484d9a668ebcf9b814\"><code>c879b01</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/rust-lang/log/issues/628\">#628</a>\nfrom Thomasdezeeuw/fix-warnings</li>\n<li><a\nhref=\"https://github.com/rust-lang/log/commit/405fdb4d9f847c93c0133469ea808f09320714ba\"><code>405fdb4</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/rust-lang/log/issues/627\">#627</a>\nfrom Thomasdezeeuw/check-features</li>\n<li><a\nhref=\"https://github.com/rust-lang/log/commit/1307ade1122549badf2b8fdd10c11e519eaa029a\"><code>1307ade</code></a>\nRemove unneeded import</li>\n<li><a\nhref=\"https://github.com/rust-lang/log/commit/710560ecb7035a6baf1fd9d97d7f09d0cc075006\"><code>710560e</code></a>\nDon't use --all-features in CI</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/rust-lang/log/compare/0.4.21...0.4.22\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `paste` from 1.0.14 to 1.0.15\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/dtolnay/paste/releases\">paste's\nreleases</a>.</em></p>\n<blockquote>\n<h2>1.0.15</h2>\n<ul>\n<li>Resolve unexpected_cfgs warning (<a\nhref=\"https://redirect.github.com/dtolnay/paste/issues/102\">#102</a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/dtolnay/paste/commit/a2c7e27875277450ed28147623ba5218dd23e732\"><code>a2c7e27</code></a>\nRelease 1.0.15</li>\n<li><a\nhref=\"https://github.com/dtolnay/paste/commit/1d23098227a01de542ea52db13dc1314eca13f00\"><code>1d23098</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/dtolnay/paste/issues/102\">#102</a>\nfrom dtolnay/checkcfg</li>\n<li><a\nhref=\"https://github.com/dtolnay/paste/commit/1edfaae644d0b27e96c26cdc4d51e9fe3f51c12d\"><code>1edfaae</code></a>\nResolve unexpected_cfgs warning</li>\n<li><a\nhref=\"https://github.com/dtolnay/paste/commit/cc6803dd049b9943c1e49b2220ff37a94711577c\"><code>cc6803d</code></a>\nExplicitly install a Rust toolchain for cargo-outdated job</li>\n<li><a\nhref=\"https://github.com/dtolnay/paste/commit/d39fb86d2d588bf63572886db340bc16c6cc6904\"><code>d39fb86</code></a>\nIgnore dead code lint in tests</li>\n<li><a\nhref=\"https://github.com/dtolnay/paste/commit/14872adf2b72140902ed6425a90517333ccc1a44\"><code>14872ad</code></a>\nWork around empty_docs clippy lint in test</li>\n<li><a\nhref=\"https://github.com/dtolnay/paste/commit/ed844dc6fe755bcee881bd93cdff5a77038aa49b\"><code>ed844dc</code></a>\nWork around dead_code warning in test</li>\n<li><a\nhref=\"https://github.com/dtolnay/paste/commit/0a4161b1318e01845cb32790b3bdadd618608361\"><code>0a4161b</code></a>\nAdd cargo.toml metadata to link to crate documentation</li>\n<li><a\nhref=\"https://github.com/dtolnay/paste/commit/5a2bce19a1f100bf62824c9e3ff03879c916cdce\"><code>5a2bce1</code></a>\nTest docs.rs documentation build in CI</li>\n<li><a\nhref=\"https://github.com/dtolnay/paste/commit/d7e0be15a74c99b303e9993365f41f3440551b8f\"><code>d7e0be1</code></a>\nUpdate actions/checkout@v3 -&gt; v4</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/dtolnay/paste/compare/1.0.14...1.0.15\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `quote` from 1.0.35 to 1.0.36\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/dtolnay/quote/releases\">quote's\nreleases</a>.</em></p>\n<blockquote>\n<h2>1.0.36</h2>\n<ul>\n<li>Documentation improvements</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/5d4880c4255b5c7f5ea0a9ac3cf9f985c418a1e7\"><code>5d4880c</code></a>\nRelease 1.0.36</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/1dd7ce794ff69a922f6b0e1b5d3a4929e1218258\"><code>1dd7ce7</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/dtolnay/quote/issues/273\">#273</a>\nfrom dtolnay/doc</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/0bc5d12f9be1bd39314b925aa786663e18f9a489\"><code>0bc5d12</code></a>\nApply doc comment to cfg(not(doc)) macros too</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/c295f5cca24108693724d8fb6f35da1faa81b78b\"><code>c295f5c</code></a>\nRevert &quot;Temporarily disable miri on doctests&quot;</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/435bd1b917e98413310c5260787fbcee3c3d01ca\"><code>435bd1b</code></a>\nUpdate ui test suite to nightly-2024-03-31</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/cc3847d3469a8e82a587fbf1608adc04b56c581a\"><code>cc3847d</code></a>\nExplicitly install a Rust toolchain for cargo-outdated job</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/6259d49d0d35030c3dea792e85f23af52bb7994d\"><code>6259d49</code></a>\nTemporarily disable miri on doctests</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/bdb4b594076d78127b99a3da768e369499e324de\"><code>bdb4b59</code></a>\nUpdate ui test suite to nightly-2024-02-08</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/c2aeca9c00b12b6f87e2e7cb545c160e6b4aa18f\"><code>c2aeca9</code></a>\nUpdate ui test suite to nightly-2024-01-31</li>\n<li><a\nhref=\"https://github.com/dtolnay/quote/commit/376a0611f3acf91a424aae58104b587530361900\"><code>376a061</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/dtolnay/quote/issues/270\">#270</a>\nfrom dtolnay/bench</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/dtolnay/quote/compare/1.0.35...1.0.36\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `serde` from 1.0.197 to 1.0.204\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/serde-rs/serde/releases\">serde's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v1.0.204</h2>\n<ul>\n<li>Apply #[diagnostic::on_unimplemented] attribute on Rust 1.78+ to\nsuggest adding serde derive or enabling a &quot;serde&quot; feature flag\nin dependencies (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2767\">#2767</a>,\nthanks <a\nhref=\"https://github.com/weiznich\"><code>@​weiznich</code></a>)</li>\n</ul>\n<h2>v1.0.203</h2>\n<ul>\n<li>Documentation improvements (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2747\">#2747</a>)</li>\n</ul>\n<h2>v1.0.202</h2>\n<ul>\n<li>Provide public access to RenameAllRules in serde_derive_internals\n(<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2743\">#2743</a>)</li>\n</ul>\n<h2>v1.0.201</h2>\n<ul>\n<li>Resolve unexpected_cfgs warning (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2737\">#2737</a>)</li>\n</ul>\n<h2>v1.0.200</h2>\n<ul>\n<li>Fix formatting of &quot;invalid type&quot; and &quot;invalid\nvalue&quot; deserialization error messages containing NaN or infinite\nfloats (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2733\">#2733</a>,\nthanks <a\nhref=\"https://github.com/jamessan\"><code>@​jamessan</code></a>)</li>\n</ul>\n<h2>v1.0.199</h2>\n<ul>\n<li>Fix ambiguous associated item when\n<code>forward_to_deserialize_any!</code> is used on an enum with\n<code>Error</code> variant (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2732\">#2732</a>,\nthanks <a\nhref=\"https://github.com/aatifsyed\"><code>@​aatifsyed</code></a>)</li>\n</ul>\n<h2>v1.0.198</h2>\n<ul>\n<li>Support serializing and deserializing\n<code>Saturating&lt;T&gt;</code> (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2709\">#2709</a>,\nthanks <a\nhref=\"https://github.com/jbethune\"><code>@​jbethune</code></a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/18dcae0a77632fb4767a420c550cb41991f750b8\"><code>18dcae0</code></a>\nRelease 1.0.204</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/58c307f9cc28a19d73a0e2869f6addf9a8a329f9\"><code>58c307f</code></a>\nAlphabetize list of rustc-check-cfg</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/8cc4809414a83de0d41eac38ecfa1040e088b61e\"><code>8cc4809</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2769\">#2769</a>\nfrom dtolnay/onunimpl</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/1179158defc5351467cbd2c340b7e1498391bce4\"><code>1179158</code></a>\nUpdate ui test with diagnostic::on_unimplemented from PR 2767</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/91aa40e749620f31bf7db01c772e672f023136b5\"><code>91aa40e</code></a>\nAdd ui test of unsatisfied serde trait bound</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/595019e979ebed5452b550bf901abcab2cf4e945\"><code>595019e</code></a>\nCut test_suite from workspace members in old toolchain CI jobs</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/b0d7917f88978eda264f8fbac13b46ece35f5348\"><code>b0d7917</code></a>\nPull in trybuild 'following types implement trait' fix</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/8e6637a1e44c30dffd37322a7107d434cd751722\"><code>8e6637a</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2767\">#2767</a>\nfrom weiznich/feature/diagnostic_on_unimplemented</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/694fe0595358aa0857120a99041d99975b1a8a70\"><code>694fe05</code></a>\nUse the <code>#[diagnostic::on_unimplemented]</code> attribute when\npossible</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/f3dfd2a2375d9caf15a18ec657dde51a32caf6ed\"><code>f3dfd2a</code></a>\nSuppress dead code warning in test of unit struct remote derive</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/serde-rs/serde/compare/v1.0.197...v1.0.204\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `serde_derive` from 1.0.197 to 1.0.204\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/serde-rs/serde/releases\">serde_derive's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v1.0.204</h2>\n<ul>\n<li>Apply #[diagnostic::on_unimplemented] attribute on Rust 1.78+ to\nsuggest adding serde derive or enabling a &quot;serde&quot; feature flag\nin dependencies (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2767\">#2767</a>,\nthanks <a\nhref=\"https://github.com/weiznich\"><code>@​weiznich</code></a>)</li>\n</ul>\n<h2>v1.0.203</h2>\n<ul>\n<li>Documentation improvements (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2747\">#2747</a>)</li>\n</ul>\n<h2>v1.0.202</h2>\n<ul>\n<li>Provide public access to RenameAllRules in serde_derive_internals\n(<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2743\">#2743</a>)</li>\n</ul>\n<h2>v1.0.201</h2>\n<ul>\n<li>Resolve unexpected_cfgs warning (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2737\">#2737</a>)</li>\n</ul>\n<h2>v1.0.200</h2>\n<ul>\n<li>Fix formatting of &quot;invalid type&quot; and &quot;invalid\nvalue&quot; deserialization error messages containing NaN or infinite\nfloats (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2733\">#2733</a>,\nthanks <a\nhref=\"https://github.com/jamessan\"><code>@​jamessan</code></a>)</li>\n</ul>\n<h2>v1.0.199</h2>\n<ul>\n<li>Fix ambiguous associated item when\n<code>forward_to_deserialize_any!</code> is used on an enum with\n<code>Error</code> variant (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2732\">#2732</a>,\nthanks <a\nhref=\"https://github.com/aatifsyed\"><code>@​aatifsyed</code></a>)</li>\n</ul>\n<h2>v1.0.198</h2>\n<ul>\n<li>Support serializing and deserializing\n<code>Saturating&lt;T&gt;</code> (<a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2709\">#2709</a>,\nthanks <a\nhref=\"https://github.com/jbethune\"><code>@​jbethune</code></a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/18dcae0a77632fb4767a420c550cb41991f750b8\"><code>18dcae0</code></a>\nRelease 1.0.204</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/58c307f9cc28a19d73a0e2869f6addf9a8a329f9\"><code>58c307f</code></a>\nAlphabetize list of rustc-check-cfg</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/8cc4809414a83de0d41eac38ecfa1040e088b61e\"><code>8cc4809</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2769\">#2769</a>\nfrom dtolnay/onunimpl</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/1179158defc5351467cbd2c340b7e1498391bce4\"><code>1179158</code></a>\nUpdate ui test with diagnostic::on_unimplemented from PR 2767</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/91aa40e749620f31bf7db01c772e672f023136b5\"><code>91aa40e</code></a>\nAdd ui test of unsatisfied serde trait bound</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/595019e979ebed5452b550bf901abcab2cf4e945\"><code>595019e</code></a>\nCut test_suite from workspace members in old toolchain CI jobs</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/b0d7917f88978eda264f8fbac13b46ece35f5348\"><code>b0d7917</code></a>\nPull in trybuild 'following types implement trait' fix</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/8e6637a1e44c30dffd37322a7107d434cd751722\"><code>8e6637a</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/serde/issues/2767\">#2767</a>\nfrom weiznich/feature/diagnostic_on_unimplemented</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/694fe0595358aa0857120a99041d99975b1a8a70\"><code>694fe05</code></a>\nUse the <code>#[diagnostic::on_unimplemented]</code> attribute when\npossible</li>\n<li><a\nhref=\"https://github.com/serde-rs/serde/commit/f3dfd2a2375d9caf15a18ec657dde51a32caf6ed\"><code>f3dfd2a</code></a>\nSuppress dead code warning in test of unit struct remote derive</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/serde-rs/serde/compare/v1.0.197...v1.0.204\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `serde_json` from 1.0.114 to 1.0.120\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/serde-rs/json/releases\">serde_json's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v1.0.120</h2>\n<ul>\n<li>Correctly specify required version of <code>indexmap</code>\ndependency (<a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1152\">#1152</a>,\nthanks <a\nhref=\"https://github.com/cforycki\"><code>@​cforycki</code></a>)</li>\n</ul>\n<h2>v1.0.119</h2>\n<ul>\n<li>Add <code>serde_json::Map::shift_insert</code> (<a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1149\">#1149</a>,\nthanks <a\nhref=\"https://github.com/joshka\"><code>@​joshka</code></a>)</li>\n</ul>\n<h2>v1.0.118</h2>\n<ul>\n<li>Implement Hash for serde_json::Value (<a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1127\">#1127</a>,\nthanks <a\nhref=\"https://github.com/edwardycl\"><code>@​edwardycl</code></a>)</li>\n</ul>\n<h2>v1.0.117</h2>\n<ul>\n<li>Resolve unexpected_cfgs warning (<a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1130\">#1130</a>)</li>\n</ul>\n<h2>v1.0.116</h2>\n<ul>\n<li>Make module structure comprehensible to static analysis (<a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1124\">#1124</a>,\nthanks <a\nhref=\"https://github.com/mleonhard\"><code>@​mleonhard</code></a>)</li>\n</ul>\n<h2>v1.0.115</h2>\n<ul>\n<li>Documentation improvements</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/bcedc3d96bcc33184f16d63eab397295e2193350\"><code>bcedc3d</code></a>\nRelease 1.0.120</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/962c0fbbecc7dc8559cfeb019c2611737512f937\"><code>962c0fb</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1152\">#1152</a>\nfrom cforycki/fix/index-map-minimal-version</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/3480feda7b572d33992544061a8e0fbf8610a803\"><code>3480fed</code></a>\nfix: indexmap minimal version with Map::shift_insert()</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/b48b9a3a0c09952579e98c8940fe0d1ee4aae588\"><code>b48b9a3</code></a>\nRelease 1.0.119</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/8878cd7c042a5f94ae4ee9889cbcbd12cc5ce334\"><code>8878cd7</code></a>\nMake shift_insert available for inlining like other Map methods</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/352b7abf007cf3b9b063b01e0b1e8f6af62a4e39\"><code>352b7ab</code></a>\nDocument the cfg required for Map::shift_insert to exist</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/c17e63f6eff6cb40594beb1bddd4562c4cc81442\"><code>c17e63f</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1149\">#1149</a>\nfrom joshka/master</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/309ef6b8870e47622a283061cbda3f5514bfaf0d\"><code>309ef6b</code></a>\nAdd Map::shift_insert()</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/a9e089a2ce245bc223b56fbb6c525e2fe7b1f0ef\"><code>a9e089a</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/serde-rs/json/issues/1146\">#1146</a>\nfrom haouvw/master</li>\n<li><a\nhref=\"https://github.com/serde-rs/json/commit/a83fe96ae2a202925f1caa7abc51991f321d7c22\"><code>a83fe96</code></a>\nchore: remove repeat words</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/serde-rs/json/compare/v1.0.114...v1.0.120\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `serde_yaml` from 0.9.33 to 0.9.34+deprecated\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/dtolnay/serde-yaml/releases\">serde_yaml's\nreleases</a>.</em></p>\n<blockquote>\n<h2>0.9.34</h2>\n<p>As of this release, I am not planning to publish further versions of\n<code>serde_yaml</code> as none of my projects have been using YAML for\na long time, so I have archived the GitHub repo and marked the crate\ndeprecated in the version number. An official replacement isn't\ndesignated for those who still need to work with YAML, but <a\nhref=\"https://crates.io/search?q=yaml&amp;sort=relevance\">https://crates.io/search?q=yaml&amp;sort=relevance</a>\nand <a\nhref=\"https://crates.io/keywords/yaml\">https://crates.io/keywords/yaml</a>\nhas a number of reasonable-looking options available.</p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/dtolnay/serde-yaml/commit/2009506d33767dfc88e979d6bc0d53d09f941c94\"><code>2009506</code></a>\nRelease 0.9.34</li>\n<li><a\nhref=\"https://github.com/dtolnay/serde-yaml/commit/3ba8462f7d3b603d832e0daeb6cfc7168a673d7a\"><code>3ba8462</code></a>\nAdd unmaintained note</li>\n<li><a\nhref=\"https://github.com/dtolnay/serde-yaml/commit/77236b0d50f6fb670fefe8146aba02f1eab211f3\"><code>77236b0</code></a>\nIgnore dead code lint in tests</li>\n<li>See full diff in <a\nhref=\"https://github.com/dtolnay/serde-yaml/compare/0.9.33...0.9.34\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore <dependency name> major version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's major version (unless you unignore this specific\ndependency's major version or upgrade to it yourself)\n- `@dependabot ignore <dependency name> minor version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's minor version (unless you unignore this specific\ndependency's minor version or upgrade to it yourself)\n- `@dependabot ignore <dependency name>` will close this group update PR\nand stop Dependabot creating any more for the specific dependency\n(unless you unignore this specific dependency or upgrade to it yourself)\n- `@dependabot unignore <dependency name>` will remove all of the ignore\nconditions of the specified dependency\n- `@dependabot unignore <dependency name> <ignore condition>` will\nremove the ignore condition of the specified dependency and ignore\nconditions\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-18T11:47:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e2d3b8b1f374ceeb6b4af5b9ef9a216bba44e4ed"
        },
        "date": 1721305163190,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63801.85,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52946.7,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.822086165830118,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.064041547480168,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.14934858894001,
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
        "date": 1721317176205,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63783.14,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.583103413969969,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.043449182130001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.956062645920148,
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
        "date": 1721336835927,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63789.81999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.7,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.695657844690036,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.044847288740046,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9596171705601693,
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
        "date": 1721389148318,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63807.509999999995,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.971363733930062,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1920876511301617,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.299208743069935,
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
        "date": 1721392895887,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63783.249999999985,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.59999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.055757469520169,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.119042650090018,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.657041110580028,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "ordian",
            "username": "ordian",
            "email": "write@reusable.software"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7f2a99fc03b58f7be4c62eb4ecd3fe2cb743fd1a",
          "message": "beefy: put not only lease parachain heads into mmr (#4751)\n\nShort-term addresses\nhttps://github.com/paritytech/polkadot-sdk/issues/4737.\n\n- [x] Resolve benchmarking\nI've digged into benchmarking mentioned\nhttps://github.com/paritytech/polkadot-sdk/issues/4737#issuecomment-2155084660,\nbut it seemed to me that this code is different proof/path. @acatangiu\ncould you confirm? (btw, in this\n[bench](https://github.com/paritytech/polkadot-sdk/blob/b65313e81465dd730e48d4ce00deb76922618375/bridges/modules/parachains/src/benchmarking.rs#L57),\nwhere do you actually set the `fn parachains()` to a reasonable number?\ni've only seen 1)\n- [ ] Communicate to Snowfork team:\nThis seems to be the relevant code:\nhttps://github.com/Snowfork/snowbridge/blob/1e18e010331777042aa7e8fff3c118094af856ba/relayer/cmd/parachain_head_proof.go#L95-L120\n- [x] Is it preferred to iter() in some random order as suggested in\nhttps://github.com/paritytech/polkadot-sdk/issues/4737#issuecomment-2155084660\nor take lowest para ids instead as implemented here currently?\n- [x] PRDoc\n\n## Updating Polkadot and Kusama runtimes:\n\nNew weights need to be generated (`pallet_mmr`) and configs updated\nsimilar to Rococo/Westend:\n```patch\ndiff --git a/polkadot/runtime/rococo/src/lib.rs b/polkadot/runtime/rococo/src/lib.rs\nindex 5adffbd7422..c7da339b981 100644\n--- a/polkadot/runtime/rococo/src/lib.rs\n+++ b/polkadot/runtime/rococo/src/lib.rs\n@@ -1307,9 +1307,11 @@ impl pallet_mmr::Config for Runtime {\n        const INDEXING_PREFIX: &'static [u8] = mmr::INDEXING_PREFIX;\n        type Hashing = Keccak256;\n        type OnNewRoot = pallet_beefy_mmr::DepositBeefyDigest<Runtime>;\n-       type WeightInfo = ();\n        type LeafData = pallet_beefy_mmr::Pallet<Runtime>;\n        type BlockHashProvider = pallet_mmr::DefaultBlockHashProvider<Runtime>;\n+       type WeightInfo = weights::pallet_mmr::WeightInfo<Runtime>;\n+       #[cfg(feature = \"runtime-benchmarks\")]\n+       type BenchmarkHelper = parachains_paras::benchmarking::mmr_setup::MmrSetup<Runtime>;\n }\n\n parameter_types! {\n@@ -1319,13 +1321,8 @@ parameter_types! {\n pub struct ParaHeadsRootProvider;\n impl BeefyDataProvider<H256> for ParaHeadsRootProvider {\n        fn extra_data() -> H256 {\n-               let mut para_heads: Vec<(u32, Vec<u8>)> = parachains_paras::Parachains::<Runtime>::get()\n-                       .into_iter()\n-                       .filter_map(|id| {\n-                               parachains_paras::Heads::<Runtime>::get(&id).map(|head| (id.into(), head.0))\n-                       })\n-                       .collect();\n-               para_heads.sort();\n+               let para_heads: Vec<(u32, Vec<u8>)> =\n+                       parachains_paras::Pallet::<Runtime>::sorted_para_heads();\n                binary_merkle_tree::merkle_root::<mmr::Hashing, _>(\n                        para_heads.into_iter().map(|pair| pair.encode()),\n                )\n@@ -1746,6 +1743,7 @@ mod benches {\n                [pallet_identity, Identity]\n                [pallet_indices, Indices]\n                [pallet_message_queue, MessageQueue]\n+               [pallet_mmr, Mmr]\n                [pallet_multisig, Multisig]\n                [pallet_parameters, Parameters]\n                [pallet_preimage, Preimage]\n```\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-07-19T16:06:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7f2a99fc03b58f7be4c62eb4ecd3fe2cb743fd1a"
        },
        "date": 1721407334676,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63803.06,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.084079165989968,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.572328113870071,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9927608010801436,
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
        "date": 1721413167561,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63808.380000000005,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.8,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.193469373849968,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.38559489896001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.279399676370173,
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
        "date": 1721430530359,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63804.219999999994,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.909253429950045,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.294806107199987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.178613848510085,
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
        "date": 1721436509651,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63795.31,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.264828571280008,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.894223107789973,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.170546287170135,
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
        "date": 1721491879316,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63799.70000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.135697307069955,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.71957388786986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0507733029601813,
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
        "date": 1721524723992,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52934.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63789.48,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.2006087971799975,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.75387442915993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1061021143901337,
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
        "date": 1721649567970,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63807.46,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.391623787819963,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.412886814700258,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.621065355439981,
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
          "id": "f13ed8de69bcfcccecf208211998b8af2ef882a2",
          "message": "Update parity publish (#5105)",
          "timestamp": "2024-07-22T15:12:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f13ed8de69bcfcccecf208211998b8af2ef882a2"
        },
        "date": 1721667023633,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63817.990000000005,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.530286685570014,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.402192689419955,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3962035078001462,
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
        "date": 1721672568542,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63852.219999999994,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 11.34038013747001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.953246858230353,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.452805435569916,
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
        "date": 1721692730731,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63802.829999999994,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.0592528178001412,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.116401884089915,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.778266370890128,
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
          "id": "216e8fa126df9ee819d524834e75a82881681e02",
          "message": "Beefy equivocation: add runtime API methods (#4993)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/4523\n\nAdd runtime API methods for:\n- generating the ancestry proof\n- submiting a fork voting report\n- submitting a future voting report",
          "timestamp": "2024-07-23T14:27:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/216e8fa126df9ee819d524834e75a82881681e02"
        },
        "date": 1721751754167,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63792.09999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.087636641880125,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.82994988239003,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.193312426419957,
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
          "id": "11dd10b46511647d8663afe063015b16a8c1e124",
          "message": "Bump backtrace from 0.3.69 to 0.3.71 (#5110)\n\nBumps [backtrace](https://github.com/rust-lang/backtrace-rs) from 0.3.69\nto 0.3.71.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/rust-lang/backtrace-rs/releases\">backtrace's\nreleases</a>.</em></p>\n<blockquote>\n<h2>0.3.71</h2>\n<p>This is mostly CI changes, with a very mild bump to our effective cc\ncrate version recorded, and a small modification to a previous changeset\nto allow backtrace to run at its current checked-in MSRV on Windows.\nSorry about that! We will be getting 0.3.70 yanked shortly.</p>\n<h2>What's Changed</h2>\n<ul>\n<li>Make sgx functions exist with cfg(miri) by <a\nhref=\"https://github.com/saethlin\"><code>@​saethlin</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/591\">rust-lang/backtrace-rs#591</a></li>\n<li>Update version of cc crate by <a\nhref=\"https://github.com/jfgoog\"><code>@​jfgoog</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/592\">rust-lang/backtrace-rs#592</a></li>\n<li>Pull back MSRV on Windows by <a\nhref=\"https://github.com/workingjubilee\"><code>@​workingjubilee</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/598\">rust-lang/backtrace-rs#598</a></li>\n<li>Force frame pointers on all i686 tests by <a\nhref=\"https://github.com/workingjubilee\"><code>@​workingjubilee</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/601\">rust-lang/backtrace-rs#601</a></li>\n<li>Use rustc from stage0 instead of stage0-sysroot by <a\nhref=\"https://github.com/Nilstrieb\"><code>@​Nilstrieb</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/602\">rust-lang/backtrace-rs#602</a></li>\n<li>Cut backtrace 0.3.71 by <a\nhref=\"https://github.com/workingjubilee\"><code>@​workingjubilee</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/599\">rust-lang/backtrace-rs#599</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a href=\"https://github.com/jfgoog\"><code>@​jfgoog</code></a> made\ntheir first contribution in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/592\">rust-lang/backtrace-rs#592</a></li>\n<li><a href=\"https://github.com/Nilstrieb\"><code>@​Nilstrieb</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/602\">rust-lang/backtrace-rs#602</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/rust-lang/backtrace-rs/compare/0.3.70...0.3.71\">https://github.com/rust-lang/backtrace-rs/compare/0.3.70...0.3.71</a></p>\n<h2>0.3.70</h2>\n<h2>New API</h2>\n<ul>\n<li>A <code>BacktraceFrame</code> can now have <code>resolve(&amp;mut\nself)</code> called on it thanks to <a\nhref=\"https://github.com/fraillt\"><code>@​fraillt</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/526\">rust-lang/backtrace-rs#526</a></li>\n</ul>\n<h2>Platform Support</h2>\n<p>We added support for new platforms in this release!</p>\n<ul>\n<li>Thanks to <a href=\"https://github.com/bzEq\"><code>@​bzEq</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/508\">rust-lang/backtrace-rs#508</a>\nwe now have AIX support!</li>\n<li>Thanks to <a\nhref=\"https://github.com/sthibaul\"><code>@​sthibaul</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/567\">rust-lang/backtrace-rs#567</a>\nwe now have GNU/Hurd support!</li>\n<li>Thanks to <a\nhref=\"https://github.com/dpaoliello\"><code>@​dpaoliello</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/587\">rust-lang/backtrace-rs#587</a>\nwe now support &quot;emulation-compatible&quot; AArch64 Windows (aka\narm64ec)</li>\n</ul>\n<h3>Windows</h3>\n<ul>\n<li>Rewrite msvc backtrace support to be much faster on 64-bit platforms\nby <a\nhref=\"https://github.com/wesleywiser\"><code>@​wesleywiser</code></a> in\n<a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/569\">rust-lang/backtrace-rs#569</a></li>\n<li>Fix i686-pc-windows-gnu missing dbghelp module by <a\nhref=\"https://github.com/wesleywiser\"><code>@​wesleywiser</code></a> in\n<a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/571\">rust-lang/backtrace-rs#571</a></li>\n<li>Fix build errors on <code>thumbv7a-*-windows-msvc</code> targets by\n<a href=\"https://github.com/kleisauke\"><code>@​kleisauke</code></a> in\n<a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/573\">rust-lang/backtrace-rs#573</a></li>\n<li>Fix panic in backtrace symbolication on win7 by <a\nhref=\"https://github.com/roblabla\"><code>@​roblabla</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/578\">rust-lang/backtrace-rs#578</a></li>\n<li>remove few unused windows ffi fn by <a\nhref=\"https://github.com/klensy\"><code>@​klensy</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/576\">rust-lang/backtrace-rs#576</a></li>\n<li>Make dbghelp look for PDBs next to their exe/dll. by <a\nhref=\"https://github.com/michaelwoerister\"><code>@​michaelwoerister</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/584\">rust-lang/backtrace-rs#584</a></li>\n<li>Revert 32-bit dbghelp to a version WINE (presumably) likes by <a\nhref=\"https://github.com/ChrisDenton\"><code>@​ChrisDenton</code></a> in\n<a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/588\">rust-lang/backtrace-rs#588</a></li>\n<li>Update for Win10+ by <a\nhref=\"https://github.com/ChrisDenton\"><code>@​ChrisDenton</code></a> in\n<a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/589\">rust-lang/backtrace-rs#589</a></li>\n</ul>\n<h3>SGX</h3>\n<p>Thanks to</p>\n<ul>\n<li>Adjust frame IP in SGX relative to image base by <a\nhref=\"https://github.com/mzohreva\"><code>@​mzohreva</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/566\">rust-lang/backtrace-rs#566</a></li>\n</ul>\n<h2>Internals</h2>\n<p>We did a bunch more work on our CI and internal cleanups</p>\n<ul>\n<li>Modularise CI workflow and validate outputs for binary size checks.\nby <a href=\"https://github.com/detly\"><code>@​detly</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/549\">rust-lang/backtrace-rs#549</a></li>\n<li>Commit Cargo.lock by <a\nhref=\"https://github.com/bjorn3\"><code>@​bjorn3</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/562\">rust-lang/backtrace-rs#562</a></li>\n<li>Enable calling build.rs externally v2 by <a\nhref=\"https://github.com/pitaj\"><code>@​pitaj</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/568\">rust-lang/backtrace-rs#568</a></li>\n<li>Upgrade to 2021 ed and inline panics by <a\nhref=\"https://github.com/nyurik\"><code>@​nyurik</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/538\">rust-lang/backtrace-rs#538</a></li>\n</ul>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/7be8953188582ea83f1e88622ccdfcab1a49461c\"><code>7be8953</code></a><code>rust-lang/backtrace-rs#599</code></li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/c31ea5ba7ac52f5c15c65cfec7d7b5d0bcf00eed\"><code>c31ea5b</code></a><code>rust-lang/backtrace-rs#602</code></li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/193125abc094b433859c4fdb2e672d391a6bdf8d\"><code>193125a</code></a><code>rust-lang/backtrace-rs#601</code></li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/bdc8b8241b16bb20124a3cec86e1b339e4c008a1\"><code>bdc8b82</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/issues/598\">#598</a>\nfrom workingjubilee/pull-back-msrv</li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/edc9f5cae874bf008e52558e4b2c6c86847c9575\"><code>edc9f5c</code></a>\nhack out binary size checks</li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/4c8fe973eb39f4cd31c3d6dfd74c6b670de6911a\"><code>4c8fe97</code></a>\nadd Windows to MSRV tests</li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/84dfe2472456a000d7cced566b06f3bada898f8e\"><code>84dfe24</code></a>\nhack CI</li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/3f08ec085fb5bb4edfb084cf9e3170e953a44107\"><code>3f08ec0</code></a>\nPull back MSRV-breaking ptr::from_ref</li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/6fa4b85b9962c3e1be8c2e5cc605cd078134152b\"><code>6fa4b85</code></a><code>rust-lang/backtrace-rs#592</code></li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/ea7dc8e964d0046d92382e40308876130e5301ba\"><code>ea7dc8e</code></a><code>rust-lang/backtrace-rs#591</code></li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/rust-lang/backtrace-rs/compare/0.3.69...0.3.71\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=backtrace&package-manager=cargo&previous-version=0.3.69&new-version=0.3.71)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-23T16:38:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/11dd10b46511647d8663afe063015b16a8c1e124"
        },
        "date": 1721758700914,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63792.46000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.570436006199923,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.93606003063015,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.027083733989935,
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
        "date": 1721767441740,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63802.05,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.126436594640035,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.659377485609946,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0932216682500613,
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
        "date": 1721776948816,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63799.77999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.607996884770015,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.0489972096300395,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.94182013012022,
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
        "date": 1721821862485,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63969.12000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.2822118197699455,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.923274878439994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1256373535901294,
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
        "date": 1721830062844,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64054.18000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52937.90000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 11.201209226340039,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.8501844944302532,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.375836038329932,
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
        "date": 1721836257364,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64069.7,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.730845707689921,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.40663879093999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 4.059841770510236,
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
        "date": 1721846897842,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64140.61000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52945.8,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 8.808411406210173,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 12.898820268049898,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 4.571164236480146,
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
        "date": 1721850759739,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63967.92999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.90000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.627318467299997,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 5.9861376609800026,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9232605662701947,
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
        "date": 1721863012964,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52946.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64061.340000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.8198089474402552,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.338013864280052,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.386957127770027,
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
        "date": 1721896207817,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63981.68999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52946,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.104004269810134,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.926773208369987,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.180963499279995,
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
        "date": 1721906869352,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64049.630000000005,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.5,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.709287557790212,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.094488307360059,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.627145700250045,
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
        "date": 1721912397530,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64035.69999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.826630459100062,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.701889821780004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.5743409141902625,
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
        "date": 1721920316577,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63983.52,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.7,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.330317394340193,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.279228767230139,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.469856485589939,
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
        "date": 1721925490529,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52936.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63983.159999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.03361461577004,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.370531822879926,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1968071995202125,
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
        "date": 1721933578840,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63965.48999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.7,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.134072619009981,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.73389691269,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.999600004460203,
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
          "id": "0d7d2177807ec6b3094f4491a45b0bc0d74d3c8b",
          "message": "CI: Prevent breaking backports (#4812)\n\n- Prevent `major` changes to be merged into a `stable` branch.\n- Place a comment on backport MRs to provide context of what it means.\n\nComment looks like this:\n\n![Screenshot 2024-07-24 at 17 36\n35](https://github.com/user-attachments/assets/6393549b-7b15-41e5-a804-8581c625ceff)\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-26T09:10:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d7d2177807ec6b3094f4491a45b0bc0d74d3c8b"
        },
        "date": 1721991111315,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63996.46,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.61620008020001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.98848817319018,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.041689003269986,
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
        "date": 1721995496583,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64099.38999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.883640486629993,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.708385827910087,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 4.1461862871102895,
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
        "date": 1722000190753,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64024.58999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52947.2,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.618315506349933,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.418214813350088,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.415951808820094,
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
          "id": "fc07bdadde1dfa3345913130f5209b8267816972",
          "message": "runtime: make the candidate relay parent progression check more strict (#5113)\n\nPreviously, we were checking if the relay parent of a new candidate does\nnot move backwards from the latest included on-chain candidate. This was\nfine prior to elastic scaling. We now need to also check that the relay\nparent progresses from the latest pending availability candidate, as\nwell as check the progression within the candidate chain in the inherent\ndata.\n\nProspective-parachains is already doing this check but we should also\nadd it in the runtime",
          "timestamp": "2024-07-26T13:07:15Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fc07bdadde1dfa3345913130f5209b8267816972"
        },
        "date": 1722005241686,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63985.340000000004,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.236627440749973,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.813284578459996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0998659353701177,
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
          "id": "7e4433e43072fda77f85cb5d0bc531fa255f3104",
          "message": "Update README.md (#5152)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/5144, plus\nremove the lines of code badge as it was not working.",
          "timestamp": "2024-07-26T16:26:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7e4433e43072fda77f85cb5d0bc531fa255f3104"
        },
        "date": 1722016861062,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52946.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64000.990000000005,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.2692029230200967,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.125741533349984,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.393988941730026,
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
        "date": 1722023072660,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63965.009999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52933,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.327263833180207,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.024397917640039,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.41573261783996,
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
        "date": 1722043626376,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52935.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63968.92999999998,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.821240777889887,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.215369921179969,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.11975137229017,
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
        "date": 1722157593045,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64028.829999999994,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.674956557170017,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.8030858085602817,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.226107624880052,
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
        "date": 1722203954788,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64037.34999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.4596119417001967,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.729378807680026,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.502599100599948,
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
          "id": "9b4acf27b869d7cbb07b03f0857763b8c8cc7566",
          "message": "Bump bs58 from 0.5.0 to 0.5.1 (#5170)\n\nBumps [bs58](https://github.com/Nullus157/bs58-rs) from 0.5.0 to 0.5.1.\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/Nullus157/bs58-rs/blob/main/CHANGELOG.md\">bs58's\nchangelog</a>.</em></p>\n<blockquote>\n<h2>0.5.1 - 2024-03-19</h2>\n<ul>\n<li>Make it possible to decode in <code>const</code>-context (by <a\nhref=\"https://github.com/joncinque\"><code>@​joncinque</code></a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/Nullus157/bs58-rs/commit/7d3c9282d2595612e5474df93dd0e017db9b684f\"><code>7d3c928</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/Nullus157/bs58-rs/issues/116\">#116</a>\nfrom joncinque/const</li>\n<li><a\nhref=\"https://github.com/Nullus157/bs58-rs/commit/d3fb50ebad42ff34454e3b49c9e93e85df08d835\"><code>d3fb50e</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/Nullus157/bs58-rs/issues/117\">#117</a>\nfrom Nemo157/criterion-update</li>\n<li><a\nhref=\"https://github.com/Nullus157/bs58-rs/commit/9038a36ae66f0f5d2b74f7a4f3a630873c71d0a1\"><code>9038a36</code></a>\nUpdate dependencies</li>\n<li><a\nhref=\"https://github.com/Nullus157/bs58-rs/commit/13af427722e681d1ba9c922380663eea2f865d4d\"><code>13af427</code></a>\nUpdate criterion to fix cargo-deny issues</li>\n<li><a\nhref=\"https://github.com/Nullus157/bs58-rs/commit/b6ad26a72010dec7caf18cf4cb4e1e7131ef57e6\"><code>b6ad26a</code></a>\nPrepare to release 0.5.1</li>\n<li><a\nhref=\"https://github.com/Nullus157/bs58-rs/commit/e18e057bf86e67e028ed6da0ee4f1850978d2301\"><code>e18e057</code></a>\nMove const-compatible API onto <code>decode::DecodeBuilder</code>\ndirectly</li>\n<li><a\nhref=\"https://github.com/Nullus157/bs58-rs/commit/e65bfa72a23c57fbc05cad66c9b667c6eae946fa\"><code>e65bfa7</code></a>\ndecode: Add const-compatible decoder</li>\n<li><a\nhref=\"https://github.com/Nullus157/bs58-rs/commit/2b0d73b9955f6a745f9b6fbb387bba2b96ea89fd\"><code>2b0d73b</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/Nullus157/bs58-rs/issues/113\">#113</a>\nfrom Nemo157/cli-version-bump</li>\n<li><a\nhref=\"https://github.com/Nullus157/bs58-rs/commit/be42edf49589d3f5135871ab129bfff4ded21d67\"><code>be42edf</code></a>\nPrepare for 0.1.2 cli release</li>\n<li><a\nhref=\"https://github.com/Nullus157/bs58-rs/commit/6bdc4b2c673f334de0dd316f2e7d988d0db5cb52\"><code>6bdc4b2</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/Nullus157/bs58-rs/issues/112\">#112</a>\nfrom Nemo157/cli-dep-update</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/Nullus157/bs58-rs/compare/0.5.0...0.5.1\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=bs58&package-manager=cargo&previous-version=0.5.0&new-version=0.5.1)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-29T09:11:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9b4acf27b869d7cbb07b03f0857763b8c8cc7566"
        },
        "date": 1722247646440,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64071.21,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.59999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.879224773490198,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.32253323336995,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.110804798730111,
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
        "date": 1722254135641,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63985.219999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.8,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.22767039507003,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.18335798460012,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1488271972601174,
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
          "id": "de73c77c3c33aa6172df33febeeec2e177381819",
          "message": "Various corrections in the documentation (#5154)\n\nAn attempt to improve [the\ndocs](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/index.html)\nby applying various corrections:\n\n- grammar/stylistics,\n- formatting,\n- broken links,\n- broken markdown table,\n- outdated vscode setting name,\n- typos,\n- consistency,\n- etc.\n\nPart of https://github.com/paritytech/eng-automation/issues/10",
          "timestamp": "2024-07-29T13:42:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/de73c77c3c33aa6172df33febeeec2e177381819"
        },
        "date": 1722262671826,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63990.68000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.369148629679994,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.7032649179299515,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.426003861550156,
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
        "date": 1722270179721,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64058.31,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.427813494159901,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.158014217519977,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.875675486790233,
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
        "date": 1722285811001,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52946.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63966.270000000004,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.0241275768699385,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.611581849149974,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.931227710190173,
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
        "date": 1722296852739,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63994.12999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.5140286449399785,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3355425609201723,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.952791369080032,
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
        "date": 1722340175938,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63992.48,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.398524111070053,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.817570888199983,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.261355975450189,
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
        "date": 1722348592941,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64070.030000000006,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 11.19602450742007,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.791821324089911,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 4.123828412870237,
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
        "date": 1722358098514,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64049.840000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.909883033220358,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.909243906309957,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.399459323970008,
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
        "date": 1722368627944,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63965.659999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.383416161139975,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.964778694830143,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2443231682501583,
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
        "date": 1722421596168,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64015.369999999995,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.6851199551699825,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4680471973101845,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.558008515250167,
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
        "date": 1722427531827,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64022.18000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.50348651062998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.5374073148502463,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.783014953219977,
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
        "date": 1722508088662,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64004.1,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.330001695460052,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.246461626520174,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.386040631450024,
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
        "date": 1722607629226,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63971.729999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.632342157680124,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.063022211030017,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9559232939901947,
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
        "date": 1722617626926,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63996.840000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.2244688953501615,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.349575927650002,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.985420694059979,
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
        "date": 1722849940373,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64064.56999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.59999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 4.146772730470229,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.8660132449399685,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.373246887820173,
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
          "id": "f170af615c0dc413482100892758b236d1fda93b",
          "message": "Coretime auto-renew (#4424)\n\nThis PR adds functionality that allows tasks to enable auto-renewal.\nEach task eligible for renewal can enable auto-renewal.\n\nA new storage value is added to track all the cores with auto-renewal\nenabled and the associated task running on the core. The `BoundedVec` is\nsorted by `CoreIndex` to make disabling auto-renewal more efficient.\n\nCores are renewed at the start of a new bulk sale. If auto-renewal\nfails(e.g. due to the sovereign account of the task not holding\nsufficient balance), an event will be emitted, and the renewal will\ncontinue for the other cores.\n\nThe two added extrinsics are:\n- `enable_auto_renew`: Extrinsic for enabling auto renewal.\n- `disable_auto_renew`: Extrinsic for disabling auto renewal.\n\nTODOs:\n- [x] Write benchmarks for the newly added extrinsics.\n\nCloses: #4351\n\n---------\n\nCo-authored-by: Dónal Murray <donalm@seadanda.dev>",
          "timestamp": "2024-08-05T10:53:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f170af615c0dc413482100892758b236d1fda93b"
        },
        "date": 1722857251008,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64001.05,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52945.8,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.983698667170035,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.210714203669974,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.095057548170151,
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
        "date": 1722864729643,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64005.219999999994,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.283696554050104,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.14365975278007,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.456661616480003,
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
        "date": 1722963346434,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64009.28999999999,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.154314093119849,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3032067104001577,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.468291103919978,
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
        "date": 1722973458762,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63987.340000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.0967248367001363,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.230671809039993,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.856199222200107,
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
        "date": 1723030826218,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63963.55,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.69527853632986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0375562558101277,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.118869406650012,
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
        "date": 1723047048155,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63977.159999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.320215698720053,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.905943664549948,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.224696411860087,
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
        "date": 1723056432315,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.2,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64006.23,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.53007133629001,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.721873502320091,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.48345716742016,
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
        "date": 1723113105411,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63954.8,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.8,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.0328102426701227,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.141307258149994,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.777787834499936,
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
        "date": 1723137557828,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63996.23,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.005329117100077,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.293943358360009,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.170019926250149,
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
        "date": 1723196577836,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64046.25,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.59999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.851929849080213,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.722713336630129,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 7.280146446339968,
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
        "date": 1723207369728,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52939.7,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63983.159999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.107701564209908,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.286202056270129,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.388987861850044,
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
        "date": 1723214642416,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63998.780000000006,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.40000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.034211411790045,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2896329767901187,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.418068984550044,
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
        "date": 1723226255534,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63970.409999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.59999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.111419028580122,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.791824688090106,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.194423815019987,
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
        "date": 1723458226287,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63988.8,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.294503039370182,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.131912311540052,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.3504744957599675,
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
        "date": 1723465302855,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63996.75,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.407186756359979,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.105310187820049,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.261590771090217,
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
        "date": 1723477311254,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64014.719999999994,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.276473770809883,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.619556662020003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.426681922260258,
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
        "date": 1723484265681,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63987.52,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.90000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.71788505289003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.051911455860103,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.110596386890091,
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
        "date": 1723500690680,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63977.880000000005,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.988955419560179,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.026731753649996,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.763066269360008,
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
        "date": 1723543989948,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63955.15,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.5,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.1260456769501817,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.959218257750056,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.199612763030009,
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
        "date": 1723556752981,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64024.47000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52937.2,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.716700935340087,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4837282963502174,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.749562231590096,
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
        "date": 1723585025115,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64029.719999999994,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.405814682390209,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.601429268539917,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.570229123949976,
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
        "date": 1723593201378,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63986.07000000002,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.899454134649883,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.088566070580165,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.212976811689993,
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
        "date": 1723632042095,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63992.11000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.2,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.361408983590005,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2764892557501653,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.063154033330084,
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
        "date": 1723643274779,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63972.62000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52945.40000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.631672977990004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.005782832950218,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.054197897689991,
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
        "date": 1723651063394,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64008.479999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944.40000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.709395991810048,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.462092159610208,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.501317037709956,
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
        "date": 1723672108478,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64019.54999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.356338950270034,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.680112417799959,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.478553549410248,
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
        "date": 1723686506957,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63970.409999999996,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.70542123019004,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.133437335499968,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0737267173301674,
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
          "id": "78c3daabd97367f70c1ebc0d7fe55abef4d76952",
          "message": "[Coretime] Always include UnpaidExecution, not just when revenue is > 0 (#5369)\n\nThe NotifyRevenue XCM from relay to coretime chain fails to pass the\nbarrier when revenue is 0.\n\n\nhttps://github.com/paritytech/polkadot-sdk/blob/master/polkadot/runtime/parachains/src/coretime/mod.rs#L401\npushes notifyrevenue onto an [empty\nvec](https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/runtime/parachains/src/coretime/mod.rs#L361)\nwhen `revenue == 0`, so it never explicitly requests unpaid execution,\nbecause that happens only in [the block where revenue is `>\n0`](https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/runtime/parachains/src/coretime/mod.rs#L387).\n\nThis will need to be backported to 1.14 when merged.",
          "timestamp": "2024-08-15T09:02:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/78c3daabd97367f70c1ebc0d7fe55abef4d76952"
        },
        "date": 1723718625627,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52944.09999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64003.17,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.924062291989944,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.140978744599984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0578605483901398,
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
        "date": 1723731571862,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52938.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63986.81999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.216413634890177,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.33324049644999,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.838291761240033,
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
        "date": 1723740152881,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64012.11,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52944,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.400284473940005,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.355313870309969,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2579830732302044,
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
        "date": 1723752242114,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64042.469999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52942.40000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.994587162860003,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.840462027519923,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.6646518794202736,
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
        "date": 1723799895059,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63961.909999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52936.09999999999,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 2.978815863780182,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.737980228909993,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.087663391119967,
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
        "date": 1723810141920,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63947.159999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52936.59999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.623635182789966,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.021944453539983,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 2.9403138501201864,
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
        "date": 1723823572229,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52937.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63983.68000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.428346748449968,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.184212557509973,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.295491811880143,
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
        "date": 1723964892982,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63972.590000000004,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52938.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.288442703410018,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.933371620160003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.211134508210249,
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
        "date": 1724016236337,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63953.72000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.778162384310123,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.2152903659700485,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1248958665701343,
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
        "date": 1724088623962,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63977.740000000005,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52938.7,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.282931874790132,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.339045963439942,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.960068987989978,
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
        "date": 1724159325059,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63987.43000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.09999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 6.085965560710032,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2214128033701135,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.200672159100067,
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
        "date": 1724165023787,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63992.73,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.328924056200027,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4158635143901845,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.32657983025002,
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
        "date": 1724172927131,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63984.31999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.000864014179959,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.3041970783502075,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.1760360518800494,
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
        "date": 1724261485043,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52943.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63950.95000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.146330659830144,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.879855602569958,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 5.943248398629958,
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
        "date": 1724314581644,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.3,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63950.43000000001,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.1101807369000873,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.902915404830065,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 5.915110222400024,
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
        "date": 1724333962056,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63971.9,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52939.3,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 9.76835633437003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1054330154900818,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 5.916512165220051,
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
        "date": 1724342565421,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64027.740000000005,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.813031549180208,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.8483215990399815,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 11.059277868179967,
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
        "date": 1724406976768,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64029.240000000005,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.769106074630089,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.895875010969908,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.889657088439929,
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
        "date": 1724427999906,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64006.159999999996,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943.40000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.255635999469906,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.3636525693100365,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.4625530340401793,
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
        "date": 1724545406323,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 64037.96,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52941.90000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.718658806359993,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.7360097644299035,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.715958127870208,
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
        "date": 1724631107071,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63982.4,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52940.8,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 5.990126799739963,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.120989197089923,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.1456280256201308,
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
          "id": "ad0de7495e0e4685408d70c755cddaa14e3b02ca",
          "message": "`MaybeConsideration` extension trait for `Consideration` (#5384)\n\nIntroduce `MaybeConsideration` extension trait for `Consideration`.\n\nThe trait allows for the management of tickets that may represent no\ncost. While the `MaybeConsideration` still requires proper handling, it\nintroduces the ability to determine if a ticket represents no cost and\ncan be safely forgotten without any side effects.\n\nThe new trait is particularly useful when a consumer expects the cost to\nbe zero under certain conditions (e.g., when the proposal count is below\na threshold N) and does not want to store such consideration tickets in\nstorage. The extension approach allows us to avoid breaking changes to\nthe existing trait and to continue using it as a non-optional version\nfor migrating pallets that utilize the `Currency` and `fungible` traits\nfor `holds` and `freezes`, without requiring any storage migration.",
          "timestamp": "2024-08-26T13:23:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ad0de7495e0e4685408d70c755cddaa14e3b02ca"
        },
        "date": 1724684755615,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64020.06999999999,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.65090767907993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.499677459320157,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.433570505210028,
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
        "date": 1724693144764,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52936.40000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64015.81000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.391039205209953,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.521128758780028,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.5982489431202667,
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
        "date": 1724707992348,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.90000000001,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63985.71,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.35911243345003,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.2641225348700225,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.43528272660017,
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
        "date": 1724723741877,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.8,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 64029.8,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 3.60841775451026,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 10.538730435899982,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.604041739390037,
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
          "id": "f0323d529615de0de121a87eb1c8c6da82bd0ff8",
          "message": "Remove deprecated calls in cumulus-parachain-system (#5439)\n\nCalls were written to be removed after June 2024. This PR removes them.\n\nThis PR will break users using those calls. The call won't be decodable\nby the runtime, so it should fail early with no consequences. The\nfunctionality must be same as before, users will just need to use the\ncalls in `System`.",
          "timestamp": "2024-08-27T09:28:52Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f0323d529615de0de121a87eb1c8c6da82bd0ff8"
        },
        "date": 1724753417489,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52941,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63998.66000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-voting",
            "value": 10.398562626889959,
            "unit": "seconds"
          },
          {
            "name": "approval-distribution",
            "value": 6.288501460940085,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.397132808970126,
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
        "date": 1724758979820,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 63984.93000000001,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 52943,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 5.941658466960047,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.116074911960094,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.838725640100085,
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
        "date": 1724781692155,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52942.59999999999,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63983.97000000001,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 5.978988129619966,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.2195390776900874,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.769498369179951,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}