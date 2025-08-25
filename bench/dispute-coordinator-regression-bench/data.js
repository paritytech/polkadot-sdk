window.BENCHMARK_DATA = {
  "lastUpdate": 1756124870190,
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
          "id": "edc8a7f95405b929318bb40867a3caca1bca9565",
          "message": "Replace `log` with `tracing` on `pallet-bridge-messages` (#9308)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`pallet-bridge-messages` by providing structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-07-28T08:02:29Z",
          "tree_id": "7bd256fb1ed72a826f529fda260828a26b18a940",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/edc8a7f95405b929318bb40867a3caca1bca9565"
        },
        "date": 1753694128689,
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
            "value": 0.008605633209999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026268005800000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005136259489999987,
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
          "id": "bb4130369ed03ec130ba41ea4cf33cbc97a98c2f",
          "message": "Replace `log` with `tracing` on `bridge-runtime-common` (#9288)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`bridge-runtime-common` by providing structured logging.\n\nPartially addresses #9211\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Andrii <ndk@parity.io>",
          "timestamp": "2025-07-28T11:54:43Z",
          "tree_id": "646c8459d9769ef4cfaf313adf64bccd2294d254",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bb4130369ed03ec130ba41ea4cf33cbc97a98c2f"
        },
        "date": 1753707913127,
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
            "value": 0.008538299879999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002612974020000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00505242120999999,
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
          "id": "492f66cdfcb2da0dfc8ce66b8b32e8801ea14fe9",
          "message": "include poll_index in voted and vote removed event (#8840)\n\ncloses #8785\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-07-28T13:10:54Z",
          "tree_id": "f964a6f7afdb1acbb1ccef2fee733840f65de696",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/492f66cdfcb2da0dfc8ce66b8b32e8801ea14fe9"
        },
        "date": 1753712971043,
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
            "value": 0.008574292499999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264460166,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005102661489999995,
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
          "id": "a15d066faac70676101854cfa9b55f00f61e865a",
          "message": "network/kad: Increase memory store capacity for providers (#9315)\n\nIncrease Kademlia memory store capacity for DHT content providers (used\nby parachain DHT-based bootnodes) and reduce provider republish interval\n& TTL. This is needed to support testnets with 1-minute fast runtime and\nup to 13 parachains.\n\nParameters set:\n- 10000 provider keys per node\n- 10h provider record TTL\n- 3.5h provider republish interval\n\nCloses https://github.com/paritytech/litep2p/issues/405.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-28T15:24:53Z",
          "tree_id": "51754d7b2f1622572ee764f0d16ad5ca318154ee",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a15d066faac70676101854cfa9b55f00f61e865a"
        },
        "date": 1753721061459,
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
            "value": 0.005135600069999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025990491500000002,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008712407089999985,
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
          "id": "ff6a5f7ec52525bfada191fd04cf6e8c34d2b78e",
          "message": "Replace `log` with `tracing` on `pallet-bridge-parachains` (#9318)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`pallet-bridge-parachains` by providing structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-07-28T16:12:47Z",
          "tree_id": "bf341a229cf46b75766555d16d51600cbf931095",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ff6a5f7ec52525bfada191fd04cf6e8c34d2b78e"
        },
        "date": 1753723377424,
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
            "value": 0.002671135479999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008559249359999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005040635599999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "140437456+hamidmuslih@users.noreply.github.com",
            "name": "Hamid Muslih",
            "username": "hamidmuslih"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ba5ed25153d37d273040925d1336620a34044fbd",
          "message": "update the builder dockerfile (#9310)\n\n# Description\n\nUpdates the base image in the Polkadot builder Dockerfile\n\nCloses #9306\n## Integration\n\nNot applicable - this PR has no downstream integration impacts as it\nonly affects the local build environment\n\n## Review Notes\n\nThis PR updates the builder base image version in\n`polkadot_builder.Dockerfile`.\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-07-28T23:31:56+02:00",
          "tree_id": "b96e77db1d8bcdf9b9df5163aa72d74c87993ed2",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ba5ed25153d37d273040925d1336620a34044fbd"
        },
        "date": 1753740412079,
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
            "value": 0.004869029299999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008463655959999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026308021399999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "82968568+miloskriz@users.noreply.github.com",
            "name": "Milos Kriz",
            "username": "miloskriz"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d55dc56df31a9f4fdd59ca7ca06f2a8b00ad808b",
          "message": "Maintenance of bootnodes for `westend` and related chains (#9353)\n\n# Description\n\nPlease consider this Pull Request to remove the bootnodes provided by\nGatotech to the following relaychain and systemchains:\n\n- `westend`\n  - `asset-hub-westend`\n  - `bridge-hub-westend`\n  - `collectives-westend`\n  - `coretime-westend`\n  - `people-westend`\n\nThis removal responds to the discontinuation of support by the\nInfrastructure Builders' Programme of Westend in favour of enhanced\nsupport to the Paseo testnet.\n\nAfter this PR is merged, we will proceed to decommission the relevant\nnodes..\n\nMany thanks!!\n\nBest regards\n\n**_Milos_**\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-29T12:39:54Z",
          "tree_id": "5e96d7613fc4bd03f97b864271e24e4e0bc984db",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d55dc56df31a9f4fdd59ca7ca06f2a8b00ad808b"
        },
        "date": 1753797238724,
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
            "value": 0.0026583770800000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008787148869999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00521899523999999,
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
          "id": "835ee4782522319cf5d8de9e35a20e56ead143b7",
          "message": "cumulus zombienet: Send transactions as immortal (#9362)\n\nDeep inside subxt the default period for a transaction is set to 32\nblocks. When you have some chain that is building blocks every 500ms,\nthis may leads to issues that manifest as invalid transaction\nsignatures. To protect the poor developers of endless debugging sessions\nwe now send transactions as immortal.",
          "timestamp": "2025-07-29T19:28:17Z",
          "tree_id": "7bf288f567d290b92248d562ce63c1d26211e02b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/835ee4782522319cf5d8de9e35a20e56ead143b7"
        },
        "date": 1753823016079,
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
            "value": 0.0051729114400000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00863501575999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025792348500000006,
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
          "id": "a64eb1fb02d4012948cba024fca2f27d94732e52",
          "message": "Remove whitespaces added by macros due to token re-parsing (#9354)\n\nRelates to: https://github.com/paritytech/polkadot-sdk/issues/9336,\nhttps://github.com/paritytech/polkadot-sdk/pull/7321\n\nThis PR aims to normalize result of `stringify` in scenarios when used\ninside nested macros to stringify token streams for benchmarking\nframework. Different versions of rust can include, or not, \"space\"\ncharacters around tokens like `<`,`>`,`::` so we are just removing\nadditional spaces.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-30T05:46:16Z",
          "tree_id": "b85a3b83c7dfcdd03e82495f9156048789f905e2",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a64eb1fb02d4012948cba024fca2f27d94732e52"
        },
        "date": 1753859133113,
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
            "value": 0.0028171183100000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008914146899999984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005404896939999998,
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
          "distinct": true,
          "id": "d753869cbb5a6db66ac0dc88c2acb407301eaa01",
          "message": "Fix definition of held balance (#9347)\n\n## Changes\n- Updated the `Held Balance` definition to reflect the current behavior.\nThe previous explanation was accurate when staking used locks (which\nwere part of the free balance), but since [staking now uses\nholds](https://github.com/paritytech/polkadot-sdk/pull/5501), the old\ndefinition is misleading.\nThis issue was originally pointed out by @michalisFr\n[here](https://github.com/w3f/polkadot-wiki/pull/6793#discussion_r2231472702).\n- Fixed a broken reference in the deprecated doc for `ExposureOf`, which\nwas (ironically) pointing to a non-existent type named `ExistenceOf`.\nThis slipped in during our [mega async staking\nPR](https://github.com/paritytech/polkadot-sdk/pull/8127).",
          "timestamp": "2025-07-30T12:06:36Z",
          "tree_id": "233781385e6bdbed9c58e4af8c5b98876f525d62",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d753869cbb5a6db66ac0dc88c2acb407301eaa01"
        },
        "date": 1753882253014,
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
            "value": 0.0026555206300000007,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005170200819999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008575080259999989,
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
          "id": "e08d8f0173db4394e0f99fff91a42d86e8d6062b",
          "message": "[Staking/AHM] Properly report weight of rc -> ah xcm back to the calls (#9380)\n\nWhich will consequently make the XCM/MQ code path aware of the weights,\nwhich was previously not the case.\n\nAdditionally, adds an event for when an era is pruned.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Paolo La Camera <paolo@parity.io>",
          "timestamp": "2025-07-30T14:51:41Z",
          "tree_id": "37ee2ae118ac7b9b9196dd78e4445a0f9a15c469",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e08d8f0173db4394e0f99fff91a42d86e8d6062b"
        },
        "date": 1753891804401,
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
            "value": 0.008631030899999989,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005054633029999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026336694300000004,
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
          "id": "5633cf5f16d226428a5e87a9a17b7415bcaaec6d",
          "message": "Ignore trie nodes while recording a proof (#8172)\n\nThis pull requests implements support for ignoring trie nodes while\nrecording a proof. It directly includes the feature into\n`basic-authorship` to later make use of it in Cumulus for multi-block\nPoVs.\n\nThe idea behind this is when you have multiple blocks per PoV that trie\nnodes accessed or produced by a block before (in the same `PoV`), are\nnot required to be added to the storage proof again. So, all the blocks\nin one `PoV` basically share the same storage proof. This also impacts\nthings like storage weight reclaim, because ignored trie node do not\ncontribute a to the storage proof size (similar to when this would\nhappen in the same block).\n\n# Example \n\nLet's say block `A` access key `X` and block `B` accesses key `X` again.\nAs `A` already has read it, we know that it is part of the storage proof\nand thus, don't need to add it again to the storage proof when building\n`B`. The same applies for storage values produced by an earlier block\n(in the same PoV). These storage values are an output of the execution\nand thus, don't need to be added to the storage proof :)\n\n\nDepends on https://github.com/paritytech/polkadot-sdk/pull/6137. Base\nbranch will be changed when this got merged.\n\nPart of: https://github.com/paritytech/polkadot-sdk/issues/6495\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Michal Kucharczyk <1728078+michalkucharczyk@users.noreply.github.com>",
          "timestamp": "2025-07-31T09:59:56Z",
          "tree_id": "431ac4005d7655af6fd7d76f89ff6162d34b87b1",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5633cf5f16d226428a5e87a9a17b7415bcaaec6d"
        },
        "date": 1753960589454,
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
            "value": 0.005123743669999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026385790100000007,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008515434699999987,
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
          "id": "d9f451a6b94ab2cf39371ee5192130379eb6e199",
          "message": "XCMv5 asset exchange test scenarios (#9195)\n\nRelates to: #9093\nRequires: #9179\n\nThis PR introduces emulated test scenarios:\n\n#### [Scenario 1]\n(Penpal -> AH -> Penpal) to showcase usage of remote `Transact` to swap\nassets remotely on AssetHub while also making use of\n`add_authorized_alias`, to transact as Sender on remote side (instead of\nSenders sovereign account).\n\n1. Prepare sovereign accounts funds, create pools, prepare aliasing\nrules\n2. Send WND from Penpal to AssetHub (AH being remote reserve for WND)\n3. Alias into sender account and exchange WNDs for USDT using `Transact`\nwith `swap_tokens_for_exact_tokens` call inside\n4. Send USDT and leftover WND back to Penpal\n\n#### [Scenario 2]\n(Penpal -> AH -> Penpal) to showcase usage of remote `Transact` to swap\nassets remotely on AssetHub.\n\n1. Prepare sovereign accounts funds, create pools, prepare aliasing\nrules\n2. Send WND from Penpal to AssetHub (AH being remote reserve for WND)\n3. Exchange WNDs for USDT using `Transact` with\n`swap_tokens_for_exact_tokens` call inside\n4. Send USDT and leftover WND back to Penpal\n\n#### [Scenario 3]\n(Penpal -> AH -> Penpal) to showcase same as above but this time using\n`ExchangeAsset` XCM instruction instead of `Transact`:\n\n1. Prepare sovereign accounts funds, create pools\n2. Send WND from Penpal to AssetHub (AH being remote reserve for WND)\n3. Exchange WNDs for USDT using `ExchangeAsset`\n4. Send USDT and leftover WND back to Penpal\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-07-31T10:42:16Z",
          "tree_id": "c39005e9c21f11c1f9743d3a32b8454e91d41b37",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d9f451a6b94ab2cf39371ee5192130379eb6e199"
        },
        "date": 1753963500826,
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
            "value": 0.005233778669999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263545215,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00874518134999999,
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
          "id": "fec2a9129a9e0238891c4102bb78b06e450e8e14",
          "message": "Replace `log` with `tracing` on `pallet-bridge-beefy` (#9378)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`pallet-bridge-beefy` by providing structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-07-31T12:36:33Z",
          "tree_id": "2acc237d85439092d66685c471f132ee381fad74",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fec2a9129a9e0238891c4102bb78b06e450e8e14"
        },
        "date": 1753969642282,
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
            "value": 0.0025956951700000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008509747089999992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005020687039999994,
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
          "distinct": true,
          "id": "177b03958c766fe053f28424ee6f6748644bb794",
          "message": "[AHM] Make stuff public and derive (#9384)\n\nMake some stuff public and derive traits. Also removes one silently\ntruncating constructor from ParaId.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-07-31T13:29:50Z",
          "tree_id": "f7914d1e9b38929282d65a0b7e256c4cea538a61",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/177b03958c766fe053f28424ee6f6748644bb794"
        },
        "date": 1753973357753,
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
            "value": 0.0026792441300000007,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005139147339999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008668094609999979,
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
          "id": "7304295748b1d85eb9fc2b598eba43d9f7971f22",
          "message": "[AHM] Staking async e2e zn and papi tests (#8802)\n\ncloses https://github.com/paritytech/polkadot-sdk/issues/8766\n\nThis PR mainly adds a setup based on PAPI to automate our e2e tests for\nstaking async. Most of the new code is in\n`frame/staking-async/runtimes/papi-tests`. There is `README`, and a\n`Justfile` there that should contain all the info you would need.\n\nBest way to get started is:\n\n```\njust setup\nbun test tests/unsigned-dev.test.ts\n```\n\nTests are written in Typescript, and monitro the underlying ZN process\nfor a specific sequence of events. An example of how to write tests is\n[here](https://github.com/paritytech/polkadot-sdk/pull/8802/files#diff-4b44e03288aeaf5ec576ae0094c7a7ae28689dfcc5b317a28478767b345991db).\n\nAll other changes are very insubstantial. \n\n### Why this setup? \n\n* Staking async e2e tests are long running, and doing multiple scenarios\nmanually is hard. Expressing them as a sequence of events is much\neasier.\n* For all scenarios, we need to monitor both the onchain weight, and the\noffchain weight/PoV recorded by the collator (therefore our only option\nis ZN). The setup reports both. For example, the logs look like this:\n\n```\nverbose: Next expected event: Observe(Para, MultiBlockElectionVerifier, Verified, no dataCheck, no byBlock), remaining events: 14\nverbose: [Para#56][⛓ 52ms / 2,119 kb][✍️ hd=0.22, xt=3.94, st=6.54, sum=10.70, cmp=9.61, time=1ms] Processing event: MultiBlockElectionVerifier Verified [1,10]\ninfo:    Primary event passed\nverbose: Next expected event: Observe(Para, MultiBlockElectionVerifier, Verified, no dataCheck, no byBlock), remaining events: 13\nverbose: [Para#56][⛓ 52ms / 2,119 kb][✍️ hd=0.22, xt=3.94, st=6.54, sum=10.70, cmp=9.61, time=1ms] Processing event: MultiBlockElectionVerifier Verified [2,10]\ninfo:    Primary event passed\nverbose: Next expected event: Observe(Para, MultiBlockElectionVerifier, Verified, no dataCheck, no byBlock), remaining events: 12\nverbose: [Para#56][⛓ 52ms / 2,119 kb][✍️ hd=0.22, xt=3.94, st=6.54, sum=10.70, cmp=9.61, time=1ms] Processing event: MultiBlockElectionVerifier Verified [3,10]\n```\n\n`⛓` indicates the onchain weights and `✍️` the collator PoV date\n(header, extrinsic, storage, sum of all, and all compressed,\nrespectively). The above lines are an example of code paths where the\nonchain weight happens to over-estimate by a lot. This setup helps us\neasily find and optimize all.\n\n---------\n\nCo-authored-by: Tsvetomir Dimitrov <tsvetomir@parity.io>\nCo-authored-by: Paolo La Camera <paolo@parity.io>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>\nCo-authored-by: Ankan <10196091+Ank4n@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexandre R. Baldé <alexandre.balde@parity.io>",
          "timestamp": "2025-07-31T17:51:37Z",
          "tree_id": "49a98e39596f07d10155e85247e4ef3dd13af3be",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7304295748b1d85eb9fc2b598eba43d9f7971f22"
        },
        "date": 1753988593316,
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
            "value": 0.008612936139999985,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026488008099999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005134206549999994,
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
          "id": "13bc266c3f3cb337a36998cfdc5940ca559051c9",
          "message": "Upgrade wasmtime (#8714)\n\nThis upgrades wasmtime to the latest version and also fixes backtraces\nfor `debug` builds.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-07-31T21:51:41Z",
          "tree_id": "26b7b22e5e91ce9e7e84a9a186f5d4d94abb898c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/13bc266c3f3cb337a36998cfdc5940ca559051c9"
        },
        "date": 1754004072029,
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
            "value": 0.008547402349999993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005096510979999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026075796000000007,
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
          "distinct": false,
          "id": "aa010dc3286063cd1f3522fc988ba472dede345b",
          "message": "Revert \"fix(minimal): pre-seal a first block to trigger maintain (#92… (#9423)\n\n# Description\n\nThis PR reverts #9207 after @michalkucharczyk's proper fix in #9338.\n\n## Integration\n\nN/A\n\n## Review Notes\n\nN/A",
          "timestamp": "2025-08-03T21:07:52Z",
          "tree_id": "757aca1f0e6e6b970edfe82ae3a36dc65718d58e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aa010dc3286063cd1f3522fc988ba472dede345b"
        },
        "date": 1754259581399,
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
            "value": 0.0026343631699999987,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008646266469999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051919826099999945,
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
          "id": "a48307b7f0c40225aa8b6fcfecf7cedb6a41d6c2",
          "message": "CoreIndexMismatch: Include more information in the error (#9396)",
          "timestamp": "2025-08-04T07:10:18Z",
          "tree_id": "f5f43827128516e54f503afddfa6f54f7c2175dd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a48307b7f0c40225aa8b6fcfecf7cedb6a41d6c2"
        },
        "date": 1754296306141,
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
            "value": 0.00867922318999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026238306399999993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00514372894,
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
          "id": "33bdd634d6ae0eb43c6660ba3ab6be6ed3668789",
          "message": "Westend Secretary Program (#9024)\n\n## Westend Secretary Program\n\nThis PR includes the Secretary program and end-to-end validation of\nXCM-based salary payments for the Westend runtime, ensuring consistency\nbetween implementations.\n\n### Key Changes\n1. Integrated Secretary configuration into Westend runtime\n- Added `SecretaryCollective` and `SecretarySalary` pallets to the\nruntime.\n   - Triggers salary payment through XCM\n   - Verifies successful:\n     - XCM message transmission\n     - Asset transfer execution\n     - Message queue processing\n\n### Context from Runtime PRs\n- Based on [Secretary Program\nimplementation](https://github.com/polkadot-fellows/runtimes/pull/347)\n- Follows patterns established in [Fellowship salary\ntests](https://github.com/paritytech/polkadot-sdk/blob/master/cumulus/parachains/integration-tests/emulated/tests/collectives/collectives-westend/src/tests/fellowship_salary.rs)\n- Addresses feedback from original implementation:\n  - Simplified polling mechanism using `NoOpPoll`\n  - Maintained consistent salary structure (6666 USDT for rank 1)\n  - Kept same XCM payment configuration",
          "timestamp": "2025-08-04T09:08:22Z",
          "tree_id": "28efe6737ecde815a68064c4a9dbf4bf6903a5ac",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/33bdd634d6ae0eb43c6660ba3ab6be6ed3668789"
        },
        "date": 1754303248054,
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
            "value": 0.008637548689999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263316841,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005060671869999993,
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
          "id": "53e30e5c60bdef92ae46f2f9b6d29a4d113e7419",
          "message": "Collator Protocol: Be more informative why a collation wasn't advertised (#9419)\n\nThis prints more information on why a collation wasn't advertised. In\nthis exact case it checks if the collation wasn't advertised because of\na session change. This is mainly some debugging help.",
          "timestamp": "2025-08-04T10:26:18Z",
          "tree_id": "9d7c051e4ae3a47c43b46c29c568fdd9227cd1c4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/53e30e5c60bdef92ae46f2f9b6d29a4d113e7419"
        },
        "date": 1754307580553,
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
            "value": 0.005050755729999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026496740900000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008563689969999993,
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
          "id": "224eab75d3a05e7c7a85baa5e044858d0f104d4a",
          "message": "Replace `log` with `tracing` on `bp-runtime` (#9401)\n\nThis PR replaces `log` with `tracing` instrumentation on `bp-runtime` by\nproviding structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-08-04T13:10:17Z",
          "tree_id": "0adad46d8710fc31ea650371fbfe51296ca7184f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/224eab75d3a05e7c7a85baa5e044858d0f104d4a"
        },
        "date": 1754317889306,
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
            "value": 0.008758490709999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005308165079999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002646625570000001,
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
          "id": "0140f9934cd553e5a36c623c75d371f6a5108774",
          "message": "cumulus tests: Improve prefix of the relay chain node (#9420)\n\nInstead of using the name of the node, we should use `Relaychain` as\ndone by normal nodes. This makes it easier to read the logs.",
          "timestamp": "2025-08-04T15:17:30Z",
          "tree_id": "fec233df11d8df3202be325ba52901bf5ba98c45",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0140f9934cd553e5a36c623c75d371f6a5108774"
        },
        "date": 1754324927121,
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
            "value": 0.00853023757999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005033950639999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026221914300000007,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hetterich.charles@gmail.com",
            "name": "Charles",
            "username": "charlesHetterich"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "bcee8dc5ba1205628b22e1ec499988d393b7e293",
          "message": "Added `substrate-node` and `eth-rpc` binaries into release workflow (#9393)\n\nAdds a total of 4 new jobs to `Release - Build node release candidate`\nCI workflow\n- 2 for releasing `substrate-node` binaries for linux/mac\n- 2 for releasing `eth-rpc` binaries for linux/mac\n\nCLOSES: #9386\n\n---------\n\nCo-authored-by: EgorPopelyaev <egor@parity.io>",
          "timestamp": "2025-08-04T18:21:20Z",
          "tree_id": "286bfd6aa3f48745d03e2fc1c6a297d361b19c80",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bcee8dc5ba1205628b22e1ec499988d393b7e293"
        },
        "date": 1754336092773,
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
            "value": 0.005105538319999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008606340929999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025249205200000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22696121+sekisamu@users.noreply.github.com",
            "name": "sekiseki",
            "username": "sekisamu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "59fb2e7482d471a7ec4e8d3b30499497efa7b34c",
          "message": "Fixes dust balance handling for pallet revive (#9357)\n\nfix issue: https://github.com/paritytech/contract-issues/issues/141\n\nCorrects the condition for minting a new currency unit when transferring\ndust. The condition was incorrectly checking\n`to_info.dust.saturating_add(dust) >= plank` which could lead to\nunexpected minting behavior. It now correctly checks if `to_info.dust >=\nplank` before minting.",
          "timestamp": "2025-08-04T19:36:58Z",
          "tree_id": "1f82f1472637c5b251995d04a7f494c144c5daf4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/59fb2e7482d471a7ec4e8d3b30499497efa7b34c"
        },
        "date": 1754341053973,
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
            "value": 0.008665357929999982,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0050287778799999894,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026385609900000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "paolo@parity.io",
            "name": "Paolo La Camera",
            "username": "sigurpol"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0f59afba36c8affac3b7fd41b0518fd9b81cefef",
          "message": "staking-async/papi-tests: fix justfile to run in CI (#9411)\n\nMake sure to run `just killall` step in a bash shell, otherwise while\ntrying to kill a non existing process e.g.\n```bash\nkillall:\n  pkill -f zombienet || true\n```\n\nwe would get the following issue while running in a container:\n\n```bash\nRun just setup\n  just setup\n  shell: sh -e {0}\n  env:\n    IMAGE: docker.io/paritytech/ci-unified:bullseye-1.85.0-2025-01-28-v202504231537\n    RUST_INFO: rustup show && cargo --version && rustup +nightly show && cargo +nightly --version\n    CACHE_ON_FAILURE: true\n    CARGO_INCREMENTAL: 0\n🧹 Killing any existing zombienet or chain processes...\npkill -f zombienet || true\nerror: Recipe `killall` was terminated on line 124 by signal 15\nerror: Recipe `setup` failed with exit code 143\nError: Process completed with exit code 143.\n```\n\nRunning the just step within a bash shell, ensure that the error is\nproperly handled and propagated without terminating the just script.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-05T08:24:56Z",
          "tree_id": "d7576a5133a9265d2155b6a6babad37d766607fd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0f59afba36c8affac3b7fd41b0518fd9b81cefef"
        },
        "date": 1754386627380,
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
            "value": 0.00860610840999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005021919349999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00265242727,
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
          "id": "56f683fabfc5a4554d4b9208e91103624264407c",
          "message": "do not trigger zombienet workflows on 'labeled' (#9427)\n\nAt some point of the stabilization process we added the 'labeled' to the\nlist of events that trigger the zombienet workflows. This is not needed\nanymore and also is causing failures because the _artifacts_ could be\nexpired\n([example](https://github.com/paritytech/polkadot-sdk/actions/runs/16529272288/job/46752021278?pr=9286#step:6:127)).\n\nThx!",
          "timestamp": "2025-08-05T15:10:46Z",
          "tree_id": "445f4b598ca4a41c328ca69c595b652faa137169",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/56f683fabfc5a4554d4b9208e91103624264407c"
        },
        "date": 1754411060761,
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
            "value": 0.0025591779099999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0050356993999999934,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008448646149999989,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "xlchen1291@gmail.com",
            "name": "Xiliang Chen",
            "username": "xlc"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "cf5a24ecc5802ecf78d943f9723b6f4ccdc0ddfa",
          "message": "pallet-timestamp is dev dependency of pallet-xcm (#9435)\n\nit is only used in mock\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-06T07:18:50Z",
          "tree_id": "18684bbb92e38d18676ffa52b4c6665a77cc1449",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cf5a24ecc5802ecf78d943f9723b6f4ccdc0ddfa"
        },
        "date": 1754469096658,
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
            "value": 0.008601640039999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026121301,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005157548269999997,
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
          "id": "43da422003fa5a5e66faee0bf9919a8a32bb630f",
          "message": "CumulusDigestItem: Add function to fetch the relay block identifier (#9432)\n\nThis simplifies code paths that are fetching these information from a\nparachain header.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-06T19:08:19Z",
          "tree_id": "bcc9d3db53b62f7a85fe3db2e791b1da77787ae7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/43da422003fa5a5e66faee0bf9919a8a32bb630f"
        },
        "date": 1754511525496,
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
            "value": 0.005212407019999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008700687759999983,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002698745,
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
          "id": "8468c3e5944ab5efdcce886e275f4cec1cdc9057",
          "message": "pr_8838.prdoc: oversight fix: major -> minor (#9440)\n\nThis fixes the\n[pr_8838.prdoc](https://github.com/paritytech/polkadot-sdk/blob/cf5a24ecc5802ecf78d943f9723b6f4ccdc0ddfa/prdoc/pr_8838.prdoc#L7).\nI somehow forgotten to fix this\n[here](https://github.com/paritytech/polkadot-sdk/pull/8838#discussion_r2152760564).",
          "timestamp": "2025-08-07T08:12:57Z",
          "tree_id": "98f2c1f36fd55a3668a7e72b4b7efc1fdd109707",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8468c3e5944ab5efdcce886e275f4cec1cdc9057"
        },
        "date": 1754558788301,
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
            "value": 0.0025801787400000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008537892599999982,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005129196109999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "mich@elmueller.net",
            "name": "Michael Müller",
            "username": "cmichi"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "beb9030b249cc078b3955232074a8495e7e0302a",
          "message": "[pallet-revive] Implement basic `System` pre-compile, move `seal_hash_blake2_256` into it (#9441)\n\nPart of closing https://github.com/paritytech/polkadot-sdk/issues/8572.\n\nJust the `hash_blake2_256` in this PR, to gauge if you're fine with this\nsetup.\n\ncc @athei @pgherveou\n\n---------\n\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-08-07T11:20:46Z",
          "tree_id": "61e2c4a158c8f86612d14047e993f1858f2b86fd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/beb9030b249cc078b3955232074a8495e7e0302a"
        },
        "date": 1754570556691,
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
            "value": 0.008649509869999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005096052659999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026123898199999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "adrian@parity.io",
            "name": "Adrian Catangiu",
            "username": "acatangiu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0d765ce37b258640a6eeb575f6bff76d6a7b7c46",
          "message": "pallet-xcm: fix authorized_alias benchmarks (#9445)\n\nDepending on runtime configuration of ED and storage deposits, the old\nbenchmark code did not set up enough funds to cover authorized aliases\nstorage deposits.\n\nFix it by adding more funds as part of benchmark setup.\n\n---------\n\nSigned-off-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Karol Kokoszka <karol.k91@gmail.com>",
          "timestamp": "2025-08-07T19:28:09Z",
          "tree_id": "143f16d946fef64897bb51a08e29cbc6ccc42185",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d765ce37b258640a6eeb575f6bff76d6a7b7c46"
        },
        "date": 1754599247855,
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
            "value": 0.005211298289999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00867143288999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026565984000000006,
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
          "distinct": false,
          "id": "c40b36c3a7c208f9a6837b80812473af3d9ba7f7",
          "message": "Cleanup staking try states + fix min bonds (#9415)\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-08T09:25:18Z",
          "tree_id": "6f1e8993c14fa59502c2a975c3bbe1d6d991ad61",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c40b36c3a7c208f9a6837b80812473af3d9ba7f7"
        },
        "date": 1754650312547,
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
            "value": 0.005185368899999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026787099000000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008681026779999984,
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
          "id": "0034d178fff88a0fd87cf0ec1d8f122ae0011d78",
          "message": "[CI] add timeout to allow alloy to process the logs (#9459)\n\nCI fix to give time to process zombienet's logs.\n\ncc https://github.com/paritytech/devops/issues/4229",
          "timestamp": "2025-08-11T18:05:05Z",
          "tree_id": "6cc0759361e6f4b043a6d1d53ef60866f5f68f67",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0034d178fff88a0fd87cf0ec1d8f122ae0011d78"
        },
        "date": 1754939832458,
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
            "value": 0.00263800379,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005133243719999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008626922539999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "claravanstaden64@gmail.com",
            "name": "Clara van Staden",
            "username": "claravanstaden"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9fe9950f2173981209dcbe1b6d640764090d9f36",
          "message": "Minor Snowbridge test fixes (#9463)\n\nThe Polkadot runtimes repo block size is too small to test all Ethereum\nclient extrinsics in a single block. This PR runs to the next block\nbefore attempting more test extriniscs. Once this PR has been released,\nthe following code can be removed from the fellows runtime repo:\nhttps://github.com/polkadot-fellows/runtimes/blob/main/system-parachains/bridge-hubs/bridge-hub-polkadot/tests/snowbridge.rs#L234-L370\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-08-12T07:06:26Z",
          "tree_id": "c5e7264aa47b9ad38029d67eae255579d1a026c6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9fe9950f2173981209dcbe1b6d640764090d9f36"
        },
        "date": 1754988067976,
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
            "value": 0.0027865009600000013,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00901572283999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005396325759999992,
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
          "id": "2db5e16bf2b497e8ef877d3d7e79b3fcdcab5f82",
          "message": "Bridges - relax trait bound from Codec to Encode (#9470)\n\n### Problem\n\nWhile bumping the parity-bridges-common repo to the latest polkadot-sdk\nmaster, we encountered a new compilation issue:\n```\nerror[E0277]: the trait bound `UncheckedExtrinsic<MultiAddress<AccountId32, ()>, ..., ..., ..., 16777216>: Decode` is not satisfied\n   --> relay-clients/client-rococo/src/lib.rs:95:3\n    |\n95  |         bp_polkadot_core::UncheckedExtrinsic<Self::Call, bp_rococo::TransactionExtension>;\n    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the trait `parity_scale_codec::Decode` is not implemented for `UncheckedExtrinsic<MultiAddress<AccountId32, ()>, ..., ..., ..., 16777216>`\n    |\n    = help: the trait `parity_scale_codec::Decode` is implemented for `sp_runtime::generic::UncheckedExtrinsic<Address, Call, Signature, Extension, MAX_CALL_SIZE>`\n    = note: required for `<Rococo as ChainWithTransactions>::SignedTransaction` to implement `parity_scale_codec::Codec`\nnote: required by a bound in `relay_substrate_client::ChainWithTransactions::SignedTransaction`\n   --> ~/.cargo/git/checkouts/polkadot-sdk-dee0edd6eefa0594/c40b36c/bridges/relays/client-substrate/src/chain.rs:227:42\n    |\n227 |     type SignedTransaction: Clone + Debug + Codec + Send + 'static;\n    |                                             ^^^^^ required by this bound in `ChainWithTransactions::SignedTransaction`\n```\n\nI added the test simulating the same compilation error here in the\npolkadot-sdk:\n```\ncargo test -p relay-substrate-client\n\nerror[E0277]: the trait bound `UncheckedExtrinsic<MultiAddress<AccountId32, ()>, ..., ..., ..., 16777216>: Decode` is not satisfied\n   --> bridges/relays/client-substrate/src/test_chain.rs:92:27\n    |\n92  |       type SignedTransaction = bp_polkadot_core::UncheckedExtrinsic<\n    |  ______________________________^\n93  | |         TestRuntimeCall,\n94  | |         bp_polkadot_core::SuffixedCommonTransactionExtension<(\n95  | |             bp_runtime::extensions::BridgeRejectObsoleteHeadersAndMessages,\n96  | |             bp_runtime::extensions::RefundBridgedParachainMessagesSchema,\n97  | |         )>,\n98  | |     >;\n    | |_____^ the trait `parity_scale_codec::Decode` is not implemented for `UncheckedExtrinsic<MultiAddress<AccountId32, ()>, EncodedOrDecodedCall<...>, ..., ..., 16777216>`\n    |\n    = help: the trait `parity_scale_codec::Decode` is implemented for `UncheckedExtrinsic<Address, Call, Signature, Extension, MAX_CALL_SIZE>`\n    = note: required for `<TestChain as ChainWithTransactions>::SignedTransaction` to implement `parity_scale_codec::Codec`\nnote: required by a bound in `ChainWithTransactions::SignedTransaction`\n   --> bridges/relays/client-substrate/src/chain.rs:227:42\n    |\n227 |     type SignedTransaction: Clone + Debug + Codec + Send + 'static;\n    |                                             ^^^^^ required by this bound in `ChainWithTransactions::SignedTransaction`\n    = note: the full name for the type has been written to '/home/bkontur/cargo-remote-builds-aaa/4049172861662423200/target/debug/deps/relay_substrate_client-3bc9e3563aed810c.long-type-11484417815568207698.txt'\n```\n\n### Solution?\n\nAfter some investigation, this compilation issue stared with\nhttps://github.com/paritytech/polkadot-sdk/pull/8234, and relaxing the\n`type SignedTransaction` constraint resolves the issue. Any other\nsolution?\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-12T10:11:16Z",
          "tree_id": "99b4240a7c41440c005fff1d1b4f9a5a6537f9b0",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2db5e16bf2b497e8ef877d3d7e79b3fcdcab5f82"
        },
        "date": 1754997755076,
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
            "value": 0.0026099018800000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008533682069999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005075402909999995,
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
          "id": "5bb3afcd733a18744a09c1df840c3813623fb0ab",
          "message": "check-semver: enable the support for edition 2024 (#9473)\n\n# Description\n\n`check-semver` job fails since some time in the majority of the PRs, and\nthe issue has been tracked down to an indirect dependency which is based\non edition 2024 rust, which can not compile successfully with the\ncurrent nightly.\n\n## Integration\n\nN/A\n\n## Review Notes\n\n- Updated parity-publish:\nhttps://github.com/paritytech/parity-publish/pull/58\n- This PR completes the circle and makes check-semver functional again\n- Tested already these changes here:\nhttps://github.com/paritytech/polkadot-sdk/actions/runs/16909276800/workflow\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>",
          "timestamp": "2025-08-13T07:37:51Z",
          "tree_id": "623c90f2a9e3da06be3694e350eafc34b23a00b3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5bb3afcd733a18744a09c1df840c3813623fb0ab"
        },
        "date": 1755074913618,
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
            "value": 0.002652146679999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008571812769999989,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004935259389999997,
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
          "id": "29a0c4a746a61a690df06822c52266ef69bf6b64",
          "message": "store headers and justifications during warp sync (#9424)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/2738.\n\nStill need to add tests for this - but I think the easiest way might be\nafter the zombienet tests are converted to Rust, in the warp sync test\nmaybe we can just request the headers (and justifications?) from\nJSON-RPC? Though I'm not sure there is an API for the justifications.\nBut in any case we can in theory make a P2P justifications request as\nwell and the node should be able to respond. Let me know if anybody has\nsome better ideas.\n\n---------\n\nSigned-off-by: sistemd <enntheprogrammer@gmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>",
          "timestamp": "2025-08-13T14:50:47Z",
          "tree_id": "b2b0080997291071c910a50b9deac8ed488c035f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/29a0c4a746a61a690df06822c52266ef69bf6b64"
        },
        "date": 1755100931870,
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
            "value": 0.00515054584999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00862464901999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026485671299999996,
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
          "distinct": true,
          "id": "c8e7a682f5961dd812fde30f9d909b86f16cd54f",
          "message": "cargo: Use rust-yamux version 0.13.6 (#9479)\n\nThis PR updates the litep2p' rust-yamux crate to version 0.13.6.\n\nThis version solves the following issue:\n\n```\n0: sp_panic_handler::set::{{closure}}\n1: std::panicking::rust_panic_with_hook\n2: std::panicking::begin_panic_handler::{{closure}}\n3: std::sys::backtrace::__rust_end_short_backtrace\n4: rust_begin_unwind\n5: core::panicking::panic_fmt\n6: core::slice::index::slice_start_index_len_fail::do_panic::runtime\n7: core::slice::index::slice_start_index_len_fail\n8: <yamux::frame::io::Io as futures_sink::Sink<yamux::frame::Frame<()>>>::poll_ready\n9: yamux::connection::Connection::poll_next_inbound\n10: litep2p::transport::websocket::connection::WebSocketConnection::start::{{closure}}\n11: <litep2p::transport::websocket::WebSocketTransport as litep2p::transport::Transport>::accept::{{closure}}\n12: <tracing_futures::Instrumented as core::future::future::Future>::poll\n13: tokio::runtime::task::raw::poll\n14: tokio::runtime::scheduler::multi_thread::worker::Context::run_task\n15: tokio::runtime::scheduler::multi_thread::worker::run\n16: tokio::runtime::task::raw::poll\n17: std::sys::backtrace::__rust_begin_short_backtrace\n18: core::ops::function::FnOnce::call_once{{vtable.shim}}\n19: std::sys::pal::unix::thread::Thread::new::thread_start\n20: start_thread\nat /build/glibc-FcRMwW/glibc-2.31/nptl/pthread_create.c:477:8\n21: clone\nat /build/glibc-FcRMwW/glibc-2.31/misc/../sysdeps/unix/sysv/linux/x86_64/clone.S:95\n```\n\nPart of: https://github.com/paritytech/polkadot-sdk/issues/9169\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-14T08:25:37Z",
          "tree_id": "7a997ec7458a771c3d93301b5c35483cbe458228",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c8e7a682f5961dd812fde30f9d909b86f16cd54f"
        },
        "date": 1755166121546,
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
            "value": 0.008563362489999985,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026181287600000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005072127819999993,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "45178695+pkhry@users.noreply.github.com",
            "name": "Pavlo Khrystenko",
            "username": "pkhry"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "fd417de617f303b84a0cac1972cf5d7090000d2f",
          "message": "[pallet-revive] expose `exec::Key` (#9482)\n\n# Description\n\nThis is a fix for the fact that `exec::Key` is exposed from within\n`pallet_revive::tracing::Tracing` interface, but not from the crate\nitself making custom tracers effectively unimplementable outside said\ncrate.\n\nIn my case it's useful for implementing custom tracers for integration\nwith `foundry`\n\n## Integration\n\nRequires no downstream changes\n\n## Review Notes\n\nThis is a fix for the fact that `exec::Key` is exposed from within\n`pallet_revive::tracing::Tracing` interface, but not from the crate\nitself making custom tracers effectively unimplementable outside said\ncrate.\n\nsee here for one of the methods: [`exec::Key` exposed to the\nimplementor, despite not being exported by the\n`crate`](https://github.com/paritytech/polkadot-sdk/blob/pkhry/expose_key_pallet_revive/substrate/frame/revive/src/tracing.rs#L68)\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-14T12:06:16Z",
          "tree_id": "989ae132623b6eea42407bc96833a8495c8fef21",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fd417de617f303b84a0cac1972cf5d7090000d2f"
        },
        "date": 1755177963515,
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
            "value": 0.0027101600699999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008863073949999984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00521436059,
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
          "id": "e117602f60bf3a0debe6843c94278275e3912a40",
          "message": "Remove free balance check in `prepare_unlock` (#9489)\n\nThe free balance check during unlocking is unnecessary since a lock can\ncover both free and reserved balances. Removing it allows locks to be\ncleared even if part of the locked funds is reserved or already slashed.",
          "timestamp": "2025-08-15T06:33:29Z",
          "tree_id": "44a9e3e9bbdefd13644497674a8362005a86ec67",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e117602f60bf3a0debe6843c94278275e3912a40"
        },
        "date": 1755243905730,
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
            "value": 0.0026895273399999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005207769489999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008697579319999984,
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
          "id": "ce5ecdd483440557d4d49f55818ed517bdf64940",
          "message": "`fatxpool`: buckets for event-timings metrics adjusted (#9495)\n\nThis PR adjusts the buckets for transactions' event-timings metrics as\nrequested in #9158 for reliability dashboard.\nMetrics were initially introduced in #7355. \n\nfixes: #9158\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-15T14:34:38Z",
          "tree_id": "f9d6c205d02c8530896693bc1f48d6cd90aff2e6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ce5ecdd483440557d4d49f55818ed517bdf64940"
        },
        "date": 1755272724056,
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
            "value": 0.005170542569999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008587653409999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263213877,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "mich@elmueller.net",
            "name": "Michael Müller",
            "username": "cmichi"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "8744f5e5ee65786e3254cf45c4de54606917effd",
          "message": "[pallet-revive] Move `to_account_id` host function to `System` pre-compile (#9455)\n\nPart of closing https://github.com/paritytech/polkadot-sdk/issues/8572.\n\ncc @athei @pgherveou\n\n---------\n\nCo-authored-by: xermicus <bigcyrill@hotmail.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-08-18T09:05:56Z",
          "tree_id": "bdf0796810802ecf2bc56112c470efa5a7f72319",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8744f5e5ee65786e3254cf45c4de54606917effd"
        },
        "date": 1755513467096,
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
            "value": 0.002637127670000001,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008712955419999984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005235125019999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bruno.devic@parity.io",
            "name": "BDevParity",
            "username": "BDevParity"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9899378386f540055b292bcfaf66b98ef2dbe774",
          "message": "[Release|CI/CD] Create pipeline with build runtimes, publish release draft and build RC all in 1 pipeline (#9437)\n\nThis PR incudes the following changes:\n\n- Creates single pipeline containing build RC, build runtimes and\npublish release candidate.\nCloses: https://github.com/paritytech/devops/issues/3828\n\n---------\n\nCo-authored-by: EgorPopelyaev <egor@parity.io>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-08-18T13:35:55Z",
          "tree_id": "95593c69c5db5e1c3aabd5589581dbe238272b25",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9899378386f540055b292bcfaf66b98ef2dbe774"
        },
        "date": 1755528323637,
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
            "value": 0.005187174989999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026449259499999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008669008949999984,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "mich@elmueller.net",
            "name": "Michael Müller",
            "username": "cmichi"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7ede4fd048f8a99e62ef31050aa2e167e99d54b9",
          "message": "[pallet-revive] Move `blake2_128` host function to `System` pre-compile (#9454)\n\nPart of closing https://github.com/paritytech/polkadot-sdk/issues/8572.\n\nI'm splitting some of the host function migrations into separate PRs, as\nthere are sometimes refactorings involved and this should make reviewing\neasier.\n\ncc @athei @pgherveou",
          "timestamp": "2025-08-18T18:40:41Z",
          "tree_id": "a1ee8d9a9483f19af3f1faf1905bef318dc29a79",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7ede4fd048f8a99e62ef31050aa2e167e99d54b9"
        },
        "date": 1755546798512,
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
            "value": 0.005112155139999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026036496500000013,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008551858059999994,
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
          "id": "7f1949d86d179d82d647a749c34a02b71492f5ff",
          "message": "Paras: Clean up `AuthorizedCodeHash` when offboarding (#9514)\n\nThis PR updates the `Paras` pallet to clear entries in\n`AuthorizedCodeHash` as part of the offboarding process.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-19T15:45:15Z",
          "tree_id": "a974eaac440aa21cf92978876f2af93910cbcd41",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7f1949d86d179d82d647a749c34a02b71492f5ff"
        },
        "date": 1755622804062,
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
            "value": 0.00261966599,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008589039879999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051881322199999955,
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
          "distinct": true,
          "id": "4619e9b6e805d132f4307752270044028273dd11",
          "message": "[revive] move existing files to prepare evm backend introduction (#9501)\n\n- Move exisiting files in pallet-revive to accomodate the upcoming EVM\nbackend\n- Add solc/resolc compilation feature for fixtures\n- Add `fn is_pvm` to later distinguish between pvm / evm bytecode\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-20T05:46:03Z",
          "tree_id": "65fbf1c1d0f6b7c75874c4ea1dd241f439b4c5bd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4619e9b6e805d132f4307752270044028273dd11"
        },
        "date": 1755673572038,
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
            "value": 0.0027029405,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008631158949999992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005138139849999996,
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
          "id": "2b56efc4d5be1a4d47b94b496193a764b8a6488b",
          "message": "[CI/CD] Fix build binary flow (#9526)\n\nThis PR fixes build-binary flow, that is used to build a binary for the\ntesting purposes from any branch. The issue was that there were too many\ninput args for the build script.",
          "timestamp": "2025-08-20T08:54:05Z",
          "tree_id": "49dc83fb4cac9dc3541ffb08932745c28fd5880f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2b56efc4d5be1a4d47b94b496193a764b8a6488b"
        },
        "date": 1755685206876,
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
            "value": 0.0027178649500000014,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00893629731999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005339076139999995,
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
          "id": "349fe9b9111d74b5e4bc3002136b211b3444f28b",
          "message": "fix: add missing crates bumps and upgrade parity-publish (#9488)\n\n# Description\n\nAdds a few crate bumps associated to PRs which missed to bump them, and\nupdates parity-publish version across the board to 0.10.6 (to support\nrustc 1.88).\n\nAdditionally, makes it so that parity-publish-check-compile runs first\non all unreleased prdocs to bump associated crates, and only after\nmoving those to an `unreleased` directory, runs on the current PR's\nprdoc. This is so that we first create a \"local release\" based on the\nunreleased prdocs, and then we follow with a \"patch\" release based on\nthe previous local release, considering only the prdoc pushed with the\ncurrent PR. If the workflow fails at the end it means current PR missed\ncertain bumps. If we don't do the plan/apply twice we risk to miss bumps\ndue to all prdocs being considered (current PR's prdoc + unreleased\nones) when running parity-publish plan/apply, which might result in a\nset of crate bumps which are sufficient, but once some unreleased prdocs\nwill be moved to a stable prodoc directory, because they will be part of\na stable release, then the ones left will not be enough from a bump\nperspective (e.g. like it happened in #9320). That's why it is important\nto check every PR that adds a prdoc whether it is self-sufficient from a\ncrates bumping perspective.\n\nIf no prdoc is provided, the parity-publish does not need to be taken\ninto consideration, but it should also pass nonetheless.\n\n## Integration\n\nN/A\n\n## Review Notes\n\nThere seems to a be a corner case parity-publish can not easily catch.\nAll bumps below are a manifestation of it. More details below:\n\n* #8714 - a major bump is necessary for `sp-wasm-interface` - context\nhere:\nhttps://github.com/paritytech/polkadot-sdk/pull/8714#discussion_r2273355186\n* `sp-keystore` was bumped during 2506 in #6010 , and the relevant prdoc\ngot moved to stable2506 dir in #9320. This moved prdoc coexisted\nalongside other unreleased prdocs, and covered a needed patch bump for\n`sp-keystore`, that is not easily visible, and also required for crates\npublishing IIUC:\n1. `sp-io` is major bumped because its direct dependency,\n`sp-state-machine`, was major bumped.\n2. `sp-io` has a direct dependency on `sp-core` (minor bumped), and\n`sp-keystore` (not touched, not bumped by now)\n3. `sp-io` fails to compile because it pulls same types from different\n`sp-core` versions (it implements `Keystore` trait from `sp-keystore`\nwith methods signatures referencing types from `sp-core 38.0.0` by using\nthe `sp-core 0.38.1` - unreleased yet - types, which confuses rustc).\n* `sp-rpc` needs a bump too due to pulling `sp-core 38.0.0`, like\n`sp-keystore`, and it is an indirect dependency of `polkadot-cli`, which\nhas also a direct dependency on unreleased `sp-core 38.1.0`, so again,\nif we don't bump `sp-rpc` (historically it has been bumped only with\nmajor, but I think we can go with patch on this one), `polkadot-cli`\ncan't compile.\n* `sc-storage-monitor` is in a similar situation as\n`sp-rpc`/`sp-keystore` - `polkadot-cli` depends on `sc-storage-monitor`\n(which is not bumped, and has a dependency on `sp-core 38.0.0`), but it\nalso depends on `sp-core 38.1.0`. And yet again, something is used in\n`polkadot-cli` from the two different `sp-core` versions, which confuses\nrustc.\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-08-20T11:47:46Z",
          "tree_id": "3b32afd34a525f52faa8db9906d3ff42149a8474",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/349fe9b9111d74b5e4bc3002136b211b3444f28b"
        },
        "date": 1755695006083,
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
            "value": 0.0049793718199999965,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026689797200000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008521806289999989,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hetterich.charles@gmail.com",
            "name": "Charles",
            "username": "charlesHetterich"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "386b3abb72283c2c0efacd0fd2975163b333bce6",
          "message": "Added `subkey` to CI release process (#9466)\n\n- Added 2 jobs to `Release - Build node release candidate` CI workflow\nfor linux/mac subkey binaries\n- Added 2 jobs to `RC Build` CI workflow to upload linux/mac `subkey`\nartifacts to S3\n- updated `release_lib.sh` to reflect new S3 artifacts\n\nCLOSES: #9465\n\n---------\n\nCo-authored-by: EgorPopelyaev <egor@parity.io>",
          "timestamp": "2025-08-20T16:20:41Z",
          "tree_id": "b37735b9bf06f64f0094cfa01debf9393f601c1d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/386b3abb72283c2c0efacd0fd2975163b333bce6"
        },
        "date": 1755711364480,
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
            "value": 0.0026707993300000006,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008609825189999979,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051522955899999945,
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
          "id": "56d3c42cf4b8b650ae416db0482ad56eb64938c9",
          "message": "EPMB/Signed: Make invulnerables non-eject-able (#9511)\n\nFollow-up to https://github.com/paritytech/polkadot-sdk/pull/8877 and\naudits: Make it such that invulnerable accounts cannot be ejected from\nthe election signed queue altogether.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-08-21T08:22:32Z",
          "tree_id": "2d9f9eabd9dddec904ce178c9d3486ceddf00d59",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/56d3c42cf4b8b650ae416db0482ad56eb64938c9"
        },
        "date": 1755768972632,
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
            "value": 0.00259508205,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005002684619999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008450085419999991,
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
          "distinct": false,
          "id": "1e4af2353ea9dcb9ad0afc1ce63b03df68108ecf",
          "message": "Replace `log` with `tracing` on `pallet-bridge-relayers` (#9381)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`pallet-bridge-relayers` by providing structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-08-21T10:16:26Z",
          "tree_id": "227e8c7bb7b7adb486125c662468b3c2894feb3b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1e4af2353ea9dcb9ad0afc1ce63b03df68108ecf"
        },
        "date": 1755775892336,
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
            "value": 0.00857276897999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005090415769999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027136283300000013,
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
          "id": "9969e1e81c94f2153412d647d92ecad8db3ccbf8",
          "message": "[Backport] Version bumps and prdoc reordering from stable2506-1 (#9529)\n\nThis PR backport regular version bumps and prdocs reordering from the\nstable2506 branch back to master",
          "timestamp": "2025-08-21T14:46:57Z",
          "tree_id": "1c02f70053ccdced9c6f2f6a599c00d6076584ef",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9969e1e81c94f2153412d647d92ecad8db3ccbf8"
        },
        "date": 1755793390852,
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
            "value": 0.00261616011,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008538731129999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005075817549999998,
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
          "distinct": false,
          "id": "2660bf5f04736beef5c7002ffb5a5856e9420d1a",
          "message": "`polkadot-omni-node`: fixes and changes related to `GetParachainInfo` (#9201)\n\n# Description\n\n- log::info! the error of accessing `GetParachainInfo::parachain_id()`\nruntime api if any, before reading the `para_id` from the chain\nspecification (relevant for debugging).\n- removes comments/deprecation notices throughout the code that\nintroduce `para-id` flag removal (from chain-spec-builder and support\nfor parsing it from chain specifications)\n\nCloses #9217 \n\n## Integration\n\nN/A\n\n## Review Notes\n\nN/A\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Sebastian Kunert <mail@skunert.dev>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2025-08-22T12:59:14Z",
          "tree_id": "5b83afefc295e7e302e0f1ae5f368c6b316f7ab8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2660bf5f04736beef5c7002ffb5a5856e9420d1a"
        },
        "date": 1755872026349,
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
            "value": 0.0027669229499999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008831722099999979,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005237363689999996,
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
          "distinct": true,
          "id": "13320f333c00619165c406fdfcb28b6056b543df",
          "message": "align eth-rpc response with geth (#9177)\n\n- Update some serde encoding for eth-rpc to match serialization behavior\nof Geth\n- Add support for serializing / deserializing EIP7702 tx types\n- Disable transaction type we don't support yet in\ntry_ino_unchecked_extrinsics\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-22T14:18:13Z",
          "tree_id": "c37739e4310b85426b09807e257c5ce83e309bb4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/13320f333c00619165c406fdfcb28b6056b543df"
        },
        "date": 1755876913801,
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
            "value": 0.00266481784,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008687426819999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005171359449999992,
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
          "id": "2cb8f12822346d3772855be8a9caa25abad21e33",
          "message": "[XCMP] `take_first_concatenated_xcm()` improvements (#9539)\n\nThis PR:\n- improves `take_first_concatenated_xcm()` avoiding the XCM re-encoding\n- makes the benchmarks for `take_first_concatenated_xcm()` more\ngranular, accounting for the number of bytes of the message\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-25T07:57:56Z",
          "tree_id": "c88ecd500c2bea0dfa451ff9a9fa2e26e5364afb",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2cb8f12822346d3772855be8a9caa25abad21e33"
        },
        "date": 1756113555704,
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
            "value": 0.0051421041999999955,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264109233,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008589589109999985,
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
          "distinct": false,
          "id": "1a512570552119a49a8ecb2abfb7021954c4422d",
          "message": "Society pallet supports non-consecutive block provider (#9497)\n\nSociety pallet supports non-consecutive block provider\n\nSociety pallet correctly handles situations where `on_initialize` is\ninvoked with block numbers that:\n- increase but are not strictly consecutive (e.g., jump from 5 → 10), or\n- are repeated (e.g., multiple blocks are built at the same Relay Chain\nparent block, all reporting the same BlockNumberProvider value).\n\nThis situation may occur when the BlockNumberProvider is not local - for\nexample, on a parachain using the Relay Chain block number provider.\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-08-25T11:04:31Z",
          "tree_id": "2d5738b63692aa5d082a7286608ad0a8ba3f9bdc",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1a512570552119a49a8ecb2abfb7021954c4422d"
        },
        "date": 1756124852007,
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
            "value": 0.008596276549999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005156241139999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026511979999999987,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}