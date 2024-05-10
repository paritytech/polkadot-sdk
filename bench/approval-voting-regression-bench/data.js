window.BENCHMARK_DATA = {
  "lastUpdate": 1715347696023,
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
      }
    ]
  }
}