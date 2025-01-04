window.BENCHMARK_DATA = {
  "lastUpdate": 1735960713117,
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
        "date": 1735914603946,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3790714,
            "range": "± 49805",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 280423,
            "range": "± 8535",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3740076,
            "range": "± 63535",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 378203,
            "range": "± 9030",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4538192,
            "range": "± 68828",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 810119,
            "range": "± 20666",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9301175,
            "range": "± 183160",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4309488,
            "range": "± 119004",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2828183,
            "range": "± 52096",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1481785,
            "range": "± 14669",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2909037,
            "range": "± 44675",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1559170,
            "range": "± 14384",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3472338,
            "range": "± 45398",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1852563,
            "range": "± 45379",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7372091,
            "range": "± 112889",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4680947,
            "range": "± 98921",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4101459,
            "range": "± 76049",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3410050,
            "range": "± 48784",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 31271719,
            "range": "± 489301",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 27258045,
            "range": "± 377716",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 243457716,
            "range": "± 2102365",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 254837166,
            "range": "± 13296301",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3267873024,
            "range": "± 8798185",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1985384616,
            "range": "± 27913000",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 3924722,
            "range": "± 116922",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 3884115,
            "range": "± 127652",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 35751783,
            "range": "± 733203",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 35967372,
            "range": "± 1067528",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 325261952,
            "range": "± 3199301",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 348435466,
            "range": "± 9604483",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3800460071,
            "range": "± 31244354",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3059649471,
            "range": "± 39515314",
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
        "date": 1735947354355,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3944990,
            "range": "± 92447",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 277780,
            "range": "± 9957",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 4037729,
            "range": "± 103709",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 365733,
            "range": "± 10243",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4647091,
            "range": "± 146536",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 820305,
            "range": "± 24687",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9461549,
            "range": "± 312455",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4496076,
            "range": "± 175532",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2841015,
            "range": "± 48307",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1479968,
            "range": "± 23352",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2966684,
            "range": "± 56924",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1542928,
            "range": "± 20895",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3627783,
            "range": "± 124814",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1876753,
            "range": "± 42325",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 8291595,
            "range": "± 382313",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4918821,
            "range": "± 300327",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4258076,
            "range": "± 125864",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3631554,
            "range": "± 174637",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 32304698,
            "range": "± 1233620",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 26866931,
            "range": "± 684322",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 243511242,
            "range": "± 3138339",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 250816924,
            "range": "± 13088899",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3283341160,
            "range": "± 28213906",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1989251853,
            "range": "± 21133350",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4152213,
            "range": "± 144076",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4115112,
            "range": "± 195587",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 36991460,
            "range": "± 1452566",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 37182421,
            "range": "± 1897911",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 350617389,
            "range": "± 10046434",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 383970463,
            "range": "± 14911332",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 4186809329,
            "range": "± 77725562",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3503044971,
            "range": "± 58740262",
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
        "date": 1735960696275,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3893421,
            "range": "± 71915",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 283137,
            "range": "± 10232",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3741320,
            "range": "± 66768",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 373672,
            "range": "± 8416",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4576450,
            "range": "± 99153",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 829460,
            "range": "± 21715",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9442735,
            "range": "± 189960",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4345652,
            "range": "± 106535",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2903042,
            "range": "± 40565",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1452544,
            "range": "± 18426",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2955019,
            "range": "± 44517",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1509819,
            "range": "± 23918",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3530483,
            "range": "± 65683",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1818739,
            "range": "± 31399",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7521213,
            "range": "± 223106",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4713694,
            "range": "± 148859",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4247828,
            "range": "± 187120",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3583987,
            "range": "± 136822",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 32427198,
            "range": "± 1137804",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 27429801,
            "range": "± 717428",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 265004638,
            "range": "± 12957562",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 264684418,
            "range": "± 10329420",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3302716996,
            "range": "± 30018213",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1998115263,
            "range": "± 23777235",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4176462,
            "range": "± 177166",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4238774,
            "range": "± 214441",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 37132306,
            "range": "± 1472061",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 36735156,
            "range": "± 1421857",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 329545744,
            "range": "± 5446563",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 369957532,
            "range": "± 10906933",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 4075679310,
            "range": "± 25664431",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3198182712,
            "range": "± 51410875",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}