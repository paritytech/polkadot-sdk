window.BENCHMARK_DATA = {
  "lastUpdate": 1736195440022,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "email": "41779041+alvicsam@users.noreply.github.com",
            "name": "Alexander Samusev",
            "username": "alvicsam"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "721f6d97613b0ece9c8414e8ec8ba31d2f67d40c",
          "message": "[WIP] Fix networking-benchmarks (#7036)\n\ncc https://github.com/paritytech/ci_cd/issues/1094",
          "timestamp": "2025-01-03T13:19:18Z",
          "tree_id": "bec3589885e7e27d15b93c25e9283008397e0049",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/721f6d97613b0ece9c8414e8ec8ba31d2f67d40c"
        },
        "date": 1735914628128,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 17243400,
            "range": "± 496233",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 17484119,
            "range": "± 365774",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 18688949,
            "range": "± 313839",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 22455693,
            "range": "± 428129",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 5322994,
            "range": "± 170673",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 31013751,
            "range": "± 1114317",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 218501431,
            "range": "± 13972729",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 1946804769,
            "range": "± 96309764",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "yangqiwei97@gmail.com",
            "name": "Qiwei Yang",
            "username": "qiweiii"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0b4f131b000e01f1aca3f023937a36dcc281d5e2",
          "message": "Replace duplicated whitelist with whitelisted_storage_keys (#7024)\n\nrelated issue: #7018\n\nreplaced duplicated whitelists with\n`AllPalletsWithSystem::whitelisted_storage_keys();` in this PR\n\n---------\n\nCo-authored-by: Guillaume Thiolliere <gui.thiolliere@gmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-03T22:22:12Z",
          "tree_id": "152f965d219452ec180463a016d757b195bd79ff",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0b4f131b000e01f1aca3f023937a36dcc281d5e2"
        },
        "date": 1735947378032,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 17771421,
            "range": "± 379395",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 17796252,
            "range": "± 375294",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 19192305,
            "range": "± 550825",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 23424069,
            "range": "± 618808",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 6000664,
            "range": "± 322927",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 35457468,
            "range": "± 1762375",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 238664091,
            "range": "± 5449223",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 2169025360,
            "range": "± 170481694",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "gui.thiolliere@gmail.com",
            "name": "Guillaume Thiolliere",
            "username": "gui1117"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "b5a5ac4487890046d226bedb0238eaccb423ae42",
          "message": "Make `TransactionExtension` tuple of tuple transparent for implication (#7028)\n\nCurrently `(A, B, C)` and `((A, B), C)` change the order of implications\nin the transaction extension pipeline. This order is not accessible in\nthe metadata, because the metadata is just a vector of transaction\nextension, the nested structure is not visible.\n\nThis PR make the implementation for tuple of `TransactionExtension`\nbetter for tuple of tuple. `(A, B, C)` and `((A, B), C)` don't change\nthe implication for the validation A.\n\nThis is a breaking change but only when using the trait\n`TransactionExtension` the code implementing the trait is not breaking\n(surprising rust behavior but fine).\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-04T02:03:30Z",
          "tree_id": "7ceca99999c8a5065cee89a611c7b74b87f097cd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b5a5ac4487890046d226bedb0238eaccb423ae42"
        },
        "date": 1735960720711,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 17906473,
            "range": "± 402710",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 18028249,
            "range": "± 376977",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 19607414,
            "range": "± 347759",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 23842723,
            "range": "± 674082",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 5669355,
            "range": "± 326213",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 33259681,
            "range": "± 1448902",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 254199472,
            "range": "± 17716451",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 2206601933,
            "range": "± 183595326",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "gui.thiolliere@gmail.com",
            "name": "Guillaume Thiolliere",
            "username": "gui1117"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "63c73bf6db1c8982ad3f2310a40799c5987f8900",
          "message": "Implement cumulus StorageWeightReclaim as wrapping transaction extension + frame system ReclaimWeight (#6140)\n\n(rebasing of https://github.com/paritytech/polkadot-sdk/pull/5234)\n\n## Issues:\n\n* Transaction extensions have weights and refund weight. So the\nreclaiming of unused weight must happen last in the transaction\nextension pipeline. Currently it is inside `CheckWeight`.\n* cumulus storage weight reclaim transaction extension misses the proof\nsize of logic happening prior to itself.\n\n## Done:\n\n* a new storage `ExtrinsicWeightReclaimed` in frame-system. Any logic\nwhich attempts to do some reclaim must use this storage to avoid double\nreclaim.\n* a new function `reclaim_weight` in frame-system pallet: info and post\ninfo in arguments, read the already reclaimed weight, calculate the new\nunused weight from info and post info. do the more accurate reclaim if\nhigher.\n* `CheckWeight` is unchanged and still reclaim the weight in post\ndispatch\n* `ReclaimWeight` is a new transaction extension in frame system. For\nsolo chains it must be used last in the transactino extension pipeline.\nIt does the final most accurate reclaim\n* `StorageWeightReclaim` is moved from cumulus primitives into its own\npallet (in order to define benchmark) and is changed into a wrapping\ntransaction extension.\nIt does the recording of proof size and does the reclaim using this\nrecording and the info and post info. So parachains don't need to use\n`ReclaimWeight`. But also if they use it, there is no bug.\n\n    ```rust\n  /// The TransactionExtension to the basic transaction logic.\npub type TxExtension =\ncumulus_pallet_weight_reclaim::StorageWeightReclaim<\n         Runtime,\n         (\n                 frame_system::CheckNonZeroSender<Runtime>,\n                 frame_system::CheckSpecVersion<Runtime>,\n                 frame_system::CheckTxVersion<Runtime>,\n                 frame_system::CheckGenesis<Runtime>,\n                 frame_system::CheckEra<Runtime>,\n                 frame_system::CheckNonce<Runtime>,\n                 frame_system::CheckWeight<Runtime>,\npallet_transaction_payment::ChargeTransactionPayment<Runtime>,\n                 BridgeRejectObsoleteHeadersAndMessages,\n\n(bridge_to_rococo_config::OnBridgeHubWestendRefundBridgeHubRococoMessages,),\nframe_metadata_hash_extension::CheckMetadataHash<Runtime>,\n         ),\n  >;\n  ```\n\n---------\n\nCo-authored-by: GitHub Action <action@github.com>\nCo-authored-by: georgepisaltu <52418509+georgepisaltu@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-05T03:25:52Z",
          "tree_id": "92e44a9c9c2bddb2fc5b835eb4da173f2b8a4077",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/63c73bf6db1c8982ad3f2310a40799c5987f8900"
        },
        "date": 1736052015632,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 19360222,
            "range": "± 427025",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 19605168,
            "range": "± 371217",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 20540575,
            "range": "± 326460",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 24560635,
            "range": "± 359968",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 5914085,
            "range": "± 229219",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 39788757,
            "range": "± 1756332",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 269388417,
            "range": "± 9017069",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 2050114568,
            "range": "± 261694105",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "taozui472@gmail.com",
            "name": "taozui472",
            "username": "taozui472"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6eca7647dc99dd0e78aacb740ba931e99e6ba71f",
          "message": "chore: delete repeat words (#7034)\n\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-01-06T08:44:06Z",
          "tree_id": "abf7ac90d5af0ebd540aee8c38f228c769112504",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6eca7647dc99dd0e78aacb740ba931e99e6ba71f"
        },
        "date": 1736157315336,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 18518156,
            "range": "± 286241",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 18459913,
            "range": "± 455375",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 19408558,
            "range": "± 305986",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 23380746,
            "range": "± 399078",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 5245347,
            "range": "± 125418",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 32064176,
            "range": "± 1157857",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 247078269,
            "range": "± 9462816",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 2038311863,
            "range": "± 101668256",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alin@parity.io",
            "name": "Alin Dima",
            "username": "alindima"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ffa90d0f2b9b4438e2f0fa3d4d532923d7ba978f",
          "message": "fix chunk fetching network compatibility zombienet test (#6988)\n\nFix this zombienet test\n\nIt was failing because in\nhttps://github.com/paritytech/polkadot-sdk/pull/6452 I enabled the v2\nreceipts for testnet genesis,\nso the collators started sending v2 receipts with zeroed collator\nsignatures to old validators that were still checking those signatures\n(which lead to disputes, since new validators considered the candidates\nvalid).\n\nThe fix is to also use an old image for collators, so that we don't\ncreate v2 receipts.\n\nWe cannot remove this test yet because collators also perform chunk\nrecovery, so until all collators are upgraded, we need to maintain this\ncompatibility with the old protocol version (which is also why\nsystematic recovery was not yet enabled)",
          "timestamp": "2025-01-06T09:57:29Z",
          "tree_id": "09edfb7ad810cd2c4e6d7ce014ae8e109aae45ec",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ffa90d0f2b9b4438e2f0fa3d4d532923d7ba978f"
        },
        "date": 1736162216242,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 18231900,
            "range": "± 476847",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 18591444,
            "range": "± 524627",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 19871592,
            "range": "± 642445",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 24544356,
            "range": "± 951555",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 6204546,
            "range": "± 387923",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 36951518,
            "range": "± 2115986",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 261906263,
            "range": "± 14637725",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 2314619027,
            "range": "± 300054232",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "skunert49@gmail.com",
            "name": "Sebastian Kunert",
            "username": "skunert"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "1dcff3df39b85fa43c7ca1dafe10f802cd812234",
          "message": "Avoid incomplete block import pipeline with full verifying import queue (#7050)\n\n## Problem\nIn the parachain template we use the [fully verifying import queue\n\n](https://github.com/paritytech/polkadot-sdk/blob/3d9eddbeb262277c79f2b93b9efb5af95a3a35a8/cumulus/client/consensus/aura/src/equivocation_import_queue.rs#L224-L224)\nwhich does extra equivocation checks.\n\nHowever, when we import a warp synced block with state, we don't set a\nfork choice, leading to an incomplete block import pipeline and error\nhere:\nhttps://github.com/paritytech/polkadot-sdk/blob/3d9eddbeb262277c79f2b93b9efb5af95a3a35a8/substrate/client/service/src/client/client.rs#L488-L488\n\nThis renders warp sync useless for chains using this import queue.\n\n## Fix\nThe fix is to always import a block with state as best block, as we\nalready do in the normal Aura Verifier.\nIn a follow up we should also take another look into unifying the usage\nof the different import queues.\n\nfixes https://github.com/paritytech/project-mythical/issues/256\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-06T13:09:06Z",
          "tree_id": "7963451289ed3c1109e365c661371697a9d65d03",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1dcff3df39b85fa43c7ca1dafe10f802cd812234"
        },
        "date": 1736173094395,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 16988649,
            "range": "± 243087",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 17163058,
            "range": "± 273688",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 18275590,
            "range": "± 242814",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 22010132,
            "range": "± 331330",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 5107806,
            "range": "± 105368",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 29263117,
            "range": "± 732023",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 208360938,
            "range": "± 5879735",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 2004721601,
            "range": "± 169827432",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "oliver.tale-yazdi@parity.io",
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "568231a9a85d94954c002532a0f4351a3bb59e83",
          "message": "[core-fellowship] Add permissionless import_member (#7030)\n\nChanges:\n- Add call `import_member` to the core-fellowship pallet.\n- Move common logic between `import` and `import_member` into\n`do_import`.\n\n## `import_member`\n\nCan be used to induct an arbitrary collective member and is callable by\nany signed origin. Pays no fees upon success.\nThis is useful in the case that members did not induct themselves and\nare idling on their rank.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-06T13:52:07Z",
          "tree_id": "d1e2d74ae5b93d0e9604eeeaca75c9b9d82569a9",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/568231a9a85d94954c002532a0f4351a3bb59e83"
        },
        "date": 1736178119494,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 18128978,
            "range": "± 434166",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 18306660,
            "range": "± 686358",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 18926907,
            "range": "± 421995",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 23033271,
            "range": "± 426547",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 5222637,
            "range": "± 140025",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 30113118,
            "range": "± 811958",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 237846203,
            "range": "± 18053490",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 2073830289,
            "range": "± 115973754",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "3776356370@qq.com",
            "name": "jasmy",
            "username": "jasmyhigh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6b6c70b0165b2c38e239eb740a7561e9ed4570de",
          "message": "Fix typos (#7027)\n\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-01-06T19:16:08Z",
          "tree_id": "abcb5e8f549fd7a7da9d45937584e1de89eb613e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6b6c70b0165b2c38e239eb740a7561e9ed4570de"
        },
        "date": 1736195422967,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 18089519,
            "range": "± 373428",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 18749405,
            "range": "± 346210",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 20363073,
            "range": "± 338558",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 24008737,
            "range": "± 477491",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 5540955,
            "range": "± 156486",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 33466866,
            "range": "± 1182922",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 247746785,
            "range": "± 13227748",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 2112305110,
            "range": "± 138039817",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}