window.BENCHMARK_DATA = {
  "lastUpdate": 1712044449426,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "Benchmark": [
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
          "id": "eb6f5abee64e979dba25924f71ef86d2b3ca2deb",
          "message": "[ci] fix subsystem-benchmarks gha (#3876)\n\nPR adds variables validation and app credentials for pushing into\ngh-pages\n\ncc https://github.com/paritytech/ci_cd/issues/934",
          "timestamp": "2024-03-28T16:42:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/eb6f5abee64e979dba25924f71ef86d2b3ca2deb"
        },
        "date": 1711649817534,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.926666666663,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.009493043573333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15172178257999994,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02619895733333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011713924659999996,
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
          "id": "30ef8651ed0ba821e59121545815280a3e9b2862",
          "message": "collation-genereation: fix tests (#3883)\n\nSomehow https://github.com/paritytech/polkadot-sdk/pull/3795 was merged\nbut tests are failing now on master. I suspect that CI is not even\nrunning these tests anymore which is a big issue.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-03-29T07:49:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/30ef8651ed0ba821e59121545815280a3e9b2862"
        },
        "date": 1711702894873,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.906666666662,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013556550140000003,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16053591725999997,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026928166213333323,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011546961813333343,
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
          "id": "b310b575cd73928cf061e1ae0d184f7e900976d5",
          "message": "Remove transient code after `im-online` pallet removal (#3383)\n\nRemoves transient code introduced to clean up offchain database after\n`im-online` pallet removal.\n\nShould be merged after #2290 has been enacted.",
          "timestamp": "2024-03-29T09:12:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b310b575cd73928cf061e1ae0d184f7e900976d5"
        },
        "date": 1711707802331,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.89333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014100756359999996,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16313833589999993,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.027004377906666672,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011511754360000005,
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
          "id": "5638d1a830dc70f56e5fdd7eded21a4f592d382c",
          "message": "Decorate mpsc-notification-to-protocol with the protocol name (#3873)\n\nCurrently, all protocols use the same metric name for\n`mpsc-notification-to-protocol` this is bad because we can't actually\ntell which protocol might cause problems.\n\nThis patch proposes we derive the name of the metric from the protocol\nname, so that we have separate metrics for each protocol and properly\ndetect which one is having problem processing its messages.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-03-29T11:24:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5638d1a830dc70f56e5fdd7eded21a4f592d382c"
        },
        "date": 1711715582196,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.89333333333,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026813563506666672,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012764946213333342,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1599529295733333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010827330106666668,
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
          "id": "0d9324847391e902bb42f84f0e76096b1f764efe",
          "message": "Fix `addresses_to_publish_respects_existing_p2p_protocol` test in sc-authority-discovery (#3895)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3887.",
          "timestamp": "2024-03-29T13:13:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d9324847391e902bb42f84f0e76096b1f764efe"
        },
        "date": 1711722476275,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.94,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.011525132520000004,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16248929208666663,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02693435398666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013765804173333333,
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
          "id": "41257069b062ea7feb2277f11a2e992d3c9d5089",
          "message": "Tokens in FRAME Docs (#2802)\n\nCloses https://github.com/paritytech/polkadot-sdk-docs/issues/70\n\nWIP PR for an overview of how to develop tokens in FRAME. \n\n- [x] Tokens in Substrate Ref Doc\n  - High-level overview of the token-related logic in FRAME\n- Improve docs with better explanation of how holds, freezes, ed, free\nbalance, etc, all work\n- [x] Update `pallet_balances` docs\n  - Clearly mark what is deprecated (currency)\n- [x] Write fungible trait docs\n- [x] Evaluate and if required update `pallet_assets`, `pallet_uniques`,\n`pallet_nfts` docs\n- [x] Absorb https://github.com/paritytech/polkadot-sdk/pull/2683/\n- [x] Audit individual trait method docs, and improve if possible\n\nFeel free to suggest additional TODOs for this PR in the comments\n\n---------\n\nCo-authored-by: Bill Laboon <laboon@users.noreply.github.com>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-03-31T09:59:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/41257069b062ea7feb2277f11a2e992d3c9d5089"
        },
        "date": 1711883514489,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.906666666662,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.01321759237333334,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16129487141999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011194817933333332,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026805748546666668,
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
          "id": "256d5aefdc83928090aa2e3f8c022484fab38e0a",
          "message": "Revert log level changes (#3913)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/3906",
          "timestamp": "2024-03-31T20:47:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/256d5aefdc83928090aa2e3f8c022484fab38e0a"
        },
        "date": 1711922170751,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.946666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.01051498940666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15811673882666663,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02668074955333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012452131600000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gemini132",
            "username": "gemini132",
            "email": "164285545+gemini132@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "aa44384e05e05705cbdfacd8d73972404be4be6f",
          "message": "Fix two typos (#3812)\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-03-31T22:28:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aa44384e05e05705cbdfacd8d73972404be4be6f"
        },
        "date": 1711928258330,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.879999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010283178633333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012317254779999998,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15785113280666668,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026546342846666664,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Matteo Muraca",
            "username": "muraca",
            "email": "56828990+muraca@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a2c9ab8c043221f4902739d678739b1fa9319cef",
          "message": "Removed `pallet::getter` usage from `pallet-alliance` (#3738)\n\nPart of #3326 \n\ncc @kianenigma @ggwpez @liamaharon \n\npolkadot address: 12poSUQPtcF1HUPQGY3zZu2P8emuW9YnsPduA4XG3oCEfJVp\n\n---------\n\nSigned-off-by: Matteo Muraca <mmuraca247@gmail.com>\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-01T05:17:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a2c9ab8c043221f4902739d678739b1fa9319cef"
        },
        "date": 1711952739100,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.85333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.01434732411333333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02694091438666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16491147128666675,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01177170572000001,
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
          "id": "e0c081dbd46c1e6edca1ce2c62298f5f3622afdd",
          "message": "network:bridge: fix peer_count metric (#3711)\n\nThe metric records the current protocol_version of the validator that\njust connected with the peer_map.len(), which contains all peers that\nconnected, that has the effect the metric will be wrong since it won't\ntell us how many peers we have connected per version because it will\nalways record the total number of peers\n\nFix this by counting by version inside peer_map, additionally because\nthat might be a bit heavier than len(), publish it only on-active\nleaves.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-01T06:29:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e0c081dbd46c1e6edca1ce2c62298f5f3622afdd"
        },
        "date": 1711957065161,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.859999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.009354435260000005,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011703227319999999,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15315551578666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026097287386666664,
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
          "id": "e6bd9205432bb524e94c9bd13048d645ec9aa5c7",
          "message": "Fix 0007-dispute-freshly-finalized.zndsl failing (#3893)\n\nTest started failing after\nhttps://github.com/paritytech/polkadot-sdk/commit/66051adb619d2119771920218e2de75fa037d7e8\nwhich enabled approval coalescing, that was expected to happen because\nthe test required an polkadot_parachain_approval_checking_finality_lag\nof 0, which can't happen with max_approval_coalesce_count greater than 1\nbecause we always delay the approval for no_show_duration_ticks/2 in\ncase we can coalesce it with other approvals.\n\n\nSo relax a bit the restrictions, since we don't actually care that the\nlags are 0, but the fact the finalities are progressing and are not\nstuck.\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-01T10:23:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e6bd9205432bb524e94c9bd13048d645ec9aa5c7"
        },
        "date": 1711968362362,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.893333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.011773653299999997,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02605690070000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00925084764666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15283237586666668,
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
          "id": "e6bd9205432bb524e94c9bd13048d645ec9aa5c7",
          "message": "Fix 0007-dispute-freshly-finalized.zndsl failing (#3893)\n\nTest started failing after\nhttps://github.com/paritytech/polkadot-sdk/commit/66051adb619d2119771920218e2de75fa037d7e8\nwhich enabled approval coalescing, that was expected to happen because\nthe test required an polkadot_parachain_approval_checking_finality_lag\nof 0, which can't happen with max_approval_coalesce_count greater than 1\nbecause we always delay the approval for no_show_duration_ticks/2 in\ncase we can coalesce it with other approvals.\n\n\nSo relax a bit the restrictions, since we don't actually care that the\nlags are 0, but the fact the finalities are progressing and are not\nstuck.\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-01T10:23:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e6bd9205432bb524e94c9bd13048d645ec9aa5c7"
        },
        "date": 1711971202350,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.89333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15441177612000007,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009595722386666674,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011976155019999999,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026138174880000004,
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
          "id": "d6f68bb9062167537211cc05286809771fc8861a",
          "message": "primitives: Move out of staging released APIs (#3925)\n\nRuntime release 1.2 includes bumping of the ParachainHost APIs up to\nv10, so let's move all the released APIs out of vstaging folder, this PR\ndoes not include any logic changes only renaming of the modules and some\nmoving around.\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-01T13:03:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d6f68bb9062167537211cc05286809771fc8861a"
        },
        "date": 1711980816655,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.899999999994,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026150050639999995,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.14862955757333332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009149089346666672,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011803030726666669,
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
          "id": "9805ba2cd01922f81621e0f3ac8adc0180fb7a49",
          "message": "Fix links (#3928)\n\nFix links\n\nRelated CI failure:\nhttps://github.com/paritytech/polkadot-sdk/actions/runs/8455425042/job/23162858534?pr=3859",
          "timestamp": "2024-04-01T20:18:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9805ba2cd01922f81621e0f3ac8adc0180fb7a49"
        },
        "date": 1712007127708,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.919999999995,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02602301328000001,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011771801226666663,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.008934485593333336,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.14830757186666665,
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
          "id": "52e103784945997cb3808cdfaaf72c468f8fc938",
          "message": "`im-online` removal final cleanup (#3902)\n\nRejoice! Rejoice! The story is nearly over.\n\nThis PR removes stale migrations, auxiliary structures, and package\ndependencies, thus making Rococo and Westend totally free from any\n`im-online`-related stuff.\n\n`im-online` still stays a part of the Substrate node and its runtime:\nhttps://github.com/paritytech/polkadot-sdk/blob/0d9324847391e902bb42f84f0e76096b1f764efe/substrate/bin/node/runtime/src/lib.rs#L2276-L2277\nI'm not sure if it makes sense to remove it from there considering that\nwe're not removing `im-online` from FRAME. Please share your opinion.",
          "timestamp": "2024-04-01T21:40:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/52e103784945997cb3808cdfaaf72c468f8fc938"
        },
        "date": 1712014483371,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.906666666666,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15032764911333335,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02614782939333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01195878642,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00985203142666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sam Johnson",
            "username": "sam0x17",
            "email": "sam@durosoft.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9a62de27a98312741b4ece2fcd1c6e61b47ee3c2",
          "message": "Update derive syn parse 0.2.0 (+ docify) (#3920)\n\nderive-syn-parse v0.2.0 came out recently which (finally) adds support\nfor syn 2x.\n\nUpgrading to this will remove many of the places where syn 1x was still\ncompiling alongside syn 2x in the polkadot-sdk workspace.\n\nThis also upgrades `docify` to 0.2.8 which is the version that upgrades\nderive-syn-pasre to 0.2.0.\n\nAdditionally, this consolidates the `docify` versions in the repo to all\nuse the latest, and in one case upgrades to the 0.2x syntax where 0.1.x\nwas still being used.\n\n---------\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-04-02T05:53:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9a62de27a98312741b4ece2fcd1c6e61b47ee3c2"
        },
        "date": 1712044425576,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.913333333334,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010171323220000002,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15118695240666669,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012241153853333332,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026353824726666662,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}