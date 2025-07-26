window.BENCHMARK_DATA = {
  "lastUpdate": 1753526946551,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "dispute-coordinator-regression-bench": [
      {
        "commit": {
          "author": {
            "email": "eresav@me.com",
            "name": "Andrei Eres",
            "username": "AndreiEres"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "83b0409093f811acb412b07ac7219b7ad1a514ff",
          "message": "[subsystem-bench] Add Dispute Coordinator subsystem benchmark (#8828)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/8811\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-03T12:22:23Z",
          "tree_id": "7dedca9f4f5317f038bb7713852df1f21eeee806",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/83b0409093f811acb412b07ac7219b7ad1a514ff"
        },
        "date": 1751549436117,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005595405729999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008679936599999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026281824699999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "egor@parity.io",
            "name": "Egor_P",
            "username": "EgorPopelyaev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "3bd01b9c89dbef0f57a3c0fb7f600fbb5befff65",
          "message": "[Release|CI/CD] Fix syncing in the release flow (#9092)\n\nThis PR adds a fix for the release pipelines. The sync flow needs a\nsecrete to be passed when it is called from another flow and syncing\nbetween release org and the main repo is needed.\nMissing secrets were added to the appropriate flows.",
          "timestamp": "2025-07-03T15:06:37Z",
          "tree_id": "806f5adc03322aa929b1b29440cb9212f69c9fe8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3bd01b9c89dbef0f57a3c0fb7f600fbb5befff65"
        },
        "date": 1751559377721,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005582663829999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026697256099999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008752567599999988,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "10196091+Ank4n@users.noreply.github.com",
            "name": "Ankan",
            "username": "Ank4n"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "f1ba2a1c7206c70ad66168859c90ab4e4327aab6",
          "message": "Optimize buffered offence storage and prevent unbounded growth in staking-async ah-client pallet (#9049)\n\n## 🤔 Why\nThis addresses potential memory issues and improves efficiency of\noffence handling during buffered operating mode (see\nhttps://github.com/paritytech-secops/srlabs_findings/issues/525)\n\n\n## 🔑 Key changes\n\n- Prevents duplicate offences for the same offender in the same session\nby keeping only the highest slash fraction\n- Introduces `BufferedOffence` struct with optional reporter and slash\nfraction fields\n- Restructures buffered offences storage from `Vec<(SessionIndex,\nVec<Offence>)>` to nested `BTreeMap<SessionIndex, BTreeMap<AccountId,\nBufferedOffence>>`\n- Adds `MaxOffenceBatchSize` configuration parameter for batching\ncontrol\n- Processes offences in batches with configurable size limits, sending\nonly first session's offences per block\n- Implements proper benchmarking infrastructure for\n`process_buffered_offences` function\n- Adds WeightInfo trait with benchmarked weights for batch processing in\n`on_initialize` hook\n\n## ✍️ Co-authors\n@Ank4n \n@sigurpol\n\n---------\n\nCo-authored-by: Paolo La Camera <paolo@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-04T09:02:33Z",
          "tree_id": "410487862394418dd87119db2954a36e4de0c43c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f1ba2a1c7206c70ad66168859c90ab4e4327aab6"
        },
        "date": 1751623985007,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002641694280000002,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00871780210999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005657479960000001,
            "unit": "seconds"
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
          "distinct": false,
          "id": "22714211e4f558abbabae28fc2e8f2c971143638",
          "message": "[AHM] Derive DecodeWithMemTracking and pub fields (#9067)\n\n- Derive `DecodeWithMemTracking` on structs\n- Make some fields public\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-07-04T10:36:12Z",
          "tree_id": "0dd0655d92d837e407ee908f523b783ecccc626a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/22714211e4f558abbabae28fc2e8f2c971143638"
        },
        "date": 1751629886195,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005486065759999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008570165919999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00267932138,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "5588131+kianenigma@users.noreply.github.com",
            "name": "Kian Paimani",
            "username": "kianenigma"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "252649fc0105efc8b32b2e1a3649bd6d09f8bd53",
          "message": "add benchmark for prune-era (#9056)\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-04T18:25:54Z",
          "tree_id": "c4480f0f14cd79f70f4a2733fab6a6d0c4c81f6b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/252649fc0105efc8b32b2e1a3649bd6d09f8bd53"
        },
        "date": 1751657691195,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005649167919999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008880581469999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026971257299999987,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "james@jsdw.me",
            "name": "James Wilson",
            "username": "jsdw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "771c9988e2a636a150d97c10e3122af8068d1687",
          "message": "Bump CI to Rustc 1.88 to support 2024 edition crates (#8592)\n\nAs one example, this allows us to use the latest version of Subxt: 0.42.\nAlso if-let chains :)\n\nMain changes:\n- Update CI image\n- Remove `forklift` from Build step in\n`check-revive-stable-uapi-polkavm`; it seemed to [cause an\nerror](https://github.com/paritytech/polkadot-sdk/actions/runs/16004536662/job/45148002314?pr=8592).\nPerhaps we can open an issue for this to fix/try again after this\nmerges.\n- Bump `polkavm` deps to 0.26 to avoid [this\nerror](https://github.com/paritytech/polkadot-sdk/actions/runs/16004991577/job/45150325849?pr=8592#step:5:1967)\n(thanks @koute!)\n- Add `result_large_err` clippy to avoid a bunch of clippy warnings\nabout a 176 byte error (again, we could fix this later more properly).\n- Clippy fixes (mainly inlining args into `format!`s where possible),\nremove one `#[no_mangle]` on a `#[panic_hook]` and a few other misc\nautomatic fixes.\n- `#[allow(clippy::useless_conversion)]` in frame macro to avoid the\ngenerated `.map(Into::into).map_err(Into::into)` code causing an issue\nwhen not necessary (it is sometimes; depends on the return type in\npallet calls)\n- UI test updates\n\nAs a side note, I haven't added a `prdoc` since I'm not making any\nbreaking changes (despite touching a bunch of pallets), just clippy/fmt\ntype things. Please comment if this isn't ok!\n\nAlso, thankyou @bkchr for the wasmtime update PR which fixed a blocker\nhere!\n\n---------\n\nCo-authored-by: Evgeny Snitko <evgeny@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-04T21:54:27Z",
          "tree_id": "bbce6a530538cfc5d3328f5239b16d133890b86d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/771c9988e2a636a150d97c10e3122af8068d1687"
        },
        "date": 1751670346956,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008583395449999991,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051193470899999925,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025815619300000006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "14218860+iulianbarbu@users.noreply.github.com",
            "name": "Iulian Barbu",
            "username": "iulianbarbu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "436b4935b52562f79a83b6ecadeac7dcbc1c2367",
          "message": "`polkadot-omni-node`: pass timestamp inherent data for block import (#9102)\n\n# Description\n\nThis should allow aura runtimes to check timestamp inherent data to\nsync/import blocks that include timestamp inherent data.\n\nCloses #8907 \n\n## Integration\n\nRuntime developers can check timestamp inherent data while using\n`polkadot-omni-node-lib`/`polkadot-omni-node`/`polkadot-parachain`\nbinaries. This change is backwards compatible and doesn't require\nruntimes to check the timestamp inherent, but they are able to do it now\nif needed.\n\n## Review Notes\n\nN/A\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-06T09:32:11Z",
          "tree_id": "239ba865d190c48c06af7d1fa35ceb411cc31cea",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/436b4935b52562f79a83b6ecadeac7dcbc1c2367"
        },
        "date": 1751798589854,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00855703834999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002733640860000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005003881119999989,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "cb12563ae4e532876c29b67be9a7f5d06fdc9fc3",
          "message": "Replace `assert_para_throughput` with `assert_finalized_para_throughput` (#9117)\n\nThere is no need to have two functions which are essentially doing the\nsame. It is also better to oberserve the finalized blocks, which also\nsimplifies the code. So, this pull request is replacing the\n`assert_para_throughput` with `assert_finalized_para_throughput`. It\nalso replaces any usage of `assert_finalized_para_throughput` with\n`assert_para_throughput`.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-08T16:04:23Z",
          "tree_id": "faed545176a9de8b004b29e5ee7e4b5c2ccecef6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cb12563ae4e532876c29b67be9a7f5d06fdc9fc3"
        },
        "date": 1751995024154,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026695474100000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00859867911999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005122107889999993,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49718502+alexggh@users.noreply.github.com",
            "name": "Alexandru Gheorghe",
            "username": "alexggh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "88fc41c9cf5e46277b7cab53a72c650b75377d25",
          "message": "make 0002-parachains-disputes a bit more robust (#9074)\n\nThere is inherently a race between the time we snapshot\nfinality_lag/disputes_finality_lag metrics and if the dispute/approvals\nfinished, so sometimes the test was failing because it was reporting 1\nwhich is in no way a problem, so let's make it a bit more robust by\nsimply waiting more time to reach 0.\n\nFixes: https://github.com/paritytech/polkadot-sdk/issues/8941.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2025-07-08T16:10:51Z",
          "tree_id": "8a90317b0febd3a60f76b56d7a854edcf7a4085d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/88fc41c9cf5e46277b7cab53a72c650b75377d25"
        },
        "date": 1751997109460,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026244691599999993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005114807139999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008560092539999998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "franciscoaguirreperez@gmail.com",
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4d5e95217831fb75942d8153a22f6864858c1d71",
          "message": "XCM precompile: don't support older xcm versions (#9126)\n\nThe latest XCM version is 5. A lot of parachains are still running V3 or\nV4 which is why we haven't removed them, but the XCM precompile is new\nand should only have to deal with versions 5 and onwards. No need to\nkeep dragging 3 and 4 in contracts.",
          "timestamp": "2025-07-08T17:27:43Z",
          "tree_id": "2944a79e52968a0f54da0a246a07867b8f95dffe",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4d5e95217831fb75942d8153a22f6864858c1d71"
        },
        "date": 1752000039848,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005085985199999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263165981,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00852913286999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "9b7c20a2a187e57433c055592609e35af0258bbc",
          "message": "Fix seal_call benchmark (#9112)\n\nFix seal_call benchmark, ensure that the benchmarked block actually\nsucceed\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-08T18:30:43Z",
          "tree_id": "a5d64f5c7d1bffccf857ee5ff83a6f6b305f5ee0",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9b7c20a2a187e57433c055592609e35af0258bbc"
        },
        "date": 1752004430350,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026429404299999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008568671429999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005159262569999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "franciscoaguirreperez@gmail.com",
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ba2a8dc536db30397c332a2aa2cd9f9863027093",
          "message": "XCM precompile: small cleanup (#9135)\n\nFollow-up to\nhttps://github.com/paritytech/polkadot-sdk/pull/9125#discussion_r2192896809",
          "timestamp": "2025-07-08T19:47:45Z",
          "tree_id": "e7aeb64bf7cbd7d415bc142f30193c7d6ec3f579",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ba2a8dc536db30397c332a2aa2cd9f9863027093"
        },
        "date": 1752008673216,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.00520881492999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026551542999999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008714952019999993,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "dharjeezy@gmail.com",
            "name": "dharjeezy",
            "username": "dharjeezy"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "cc972542e0df0266cde2ead4cfac3b1558c860af",
          "message": "pallet bounties v2 benchmark (#8952)\n\ncloses #8649\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-08T21:47:29Z",
          "tree_id": "92ea303bb8df02e5752f9903f5541e35918ac3a9",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cc972542e0df0266cde2ead4cfac3b1558c860af"
        },
        "date": 1752015675272,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026522110800000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008721413299999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005168960659999988,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "Sajjon@users.noreply.github.com",
            "name": "Alexander Cyon",
            "username": "Sajjon"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7ab0dcd62887ea3c5e50cfb5b1b01beb09d0ec92",
          "message": "Add `para_ids` Runtime API (#9055)\n\nImplementation of https://github.com/paritytech/polkadot-sdk/issues/9053\n\n---------\n\nCo-authored-by: alindima <alin@parity.io>",
          "timestamp": "2025-07-09T07:17:25Z",
          "tree_id": "efefbe78f8e545dae503496bbc822b03e32d1e13",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7ab0dcd62887ea3c5e50cfb5b1b01beb09d0ec92"
        },
        "date": 1752049594274,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002608908810000001,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008476387969999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005002263799999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "egor@parity.io",
            "name": "Egor_P",
            "username": "EgorPopelyaev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "cd39c26a4da04693b07b3ed752ea239f452795ea",
          "message": "[Release|CI/CD] Move runtimes build to a separate pipeline and let it trigger the next flow (#9118)\n\nThis PR incudes the following changes:\n- Cut the runtimes build from the Create Draft flow into a standalone\npipeline\n- Add a trigger to the Build Runtimes pipeline that will be starting the\nCreate Draft flow automatically when the runtimes are built\nsuccessfully.\n\nCloses: https://github.com/paritytech/devops/issues/3827 and partially:\nhttps://github.com/paritytech/devops/issues/3828",
          "timestamp": "2025-07-09T08:40:25Z",
          "tree_id": "69aff4dc6192fec945b7a0b030222c92ac453a33",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cd39c26a4da04693b07b3ed752ea239f452795ea"
        },
        "date": 1752054592670,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00271226005,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005194933789999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008831572839999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bkontur@gmail.com",
            "name": "Branislav Kontur",
            "username": "bkontur"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "83afbeeb906131755fdcea3b891ea1883c4d17d0",
          "message": "Expose more constants for pallet-xcm (#9139)\n\nLet's expose more constants, similar as `AdvertisedXcmVersion`.\n\n\n![image](https://github.com/user-attachments/assets/5ddc265f-546b-45a0-8235-3f53c3108823)\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-09T12:29:35Z",
          "tree_id": "6fb2c4c504887609989d96ab44ba1a1afbe03294",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/83afbeeb906131755fdcea3b891ea1883c4d17d0"
        },
        "date": 1752068758017,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005127152969999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00260139055,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00855473461999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "117115317+lrubasze@users.noreply.github.com",
            "name": "Lukasz Rubaszewski",
            "username": "lrubasze"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7305f96aa8fc68b7249587c21f5fa2d4520c54cd",
          "message": "CI - zombienet cumulus tests zombienet sdk (#8954)\n\n### This PR includes the following changes:\n\n- Migrates Zombienet Cumulus tests to `zombienet-sdk`\n- Re-enables the tests, with the following exceptions (to be addressed\nseparately):\n  - `zombienet-cumulus-0002-pov_recovery` - #8985 \n- `zombienet-cumulus-0006-rpc_collator_builds_blocks` - root cause the\nsame as #8985\n  - `zombienet-cumulus-0009-elastic_scaling_pov_recovery` – #8999\n- `zombienet-cumulus-0010-elastic_scaling_multiple_block_per_slot` –\n#9018\n- Adds the following tests to CI:\n  - `zombienet-cumulus-0011-dht-bootnodes`\n  - `zombienet-cumulus-0012-parachain_extrinsic_gets_finalized`\n  - `zombienet-cumulus-0013-elastic_scaling_slot_based_rp_offset`\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: Javier Viola <javier@parity.io>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Anthony Lazam <xlzm.tech@gmail.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2025-07-09T16:01:41Z",
          "tree_id": "7b46e0ac8c2ed95e791c472fb7a82ebbc6a32685",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7305f96aa8fc68b7249587c21f5fa2d4520c54cd"
        },
        "date": 1752081449064,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.004915220189999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00252144691,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008389578499999993,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "117115317+lrubasze@users.noreply.github.com",
            "name": "Lukasz Rubaszewski",
            "username": "lrubasze"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "409587adfb4cc5e28e28272e768361afdbea2191",
          "message": "Enable parachain-templates zombienet tests (#9131)\n\nThis PR includes the following changes:\n- Refactor Parachain Templates workflow to run tests individually\n- Enables Zombienet Parachain Templates tests in CI\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: Javier Viola <javier@parity.io>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Anthony Lazam <xlzm.tech@gmail.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>",
          "timestamp": "2025-07-10T06:33:27Z",
          "tree_id": "36c66069301310187811ad4f0537df4b18e2050f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/409587adfb4cc5e28e28272e768361afdbea2191"
        },
        "date": 1752133208346,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005032093999999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025824143899999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008484359889999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49718502+alexggh@users.noreply.github.com",
            "name": "Alexandru Gheorghe",
            "username": "alexggh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "12ddb5a71ddd744e48bbf49a4cc0b44c5381747e",
          "message": "bitfield_distribution: fix subsystem clogged at begining of a session (#9094)\n\n`handle_peer_view_change` gets called on NewGossipTopology with the\nexisting view of the peer to cover for the case when the topology might\narrive late, but in that case in the view will contain old blocks from\nprevious session, so since the X/Y neighbour change because of the\ntopology change you end up sending a lot of messages for blocks before\nthe session changed.\n\nFix it by checking the send message only for relay chains that are in\nthe same session as the current topology.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-10T10:00:44Z",
          "tree_id": "0adae7550a477fef6b79346b2a017a665b321042",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/12ddb5a71ddd744e48bbf49a4cc0b44c5381747e"
        },
        "date": 1752145985591,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005142384619999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026690400299999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008612664559999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "178801527+raymondkfcheung@users.noreply.github.com",
            "name": "Raymond Cheung",
            "username": "raymondkfcheung"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "466149d0eac8e608a6e6b6db8cda98a555b6c7e8",
          "message": "Replace `log` with `tracing` on XCM-related modules (#8732)\n\nThis PR replaces `log` with `tracing` instrumentation on XCM-related\nmodules to significantly improve debugging capabilities for XCM\nexecution flows.\n\nContinues #8724 and partially addresses #6119 by providing structured\nlogging throughout XCM components, making it easier to diagnose\nexecution failures, fee calculation errors, and routing issues.\n\n## Key Features\n\n- **Consistent targets**: All components use predictable `xcm::*` log\ntargets\n- **Structured fields**: Uses `?variable` syntax for automatic Debug\nformatting\n- **Zero runtime impact**: No behavioural changes, only observability\nimprovements",
          "timestamp": "2025-07-10T12:54:12Z",
          "tree_id": "363cb00f3cfd55c0e8a1f74f8964ebc2e32b0156",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/466149d0eac8e608a6e6b6db8cda98a555b6c7e8"
        },
        "date": 1752156645834,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.004946151739999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008473425829999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002641511270000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1728078+michalkucharczyk@users.noreply.github.com",
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "62a9808172832e13ca2ae02c1888491ee74b03fb",
          "message": "`fatxpool`: debug levels adjusted (#9159)\n\nThis PR removes redundant debug message and lowers the info about\ntimeout in `ready_at`.\n\nRelated: #9151\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-10T13:42:24Z",
          "tree_id": "cbedb9094437416e71f65e6fc550c42db2cc5e48",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/62a9808172832e13ca2ae02c1888491ee74b03fb"
        },
        "date": 1752159160895,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008731686799999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0052292882599999915,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00266668266,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "psykyodai@gmail.com",
            "name": "psykyo-dai(精神 大)",
            "username": "PsyKyodai"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "874a8dbdd9cbc7fdbfffc4c307f6f21974650a55",
          "message": "Add BlockNumberProvider to PureCreated Event (#9107)\n\n[AHM] [Proxy] Add creation block number to PureCreated event\n\nCloses #9066 \n\n## Problem\nAfter AHM, killing pure proxies requires the relay chain block height at\ncreation time. This information is non-trivial to obtain since the proxy\npallet lives on Asset Hub while the block height refers to Relay Chain.\n\n## Solution\nAdd `at: BlockNumberFor<T>` field to `Event::PureCreated` to include the\ncreation block height. This is populated using the `BlockNumberProvider`\nat creation time.\n\n## Changes\n1. Added `at` field to `Event::PureCreated` containing current block\nnumber\n2. Modified tests and benchmarks to reflect new event structure\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-07-10T15:19:15Z",
          "tree_id": "e16c795118f66c71b0a031259521c3beef122083",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/874a8dbdd9cbc7fdbfffc4c307f6f21974650a55"
        },
        "date": 1752165442081,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008586072849999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026598708800000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005140858249999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "5588131+kianenigma@users.noreply.github.com",
            "name": "Kian Paimani",
            "username": "kianenigma"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d4e4773ea531db55149191693f038e65d64f8107",
          "message": "use correct era planning config in westend-asset-hub (#9152)\n\ntiny mistake of the past, will use the automatic type rather than\nhard-coding it.",
          "timestamp": "2025-07-10T21:44:42Z",
          "tree_id": "325fb85d58fc53b2a8bd2826058c53e9398eb817",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d4e4773ea531db55149191693f038e65d64f8107"
        },
        "date": 1752188048792,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026505049499999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008641344849999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005308614759999992,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "serban@parity.io",
            "name": "Serban Iorga",
            "username": "serban300"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "540941cc654ece30dcd5dfed3cbc93828cd25b81",
          "message": "Improve `pr_8860.prdoc` (#9171)\n\nImproved PR doc for https://github.com/paritytech/polkadot-sdk/pull/8860\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-07-11T10:53:15Z",
          "tree_id": "8b1fbfcc7a1599623446a446914cc1e37a981b75",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/540941cc654ece30dcd5dfed3cbc93828cd25b81"
        },
        "date": 1752235480495,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005092631319999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008540824899999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026675525299999993,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49718502+alexggh@users.noreply.github.com",
            "name": "Alexandru Gheorghe",
            "username": "alexggh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7058819a45ed5b74cedd6d21698f1c2ac2445d6b",
          "message": "add block hashes to the randomness used by hashmaps and friends in validation context (#9127)\n\nhttps://github.com/paritytech/polkadot-sdk/pull/8606\nhttps://github.com/paritytech/trie/pull/221 replaced the usage of\nBTreeMap with HashMaps in validation context. The keys are already\nderived with a cryptographic hash function from user data, so users\nshould not be able to manipulate it.\n\nTo be on safe side this PR also modifies the TrieCache, TrieRecorder and\nMemoryDB to use a hasher that on top of the default generated randomness\nalso adds randomness generated from the hash of the relaychain and that\nof the parachain blocks, which is not something users can control or\nguess ahead of time.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-11T15:34:56Z",
          "tree_id": "6b0e66c2eaa94537bb1ed602b345585455da88be",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7058819a45ed5b74cedd6d21698f1c2ac2445d6b"
        },
        "date": 1752252959245,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00268550198,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008560138529999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005147522609999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "5588131+kianenigma@users.noreply.github.com",
            "name": "Kian Paimani",
            "username": "kianenigma"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "064512ed11042b34ca7330b93e39aa864219d475",
          "message": "pallet-bags-list: Emit `ScoreUpdated` event only if it has changed (#9166)\n\nquick follow-up to https://github.com/paritytech/polkadot-sdk/pull/8684,\nensuring all blocks don't have x events when the feature is enabled (as\nit is now in WAH)\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-12T16:17:44Z",
          "tree_id": "bcdf93b2b053f979c59ad0094670fadf95855c33",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/064512ed11042b34ca7330b93e39aa864219d475"
        },
        "date": 1752341163429,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005091624569999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263388819,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008532146189999992,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jesse.chejieh@gmail.com",
            "name": "Doordashcon",
            "username": "Doordashcon"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9339acc7e4eb58498fe7a4c412dfb9f8e75ae72a",
          "message": "Add Missing Events for Balances Pallet (#7250)\n\nAttempts to resolve #6974\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-13T00:04:30+02:00",
          "tree_id": "c5a5b6fa875bb790a7f98206b6d220ac1a957b32",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9339acc7e4eb58498fe7a4c412dfb9f8e75ae72a"
        },
        "date": 1752359944023,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005217768139999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008592346529999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025697406199999993,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cyrill@parity.io",
            "name": "xermicus",
            "username": "xermicus"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "fb0d310e07438caafcc2dda4d502eba040ecf06c",
          "message": "emit sparse debug info in unoptimized builds (#8646)\n\nSee\n[here](https://kobzol.github.io/rust/rustc/2025/05/20/disable-debuginfo-to-improve-rust-compile-times.html)\nfor more details.\n\nI found that on my host, this reduces `cargo build` (after `cargo\nclean`) from 19m 35s to 17m 50s, or about 10%.\n\nThanks @pgherveou\n\n---------\n\nSigned-off-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-13T22:45:18Z",
          "tree_id": "6fa4ad83ce7581d17e6bfc24fc886cf3fe8b40d7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fb0d310e07438caafcc2dda4d502eba040ecf06c"
        },
        "date": 1752450977544,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005105971109999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026844928300000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008627228169999984,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "109800286+StackOverflowExcept1on@users.noreply.github.com",
            "name": "StackOverflowExcept1on",
            "username": "StackOverflowExcept1on"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e98c88e297f58fa0a28b85bc8eee68fcf5cdaec3",
          "message": "feat(binary-merkle-tree): add `merkle_root_raw` and `merkle_proof_raw` methods (#9105)\n\n# Description\n\nResolves [#9103](https://github.com/paritytech/polkadot-sdk/issues/9103)\n\nAdded `merkle_root_raw` and `merkle_proof_raw` methods, which allow\ndevelopers to avoid double hashing when working with sequences like\n`Vec<H256>`, where `H256` is already hash of some message.\n\n## Integration\n\nThere were no breaking changes.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-14T06:42:30Z",
          "tree_id": "0c3604f400a15e405af3ecb3b31b480883e07235",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e98c88e297f58fa0a28b85bc8eee68fcf5cdaec3"
        },
        "date": 1752480237515,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008463795659999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026059141200000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004986761559999993,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "serban@parity.io",
            "name": "Serban Iorga",
            "username": "serban300"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f8a1fe64c29b1ddcb5824bbb3bf327f528f18d40",
          "message": "Deduplicate client-side inherents checking logic (#9175)\n\nStumbled upon this while working on other issue\n(https://github.com/paritytech/polkadot-sdk/pull/7902). I thought I\nmight need to change the `CheckInherentsResult` and this deduplication\nwould have made everything easier. Probably changing\n`CheckInherentsResult` won't be needed in the end, but even so it would\nbe nice to reduce the duplication.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-14T08:22:53Z",
          "tree_id": "bfca803819835b7f3000ebe25955951078a64f09",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f8a1fe64c29b1ddcb5824bbb3bf327f528f18d40"
        },
        "date": 1752486629323,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008531452779999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005127110599999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026487071699999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "evgeny@parity.io",
            "name": "Evgeny Snitko",
            "username": "AndWeHaveAPlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "8f4d80071a4f478a4540aa8ab63dc1a1b26a8187",
          "message": "Update forklift to 0.14.1 (#9163)\n\ncc https://github.com/paritytech/polkadot-sdk/issues/9123\n\ncc https://github.com/paritytech/devops/issues/4151\n\n---------\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>\nCo-authored-by: alvicsam <alvicsam@gmail.com>",
          "timestamp": "2025-07-14T10:59:34Z",
          "tree_id": "ed66147a2d1d0f7bcd93cfeaa94fba29aacdfe07",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8f4d80071a4f478a4540aa8ab63dc1a1b26a8187"
        },
        "date": 1752495770437,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026515603000000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00872908804999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005118706869999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49699333+dependabot[bot]@users.noreply.github.com",
            "name": "dependabot[bot]",
            "username": "dependabot[bot]"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "641cca3841e7599380d66c14e12ebbe248c739e9",
          "message": "Bump the ci_dependencies group across 1 directory with 5 updates (#9017)\n\nBumps the ci_dependencies group with 5 updates in the / directory:\n\n| Package | From | To |\n| --- | --- | --- |\n| [Swatinem/rust-cache](https://github.com/swatinem/rust-cache) |\n`2.7.8` | `2.8.0` |\n|\n[actions-rust-lang/setup-rust-toolchain](https://github.com/actions-rust-lang/setup-rust-toolchain)\n| `1.12.0` | `1.13.0` |\n|\n[stefanzweifel/git-auto-commit-action](https://github.com/stefanzweifel/git-auto-commit-action)\n| `5` | `6` |\n|\n[docker/setup-buildx-action](https://github.com/docker/setup-buildx-action)\n| `3.10.0` | `3.11.1` |\n|\n[actions/attest-build-provenance](https://github.com/actions/attest-build-provenance)\n| `2.3.0` | `2.4.0` |\n\n\nUpdates `Swatinem/rust-cache` from 2.7.8 to 2.8.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/swatinem/rust-cache/releases\">Swatinem/rust-cache's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v2.8.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Add cache-workspace-crates feature by <a\nhref=\"https://github.com/jbransen\"><code>@​jbransen</code></a> in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/246\">Swatinem/rust-cache#246</a></li>\n<li>Feat: support warpbuild cache provider by <a\nhref=\"https://github.com/stegaBOB\"><code>@​stegaBOB</code></a> in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/247\">Swatinem/rust-cache#247</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a href=\"https://github.com/jbransen\"><code>@​jbransen</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/246\">Swatinem/rust-cache#246</a></li>\n<li><a href=\"https://github.com/stegaBOB\"><code>@​stegaBOB</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/247\">Swatinem/rust-cache#247</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/Swatinem/rust-cache/compare/v2.7.8...v2.8.0\">https://github.com/Swatinem/rust-cache/compare/v2.7.8...v2.8.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/Swatinem/rust-cache/blob/master/CHANGELOG.md\">Swatinem/rust-cache's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>Changelog</h1>\n<h2>2.8.0</h2>\n<ul>\n<li>Add support for <code>warpbuild</code> cache provider</li>\n<li>Add new <code>cache-workspace-crates</code> feature</li>\n</ul>\n<h2>2.7.8</h2>\n<ul>\n<li>Include CPU arch in the cache key</li>\n</ul>\n<h2>2.7.7</h2>\n<ul>\n<li>Also cache <code>cargo install</code> metadata</li>\n</ul>\n<h2>2.7.6</h2>\n<ul>\n<li>Allow opting out of caching $CARGO_HOME/bin</li>\n<li>Add runner OS in cache key</li>\n<li>Adds an option to do lookup-only of the cache</li>\n</ul>\n<h2>2.7.5</h2>\n<ul>\n<li>Support Cargo.lock format cargo-lock v4</li>\n<li>Only run macOsWorkaround() on macOS</li>\n</ul>\n<h2>2.7.3</h2>\n<ul>\n<li>Work around upstream problem that causes cache saving to hang for\nminutes.</li>\n</ul>\n<h2>2.7.2</h2>\n<ul>\n<li>Only key by <code>Cargo.toml</code> and <code>Cargo.lock</code>\nfiles of workspace members.</li>\n</ul>\n<h2>2.7.1</h2>\n<ul>\n<li>Update toml parser to fix parsing errors.</li>\n</ul>\n<h2>2.7.0</h2>\n<ul>\n<li>Properly cache <code>trybuild</code> tests.</li>\n</ul>\n<h2>2.6.2</h2>\n<ul>\n<li>Fix <code>toml</code> parsing.</li>\n</ul>\n<h2>2.6.1</h2>\n<ul>\n<li>Fix hash contributions of\n<code>Cargo.lock</code>/<code>Cargo.toml</code> files.</li>\n</ul>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/98c8021b550208e191a6a3145459bfc9fb29c4c0\"><code>98c8021</code></a>\n2.8.0</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/14d3bc39c43eec8ca2cd08dd0805a32ee0cb3666\"><code>14d3bc3</code></a>\nupdate Changelog</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/52ea1434f87f7081841d430fb7b1235754488e51\"><code>52ea143</code></a>\nsupport warpbuild cache provider (<a\nhref=\"https://redirect.github.com/swatinem/rust-cache/issues/247\">#247</a>)</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/eaa85be6b1bfdc6616fd14d8916fc5aa0435e435\"><code>eaa85be</code></a>\nAdd cache-workspace-crates feature (<a\nhref=\"https://redirect.github.com/swatinem/rust-cache/issues/246\">#246</a>)</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/901019c0f83889e6f8eaa395f97093151c05c4b0\"><code>901019c</code></a>\nUpdate the test lockfiles</li>\n<li>See full diff in <a\nhref=\"https://github.com/swatinem/rust-cache/compare/9d47c6ad4b02e050fd481d890b2ea34778fd09d6...98c8021b550208e191a6a3145459bfc9fb29c4c0\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `actions-rust-lang/setup-rust-toolchain` from 1.12.0 to 1.13.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/releases\">actions-rust-lang/setup-rust-toolchain's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v1.13.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>feat: support cache-provider by <a\nhref=\"https://github.com/mindrunner\"><code>@​mindrunner</code></a> in <a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/pull/65\">actions-rust-lang/setup-rust-toolchain#65</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a\nhref=\"https://github.com/mindrunner\"><code>@​mindrunner</code></a> made\ntheir first contribution in <a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/pull/65\">actions-rust-lang/setup-rust-toolchain#65</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/compare/v1.12.0...v1.13.0\">https://github.com/actions-rust-lang/setup-rust-toolchain/compare/v1.12.0...v1.13.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/blob/main/CHANGELOG.md\">actions-rust-lang/setup-rust-toolchain's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>Changelog</h1>\n<p>All notable changes to this project will be documented in this\nfile.</p>\n<p>The format is based on <a\nhref=\"https://keepachangelog.com/en/1.0.0/\">Keep a Changelog</a>,\nand this project adheres to <a\nhref=\"https://semver.org/spec/v2.0.0.html\">Semantic Versioning</a>.</p>\n<h2>[Unreleased]</h2>\n<h2>[1.13.0] - 2025-06-16</h2>\n<ul>\n<li>Add new parameter <code>cache-provider</code> that is propagated to\n<code>Swatinem/rust-cache</code> as <code>cache-provider</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/65\">#65</a>\nby <a\nhref=\"https://github.com/mindrunner\"><code>@​mindrunner</code></a>)</li>\n</ul>\n<h2>[1.12.0] - 2025-04-23</h2>\n<ul>\n<li>Add support for installing rustup on Windows (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/58\">#58</a>\nby <a href=\"https://github.com/maennchen\"><code>@​maennchen</code></a>)\nThis adds support for using Rust on the GitHub provided Windows ARM\nrunners.</li>\n</ul>\n<h2>[1.11.0] - 2025-02-24</h2>\n<ul>\n<li>Add new parameter <code>cache-bin</code> that is propagated to\n<code>Swatinem/rust-cache</code> as <code>cache-bin</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/51\">#51</a>\nby <a\nhref=\"https://github.com/enkhjile\"><code>@​enkhjile</code></a>)</li>\n<li>Add new parameter <code>cache-shared-key</code> that is propagated\nto <code>Swatinem/rust-cache</code> as <code>shared-key</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/52\">#52</a>\nby <a\nhref=\"https://github.com/skanehira\"><code>@​skanehira</code></a>)</li>\n</ul>\n<h2>[1.10.1] - 2024-10-01</h2>\n<ul>\n<li>Fix problem matcher for rustfmt output.\nThe format has changed since <a\nhref=\"https://redirect.github.com/rust-lang/rustfmt/pull/5971\">rust-lang/rustfmt#5971</a>\nand now follows the form &quot;filename:line&quot;.\nThanks to <a\nhref=\"https://github.com/0xcypher02\"><code>@​0xcypher02</code></a> for\npointing out the problem.</li>\n</ul>\n<h2>[1.10.0] - 2024-09-23</h2>\n<ul>\n<li>Add new parameter <code>cache-directories</code> that is propagated\nto <code>Swatinem/rust-cache</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/44\">#44</a>\nby <a\nhref=\"https://github.com/pranc1ngpegasus\"><code>@​pranc1ngpegasus</code></a>)</li>\n<li>Add new parameter <code>cache-key</code> that is propagated to\n<code>Swatinem/rust-cache</code> as <code>key</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/41\">#41</a>\nby <a\nhref=\"https://github.com/iainlane\"><code>@​iainlane</code></a>)</li>\n<li>Make rustup toolchain installation more robust in light of planned\nchanges <a\nhref=\"https://redirect.github.com/rust-lang/rustup/issues/3635\">rust-lang/rustup#3635</a>\nand <a\nhref=\"https://redirect.github.com/rust-lang/rustup/pull/3985\">rust-lang/rustup#3985</a></li>\n<li>Allow installing multiple Rust toolchains by specifying multiple\nversions in the <code>toolchain</code> input parameter.</li>\n<li>Configure the <code>rustup override</code> behavior via the new\n<code>override</code> input. (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/38\">#38</a>)</li>\n</ul>\n<h2>[1.9.0] - 2024-06-08</h2>\n<ul>\n<li>Add extra argument <code>cache-on-failure</code> and forward it to\n<code>Swatinem/rust-cache</code>. (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/39\">#39</a>\nby <a\nhref=\"https://github.com/samuelhnrq\"><code>@​samuelhnrq</code></a>)<br\n/>\nSet the default the value to true.\nThis will result in more caching than previously.\nThis helps when large dependencies are compiled only for testing to\nfail.</li>\n</ul>\n<h2>[1.8.0] - 2024-01-13</h2>\n<ul>\n<li>Allow specifying subdirectories for cache.</li>\n<li>Fix toolchain file overriding.</li>\n</ul>\n<h2>[1.7.0] - 2024-01-11</h2>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/commit/fb51252c7ba57d633bc668f941da052e410add48\"><code>fb51252</code></a>\nUpdate CHANGELOG.md</li>\n<li><a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/commit/33b85c358d935f8a72fcfe469bdb7d9f78182141\"><code>33b85c3</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/65\">#65</a>\nfrom mindrunner/main</li>\n<li><a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/commit/82947d77a9ec18480f3f187b0102cac015771477\"><code>82947d7</code></a>\nfeat: support cache-provider</li>\n<li>See full diff in <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/compare/9d7e65c320fdb52dcd45ffaa68deb6c02c8754d9...fb51252c7ba57d633bc668f941da052e410add48\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `stefanzweifel/git-auto-commit-action` from 5 to 6\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/releases\">stefanzweifel/git-auto-commit-action's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v6.0.0</h2>\n<h2>Added</h2>\n<ul>\n<li>Throw error early if repository is in a detached state (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/357\">#357</a>)</li>\n</ul>\n<h2>Fixed</h2>\n<ul>\n<li>Fix PAT instructions with Dependabot (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/376\">#376</a>)\n<a\nhref=\"https://github.com/@Dreamsorcerer\"><code>@​Dreamsorcerer</code></a></li>\n</ul>\n<h2>Removed</h2>\n<ul>\n<li>Remove support for <code>create_branch</code>,\n<code>skip_checkout</code>, <code>skip_Fetch</code> (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/314\">#314</a>)</li>\n</ul>\n<h2>v5.2.0</h2>\n<h2>Added</h2>\n<ul>\n<li>Add <code>create_git_tag_only</code> option to skip commiting and\nalways create a git-tag. (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/364\">#364</a>)\n<a href=\"https://github.com/@zMynxx\"><code>@​zMynxx</code></a></li>\n<li>Add Test for <code>create_git_tag_only</code> feature (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/367\">#367</a>)\n<a\nhref=\"https://github.com/@stefanzweifel\"><code>@​stefanzweifel</code></a></li>\n</ul>\n<h2>Fixed</h2>\n<ul>\n<li>docs: Update README.md per <a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/issues/354\">#354</a>\n(<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/361\">#361</a>)\n<a href=\"https://github.com/@rasa\"><code>@​rasa</code></a></li>\n</ul>\n<h2>v5.1.0</h2>\n<h2>Changed</h2>\n<ul>\n<li>Include <code>github.actor_id</code> in default\n<code>commit_author</code> (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/354\">#354</a>)\n<a\nhref=\"https://github.com/@parkerbxyz\"><code>@​parkerbxyz</code></a></li>\n</ul>\n<h2>Fixed</h2>\n<ul>\n<li>docs(README): fix broken protected branch docs link (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/346\">#346</a>)\n<a href=\"https://github.com/@scarf005\"><code>@​scarf005</code></a></li>\n<li>Update README.md (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/343\">#343</a>)\n<a href=\"https://github.com/@Kludex\"><code>@​Kludex</code></a></li>\n</ul>\n<h2>Dependency Updates</h2>\n<ul>\n<li>Bump bats from 1.11.0 to 1.11.1 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/353\">#353</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump github/super-linter from 6 to 7 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/342\">#342</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump github/super-linter from 5 to 6 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/335\">#335</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n</ul>\n<h2>v5.0.1</h2>\n<h2>Fixed</h2>\n<ul>\n<li>Fail if attempting to execute git commands in a directory that is\nnot a git-repo. (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/326\">#326</a>)\n<a\nhref=\"https://github.com/@ccomendant\"><code>@​ccomendant</code></a></li>\n</ul>\n<h2>Dependency Updates</h2>\n<ul>\n<li>Bump bats from 1.10.0 to 1.11.0 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/325\">#325</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump release-drafter/release-drafter from 5 to 6 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/319\">#319</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n</ul>\n<h2>Misc</h2>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/blob/master/CHANGELOG.md\">stefanzweifel/git-auto-commit-action's\nchangelog</a>.</em></p>\n<blockquote>\n<h2><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/compare/v4.16.0...v5.0.0\">v5.0.0</a>\n- 2023-10-06</h2>\n<p>New major release that bumps the default runtime to Node 20. There\nare no other breaking changes.</p>\n<h3>Changed</h3>\n<ul>\n<li>Update node version to node20 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/300\">#300</a>)\n<a\nhref=\"https://github.com/@ryudaitakai\"><code>@​ryudaitakai</code></a></li>\n<li>Add _log and _set_github_output functions (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/273\">#273</a>)\n<a\nhref=\"https://github.com/@stefanzweifel\"><code>@​stefanzweifel</code></a></li>\n</ul>\n<h3>Fixed</h3>\n<ul>\n<li>Seems like there is an extra space (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/288\">#288</a>)\n<a\nhref=\"https://github.com/@pedroamador\"><code>@​pedroamador</code></a></li>\n<li>Fix git-auto-commit.yml (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/277\">#277</a>)\n<a\nhref=\"https://github.com/@zcong1993\"><code>@​zcong1993</code></a></li>\n</ul>\n<h3>Dependency Updates</h3>\n<ul>\n<li>Bump actions/checkout from 3 to 4 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/302\">#302</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump bats from 1.9.0 to 1.10.0 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/293\">#293</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump github/super-linter from 4 to 5 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/289\">#289</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump bats from 1.8.2 to 1.9.0 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/282\">#282</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n</ul>\n<h2><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/compare/v4.15.4...v4.16.0\">v4.16.0</a>\n- 2022-12-02</h2>\n<h3>Changed</h3>\n<ul>\n<li>Don't commit files when only LF/CRLF changes (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/265\">#265</a>)\n<a href=\"https://github.com/@ZeroRin\"><code>@​ZeroRin</code></a></li>\n<li>Update default email address of github-actions[bot] (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/264\">#264</a>)\n<a href=\"https://github.com/@Teko012\"><code>@​Teko012</code></a></li>\n</ul>\n<h3>Fixed</h3>\n<ul>\n<li>Fix link and text for workflow limitation (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/263\">#263</a>)\n<a href=\"https://github.com/@Teko012\"><code>@​Teko012</code></a></li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/778341af668090896ca464160c2def5d1d1a3eb0\"><code>778341a</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/issues/379\">#379</a>\nfrom stefanzweifel/disable-detached-state-check</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/33b203d92a47ab2370a88ce03d9825cdb52cc98c\"><code>33b203d</code></a>\nDisable Check if Repo is in Detached State</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/a82d80a75f85e7feb8d2777704c545af1c7affd9\"><code>a82d80a</code></a>\nUpdate CHANGELOG</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/3cc016cfc892e0844046da36fc68da4e525e081f\"><code>3cc016c</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/issues/375\">#375</a>\nfrom stefanzweifel/v6-next</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/ddb7ae415961225797e0234a7018a30ba1e66bb3\"><code>ddb7ae4</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/issues/376\">#376</a>\nfrom Dreamsorcerer/patch-1</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/b001e5f0ff05d7297c0101f4b44e861799e417dd\"><code>b001e5f</code></a>\nApply suggestions from code review</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/6494dc61d3e663a9f5166a099d9736ceefc5a3aa\"><code>6494dc6</code></a>\nFix PAT instructions with Dependabot</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/76180511d9f2354bb712ec6338ce79d4f2061bfe\"><code>7618051</code></a>\nAdd deprecated inputs to fix unbound variable issue</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/ae114628ea78fd141aa4fa7730f70c984b29c391\"><code>ae11462</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/issues/371\">#371</a>\nfrom stefanzweifel/dependabot/npm_and_yarn/bats-1.12.0</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/3058f91afb4f03b73d38f33c35023fb22cf546b8\"><code>3058f91</code></a>\nBump bats from 1.11.1 to 1.12.0</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/compare/v5...v6\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `docker/setup-buildx-action` from 3.10.0 to 3.11.1\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/docker/setup-buildx-action/releases\">docker/setup-buildx-action's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v3.11.1</h2>\n<ul>\n<li>Fix <code>keep-state</code> not being respected by <a\nhref=\"https://github.com/crazy-max\"><code>@​crazy-max</code></a> in <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/429\">docker/setup-buildx-action#429</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/docker/setup-buildx-action/compare/v3.11.0...v3.11.1\">https://github.com/docker/setup-buildx-action/compare/v3.11.0...v3.11.1</a></p>\n<h2>v3.11.0</h2>\n<ul>\n<li>Keep BuildKit state support by <a\nhref=\"https://github.com/crazy-max\"><code>@​crazy-max</code></a> in <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/427\">docker/setup-buildx-action#427</a></li>\n<li>Remove aliases created when installing by default by <a\nhref=\"https://github.com/hashhar\"><code>@​hashhar</code></a> in <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/139\">docker/setup-buildx-action#139</a></li>\n<li>Bump <code>@​docker/actions-toolkit</code> from 0.56.0 to 0.62.1 in\n<a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/422\">docker/setup-buildx-action#422</a>\n<a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/425\">docker/setup-buildx-action#425</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/docker/setup-buildx-action/compare/v3.10.0...v3.11.0\">https://github.com/docker/setup-buildx-action/compare/v3.10.0...v3.11.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/e468171a9de216ec08956ac3ada2f0791b6bd435\"><code>e468171</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/issues/429\">#429</a>\nfrom crazy-max/fix-keep-state</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/a3e7502fd02828f4a26a8294ad2621a6c2204952\"><code>a3e7502</code></a>\nchore: update generated content</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/b145473295476dbef957d01d109fe7810b511c95\"><code>b145473</code></a>\nfix keep-state not being respected</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/18ce135bb5112fa8ce4ed6c17ab05699d7f3a5e0\"><code>18ce135</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/issues/425\">#425</a>\nfrom docker/dependabot/npm_and_yarn/docker/actions-to...</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/0e198e93af3b40a76583e851660b876e62b3a155\"><code>0e198e9</code></a>\nchore: update generated content</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/05f3f3ac108784e8fb56815c12fbfcf2d0ed660f\"><code>05f3f3a</code></a>\nbuild(deps): bump <code>@​docker/actions-toolkit</code> from 0.61.0 to\n0.62.1</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/622913496df23a5293cfb3418e5836ee4dd28f3a\"><code>6229134</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/issues/427\">#427</a>\nfrom crazy-max/keep-state</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/c6f6a0702519e6c47b71b117b24c0c1c130fdf32\"><code>c6f6a07</code></a>\nchore: update generated content</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/6c5e29d8485c56f3f8d1cb2197b657959dd6e032\"><code>6c5e29d</code></a>\nskip builder creation if one already exists with the same name</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/548b2977492e10f459d0f0df8bee7ce3c5937792\"><code>548b297</code></a>\nci: keep-state check</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/docker/setup-buildx-action/compare/b5ca514318bd6ebac0fb2aedd5d36ec1b5c232a2...e468171a9de216ec08956ac3ada2f0791b6bd435\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `actions/attest-build-provenance` from 2.3.0 to 2.4.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions/attest-build-provenance/releases\">actions/attest-build-provenance's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v2.4.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Bump undici from 5.28.5 to 5.29.0 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/633\">actions/attest-build-provenance#633</a></li>\n<li>Bump actions/attest from 2.3.0 to <a\nhref=\"https://github.com/actions/attest/releases/tag/v2.4.0\">2.4.0</a>\nby <a href=\"https://github.com/bdehamer\"><code>@​bdehamer</code></a> in\n<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/654\">actions/attest-build-provenance#654</a>\n<ul>\n<li>Includes support for the new well-known summary file which will\naccumulate paths to all attestations generated in a given workflow\nrun</li>\n</ul>\n</li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions/attest-build-provenance/compare/v2.3.0...v2.4.0\">https://github.com/actions/attest-build-provenance/compare/v2.3.0...v2.4.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/e8998f949152b193b063cb0ec769d69d929409be\"><code>e8998f9</code></a>\nbump actions/attest from 2.3.0 to 2.4.0 (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/654\">#654</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/11c67f22cd5a3968528de1f8de4bb4487ee5306e\"><code>11c67f2</code></a>\nBump the npm-development group across 1 directory with 6 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/649\">#649</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/39cb715ce0ddd23df1f705e863f642bfb72dfb2b\"><code>39cb715</code></a>\nBump the npm-development group across 1 directory with 7 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/641\">#641</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/7d91c4030d8fdc376f87f022d8ca01fe8bf07fcd\"><code>7d91c40</code></a>\nBump undici from 5.28.5 to 5.29.0 (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/633\">#633</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/d848170917c12653fb344e617a79614f36d13e00\"><code>d848170</code></a>\nBump super-linter/super-linter in the actions-minor group (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/640\">#640</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/0ca36ea29fc5b46379679e3d2a9ce33a62c57e04\"><code>0ca36ea</code></a>\nBump the npm-development group with 7 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/582\">#582</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/d82e7cd0c70d3e7b2615badc4d8824ca0b098d86\"><code>d82e7cd</code></a>\noffboard from eslint in superlinter (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/618\">#618</a>)</li>\n<li>See full diff in <a\nhref=\"https://github.com/actions/attest-build-provenance/compare/db473fddc028af60658334401dc6fa3ffd8669fd...e8998f949152b193b063cb0ec769d69d929409be\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore <dependency name> major version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's major version (unless you unignore this specific\ndependency's major version or upgrade to it yourself)\n- `@dependabot ignore <dependency name> minor version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's minor version (unless you unignore this specific\ndependency's minor version or upgrade to it yourself)\n- `@dependabot ignore <dependency name>` will close this group update PR\nand stop Dependabot creating any more for the specific dependency\n(unless you unignore this specific dependency or upgrade to it yourself)\n- `@dependabot unignore <dependency name>` will remove all of the ignore\nconditions of the specified dependency\n- `@dependabot unignore <dependency name> <ignore condition>` will\nremove the ignore condition of the specified dependency and ignore\nconditions\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-07-14T14:37:10Z",
          "tree_id": "b1fc202864e0d0bdd4231a00f446f544f2eb6992",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/641cca3841e7599380d66c14e12ebbe248c739e9"
        },
        "date": 1752508312883,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00260669658,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008522426739999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005078379879999991,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "franciscoaguirreperez@gmail.com",
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "efa765b6d9fbab59dd9bab944f99b40a157d0d64",
          "message": "Pallet XCM - `transfer_assets` pre-ahm patch (#9137)\n\nAddresses https://github.com/paritytech/polkadot-sdk/issues/9054\n\n`transfer_assets` automatically figures out the reserve for a\ncross-chain transfer\nbased on on-chain configurations like `IsReserve` and the asset ids.\nThe Asset Hub Migration (AHM) will make it unable to return the correct\nreserve for\nthe network native asset (DOT, KSM, WND, PAS) since its reserve will\nmove from the\nRelay Chain to the Asset Hub.\n\nBefore the migration, it'll be disabled to do network native reserve\ntransfers\nvia `transfer_assets`. After the migration, once everything is\nconfigured properly,\nit'll be patched to use the correct reserve.\n\n## TODO\n\n- [x] Patch\n- [x] Tests\n- [x] PRDoc",
          "timestamp": "2025-07-14T19:28:37Z",
          "tree_id": "13bb2dff7ac3a2f86d6b38b9817c02e34410e467",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/efa765b6d9fbab59dd9bab944f99b40a157d0d64"
        },
        "date": 1752525554130,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008708443639999987,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00265841113,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005321601519999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ismailov.m.h@gmail.com",
            "name": "muharem",
            "username": "muharem"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ec6e6843b847de92be649073317fa729898d0e1b",
          "message": "Asset Hub Westend: nfts block provider is RC (#9141)\n\nAsset Hub Westend: nfts block provider is Relay Chain.\n\nnfts pallet uses the blocks to define `mint.start_block` and\n`mint.end_block` for collections. therefor the RC is a better choice\nhere since its more time accurate.\n\nthis does not requires a migration since there is no single collection\nwith the start and end block set.\n\nit would be nice to deploy this change asap to let clients test this\nbefore it hit production on Kusama and Polkadot.",
          "timestamp": "2025-07-14T20:25:23Z",
          "tree_id": "06fb447dd706c0508dbf8c601711dc3d56a98d56",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ec6e6843b847de92be649073317fa729898d0e1b"
        },
        "date": 1752528743675,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005144160799999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264743916,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008559953459999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "32168055+antkve@users.noreply.github.com",
            "name": "Anthony Kveder",
            "username": "antkve"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "999b4fa90ea0073a193662b1162b5f1f25f3beb6",
          "message": "Fixed westend asset hub ID (#9191)\n\nAddresses #9190 by adding cumulus_primitives_core::GetParachainInfo impl\nto the AHW runtime.\n\n---------\n\nCo-authored-by: Karol Kokoszka <karol@parity.io>",
          "timestamp": "2025-07-15T10:47:05Z",
          "tree_id": "db38b696266f561a16f37e601e33a516a9086616",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/999b4fa90ea0073a193662b1162b5f1f25f3beb6"
        },
        "date": 1752580513301,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005073641599999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008560284299999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026340730199999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "607a1b24b7902a657426ce2412e316a57b61894b",
          "message": "`apply_authorized_force_set_current_code` does not need to consume the whole block (#9202)\n\nThere is no need that this dispatchable consumes the full block as this\nis just writing the given value to storage. On a chain this is done,\nbecause the runtime changes and thus, a lot of stuff potentially\nchanges. In the case of the relay chain only on parachain changes and\nnot the relay chain runtime itself.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-07-15T11:47:13Z",
          "tree_id": "275460b2842ffe07aa0ea2d00e95f080163d9b74",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/607a1b24b7902a657426ce2412e316a57b61894b"
        },
        "date": 1752584733291,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026354900100000007,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008632280229999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005237780869999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "14218860+iulianbarbu@users.noreply.github.com",
            "name": "Iulian Barbu",
            "username": "iulianbarbu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "495d5a24c8078a0da1eb5e0fe8742a09f1f1bd5c",
          "message": "fix(minimal): pre-seal a first block to trigger maintain (#9207)\n\n# Description\n\nAfter making fork aware txpool the default, instant seal of minimal node\nstopped working as expected because any transaction sent to it got stuck\nin mempool , since there are no active views to include the tx in.\n\nTo overcome this we can create a first view by pre-sealing a first empty\nblock, which triggers the `maintain` phase and view building logic. This\nis compatible with single-state tx pool too.\n\n## Integration\n\nN/A\n\n## Review Notes\n\nN/A\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-16T06:51:33Z",
          "tree_id": "7fac401304e67f49b7999d7b132e8d00bd241316",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/495d5a24c8078a0da1eb5e0fe8742a09f1f1bd5c"
        },
        "date": 1752652753636,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00260627661,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008597162699999978,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005186324109999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bkontur@gmail.com",
            "name": "Branislav Kontur",
            "username": "bkontur"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "40e1a2a7c99c67fe5201145e473c87e1aea4bf05",
          "message": "Allow create backport branches to unstable by A4-backport-unstable* tag (#9167)\n\nIn this\n[PR](https://github.com/paritytech/polkadot-sdk/pull/9139#issuecomment-3052828167),\nI added the `A4-backport-unstable2507` label, but no backport branch was\ncreated for `unstable2507`.\n\nWas this intentional or just an oversight or did I miss anything in the\nrelease channel?\nHow do we do backports to unstable2507? If manually, just close this.\n\ncc: @EgorPopelyaev - this PR is just a blind draft (not sure if it\nworks), probably more needs to be fixed and properly tested. If we\nreally need this, could you please take it over the finish line? If not,\njust close it :)\n\nCo-authored-by: Egor_P <egor@parity.io>",
          "timestamp": "2025-07-16T16:23:51Z",
          "tree_id": "42f9ddd2a1ed680cc694879a7f84761b03ea1e9c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/40e1a2a7c99c67fe5201145e473c87e1aea4bf05"
        },
        "date": 1752687144561,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008535617119999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00259539007,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005169772949999998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22591718+RomarQ@users.noreply.github.com",
            "name": "Rodrigo Quelhas",
            "username": "RomarQ"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6ecd83761b4fcac8c2c02ee05e8ec7213bacbc30",
          "message": "feat(pallet-xcm): Add supported_version to pallet-xcm genesis config (#9225)\n\nRelates to: https://github.com/polkadot-fellows/runtimes/issues/544\nCloses https://github.com/paritytech/polkadot-sdk/issues/9075\n\nAdds a `supported_version` field to the pallet-xcm genesis config. Which\nallows specifying versioned locations at genesis.",
          "timestamp": "2025-07-17T08:34:46Z",
          "tree_id": "4c37cf3bac9a7da5b7949782b000a08141fc207b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6ecd83761b4fcac8c2c02ee05e8ec7213bacbc30"
        },
        "date": 1752745545781,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027113571899999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008648413839999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005089740139999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "egor@parity.io",
            "name": "Egor_P",
            "username": "EgorPopelyaev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "86d2a410ca95643e589f270808a6fa57de41369f",
          "message": "Don't use labels for branch names creation in the backport bot (#9243)",
          "timestamp": "2025-07-17T10:25:22Z",
          "tree_id": "438b192e6b4d0c685f1c5920b5081911a4e5283b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/86d2a410ca95643e589f270808a6fa57de41369f"
        },
        "date": 1752753205743,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008647039529999987,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002618373079999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005179710219999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "diego2737@gmail.com",
            "name": "Diego",
            "username": "dimartiro"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6b17df5ae96f7970109ec3934c7d288f05baa23b",
          "message": "Remove unused deps (#9235)\n\n# Description\n\nRemove unused deps using `cargo udeps`\n\nPart of: #6906\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2025-07-17T14:18:53Z",
          "tree_id": "20c94cc5015d6ff1c010a46fd69c90c70442033b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6b17df5ae96f7970109ec3934c7d288f05baa23b"
        },
        "date": 1752767045610,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008528037719999992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005030579369999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264220842,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "32168055+antkve@users.noreply.github.com",
            "name": "Anthony Kveder",
            "username": "antkve"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0ae5c5bbd96a600aed81358339be2f16bade4a81",
          "message": "Fixed genesis config presets for bridge tests (#9185)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/9116\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Karol Kokoszka <karol@parity.io>",
          "timestamp": "2025-07-17T16:04:44Z",
          "tree_id": "19d750bba6685e132f90c471118eb1342e943c9f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0ae5c5bbd96a600aed81358339be2f16bade4a81"
        },
        "date": 1752775967882,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026003321699999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008560757039999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005194371439999989,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "b9fd81b1d511c1b82d44129ae6e3592620508d85",
          "message": "Remove `subwasmlib` (#9252)\n\nThis removes `subwasmlib` and replaces it with some custom code to fetch\nthe metadata. Main point of this change is the removal of some external\ndependency.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/9203\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-18T11:26:43Z",
          "tree_id": "8158e2688a0180b1512385a845c592f5319d7f2d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b9fd81b1d511c1b82d44129ae6e3592620508d85"
        },
        "date": 1752842282023,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008526041989999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005162670399999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263012882,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "363911+pepoviola@users.noreply.github.com",
            "name": "Javier Viola",
            "username": "pepoviola"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "8b21416986049b26bf99e61bf1c43b7347ed564f",
          "message": "zombienet, make logs for para works (#9230)\n\nFix for correctly display the logs (urls) for paras.",
          "timestamp": "2025-07-18T15:25:49Z",
          "tree_id": "2bf2a30b19851859a8c1db0ac7e145031cf773ed",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8b21416986049b26bf99e61bf1c43b7347ed564f"
        },
        "date": 1752856888443,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005145037809999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008526873949999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026067719700000004,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22591718+RomarQ@users.noreply.github.com",
            "name": "Rodrigo Quelhas",
            "username": "RomarQ"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e007db09171dd5248f5d8663a56be679b92fdbe7",
          "message": "feat(cumulus): Adds support for additional relay state keys in parachain validation data inherent (#9262)\n\nAdds the possibility for parachain clients to collect additional relay\nstate keys into the validation data inherent.\n\nWith this change, other consensus engines can collect additional relay\nkeys into the parachain inherent data:\n```rs\nlet paras_inherent_data = ParachainInherentDataProvider::create_at(\n  relay_parent,\n  relay_client,\n  validation_data,\n  para_id,\n  vec![\n     relay_well_known_keys::EPOCH_INDEX.to_vec() // <----- Example\n  ],\n)\n.await;\n```",
          "timestamp": "2025-07-18T21:26:30Z",
          "tree_id": "12ecd4a047e3074ed0ff7953b85e24443d9a7332",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e007db09171dd5248f5d8663a56be679b92fdbe7"
        },
        "date": 1752878226694,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00261525499,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00841458589999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004945185839999989,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "5588131+kianenigma@users.noreply.github.com",
            "name": "Kian Paimani",
            "username": "kianenigma"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0d2106685025a0dad542b0980aab763ce0352755",
          "message": "Allow locking to bump consumer without limits (#9176)\n\nLocking is a system-level operation, and can only increment the consumer\nlimit at most once. Therefore, it should use\n`inc_consumer_without_limits`. This behavior is optional, and is only\nused in the call path of `LockableCurrency`. Reserves, Holds and Freezes\n(and other operations like transfer etc.) have the ability to return\n`DispatchResult` and don't need this bypass. This is demonstrated in the\nunit tests added.\n\nBeyond this, this PR: \n\n* uses the correct way to get the account data in tests\n* adds an `Unexpected` event instead of a silent `debug_assert!`. \n* Adds `try_state` checks for correctness of `account.frozen` invariant.\n\n---------\n\nCo-authored-by: Ankan <10196091+Ank4n@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-20T08:51:04Z",
          "tree_id": "f4835fef77bc77f12a7b25e5789a72edb66a8110",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d2106685025a0dad542b0980aab763ce0352755"
        },
        "date": 1753005685826,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005183158079999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00856138804999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026465043899999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "enntheprogrammer@gmail.com",
            "name": "sistemd",
            "username": "sistemd"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b17f06bf06dbee585bbd8dc6d070c5edf56916e1",
          "message": "babe: keep stateless verification in `Verifier`, move everything else to the import queue (#9147)\n\nWe agreed to split https://github.com/paritytech/polkadot-sdk/pull/8446\ninto two PRs: one for BABE (this one) and one for AURA. This is the\neasier one.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-20T16:43:16Z",
          "tree_id": "c968ceb147b12e27e9ff5063f8c4303d14b3aeb9",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b17f06bf06dbee585bbd8dc6d070c5edf56916e1"
        },
        "date": 1753034282957,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008536643269999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026549024399999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051494235899999935,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "40807189+AlexandruCihodaru@users.noreply.github.com",
            "name": "Alexandru Cihodaru",
            "username": "AlexandruCihodaru"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "443c2ffa03ee20e8244fa4b52ec3c62750d55ca6",
          "message": "Rewrite validator disabling test with zombienet-sdk (#9128)\n\nFixes #9085\n\n---------\n\nSigned-off-by: Alexandru Cihodaru <alexandru.cihodaru@parity.io>",
          "timestamp": "2025-07-21T09:08:32Z",
          "tree_id": "60764da01881d68bd585187a0cf3d60596bfbc12",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/443c2ffa03ee20e8244fa4b52ec3c62750d55ca6"
        },
        "date": 1753093279804,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026315498100000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051757640699999965,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008546156399999985,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49718502+alexggh@users.noreply.github.com",
            "name": "Alexandru Gheorghe",
            "username": "alexggh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c7f9908c2eeb1be70e57819537058beb53664446",
          "message": "gossip-support: make low connectivity message an error (#9264)\n\nAll is not well when a validator is not properly connected, e.g: of\nthings that might happen:\n- Finality might be slightly delay because validator will be no-show\nbecause they can't retrieve PoVs to validate approval work:\nhttps://github.com/paritytech/polkadot-sdk/issues/8915.\n- When they author blocks they won't back things because gossiping of\nbacking statements happen using the grid topology:, e.g blocks authored\nby validators with a low number of peers:\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Frpc-polkadot.helixstreet.io#/explorer/query/26931262\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Frpc-polkadot.helixstreet.io#/explorer/query/26931260\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.api.onfinality.io%2Fpublic-ws#/explorer/query/26931334\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot-public-rpc.blockops.network%2Fws#/explorer/query/26931314\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot-public-rpc.blockops.network%2Fws#/explorer/query/26931292\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot-public-rpc.blockops.network%2Fws#/explorer/query/26931447\n\n\nThe problem is seen in `polkadot_parachain_peer_count` metrics, but it\nseems people are not monitoring that well enough, so let's make it more\nvisible nodes with low connectivity are not working in good conditions.\n\nI also reduced the threshold to 85%, so that we don't trigger this error\nto eagerly.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-21T09:57:39Z",
          "tree_id": "472c2b031140ce823b7947201ff39347eaf6dbee",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c7f9908c2eeb1be70e57819537058beb53664446"
        },
        "date": 1753096166257,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008698310099999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005238696959999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026194913499999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "40807189+AlexandruCihodaru@users.noreply.github.com",
            "name": "Alexandru Cihodaru",
            "username": "AlexandruCihodaru"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "161e7f4d8b9b6908694d0ccc9bd0ef4a1674e860",
          "message": "Rewrite old disputes test with zombienet-sdk (#9257)\n\nFixes: #9256\n\n---------\n\nSigned-off-by: Alexandru Cihodaru <alexandru.cihodaru@parity.io>",
          "timestamp": "2025-07-21T13:46:01Z",
          "tree_id": "24c826bb9c1c557cd72bf46b1b000eea42cc3c0f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/161e7f4d8b9b6908694d0ccc9bd0ef4a1674e860"
        },
        "date": 1753109706142,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00853376491999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026080278799999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005051998489999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "117115317+lrubasze@users.noreply.github.com",
            "name": "Lukasz Rubaszewski",
            "username": "lrubasze"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b4b019e4db0ef47b0952638388eba4958e1c4004",
          "message": "Zombienet CI improvements (#9172)\n\n## 🔄 Zombienet CI Refactor: Matrix-Based Workflows\n\nThis PR refactors the Zombienet CI workflows to use a **matrix-based\napproach**, resulting in:\n\n- ✅ **Easier test maintenance** – easily add or remove tests without\nduplicating workflow logic.\n- 🩹 **Improved flaky test handling** – flaky tests are excluded by\ndefault but can be explicitly included by pattern.\n- 🔍 **Pattern-based test selection** – run only tests matching a name\npattern, ideal for debugging.\n\n---\n\n## 🗂️ Structure Changes\n\n- **Test definitions** are now stored in `.github/zombienet-tests/`.\n- Each workflow (`Cumulus`, `Substrate`, `Polkadot`, `Parachain\nTemplate`) has its own YAML file with test configurations.\n\n---\n\n## 🧰 Added Scripts\n\n### `.github/scripts/parse-zombienet-tests.py`\n- Parses test definitions and generates a GitHub Actions matrix.\n- Filters out flaky tests by default.\n- If a `test_pattern` is provided, matching tests are **included even if\nflaky**.\n\n### `.github/scripts/dispatch-zombienet-workflow.sh`\n- Triggers a Zombienet workflow multiple times, optionally filtered by\ntest name pattern.\n- Stores results in a **CSV file** for analysis.\n- Useful for debugging flaky tests or stress-testing specific workflows.\n- Intended to be run from the local machine.\n\n---------\n\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>\nCo-authored-by: Javier Viola <javier@parity.io>",
          "timestamp": "2025-07-21T16:28:18Z",
          "tree_id": "b6ee4c0f3e3cb8b9bd8a8cadc045014f1ac0fd77",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b4b019e4db0ef47b0952638388eba4958e1c4004"
        },
        "date": 1753119635777,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005184043969999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026784164,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008702161579999989,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "60601340+lexnv@users.noreply.github.com",
            "name": "Alexandru Vasile",
            "username": "lexnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "2f8d2a2b875a3f08aae5dd82c09ffe65932d2e62",
          "message": "consensus/grandpa: Fix high number of peer disconnects with invalid justification (#9015)\n\nA grandpa race-casse has been identified in the versi-net stack around\nauthority set changes, which leads to the following:\n\n- T0 / Node A: Completes round (15)\n- T1 / Node A: Applies new authority set change and increments the SetID\n(from 0 to 1)\n- T2 / Node B: Sends Precommit for round (15) with SetID (0) -- previous\nset ID\n- T3 / Node B: Applies new authority set change and increments the SetID\n(1)\n\nIn this scenario, Node B is not aware at the moment of sending\njustifications that the Set ID has changed.\nThe downstream effect is that Node A will not be able to verify the\nsignature of justifications, since a different SetID is taken into\naccount. This will cascade through the sync engine, where the Node B is\nwrongfully banned and disconnected.\n\nThis PR aims to fix the edge-case by making the grandpa resilient to\nverifying prior setIDs for signatures.\nWhen the signature of the grandpa justification fails to decode, the\nprior SetID is also verified. If the prior SetID produces a valid\nsignature, then the outdated justification error is propagated through\nthe code (ie `SignatureResult::OutdatedSet`).\n\nThe sync engine will handle the outdated justifications as invalid, but\nwithout banning the peer. This leads to increased stability of the\nnetwork during authority changes, which caused frequent disconnects to\nversi-net in the past.\n\n### Review Notes\n- Main changes that verify prior SetId on failures are placed in\n[check_message_signature_with_buffer](https://github.com/paritytech/polkadot-sdk/pull/9015/files#diff-359d7a46ea285177e5d86979f62f0f04baabf65d595c61bfe44b6fc01af70d89R458-R501)\n- Sync engine no longer disconnects outdated justifications in\n[process_service_command](https://github.com/paritytech/polkadot-sdk/pull/9015/files#diff-9ab3391aa82ee2b2868ece610100f84502edcf40638dba9ed6953b6e572dfba5R678-R703)\n\n### Testing Done\n- Deployed the PR to versi-net with 40 validators\n- Prior we have noticed 10/40 validators disconnecting every 15-20\nminutes, leading to instability\n- Over past 24h the issue has been mitigated:\nhttps://grafana.teleport.parity.io/goto/FPNWlmsHR?orgId=1\n- Note: bootnodes 0 and 1 are currently running outdated versions that\ndo not incorporate this SetID verification improvement\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/8872\nCloses: https://github.com/paritytech/polkadot-sdk/issues/1147\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Dmitry Markin <dmitry@markin.tech>",
          "timestamp": "2025-07-22T12:08:36Z",
          "tree_id": "e83cda247a4ac590cf45c24390e0736eea169d4c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2f8d2a2b875a3f08aae5dd82c09ffe65932d2e62"
        },
        "date": 1753190879648,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025613894599999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004870168669999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008396031089999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "dmitry@markin.tech",
            "name": "Dmitry Markin",
            "username": "dmitry-markin"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "db5e645422ccf952018a3c466a33fef477858602",
          "message": "network: Upgrade litep2p to v0.10.0 (#9287)\n\n## litep2p v0.10.0\n\nThis release adds the ability to use system DNS resolver and change\nKademlia DNS memory store capacity. It also fixes the Bitswap protocol\nimplementation and correctly handles the dropped notification substreams\nby unregistering them from the protocol list.\n\n### Added\n\n- kad: Expose memory store configuration\n([#407](https://github.com/paritytech/litep2p/pull/407))\n- transport: Allow changing DNS resolver config\n([#384](https://github.com/paritytech/litep2p/pull/384))\n\n### Fixed\n\n- notification: Unregister dropped protocols\n([#391](https://github.com/paritytech/litep2p/pull/391))\n- bitswap: Fix protocol implementation\n([#402](https://github.com/paritytech/litep2p/pull/402))\n- transport-manager: stricter supported multiaddress check\n([#403](https://github.com/paritytech/litep2p/pull/403))\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-22T14:24:10Z",
          "tree_id": "a01eacbdd376755eea81cbd3e34a8279b13055c8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/db5e645422ccf952018a3c466a33fef477858602"
        },
        "date": 1753198888028,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008526110029999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005134003279999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002580963229999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "diego2737@gmail.com",
            "name": "Diego",
            "username": "dimartiro"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6a951f77bf0cbdb4bbb07783aac8a45bfb38351a",
          "message": "Dedup dependencies between dependencies and dev-dependencies (#9233)\n\n# Description\n\nDeduplicate some dependencies between `dependencies` and\n`dev-dependencies` sections\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-22T22:17:02+02:00",
          "tree_id": "8cb1aa69bfd7b4adc90c07e3af54a8f5ef858e5b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6a951f77bf0cbdb4bbb07783aac8a45bfb38351a"
        },
        "date": 1753217601791,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005121498869999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026405267999999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008573786009999983,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "enntheprogrammer@gmail.com",
            "name": "sistemd",
            "username": "sistemd"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e2802be4f32f55006abd3a40fc1808d997eaa4e1",
          "message": "fix: skip verifying imported blocks (#9280)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/9277. Still WIP\ntesting\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-22T21:08:01Z",
          "tree_id": "81087a8a688af80d3a3c027177554189be9e4050",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e2802be4f32f55006abd3a40fc1808d997eaa4e1"
        },
        "date": 1753222801759,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005010651049999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008487558749999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002620140880000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "10196091+Ank4n@users.noreply.github.com",
            "name": "Ankan",
            "username": "Ank4n"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "a34de56236e741a081aeb9f7af5094d781e0ac9d",
          "message": "[Staking Async] Saturating accrue era reward points (#9186)\n\nReplaces regular addition with saturating addition when accumulating era\nreward points in `pallet-staking-async` to prevent potential overflow.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-23T07:59:42Z",
          "tree_id": "17724309115bdf377ef4cbb9701eb49cfe7146f6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a34de56236e741a081aeb9f7af5094d781e0ac9d"
        },
        "date": 1753261860342,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008485585319999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00262507239,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005108467979999992,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "178801527+raymondkfcheung@users.noreply.github.com",
            "name": "Raymond Cheung",
            "username": "raymondkfcheung"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9428742a2994c4fb2b2de8d4bfc36deeca01e19d",
          "message": "Replace `log` with `tracing` on `pallet-bridge-grandpa` (#9294)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`pallet-bridge-grandpa` by providing structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-07-23T16:44:52Z",
          "tree_id": "531de75c002557fd8ed854570af158ed405f2c2f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9428742a2994c4fb2b2de8d4bfc36deeca01e19d"
        },
        "date": 1753293439922,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00871029359999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005195045979999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263738035,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "karol@parity.io",
            "name": "Karol Kokoszka",
            "username": "karolk91"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "069b7b56118fd65ecdbc6fea6c4dd1ffbf586d67",
          "message": "Fix subsume_assets incorrectly merging two AssetsInHolding (#9179)\n\n`subsume_assets` fails to correctly subsume two instances of\n`AssetsInHolding` under certain conditions which can result in loss of\nfunds (as assets are overriden rather than summed together)\n\nEg. consider following test:\n```\n\t#[test]\n\tfn subsume_assets_different_length_holdings() {\n\t\tlet mut t1 = AssetsInHolding::new();\n\t\tt1.subsume(CFP(400));\n\n\t\tlet mut t2 = AssetsInHolding::new();\n\t\tt2.subsume(CF(100));\n\t\tt2.subsume(CFP(100));\n\n\t\tt1.subsume_assets(t2);\n```\n\ncurrent result (without this PR change):\n```\n\t\tlet mut iter = t1.into_assets_iter();\n\t\tassert_eq!(Some(CF(100)), iter.next());\n\t\tassert_eq!(Some(CFP(100)), iter.next());\n```\n\nexpected result:\n```\n\t\tlet mut iter = t1.into_assets_iter();\n\t\tassert_eq!(Some(CF(100)), iter.next());\n\t\tassert_eq!(Some(CFP(500)), iter.next());\n```\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2025-07-24T05:51:09Z",
          "tree_id": "a2e8b99e5afdb2a058e9db3d8566040f491d5955",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/069b7b56118fd65ecdbc6fea6c4dd1ffbf586d67"
        },
        "date": 1753340645750,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008624006969999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026637322000000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005221194950000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "4211399+ordian@users.noreply.github.com",
            "name": "ordian",
            "username": "ordian"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e5e3941b5fb480a27e37f28fd437962dd029ec96",
          "message": "yap-runtime: fixes for `GetParachainInfo` (#9312)\n\nThis fixes the YAP parachain runtimes in case you encounter a panic in\nthe collator similar to\nhttps://github.com/paritytech/zombienet/issues/2050:\n```\nFailed to retrieve the parachain id\n```\n(which we do have zombienet-sdk tests for\n[here](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/client/transaction-pool/tests/zombienet/yap_test.rs))\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-24T13:03:01Z",
          "tree_id": "d908f5b48bf7d1a16c929565a909eeb7371482b8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e5e3941b5fb480a27e37f28fd437962dd029ec96"
        },
        "date": 1753366352004,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005268503699999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00867873461999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026467916899999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "40807189+AlexandruCihodaru@users.noreply.github.com",
            "name": "Alexandru Cihodaru",
            "username": "AlexandruCihodaru"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "bb39b4ecea005157687ed61c6ca4775f2264494f",
          "message": "RecentDisputes/ActiveDisputes use BTreeMap instead of Vec (#9309)\n\nFixes #782\n\n---------\n\nSigned-off-by: Alexandru Cihodaru <alexandru.cihodaru@parity.io>",
          "timestamp": "2025-07-24T15:30:53Z",
          "tree_id": "f92f583f62b60e6c2c65e160ad06d8fce52e9c06",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bb39b4ecea005157687ed61c6ca4775f2264494f"
        },
        "date": 1753375260085,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008601914719999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005142760259999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026224171099999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "dmitry@markin.tech",
            "name": "Dmitry Markin",
            "username": "dmitry-markin"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5b4ce9cbf1a46c843e391768ae179ac377aab951",
          "message": "network/litep2p: Switch to system DNS resolver (#9321)\n\nSwitch to system DNS resolver instead of 8.8.8.8 that litep2p uses by\ndefault. This enables full administrator control of what upstream DNS\nservers to use, including resolution of local names using custom DNS\nservers.\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/9298.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-25T09:05:15Z",
          "tree_id": "0487343c65a8af24281068c6c4aa4dcbfe0dab75",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5b4ce9cbf1a46c843e391768ae179ac377aab951"
        },
        "date": 1753438570743,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008515756469999984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00510302597999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00261330908,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "egor@parity.io",
            "name": "Egor_P",
            "username": "EgorPopelyaev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e11b1dcecc10f0fe3dab8785e6c52f243f82030b",
          "message": "[Backport] Regular version bumps and prdoc reordering from the stable2506 release branch back to master (#9320)\n\nThis PR backports:\n- NODE_VERSION bumps\n- spec_version bumps\n- prdoc reordering\nfrom the release branch back to master\n\n---------\n\nCo-authored-by: ParityReleases <release-team@parity.io>",
          "timestamp": "2025-07-25T10:39:37Z",
          "tree_id": "754e61e7d28611ce0028d2067705a17bf7d8e6ae",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e11b1dcecc10f0fe3dab8785e6c52f243f82030b"
        },
        "date": 1753444634118,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008571068779999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004993273269999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002675667429999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "50408393+TDemeco@users.noreply.github.com",
            "name": "Tobi Demeco",
            "username": "TDemeco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "33a43cf48b0dc78fad1212ceb15b64b81fb8e761",
          "message": "fix: :bug: use `MaxKeys` from `pallet-im-online`'s Config trait instead of hardcoded one in benchmarks (#9325)\n\nThis PR is a simple fix for issue #9324, by making the benchmarks of\n`pallet-im-online` linear up to `pallet_im_online::Config::MaxKeys`\ninstead of the hardcoded constant `MAX_KEYS = 1000`.\n\nThis should allow any runtime that uses `pallet-im-online` with less\nthan 1000 max keys to be able to benchmark the pallet correctly.",
          "timestamp": "2025-07-25T11:25:12Z",
          "tree_id": "162909f5bbb972eef3d874f961d45df3fd0315c3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/33a43cf48b0dc78fad1212ceb15b64b81fb8e761"
        },
        "date": 1753446950071,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026286785399999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008580544459999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005081402369999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "73b44193c8e66acd699f04265027289d030f6c66",
          "message": "frame_system: Whitelist storage items and do not double kill! (#9335)\n\nThis pull requests adds some storage values to the whitelisted storage\nitem list, because they are written in every block. Also it stops double\nkilling `InherentsApplied`. It is killed in `on_finalize`, so there is\nno need to do it again in `on_initialize`.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-25T14:45:45Z",
          "tree_id": "250c3b45b5be25c30e0286f5dab152b98fee7eef",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/73b44193c8e66acd699f04265027289d030f6c66"
        },
        "date": 1753459074924,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026509467199999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005055511939999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008539114469999989,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1728078+michalkucharczyk@users.noreply.github.com",
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "7ef027551fd1290c42581a85052b643bffc9cbe4",
          "message": "`fatxpool`: avoid premature revalidation of transactions (#9189)\n\nThis PR improves handling of the following scenario:\n```\nsend tx1: transfer to fund new  X account \n# wait for tx1 in block event (let's assume it happens at block N) \nsend tx2: spend from X account\n```\n\nBefore this PR `tx2` could be invalidated (and most likely was) when\n`block N-k` was finalized, because transactions are checked for being\ninvalid on finalized block. (The `X account` does not yet exists for any\nblock before `block N`).\n\nAfter this commit transactions will be revalidated on finalized blocks\nonly if their height is greater then height of the block at which\ntransactions was originally submitted.\n\nNote: There are no guarantees that `tx2` will be actually included, it\nstill may happen that it will be dropped under some circumstances. This\nonly reduces likelihood of dropping transaction.\n\n\nNote for reviewers:\nThe fix is to simply initialize\n[`validated_at`](https://github.com/paritytech/polkadot-sdk/blob/f8a1fe64c29b1ddcb5824bbb3bf327f528f18d40/substrate/client/transaction-pool/src/fork_aware_txpool/tx_mem_pool.rs#L98-L99)\nfield of `TxInMemPool` which is used to\n[select](https://github.com/paritytech/polkadot-sdk/blob/f8a1fe64c29b1ddcb5824bbb3bf327f528f18d40/substrate/client/transaction-pool/src/fork_aware_txpool/tx_mem_pool.rs#L583-L586)\ntransactions for mempool revalidation on finalized block.\n\nFixes: #9150\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>",
          "timestamp": "2025-07-26T09:38:10Z",
          "tree_id": "9290436971f50c36e2f18f32158c7ff376adf03a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7ef027551fd1290c42581a85052b643bffc9cbe4"
        },
        "date": 1753526929103,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005298488929999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026702950300000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008772860789999977,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}