window.BENCHMARK_DATA = {
  "lastUpdate": 1717067908070,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "availability-distribution-regression-bench": [
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
          "id": "d0ebb850ed2cefeb3e4ef8b8e0a16eb7fb6b3f3e",
          "message": "pallet-xcm: fix weights for all XTs and deprecate unlimited weight ones  (#3927)\n\nFix \"double-weights\" for extrinsics, use only the ones benchmarked in\nthe runtime.\n\nDeprecate extrinsics that don't specify WeightLimit, remove their usage\nacross the repo.\n\n---------\n\nSigned-off-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-02T07:57:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d0ebb850ed2cefeb3e4ef8b8e0a16eb7fb6b3f3e"
        },
        "date": 1712049017465,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.91333333333,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02636886516666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012324231520000006,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15160163568666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010125237973333341,
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
          "id": "12eb285dbe6271c365db7ba17cf643bfc77fe753",
          "message": "Fix parachain upgrade scheduling when done by the owner/root (#3341)\n\nWhen using `schedule_code_upgrade` to change the code of a parachain in\nthe relay chain runtime, we had already fixed to not set the `GoAhead`\nsignal. This was done to not brick any parachain after the upgrade,\nbecause they were seeing the signal without having any upgrade prepared.\nThe remaining problem is that the parachain code is only upgraded after\na parachain header was enacted, aka the parachain made some progress.\nHowever, this is quite complicated if the parachain is bricked (which is\nthe most common scenario why to manually schedule a code upgrade). Thus,\nthis pull request replaces `SetGoAhead` with `UpgradeStrategy` to signal\nto the logic kind of strategy want to use. The strategies are either\n`SetGoAheadSignal` or `ApplyAtExpectedBlock`. `SetGoAheadSignal` sets\nthe go ahead signal as before and awaits a parachain block.\n`ApplyAtExpectedBlock` schedules the upgrade and applies it directly at\nthe `expected_block` without waiting for the parachain to make any kind\nof progress.",
          "timestamp": "2024-04-02T09:44:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/12eb285dbe6271c365db7ba17cf643bfc77fe753"
        },
        "date": 1712055712151,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.893333333326,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026532699566666665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01059363744,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15453456288666673,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012608220059999998,
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
          "id": "7430f413503f8008fe60eb2e4ebd76d14af12ea9",
          "message": "chainHead: Allow methods to be called from within a single connection context and limit connections (#3481)\n\nThis PR ensures that the chainHead RPC class can be called only from\nwithin the same connection context.\n\nThe chainHead methods are now registered as raw methods. \n- https://github.com/paritytech/jsonrpsee/pull/1297\nThe concept of raw methods is introduced in jsonrpsee, which is an async\nmethod that exposes the connection ID:\nThe raw method doesn't have the concept of a blocking method. Previously\nblocking methods are now spawning a blocking task to handle their\nblocking (ie DB) access. We spawn the same number of tasks as before,\nhowever we do that explicitly.\n\nAnother approach would be implementing a RPC middleware that captures\nand decodes the method parameters:\n- https://github.com/paritytech/polkadot-sdk/pull/3343\nHowever, that approach is prone to errors since the methods are\nhardcoded by name. Performace is affected by the double deserialization\nthat needs to happen to extract the subscription ID we'd like to limit.\nOnce from the middleware, and once from the methods itself.\n\nThis PR paves the way to implement the chainHead connection limiter:\n- https://github.com/paritytech/polkadot-sdk/issues/1505\nRegistering tokens (subscription ID / operation ID) on the\n`RpcConnections` could be extended to return an error when the maximum\nnumber of operations is reached.\n\nWhile at it, have added an integration-test to ensure that chainHead\nmethods can be called from within the same connection context.\n\nBefore this is merged, a new JsonRPC release should be made to expose\nthe `raw-methods`:\n- [x] Use jsonrpsee from crates io (blocked by:\nhttps://github.com/paritytech/jsonrpsee/pull/1297)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/3207\n\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Niklas Adolfsson <niklasadolfsson1@gmail.com>",
          "timestamp": "2024-04-02T13:12:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7430f413503f8008fe60eb2e4ebd76d14af12ea9"
        },
        "date": 1712065558290,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666662,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15478787650666662,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026301312180000008,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012363761973333331,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010598762613333335,
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
          "id": "5d9826c2620aff205811edf0e6a07b55a52cbf50",
          "message": "Snowbridge: Synchronize from Snowfork repository (#3761)\n\nThis PR includes the following 2 improvements:\n\n## Ethereum Client\n\nAuthor: @yrong \n### Original Upstream PRs\n- https://github.com/Snowfork/polkadot-sdk/pull/123\n- https://github.com/Snowfork/polkadot-sdk/pull/125\n\n### Description\nThe Ethereum client syncs beacon headers as they are finalized, and\nimports every execution header. When a message is received, it is\nverified against the import execution header. This is unnecessary, since\nthe execution header can be sent with the message as proof. The recent\nDeneb Ethereum upgrade made it easier to locate the relevant beacon\nheader from an execution header, and so this improvement was made\npossible. This resolves a concern @svyatonik had in our initial Rococo\nPR:\nhttps://github.com/paritytech/polkadot-sdk/pull/2522#discussion_r1431270691\n\n## Inbound Queue\n\nAuthor: @yrong \n### Original Upstream PR\n- https://github.com/Snowfork/polkadot-sdk/pull/118\n\n### Description\nWhen the AH sovereign account (who pays relayer rewards) is depleted,\nthe inbound message will not fail. The relayer just will not receive\nrewards.\n\nBoth these changes were done by @yrong, many thanks. ❤️\n\n---------\n\nCo-authored-by: claravanstaden <Cats 4 life!>\nCo-authored-by: Ron <yrong1997@gmail.com>\nCo-authored-by: Vincent Geddes <vincent@snowfork.com>\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>",
          "timestamp": "2024-04-02T13:53:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5d9826c2620aff205811edf0e6a07b55a52cbf50"
        },
        "date": 1712070483072,
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
            "name": "availability-store",
            "value": 0.15313521236000002,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026420130766666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01009018272666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012255431726666668,
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
          "id": "0becc45bd826aea6ec128da8525ed73b3657d474",
          "message": "sp_runtime: TryFrom<RuntimeString> for &str (#3942)\n\nAdded `TryFrom<&'a RuntimeString> for &'a str`",
          "timestamp": "2024-04-02T16:06:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0becc45bd826aea6ec128da8525ed73b3657d474"
        },
        "date": 1712079246632,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.913333333327,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15087253584000002,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011770235079999998,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02610349496666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00934799671333334,
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
          "id": "665e3654ceca5a34e8ada66a9805fa7b76fc9ebb",
          "message": "Remove nextest filtration (#3885)\n\nFixes\nhttps://github.com/paritytech/polkadot-sdk/issues/3884#issuecomment-2026058687\n\nAfter moving regression tests to benchmarks\n(https://github.com/paritytech/polkadot-sdk/pull/3741) we don't need to\nfilter tests anymore.\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>\nCo-authored-by: Alin Dima <alin@parity.io>\nCo-authored-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: Serban Iorga <serban@parity.io>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Alexandru Vasile <60601340+lexnv@users.noreply.github.com>\nCo-authored-by: Niklas Adolfsson <niklasadolfsson1@gmail.com>\nCo-authored-by: Dastan <88332432+dastansam@users.noreply.github.com>\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Clara van Staden <claravanstaden64@gmail.com>\nCo-authored-by: Ron <yrong1997@gmail.com>\nCo-authored-by: Vincent Geddes <vincent@snowfork.com>\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2024-04-02T19:27:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/665e3654ceca5a34e8ada66a9805fa7b76fc9ebb"
        },
        "date": 1712090442829,
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
            "name": "test-environment",
            "value": 0.009364546473333343,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15020474942,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011809206366666672,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026126676560000003,
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
          "id": "9b378a2ffef1d5846872adc4336341805bffbc30",
          "message": "sp-wasm-interface: `wasmtime` should not be enabled by `std` (#3954)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/3909",
          "timestamp": "2024-04-03T08:35:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9b378a2ffef1d5846872adc4336341805bffbc30"
        },
        "date": 1712138920682,
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
            "value": 0.012225607746666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1542211771466667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026381438379999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010117722926666673,
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
          "id": "287b116c3e50ff8be275b093674404b2f370c553",
          "message": "chainHead: Ensure reasonable distance between leaf and finalized block (#3562)\n\nThis PR ensure that the distance between any leaf and the finalized\nblock is within a reasonable distance.\n\nFor a new subscription, the chainHead has to provide all blocks between\nthe leaves of the chain and the finalized block.\n When the distance between a leaf and the finalized block is large:\n - The tree route is costly to compute\n - We could deliver an unbounded number of blocks (potentially millions)\n(For more details see\nhttps://github.com/paritytech/polkadot-sdk/pull/3445#discussion_r1507210283)\n\nThe configuration of the ChainHead is extended with:\n- suspend on lagging distance: When the distance between any leaf and\nthe finalized block is greater than this number, the subscriptions are\nsuspended for a given duration.\n- All active subscriptions are terminated with the `Stop` event, all\nblocks are unpinned and data discarded.\n- For incoming subscriptions, until the suspended period expires the\nsubscriptions will immediately receive the `Stop` event.\n    - Defaults to 128 blocks\n- suspended duration: The amount of time for which subscriptions are\nsuspended\n    - Defaults to 30 seconds\n \n \n cc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-04-03T11:46:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/287b116c3e50ff8be275b093674404b2f370c553"
        },
        "date": 1712149575020,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.886666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.01028775906666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02666044656666666,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012328623913333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1541341964866666,
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
          "id": "0f4e849e0ac2de8c9880077c085985c5f656329c",
          "message": "Add ClaimQueue wrapper (#3950)\n\nRemove `fetch_next_scheduled_on_core` in favor of new wrapper and\nmethods for accessing it.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-03T15:01:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0f4e849e0ac2de8c9880077c085985c5f656329c"
        },
        "date": 1712160843652,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.01031669651333334,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026681705533333332,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1552817023733333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012435443793333336,
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
          "id": "3836376965104d7723a1659d52ee26232019b929",
          "message": "Renames `frame` crate to `polkadot-sdk-frame` (#3813)\n\nStep in https://github.com/paritytech/polkadot-sdk/issues/3155\n\nNeeded for https://github.com/paritytech/eng-automation/issues/6\n\nThis PR renames `frame` crate to `polkadot-sdk-frame` as `frame` is not\navailable on crates.io\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-04T02:20:15Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3836376965104d7723a1659d52ee26232019b929"
        },
        "date": 1712201879635,
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
            "name": "test-environment",
            "value": 0.008831902806666672,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.14943051571333338,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02604288751333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011563848166666663,
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
          "id": "0bbda78d86bc6210cda123042d817aeaf45b3d77",
          "message": "Use 0.1.0 as minimum version for crates (#3941)\n\nCI will be enforcing this with next parity-publish release",
          "timestamp": "2024-04-04T09:26:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0bbda78d86bc6210cda123042d817aeaf45b3d77"
        },
        "date": 1712227778440,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010619473013333327,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15767303358,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012514623186666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02652186586666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Juan Girini",
            "username": "juangirini",
            "email": "juangirini@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "bcb4d137c9efffde8f27c3906177519da031552c",
          "message": "[doc] Example MBM pallet (#2119)\n\n## Basic example showcasing a migration using the MBM framework\n\nThis PR has been built on top of\nhttps://github.com/paritytech/polkadot-sdk/pull/1781 and adds two new\nexample crates to the `examples` pallet\n\n### Changes Made:\n\nAdded the `pallet-example-mbm` crate: This crate provides a minimal\nexample of a pallet that uses MBM. It showcases a storage migration\nwhere values are migrated from a `u32` to a `u64`.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-04-04T11:47:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bcb4d137c9efffde8f27c3906177519da031552c"
        },
        "date": 1712236048463,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15776303835999994,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026543350853333335,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012428478939999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010459442440000001,
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
          "id": "c130ea9939b01d0ce8c0da8e5e5094ffdb3479e3",
          "message": "XCM builder pattern improvement - Accept `impl Into<T>` instead of just `T` (#3708)\n\nThe XCM builder pattern lets you build xcms like so:\n\n```rust\nlet xcm = Xcm::builder()\n    .withdraw_asset((Parent, 100u128).into())\n    .buy_execution((Parent, 1u128).into())\n    .deposit_asset(All.into(), AccountId32 { id: [0u8; 32], network: None }.into())\n    .build();\n```\n\nAll the `.into()` become quite annoying to have to write.\nI accepted `impl Into<T>` instead of `T` in the generated methods from\nthe macro.\nNow the previous example can be simplified as follows:\n\n```rust\nlet xcm = Xcm::builder()\n    .withdraw_asset((Parent, 100u128))\n    .buy_execution((Parent, 1u128))\n    .deposit_asset(All, [0u8; 32])\n    .build();\n```\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-04-04T12:40:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c130ea9939b01d0ce8c0da8e5e5094ffdb3479e3"
        },
        "date": 1712239348021,
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
            "name": "bitfield-distribution",
            "value": 0.026932037893333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013780285639999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01126823984666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1635060440866667,
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
          "id": "bda4e75ac49786a7246531cf729b25c208cd38e6",
          "message": "Migrate fee payment from `Currency` to `fungible` (#2292)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/226 \nRelated https://github.com/paritytech/polkadot-sdk/issues/1833\n\n- Deprecate `CurrencyAdapter` and introduce `FungibleAdapter`\n- Deprecate `ToStakingPot` and replace usage with `ResolveTo`\n- Required creating a new `StakingPotAccountId` struct that implements\n`TypedGet` for the staking pot account ID\n- Update parachain common utils `DealWithFees`, `ToAuthor` and\n`AssetsToBlockAuthor` implementations to use `fungible`\n- Update runtime XCM Weight Traders to use `ResolveTo` instead of\n`ToStakingPot`\n- Update runtime Transaction Payment pallets to use `FungibleAdapter`\ninstead of `CurrencyAdapter`\n- [x] Blocked by https://github.com/paritytech/polkadot-sdk/pull/1296,\nneeds the `Unbalanced::decrease_balance` fix",
          "timestamp": "2024-04-04T13:56:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bda4e75ac49786a7246531cf729b25c208cd38e6"
        },
        "date": 1712245426605,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.939999999995,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010706485933333332,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012922830486666665,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15970154579333326,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026684787239999994,
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
          "id": "68cdb126498df7d5d212f8e10b514a79502f88da",
          "message": "Added support for coretime-kusama/polkadot and people-kusama/polkadot (#3961)\n\n## Running  `./polkadot-parachain --chain coretime-kusama` works now:\n\n**Parachain genesis state and header** match expected ones from\nhttps://gist.github.com/bkontur/f74fc00fd726d09bc7f0f3a9f51ec113?permalink_comment_id=5009857#gistcomment-5009857\n```\n2024-04-03 12:03:58 [Parachain] 🔨 Initializing Genesis block/state (state: 0xc418…889c, header-hash: 0x638c…d050) \n...\n2024-04-03 12:04:04 [Parachain] 💤 Idle (0 peers), best: #0 (0x638c…d050), finalized #0 (0x638c…d050)\n```\n\n**Relaychain genesis state and header** match expected ones:\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fkusama-rpc.polkadot.io#/explorer/query/0\n\n```\n2024-04-03 12:03:59 [Relaychain] 🔨 Initializing Genesis block/state (state: 0xb000…ef6b, header-hash: 0xb0a8…dafe)    \n```\n\n\n\n\n\n**Full logs:**\n```\nbparity@bkontur-ThinkPad-P14s-Gen-2i:~/parity/polkadot-sdk$ ./target/debug/polkadot-parachain --chain coretime-kusama\n2024-04-03 12:03:52 Polkadot parachain    \n2024-04-03 12:03:52 ✌️  version 4.0.0-665e3654cec    \n2024-04-03 12:03:52 ❤️  by Parity Technologies <admin@parity.io>, 2017-2024    \n2024-04-03 12:03:52 📋 Chain specification: Kusama Coretime    \n2024-04-03 12:03:52 🏷  Node name: subsequent-quicksand-2382    \n2024-04-03 12:03:52 👤 Role: FULL    \n2024-04-03 12:03:52 💾 Database: RocksDb at /home/bparity/.local/share/polkadot-parachain/chains/coretime-kusama/db/full    \n2024-04-03 12:03:54 Parachain id: Id(1005)    \n2024-04-03 12:03:54 Parachain Account: 5Ec4AhPakEiNWFbAd26nRrREnaGQZo3uukPDC5xLr6314Dwg    \n2024-04-03 12:03:54 Is collating: no    \n2024-04-03 12:03:58 [Parachain] 🔨 Initializing Genesis block/state (state: 0xc418…889c, header-hash: 0x638c…d050)    \n2024-04-03 12:03:59 [Relaychain] 🔨 Initializing Genesis block/state (state: 0xb000…ef6b, header-hash: 0xb0a8…dafe)    \n2024-04-03 12:03:59 [Relaychain] 👴 Loading GRANDPA authority set from genesis on what appears to be first startup.    \n2024-04-03 12:03:59 [Relaychain] 👶 Creating empty BABE epoch changes on what appears to be first startup.    \n2024-04-03 12:03:59 [Relaychain] 🏷  Local node identity is: 12D3KooWSfXNBZYimwSKBqfKf7F1X6adNQQD5HVQbdnvSyBFn8Wd    \n2024-04-03 12:03:59 [Relaychain] 💻 Operating system: linux    \n2024-04-03 12:03:59 [Relaychain] 💻 CPU architecture: x86_64    \n2024-04-03 12:03:59 [Relaychain] 💻 Target environment: gnu    \n2024-04-03 12:03:59 [Relaychain] 💻 CPU: 11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz    \n2024-04-03 12:03:59 [Relaychain] 💻 CPU cores: 4    \n2024-04-03 12:03:59 [Relaychain] 💻 Memory: 31797MB    \n2024-04-03 12:03:59 [Relaychain] 💻 Kernel: 5.15.0-101-generic    \n2024-04-03 12:03:59 [Relaychain] 💻 Linux distribution: Ubuntu 20.04.6 LTS    \n2024-04-03 12:03:59 [Relaychain] 💻 Virtual machine: no    \n2024-04-03 12:03:59 [Relaychain] 📦 Highest known block at #0    \n2024-04-03 12:03:59 [Relaychain] 〽️ Prometheus exporter started at 127.0.0.1:9616    \n2024-04-03 12:03:59 [Relaychain] Running JSON-RPC server: addr=127.0.0.1:9945, allowed origins=[\"http://localhost:*\", \"http://127.0.0.1:*\", \"https://localhost:*\", \"https://127.0.0.1:*\", \"https://polkadot.js.org\"]    \n2024-04-03 12:03:59 [Relaychain] 🏁 CPU score: 1.40 GiBs    \n2024-04-03 12:03:59 [Relaychain] 🏁 Memory score: 15.42 GiBs    \n2024-04-03 12:03:59 [Relaychain] 🏁 Disk score (seq. writes): 1.39 GiBs    \n2024-04-03 12:03:59 [Relaychain] 🏁 Disk score (rand. writes): 690.56 MiBs    \n2024-04-03 12:03:59 [Parachain] Using default protocol ID \"sup\" because none is configured in the chain specs    \n2024-04-03 12:03:59 [Parachain] 🏷  Local node identity is: 12D3KooWAAvNqXn8WPmvnEj36j7HsdbtpRpmWDPT9xtp4CuphvxW    \n2024-04-03 12:03:59 [Parachain] 💻 Operating system: linux    \n2024-04-03 12:03:59 [Parachain] 💻 CPU architecture: x86_64    \n2024-04-03 12:03:59 [Parachain] 💻 Target environment: gnu    \n2024-04-03 12:03:59 [Parachain] 💻 CPU: 11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz    \n2024-04-03 12:03:59 [Parachain] 💻 CPU cores: 4    \n2024-04-03 12:03:59 [Parachain] 💻 Memory: 31797MB    \n2024-04-03 12:03:59 [Parachain] 💻 Kernel: 5.15.0-101-generic    \n2024-04-03 12:03:59 [Parachain] 💻 Linux distribution: Ubuntu 20.04.6 LTS    \n2024-04-03 12:03:59 [Parachain] 💻 Virtual machine: no    \n2024-04-03 12:03:59 [Parachain] 📦 Highest known block at #0    \n2024-04-03 12:03:59 [Parachain] 〽️ Prometheus exporter started at 127.0.0.1:9615    \n2024-04-03 12:03:59 [Parachain] Running JSON-RPC server: addr=127.0.0.1:9944, allowed origins=[\"http://localhost:*\", \"http://127.0.0.1:*\", \"https://localhost:*\", \"https://127.0.0.1:*\", \"https://polkadot.js.org\"]    \n2024-04-03 12:03:59 [Parachain] 🏁 CPU score: 1.40 GiBs    \n2024-04-03 12:03:59 [Parachain] 🏁 Memory score: 15.42 GiBs    \n2024-04-03 12:03:59 [Parachain] 🏁 Disk score (seq. writes): 1.39 GiBs    \n2024-04-03 12:03:59 [Parachain] 🏁 Disk score (rand. writes): 690.56 MiBs    \n2024-04-03 12:03:59 [Parachain] discovered: 12D3KooWSfXNBZYimwSKBqfKf7F1X6adNQQD5HVQbdnvSyBFn8Wd /ip4/192.168.1.100/tcp/30334/ws    \n2024-04-03 12:03:59 [Relaychain] discovered: 12D3KooWAAvNqXn8WPmvnEj36j7HsdbtpRpmWDPT9xtp4CuphvxW /ip4/192.168.1.100/tcp/30333/ws    \n2024-04-03 12:03:59 [Relaychain] discovered: 12D3KooWAAvNqXn8WPmvnEj36j7HsdbtpRpmWDPT9xtp4CuphvxW /ip4/172.18.0.1/tcp/30333/ws    \n2024-04-03 12:03:59 [Parachain] discovered: 12D3KooWSfXNBZYimwSKBqfKf7F1X6adNQQD5HVQbdnvSyBFn8Wd /ip4/172.17.0.1/tcp/30334/ws    \n2024-04-03 12:03:59 [Relaychain] discovered: 12D3KooWAAvNqXn8WPmvnEj36j7HsdbtpRpmWDPT9xtp4CuphvxW /ip4/172.17.0.1/tcp/30333/ws    \n2024-04-03 12:03:59 [Parachain] discovered: 12D3KooWSfXNBZYimwSKBqfKf7F1X6adNQQD5HVQbdnvSyBFn8Wd /ip4/172.18.0.1/tcp/30334/ws    \n2024-04-03 12:04:00 [Relaychain] 🔍 Discovered new external address for our node: /ip4/178.41.176.246/tcp/30334/ws/p2p/12D3KooWSfXNBZYimwSKBqfKf7F1X6adNQQD5HVQbdnvSyBFn8Wd    \n2024-04-03 12:04:00 [Relaychain] Sending fatal alert BadCertificate    \n2024-04-03 12:04:00 [Relaychain] Sending fatal alert BadCertificate    \n2024-04-03 12:04:04 [Relaychain] ⚙️  Syncing, target=#22575321 (7 peers), best: #738 (0x1803…bbef), finalized #512 (0xb9b6…7014), ⬇ 328.5kiB/s ⬆ 102.9kiB/s    \n2024-04-03 12:04:04 [Parachain] 💤 Idle (0 peers), best: #0 (0x638c…d050), finalized #0 (0x638c…d050), ⬇ 0 ⬆ 0    \n2024-04-03 12:04:09 [Relaychain] ⚙️  Syncing 169.5 bps, target=#22575322 (8 peers), best: #1586 (0x405b…a8aa), finalized #1536 (0x55d1…fb04), ⬇ 232.3kiB/s ⬆ 55.9kiB/s    \n2024-04-03 12:04:09 [Parachain] 💤 Idle (0 peers), best: #0 (0x638c…d050), finalized #0 (0x638c…d050), ⬇ 0 ⬆ 0    \n2024-04-03 12:04:14 [Relaychain] ⚙️  Syncing 168.0 bps, target=#22575323 (8 peers), best: #2426 (0x155f…d083), finalized #2048 (0xede6…f879), ⬇ 235.8kiB/s ⬆ 67.2kiB/s    \n2024-04-03 12:04:14 [Parachain] 💤 Idle (0 peers), best: #0 (0x638c…d050), finalized #0 (0x638c…d050), ⬇ 0 ⬆ 0    \n2024-04-03 12:04:19 [Relaychain] ⚙️  Syncing 170.0 bps, target=#22575324 (8 peers), best: #3276 (0x94d8…097e), finalized #3072 (0x0e4c…f587), ⬇ 129.0kiB/s ⬆ 34.0kiB/s\n...\n```\n\n## Running  `./polkadot-parachain --chain people-kusama` works now:\n\n**Parachain genesis state and header** match expected ones from\nhttps://gist.github.com/bkontur/f74fc00fd726d09bc7f0f3a9f51ec113?permalink_comment_id=5011798#gistcomment-5011798\n```\n2024-04-04 10:26:24 [Parachain] 🔨 Initializing Genesis block/state (state: 0x023a…2733, header-hash: 0x07b8…2645)    \n...\n2024-04-04 10:26:30 [Parachain] 💤 Idle (0 peers), best: #0 (0x07b8…2645), finalized #0 (0x07b8…2645), ⬇ 0 ⬆ 0    \n```\n\n**Relaychain genesis state and header** match expected ones:\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fkusama-rpc.polkadot.io#/explorer/query/0\n\n```\n2024-04-04 10:26:25 [Relaychain] 🔨 Initializing Genesis block/state (state: 0xb000…ef6b, header-hash: 0xb0a8…dafe)  \n```\n\n\n\n\n\n**Full logs:**\n```\nbparity@bkontur-ThinkPad-P14s-Gen-2i:~/parity/aaa/polkadot-sdk$ ./target/debug/polkadot-parachain --chain people-kusama\n2024-04-04 10:26:18 Polkadot parachain    \n2024-04-04 10:26:18 ✌️  version 4.0.0-39274bb75fc    \n2024-04-04 10:26:18 ❤️  by Parity Technologies <admin@parity.io>, 2017-2024    \n2024-04-04 10:26:18 📋 Chain specification: Kusama People    \n2024-04-04 10:26:18 🏷  Node name: knotty-flight-5398    \n2024-04-04 10:26:18 👤 Role: FULL    \n2024-04-04 10:26:18 💾 Database: RocksDb at /home/bparity/.local/share/polkadot-parachain/chains/people-kusama/db/full    \n2024-04-04 10:26:21 Parachain id: Id(1004)    \n2024-04-04 10:26:21 Parachain Account: 5Ec4AhPaYcfBz8fMoPd4EfnAgwbzRS7np3APZUnnFo12qEYk    \n2024-04-04 10:26:21 Is collating: no    \n2024-04-04 10:26:24 [Parachain] 🔨 Initializing Genesis block/state (state: 0x023a…2733, header-hash: 0x07b8…2645)    \n2024-04-04 10:26:25 [Relaychain] 🔨 Initializing Genesis block/state (state: 0xb000…ef6b, header-hash: 0xb0a8…dafe)    \n2024-04-04 10:26:25 [Relaychain] 👴 Loading GRANDPA authority set from genesis on what appears to be first startup.    \n2024-04-04 10:26:25 [Relaychain] 👶 Creating empty BABE epoch changes on what appears to be first startup.    \n2024-04-04 10:26:25 [Relaychain] 🏷  Local node identity is: 12D3KooWPoTVhnrFNzVYJPR42HE9rYjXhkKHFDL9ut5nafDqJHKB    \n2024-04-04 10:26:25 [Relaychain] 💻 Operating system: linux    \n2024-04-04 10:26:25 [Relaychain] 💻 CPU architecture: x86_64    \n2024-04-04 10:26:25 [Relaychain] 💻 Target environment: gnu    \n2024-04-04 10:26:25 [Relaychain] 💻 CPU: 11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz    \n2024-04-04 10:26:25 [Relaychain] 💻 CPU cores: 4    \n2024-04-04 10:26:25 [Relaychain] 💻 Memory: 31797MB    \n2024-04-04 10:26:25 [Relaychain] 💻 Kernel: 5.15.0-101-generic    \n2024-04-04 10:26:25 [Relaychain] 💻 Linux distribution: Ubuntu 20.04.6 LTS    \n2024-04-04 10:26:25 [Relaychain] 💻 Virtual machine: no    \n2024-04-04 10:26:25 [Relaychain] 📦 Highest known block at #0    \n2024-04-04 10:26:25 [Relaychain] 〽️ Prometheus exporter started at 127.0.0.1:9616    \n2024-04-04 10:26:25 [Relaychain] Running JSON-RPC server: addr=127.0.0.1:9945, allowed origins=[\"http://localhost:*\", \"http://127.0.0.1:*\", \"https://localhost:*\", \"https://127.0.0.1:*\", \"https://polkadot.js.org\"]    \n2024-04-04 10:26:25 [Relaychain] 🏁 CPU score: 1.18 GiBs    \n2024-04-04 10:26:25 [Relaychain] 🏁 Memory score: 15.61 GiBs    \n2024-04-04 10:26:25 [Relaychain] 🏁 Disk score (seq. writes): 1.49 GiBs    \n2024-04-04 10:26:25 [Relaychain] 🏁 Disk score (rand. writes): 650.01 MiBs    \n2024-04-04 10:26:25 [Parachain] Using default protocol ID \"sup\" because none is configured in the chain specs    \n2024-04-04 10:26:25 [Parachain] 🏷  Local node identity is: 12D3KooWS2WPQgtiZZYT6bLGjwGcJU7QVd5EeQvb4jHN3NVSWDdj    \n2024-04-04 10:26:25 [Parachain] 💻 Operating system: linux    \n2024-04-04 10:26:25 [Parachain] 💻 CPU architecture: x86_64    \n2024-04-04 10:26:25 [Parachain] 💻 Target environment: gnu    \n2024-04-04 10:26:25 [Parachain] 💻 CPU: 11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz    \n2024-04-04 10:26:25 [Parachain] 💻 CPU cores: 4    \n2024-04-04 10:26:25 [Parachain] 💻 Memory: 31797MB    \n2024-04-04 10:26:25 [Parachain] 💻 Kernel: 5.15.0-101-generic    \n2024-04-04 10:26:25 [Parachain] 💻 Linux distribution: Ubuntu 20.04.6 LTS    \n2024-04-04 10:26:25 [Parachain] 💻 Virtual machine: no    \n2024-04-04 10:26:25 [Parachain] 📦 Highest known block at #0    \n2024-04-04 10:26:25 [Parachain] 〽️ Prometheus exporter started at 127.0.0.1:9615    \n2024-04-04 10:26:25 [Parachain] Running JSON-RPC server: addr=127.0.0.1:9944, allowed origins=[\"http://localhost:*\", \"http://127.0.0.1:*\", \"https://localhost:*\", \"https://127.0.0.1:*\", \"https://polkadot.js.org\"]    \n2024-04-04 10:26:25 [Parachain] 🏁 CPU score: 1.18 GiBs    \n2024-04-04 10:26:25 [Parachain] 🏁 Memory score: 15.61 GiBs    \n2024-04-04 10:26:25 [Parachain] 🏁 Disk score (seq. writes): 1.49 GiBs    \n2024-04-04 10:26:25 [Parachain] 🏁 Disk score (rand. writes): 650.01 MiBs    \n2024-04-04 10:26:25 [Parachain] discovered: 12D3KooWPoTVhnrFNzVYJPR42HE9rYjXhkKHFDL9ut5nafDqJHKB /ip4/172.17.0.1/tcp/30334/ws    \n2024-04-04 10:26:25 [Relaychain] discovered: 12D3KooWS2WPQgtiZZYT6bLGjwGcJU7QVd5EeQvb4jHN3NVSWDdj /ip4/172.18.0.1/tcp/30333/ws    \n2024-04-04 10:26:25 [Relaychain] discovered: 12D3KooWS2WPQgtiZZYT6bLGjwGcJU7QVd5EeQvb4jHN3NVSWDdj /ip4/192.168.1.100/tcp/30333/ws    \n2024-04-04 10:26:25 [Parachain] discovered: 12D3KooWPoTVhnrFNzVYJPR42HE9rYjXhkKHFDL9ut5nafDqJHKB /ip4/172.18.0.1/tcp/30334/ws    \n2024-04-04 10:26:25 [Relaychain] discovered: 12D3KooWS2WPQgtiZZYT6bLGjwGcJU7QVd5EeQvb4jHN3NVSWDdj /ip4/172.17.0.1/tcp/30333/ws    \n2024-04-04 10:26:25 [Parachain] discovered: 12D3KooWPoTVhnrFNzVYJPR42HE9rYjXhkKHFDL9ut5nafDqJHKB /ip4/192.168.1.100/tcp/30334/ws    \n2024-04-04 10:26:26 [Relaychain] 🔍 Discovered new external address for our node: /ip4/178.41.176.246/tcp/30334/ws/p2p/12D3KooWPoTVhnrFNzVYJPR42HE9rYjXhkKHFDL9ut5nafDqJHKB    \n2024-04-04 10:26:27 [Relaychain] Sending fatal alert BadCertificate    \n2024-04-04 10:26:27 [Relaychain] Sending fatal alert BadCertificate    \n2024-04-04 10:26:30 [Relaychain] ⚙️  Syncing, target=#22588722 (8 peers), best: #638 (0xa9cd…7c30), finalized #512 (0xb9b6…7014), ⬇ 345.6kiB/s ⬆ 108.7kiB/s    \n2024-04-04 10:26:30 [Parachain] 💤 Idle (0 peers), best: #0 (0x07b8…2645), finalized #0 (0x07b8…2645), ⬇ 0 ⬆ 0    \n2024-04-04 10:26:35 [Relaychain] ⚙️  Syncing 174.4 bps, target=#22588722 (9 peers), best: #1510 (0xec0b…72f0), finalized #1024 (0x3f17…fd7f), ⬇ 203.1kiB/s ⬆ 45.0kiB/s    \n2024-04-04 10:26:35 [Parachain] 💤 Idle (0 peers), best: #0 (0x07b8…2645), finalized #0 (0x07b8…2645), ⬇ 0 ⬆ 0    \n2024-04-04 10:26:40 [Relaychain] ⚙️  Syncing 168.9 bps, target=#22588723 (9 peers), best: #2355 (0xa68b…3a64), finalized #2048 (0xede6…f879), ⬇ 201.6kiB/s ⬆ 47.4kiB/s    \n2024-04-04 10:26:40 [Parachain] 💤 Idle (0 peers), best: #0 (0x07b8…2645), finalized #0 (0x07b8…2645), ⬇ 0 ⬆ 0    \n\n```\n\n## TODO\n- [x] double check\n`cumulus/polkadot-parachain/chain-specs/coretime-kusama.json`\n(safeXcmVersion=3) see\n[comment](https://github.com/paritytech/polkadot-sdk/pull/3961#discussion_r1549473587)\n- [x] check if ~~`start_generic_aura_node`~~ or\n`start_generic_aura_lookahead_node`\n- [x] generate chain-spec for `people-kusama`\n\n---------\n\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2024-04-04T15:26:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/68cdb126498df7d5d212f8e10b514a79502f88da"
        },
        "date": 1712248775316,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.919999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026682451593333337,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15872251818000002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010586297633333337,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012656740113333328,
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
          "id": "f910a15c1ca7255457a6db17ced5bf9c525ec5f0",
          "message": "`GenesisConfig` presets for runtime (#2714)\n\nThe runtime now can provide a number of predefined presets of\n`RuntimeGenesisConfig` struct. This presets are intended to be used in\ndifferent deployments, e.g.: `local`, `staging`, etc, and should be\nincluded into the corresponding chain-specs.\n\nHaving `GenesisConfig` presets in runtime allows to fully decouple node\nfrom runtime types (the problem is described in #1984).\n\n**Summary of changes:**\n- The `GenesisBuilder` API was adjusted to enable this functionality\n(and provide better naming - #150):\n   ```rust\n    fn preset_names() -> Vec<PresetId>;\nfn get_preset(id: Option<PresetId>) -> Option<serde_json::Value>;\n//`None` means default\n    fn build_state(value: serde_json::Value);\n    pub struct PresetId(Vec<u8>);\n   ```\n\n- **Breaking change**: Old `create_default_config` method was removed,\n`build_config` was renamed to `build_state`. As a consequence a node\nwon't be able to interact with genesis config for older runtimes. The\ncleanup was made for sake of API simplicity. Also IMO maintaining\ncompatibility with old API is not so crucial.\n- Reference implementation was provided for `substrate-test-runtime` and\n`rococo` runtimes. For rococo new\n[`genesis_configs_presets`](https://github.com/paritytech/polkadot-sdk/blob/3b41d66b97c5ff0ec4a1989da5ffd8b9f3f588e3/polkadot/runtime/rococo/src/genesis_config_presets.rs#L530)\nmodule was added and is used in `GenesisBuilder`\n[_presets-related_](https://github.com/paritytech/polkadot-sdk/blob/3b41d66b97c5ff0ec4a1989da5ffd8b9f3f588e3/polkadot/runtime/rococo/src/lib.rs#L2462-L2485)\nmethods.\n\n- The `chain-spec-builder` util was also improved and allows to\n([_doc_](https://github.com/paritytech/polkadot-sdk/blob/3b41d66b97c5ff0ec4a1989da5ffd8b9f3f588e3/substrate/bin/utils/chain-spec-builder/src/lib.rs#L19)):\n   - list presets provided by given runtime (`list-presets`),\n- display preset or default config provided by the runtime\n(`display-preset`),\n   - build chain-spec using named preset (`create ... named-preset`),\n\n\n- The `ChainSpecBuilder` is extended with\n[`with_genesis_config_preset_name`](https://github.com/paritytech/polkadot-sdk/blob/3b41d66b97c5ff0ec4a1989da5ffd8b9f3f588e3/substrate/client/chain-spec/src/chain_spec.rs#L447)\nmethod which allows to build chain-spec using named preset provided by\nthe runtime. Sample usage on the node side\n[here](https://github.com/paritytech/polkadot-sdk/blob/2caffaae803e08a3d5b46c860e8016da023ff4ce/polkadot/node/service/src/chain_spec.rs#L404).\n\nImplementation of #1984.\nfixes: #150\npart of: #25\n\n---------\n\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-04T18:30:54Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f910a15c1ca7255457a6db17ced5bf9c525ec5f0"
        },
        "date": 1712260093630,
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
            "name": "bitfield-distribution",
            "value": 0.026749448153333327,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15720154026666658,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010831808406666673,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01331581933333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ermal Kaleci",
            "username": "ermalkaleci",
            "email": "ermalkaleci@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5fb4397810e167a251d4c909e06784564452e56f",
          "message": "Update pr_3844.prdoc (#3988)",
          "timestamp": "2024-04-04T23:07:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5fb4397810e167a251d4c909e06784564452e56f"
        },
        "date": 1712276178423,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.94,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012078573886666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009898503726666672,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026136980199999996,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15402689443999998,
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
          "id": "cb0748b6ecb84d16f595bfdaf7a98fb46aa5c590",
          "message": "Revert \"[prdoc] Require SemVer bump level\" (#3987)\n\nReverts paritytech/polkadot-sdk#3816",
          "timestamp": "2024-04-05T09:51:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cb0748b6ecb84d16f595bfdaf7a98fb46aa5c590"
        },
        "date": 1712316394995,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.886666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.009836394480000005,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15225480829333338,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012012081966666664,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026239054646666662,
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
          "id": "03e9dd77e945433a1835813b49492eb9a045cb64",
          "message": "Update pr_3302.prdoc (#3985)\n\nProperly account for #3302, cc #3984.",
          "timestamp": "2024-04-05T11:53:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/03e9dd77e945433a1835813b49492eb9a045cb64"
        },
        "date": 1712322349605,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.88666666666,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010530766260000006,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15477605183333332,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012650191979999998,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026656638060000005,
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
          "id": "ba0f8de0c74b1308a53942c406137a748ef79925",
          "message": "[pallet-broker] Fix claim revenue behaviour for zero timeslices (#3997)\n\nThis PR adds a check that `max_timeslices > 0` and errors if not. It\nalso adds a test for this behaviour and cleans up some misleading docs.",
          "timestamp": "2024-04-05T13:18:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ba0f8de0c74b1308a53942c406137a748ef79925"
        },
        "date": 1712329100159,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.92666666666,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15965045178,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013722357560000002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01129640833333334,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026859770153333337,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alessandro Siniscalchi",
            "username": "asiniscalchi",
            "email": "asiniscalchi@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "33bbdb3cae5911562edbeb55a53549650e66f3e1",
          "message": "[parachain-template] benchmarks into `mod benchmarks` (#3818)\n\nThis PR introduces a dedicated module for benchmarks within the\nparachain runtime. By segregating benchmarks into their own module, we\nachieve a cleaner project structure and improved readability,\nfacilitating easier maintenance and updates.\n\n### Key Changes:\n- **New Benchmarks Module**: A new file `benchmarks.rs` is added,\nencapsulating the benchmarking code for various pallets.\n- **Refactoring `lib.rs`**: The main runtime library file (`lib.rs`) has\nbeen updated to reflect the extraction of benchmark definitions. By\nmoving these definitions to `benchmarks.rs`, we reduce clutter in\n`lib.rs`, streamlining the runtime's core logic and configuration.\n\n### Benefits of This Refactoring:\n- **Focused Benchmarking**: Developers can now easily locate and modify\nbenchmarks without navigating through the core runtime logic, enabling\ntargeted performance improvements.\n- **Cleaner Codebase**: Segregating benchmarks from the main runtime\nlogic helps maintain a clean, well-organized codebase, simplifying\nnavigation and maintenance.\n- **Scalability**: As the parachain evolves, adding or updating\nbenchmarks becomes more straightforward, supporting scalability and\nadaptability of the runtime.\n\n### Summary of Changes:\n- Created `benchmarks.rs` to house the benchmarking suite.\n- Streamlined `lib.rs` by removing the inlined benchmark definitions and\nlinking to the new benchmarks module.\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-05T14:46:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/33bbdb3cae5911562edbeb55a53549650e66f3e1"
        },
        "date": 1712332828394,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.893333333326,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012221901679999998,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1530291393200001,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026269724486666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009805219900000009,
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
          "id": "0832f0f36db3ff04545655f3c33bea03dc161987",
          "message": "Rococo/Westend: publish `claim_queue` Runtime API  (#4005)\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-05T15:50:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0832f0f36db3ff04545655f3c33bea03dc161987"
        },
        "date": 1712336610616,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.94,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.0126298902,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02644090448000001,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15522322723333334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010642610940000005,
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
          "id": "05b97068f9a440f89246c5fdea532fda369e7794",
          "message": "Bump h2 from 0.3.24 to 0.3.26 (#4008)\n\nBumps [h2](https://github.com/hyperium/h2) from 0.3.24 to 0.3.26.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/hyperium/h2/releases\">h2's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v0.3.26</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Limit number of CONTINUATION frames for misbehaving\nconnections.</li>\n</ul>\n<p>See <a\nhref=\"https://seanmonstar.com/blog/hyper-http2-continuation-flood/\">https://seanmonstar.com/blog/hyper-http2-continuation-flood/</a>\nfor more info.</p>\n<h2>v0.3.25</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>perf: optimize header list size calculations by <a\nhref=\"https://github.com/Noah-Kennedy\"><code>@​Noah-Kennedy</code></a>\nin <a\nhref=\"https://redirect.github.com/hyperium/h2/pull/750\">hyperium/h2#750</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/hyperium/h2/compare/v0.3.24...v0.3.25\">https://github.com/hyperium/h2/compare/v0.3.24...v0.3.25</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/hyperium/h2/blob/v0.3.26/CHANGELOG.md\">h2's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>0.3.26 (April 3, 2024)</h1>\n<ul>\n<li>Limit number of CONTINUATION frames for misbehaving\nconnections.</li>\n</ul>\n<h1>0.3.25 (March 15, 2024)</h1>\n<ul>\n<li>Improve performance decoding many headers.</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/hyperium/h2/commit/357127e279c06935830fe2140378312eac801494\"><code>357127e</code></a>\nv0.3.26</li>\n<li><a\nhref=\"https://github.com/hyperium/h2/commit/1a357aaefc7243fdfa9442f45d90be17794a4004\"><code>1a357aa</code></a>\nfix: limit number of CONTINUATION frames allowed</li>\n<li><a\nhref=\"https://github.com/hyperium/h2/commit/5b6c9e0da092728d702dff3607626aafb7809d77\"><code>5b6c9e0</code></a>\nrefactor: cleanup new unused warnings (<a\nhref=\"https://redirect.github.com/hyperium/h2/issues/757\">#757</a>)</li>\n<li><a\nhref=\"https://github.com/hyperium/h2/commit/3a798327211345b9b2bf797e2e4f3aca4e0ddfee\"><code>3a79832</code></a>\nv0.3.25</li>\n<li><a\nhref=\"https://github.com/hyperium/h2/commit/94e80b1c72bec282bb5d13596803e6fb341fec4c\"><code>94e80b1</code></a>\nperf: optimize header list size calculations (<a\nhref=\"https://redirect.github.com/hyperium/h2/issues/750\">#750</a>)</li>\n<li>See full diff in <a\nhref=\"https://github.com/hyperium/h2/compare/v0.3.24...v0.3.26\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=h2&package-manager=cargo&previous-version=0.3.24&new-version=0.3.26)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\nYou can disable automated security fix PRs for this repo from the\n[Security Alerts\npage](https://github.com/paritytech/polkadot-sdk/network/alerts).\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-04-05T18:37:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/05b97068f9a440f89246c5fdea532fda369e7794"
        },
        "date": 1712348753600,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.933333333334,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026783752100000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010619995000000009,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012661202060000003,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15573807504,
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
          "id": "1c85bfe901741f9c456af1ac92008d647660a2f4",
          "message": "Broker: sale price runtime api (#3485)\n\nDefines a runtime api for `pallet-broker` for getting the current price\nof a core if there is an ongoing sale.\n\nCloses: #3413\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-05T23:29:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1c85bfe901741f9c456af1ac92008d647660a2f4"
        },
        "date": 1712363940562,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.893333333326,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026142778859999997,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15066404032000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009210010906666672,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011864675520000001,
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
          "id": "9d6261892814fa27c97881c0321c008d7340b54b",
          "message": "`pallet-uniques`: decrement `total_deposit` when clearing collection metadata (#3976)\n\nDecrements `total_deposit` when collection metadata is cleared in\n`pallet-nfts` and `pallet-uniques`.",
          "timestamp": "2024-04-06T05:10:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9d6261892814fa27c97881c0321c008d7340b54b"
        },
        "date": 1712384529907,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.92666666666,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16078891267333334,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02695004634,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011440896353333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01397933785333333,
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
          "id": "74d6309c0cecf0636edd729365e7b723a62c8c72",
          "message": "Improve frame umbrella crate doc experience (#4007)\n\n1. Add `#[doc(no_inline)]` to frame umbrella crate re-exports that\neventually resolve to `frame_support_procedural` so docs don't look like\nthe screenshot below and instead link to the proper `frame-support`\ndocs.\n<img width=\"1512\" alt=\"Screenshot 2024-04-05 at 20 05 01\"\nsrc=\"https://github.com/paritytech/polkadot-sdk/assets/16665596/a41daa4c-ebca-44a4-9fea-f9f336314e13\">\n\n\n2. Remove `\"Rust-Analyzer Users: \"` prefix from\n`frame_support_procedural` doc comments, since these doc comments are\nvisible in the web documentation and possible to stumble upon especially\nwhen navigating from the frame umbrella crate.\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-06T10:02:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/74d6309c0cecf0636edd729365e7b723a62c8c72"
        },
        "date": 1712402172219,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666662,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.011230859346666676,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1590249008133333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02692720411333332,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013517542339999999,
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
          "id": "994003854363b3c5fd0d7343f93aa1e54edb1ad0",
          "message": "Major bump of tracing-subscriber version (#3891)\n\nI don't think there are any more releases to the 0.2.x versions, so best\nwe're on the 0.3.x release.\n\nNo change on the benchmarks, fast local time is still just as fast as\nbefore:\n\nnew version bench:\n```\nfast_local_time         time:   [30.551 ns 30.595 ns 30.668 ns]\n```\n\nold version bench:\n```\nfast_local_time         time:   [30.598 ns 30.646 ns 30.723 ns]\n```\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-06T13:54:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/994003854363b3c5fd0d7343f93aa1e54edb1ad0"
        },
        "date": 1712415963140,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026182386566666657,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1495670546733333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009092916380000005,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011951857933333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "HongKuang",
            "username": "HongKuang",
            "email": "166261675+HongKuang@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "bd4471b4fcf46123df6115b70b32e47276a7ae60",
          "message": "Fix some typos (#4018)\n\nSigned-off-by: hongkuang <liurenhong@outlook.com>",
          "timestamp": "2024-04-08T04:21:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bd4471b4fcf46123df6115b70b32e47276a7ae60"
        },
        "date": 1712554575103,
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
            "value": 0.027104517933333332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011816775973333336,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014098499620000007,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16054742537333333,
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
          "id": "59f868d1e9502cb4e434127cac6e439d01d7dd2b",
          "message": "Deprecate `para_id()` from `CoreState` in polkadot primitives (#3979)\n\nWith Coretime enabled we can no longer assume there is a static 1:1\nmapping between core index and para id. This mapping should be obtained\nfrom the scheduler/claimqueue on block by block basis.\n\nThis PR modifies `para_id()` (from `CoreState`) to return the scheduled\n`ParaId` for occupied cores and removes its usages in the code.\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/3948\n\n---------\n\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2024-04-08T05:58:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/59f868d1e9502cb4e434127cac6e439d01d7dd2b"
        },
        "date": 1712560600198,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.919999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012468067506666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010013972960000008,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026229283526666678,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15141726379999995,
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
          "id": "c1063a530e46c4af0a934ca50422b69182255e60",
          "message": "sc-beefy-consensus: Remove unneeded stream. (#4015)\n\nThe stream was just used to communicate from the validator the peer\nreports back to the gossip engine. Internally the gossip engine just\nforwards these reports to the networking engine. So, we can just do this\ndirectly.\n\nThe reporting stream was also pumped [in the worker behind the\nengine](https://github.com/paritytech/polkadot-sdk/blob/9d6261892814fa27c97881c0321c008d7340b54b/substrate/client/consensus/beefy/src/worker.rs#L939).\nThis means if there was a lot of data incoming over the engine, the\nreporting stream was almost never processed and thus, it could have\nstarted to grow and we have seen issues around this.\n\nPartly Closes: https://github.com/paritytech/polkadot-sdk/issues/3945",
          "timestamp": "2024-04-08T08:28:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c1063a530e46c4af0a934ca50422b69182255e60"
        },
        "date": 1712569310311,
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
            "name": "test-environment",
            "value": 0.008796588453333339,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02608197736000001,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.14900762522,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01185666667333334,
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
          "id": "fdb1dba2e1eded282ff2eaf745d55c378f777cc2",
          "message": "Add best block indicator to informant message + print parent block on import  message (#4021)\n\nSometimes you need to debug some issues just by the logs and reconstruct\nwhat happened.\nIn these scenarios it would be nice to know if a block was imported as\nbest block, and what it parent was.\nSo here I propose to change the output of the informant to this:\n\n```\n2024-04-05 20:38:22.004  INFO ⋮substrate: [Parachain] ✨ Imported #18 (0xe7b3…4555 -> 0xbd6f…ced7)    \n2024-04-05 20:38:24.005  INFO ⋮substrate: [Parachain] ✨ Imported #19 (0xbd6f…ced7 -> 0x4dd0…d81f)    \n2024-04-05 20:38:24.011  INFO ⋮substrate: [jobless-children-5352] 🌟 Imported #42 (0xed2e…27fc -> 0x718f…f30e)    \n2024-04-05 20:38:26.005  INFO ⋮substrate: [Parachain] ✨ Imported #20 (0x4dd0…d81f -> 0x6e85…e2b8)    \n2024-04-05 20:38:28.004  INFO ⋮substrate: [Parachain] 🌟 Imported #21 (0x6e85…e2b8 -> 0xad53…2a97)    \n2024-04-05 20:38:30.004  INFO ⋮substrate: [Parachain] 🌟 Imported #22 (0xad53…2a97 -> 0xa874…890f)    \n```\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-08T13:30:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fdb1dba2e1eded282ff2eaf745d55c378f777cc2"
        },
        "date": 1712584862378,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.88666666666,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012419132773333326,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010075397526666672,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026427119500000002,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1543802726133333,
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
          "id": "216509dbaa2c2941ee75fbcc9a086deab5e2c8a6",
          "message": "Github workflow to automate release draft creation (#3978)\n\nThis PR introduces the github flow which will create a release draft\nautomatically when the rc tag is pushed. The flow contains the following\nsteps:\n\n- Gets the info about rust version used to build the node\n- Builds the runtimes using `srtool`\n- Extracts the info about each runtime \n- Aggregates the changelog from the prdocs\n- Creates the release draft containing all the info related to the\nrelease (changelog, runtimes, rust versions)\n- Attaches the runtimes to the draft\n- Posts the message to the RelEng internal channel to inform that the\nbuild is done.\n\nRelated to the #3295\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-08T13:36:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/216509dbaa2c2941ee75fbcc9a086deab5e2c8a6"
        },
        "date": 1712590548661,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.919999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012582067740000001,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026498723279999997,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15519013999333336,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010327803820000012,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Aaro Altonen",
            "username": "altonen",
            "email": "48052676+altonen@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "80616f6d03661106326b621e9cc3ee1d2fa283ed",
          "message": "Integrate litep2p into Polkadot SDK (#2944)\n\n[litep2p](https://github.com/altonen/litep2p) is a libp2p-compatible P2P\nnetworking library. It supports all of the features of `rust-libp2p`\nthat are currently being utilized by Polkadot SDK.\n\nCompared to `rust-libp2p`, `litep2p` has a quite different architecture\nwhich is why the new `litep2p` network backend is only able to use a\nlittle of the existing code in `sc-network`. The design has been mainly\ninfluenced by how we'd wish to structure our networking-related code in\nPolkadot SDK: independent higher-levels protocols directly communicating\nwith the network over links that support bidirectional backpressure. A\ngood example would be `NotificationHandle`/`RequestResponseHandle`\nabstractions which allow, e.g., `SyncingEngine` to directly communicate\nwith peers to announce/request blocks.\n\nI've tried running `polkadot --network-backend litep2p` with a few\ndifferent peer configurations and there is a noticeable reduction in\nnetworking CPU usage. For high load (`--out-peers 200`), networking CPU\nusage goes down from ~110% to ~30% (80 pp) and for normal load\n(`--out-peers 40`), the usage goes down from ~55% to ~18% (37 pp).\n\nThese should not be taken as final numbers because:\n\na) there are still some low-hanging optimization fruits, such as\nenabling [receive window\nauto-tuning](https://github.com/libp2p/rust-yamux/pull/176), integrating\n`Peerset` more closely with `litep2p` or improving memory usage of the\nWebSocket transport\nb) fixing bugs/instabilities that incorrectly cause `litep2p` to do less\nwork will increase the networking CPU usage\nc) verification in a more diverse set of tests/conditions is needed\n\nNevertheless, these numbers should give an early estimate for CPU usage\nof the new networking backend.\n\nThis PR consists of three separate changes:\n* introduce a generic `PeerId` (wrapper around `Multihash`) so that we\ndon't have use `NetworkService::PeerId` in every part of the code that\nuses a `PeerId`\n* introduce `NetworkBackend` trait, implement it for the libp2p network\nstack and make Polkadot SDK generic over `NetworkBackend`\n  * implement `NetworkBackend` for litep2p\n\nThe new library should be considered experimental which is why\n`rust-libp2p` will remain as the default option for the time being. This\nPR currently depends on the master branch of `litep2p` but I'll cut a\nnew release for the library once all review comments have been\naddresses.\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Dmitry Markin <dmitry@markin.tech>\nCo-authored-by: Alexandru Vasile <60601340+lexnv@users.noreply.github.com>\nCo-authored-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-08T16:44:13Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/80616f6d03661106326b621e9cc3ee1d2fa283ed"
        },
        "date": 1712599174098,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.919999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012570174786666668,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026756550120000003,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15879615969333336,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010719966380000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Léa Narzis",
            "username": "lean-apple",
            "email": "78718413+lean-apple@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d733c77ee2d2e8e2d5205c552a5efb2e5b5242c8",
          "message": "Adapt `RemoteExternalities` and its related types to be used with generic hash parameters (#3953)\n\nCloses  https://github.com/paritytech/polkadot-sdk/issues/3737\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-08T21:56:41Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d733c77ee2d2e8e2d5205c552a5efb2e5b5242c8"
        },
        "date": 1712617911884,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.88666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013666376760000003,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02696124142666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16148127203333337,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011503686333333341,
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
          "id": "9d6c0f446a0b3d5774a2d667b67ecce2d4655209",
          "message": "Removed unused deps from Snowbridge deps (#4029)\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-09T08:45:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9d6c0f446a0b3d5774a2d667b67ecce2d4655209"
        },
        "date": 1712657052560,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15522435239333332,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026236622840000008,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010162720766666671,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012250314580000001,
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
          "id": "a26d25d5c75d4feb02250f775cd162cc4952d8d2",
          "message": "Detect closed notification substreams instead of evicting all peers (#3983)\n\nThis PR brings the fix\nhttps://github.com/paritytech/substrate/pull/13396 to polkadot-sdk.\n\nIn the past, due to insufficient inbound slot count on polkadot &\nkusama, this fix led to low peer count. The situation has improved since\nthen after changing the default ratio between `--in-peers` &\n`--out-peers`.\n\nNevertheless, it's expected that the reported total peer count with this\nfix is going to be lower than without it. This should be seen as the\ncorrect number of working connections reported, as opposed to also\nreporting already closed connections, and not as lower count of working\nconnections with peers.\n\nThis PR also removes the peer eviction mechanism, as closed substream\ndetection is a more granular way of detecting peers that stopped syncing\nwith us.\n\nThe burn-in has been already performed as part of testing these changes\nin https://github.com/paritytech/polkadot-sdk/pull/3426.\n\n---------\n\nCo-authored-by: Aaro Altonen <a.altonen@hotmail.com>",
          "timestamp": "2024-04-09T12:40:52Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a26d25d5c75d4feb02250f775cd162cc4952d8d2"
        },
        "date": 1712668021624,
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
            "value": 0.011941202213333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.14837780000666673,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009245991313333346,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.025657211680000003,
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
          "id": "a26d25d5c75d4feb02250f775cd162cc4952d8d2",
          "message": "Detect closed notification substreams instead of evicting all peers (#3983)\n\nThis PR brings the fix\nhttps://github.com/paritytech/substrate/pull/13396 to polkadot-sdk.\n\nIn the past, due to insufficient inbound slot count on polkadot &\nkusama, this fix led to low peer count. The situation has improved since\nthen after changing the default ratio between `--in-peers` &\n`--out-peers`.\n\nNevertheless, it's expected that the reported total peer count with this\nfix is going to be lower than without it. This should be seen as the\ncorrect number of working connections reported, as opposed to also\nreporting already closed connections, and not as lower count of working\nconnections with peers.\n\nThis PR also removes the peer eviction mechanism, as closed substream\ndetection is a more granular way of detecting peers that stopped syncing\nwith us.\n\nThe burn-in has been already performed as part of testing these changes\nin https://github.com/paritytech/polkadot-sdk/pull/3426.\n\n---------\n\nCo-authored-by: Aaro Altonen <a.altonen@hotmail.com>",
          "timestamp": "2024-04-09T12:40:52Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a26d25d5c75d4feb02250f775cd162cc4952d8d2"
        },
        "date": 1712670813531,
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
            "value": 0.009852582280000004,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011926104120000001,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.025696016626666665,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15158539683333336,
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
          "id": "598e95577dbf10c4d940393c29a6476be91d8fd2",
          "message": "rpc-v2/transaction: Generate `Invalid` events and add tests (#3784)\n\nThis PR ensures that the transaction API generates an `Invalid` events\nfor transaction bytes that fail to decode.\n\nThe spec mentioned the `Invalid` event at the jsonrpc error section,\nhowever this spec PR makes things clearer:\n- https://github.com/paritytech/json-rpc-interface-spec/pull/146\n\nWhile at it have discovered an inconsistency with the generated events.\nThe drop event from the transaction pool was incorrectly mapped to the\n`invalid` event.\n\nAdded tests for the API stabilize the API soon:\n- https://github.com/paritytech/json-rpc-interface-spec/pull/144\n\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/3083\n\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-09T13:57:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/598e95577dbf10c4d940393c29a6476be91d8fd2"
        },
        "date": 1712675745811,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.893333333333,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.011632582920000002,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16061375685333332,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014018964333333326,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026503171779999998,
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
          "id": "df818d2974e059008cd8fa531f70b6657787b5be",
          "message": "Move cumulus zombienet tests to aura & async backing (#3568)\n\nCumulus test-parachain node and test runtime were still using relay\nchain consensus and 12s blocktimes. With async backing around the corner\non the major chains we should switch our tests too.\n\nAlso needed to nicely test the changes coming to collators in #3168.\n\n### Changes Overview\n- Followed the [migration\nguide](https://wiki.polkadot.network/docs/maintain-guides-async-backing)\nfor async backing for the cumulus-test-runtime\n- Adjusted the cumulus-test-service to use the correct import-queue,\nlookahead collator etc.\n- The block validation function now uses the Aura Ext Executor so that\nthe seal of the block is validated\n- Previous point requires that we seal block before calling into\n`validate_block`, I introduced a helper function for that\n- Test client adjusted to provide a slot to the relay chain proof and\nthe aura pre-digest",
          "timestamp": "2024-04-09T16:53:30Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/df818d2974e059008cd8fa531f70b6657787b5be"
        },
        "date": 1712696811712,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.873333333322,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.01369034497333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1624092325133333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011186242593333338,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02594547025333333,
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
          "id": "df818d2974e059008cd8fa531f70b6657787b5be",
          "message": "Move cumulus zombienet tests to aura & async backing (#3568)\n\nCumulus test-parachain node and test runtime were still using relay\nchain consensus and 12s blocktimes. With async backing around the corner\non the major chains we should switch our tests too.\n\nAlso needed to nicely test the changes coming to collators in #3168.\n\n### Changes Overview\n- Followed the [migration\nguide](https://wiki.polkadot.network/docs/maintain-guides-async-backing)\nfor async backing for the cumulus-test-runtime\n- Adjusted the cumulus-test-service to use the correct import-queue,\nlookahead collator etc.\n- The block validation function now uses the Aura Ext Executor so that\nthe seal of the block is validated\n- Previous point requires that we seal block before calling into\n`validate_block`, I introduced a helper function for that\n- Test client adjusted to provide a slot to the relay chain proof and\nthe aura pre-digest",
          "timestamp": "2024-04-09T16:53:30Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/df818d2974e059008cd8fa531f70b6657787b5be"
        },
        "date": 1712711066717,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.025773482119999996,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1565702389066667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012297343400000002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010312346980000003,
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
          "id": "2d927b077263f2d39586a49cf993ecf3885e4de5",
          "message": "Contracts: Fix legacy uapi (#3994)\n\nFix some broken legacy definitions of pallet_contracts_uapi storage host\nfunctions",
          "timestamp": "2024-04-10T05:05:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2d927b077263f2d39586a49cf993ecf3885e4de5"
        },
        "date": 1712730215025,
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
            "name": "bitfield-distribution",
            "value": 0.025973819813333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011012876966666675,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013607199713333334,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16275950642,
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
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "id": "b8956fe1500860fad1dc0b9c55966834fc9a60c8",
          "message": "Reapply lost changes",
          "timestamp": "2024-04-10T07:29:18Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b8956fe1500860fad1dc0b9c55966834fc9a60c8"
        },
        "date": 1712738014172,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.91333333333,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013439692586666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011197681240000004,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026302755686666675,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16180000010666667,
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
          "id": "92e142555d45f97aa88d241665d9952d12f4ae40",
          "message": "Removed `pallet::getter` usage from Polkadot Runtime pallets (#3660)\n\nPart of #3326 \n\n@kianenigma @ggwpez \n\npolkadot address: 12poSUQPtcF1HUPQGY3zZu2P8emuW9YnsPduA4XG3oCEfJVp\n\n---------\n\nSigned-off-by: Matteo Muraca <mmuraca247@gmail.com>\nCo-authored-by: ordian <write@reusable.software>",
          "timestamp": "2024-04-10T08:35:10Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/92e142555d45f97aa88d241665d9952d12f4ae40"
        },
        "date": 1712744368481,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.933333333334,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.011864311826666665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.008711184726666672,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15040602485333326,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.025634620213333334,
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
          "id": "0d71753e0eb1c0fdbeb09af5db7a2e1b5f150b13",
          "message": "Contracts: Only exec parsed code in benchmarks (#3915)\n\n[Weights\ncompare](https://weights.tasty.limo/compare?unit=weight&ignore_errors=true&threshold=10&method=asymptotic&repo=polkadot-sdk&old=master&new=pg%2Fbench_tweaks&path_pattern=substrate%2Fframe%2F**%2Fsrc%2Fweights.rs%2Cpolkadot%2Fruntime%2F*%2Fsrc%2Fweights%2F**%2F*.rs%2Cpolkadot%2Fbridges%2Fmodules%2F*%2Fsrc%2Fweights.rs%2Ccumulus%2F**%2Fweights%2F*.rs%2Ccumulus%2F**%2Fweights%2Fxcm%2F*.rs%2Ccumulus%2F**%2Fsrc%2Fweights.rs)\n\nNote: Raw weights change does not mean much here, as this PR reduce the\nscope of what is benchmarked, they are therefore decreased by a good\nmargin. One should instead print the Schedule using\n\ncargo test --features runtime-benchmarks bench_print_schedule --\n--nocapture\nor following the instructions from the\n[README](https://github.com/paritytech/polkadot-sdk/tree/pg/bench_tweaks/substrate/frame/contracts#schedule)\nfor looking at the Schedule of a specific runtime\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-10T13:05:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d71753e0eb1c0fdbeb09af5db7a2e1b5f150b13"
        },
        "date": 1712761221498,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.926666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.01379872139333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16378812730000003,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026526825519999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011644010193333339,
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
          "id": "cd010925e12c3c1d22b47cc9185c394366e65c5f",
          "message": "net/strategy: Log bad peerId from on_validated_block_announce (#4051)\n\nThis tiny PR extends the `on_validated_block_announce` log with the bad\nPeerID.\nUsed to identify if the peerID is malicious by correlating with other\nlogs (ie peer-set).\n\nWhile at it, have removed the `\\n` from a multiline log, which did not\nplay well with\n[sub-triage-logs](https://github.com/lexnv/sub-triage-logs/tree/master).\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-10T15:29:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cd010925e12c3c1d22b47cc9185c394366e65c5f"
        },
        "date": 1712772400873,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.94,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026479643933333335,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01381253341333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16175993568,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011674306286666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Milos Kriz",
            "username": "miloskriz",
            "email": "82968568+miloskriz@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d21a41f23847f1aeca637ace60f58723c38f6bf3",
          "message": "Amend chainspecs for `people-westend` and add IBP bootnodes (#4072)\n\nDear team, dear @NachoPal @joepetrowski @bkchr @ggwpez,\n\nThis is a retry of #3957, after merging master as advised!,\n\nMany thanks!\n\n**_Milos_**\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-10T19:31:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d21a41f23847f1aeca637ace60f58723c38f6bf3"
        },
        "date": 1712781902311,
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
            "name": "bitfield-distribution",
            "value": 0.025734098619999996,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15135547393999993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009256530433333337,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011834370739999996,
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
          "id": "643aa2be2a2c0611eeb648cfc21eb4cb3c1c9cd8",
          "message": "Contracts: Remove ED from base deposit (#3536)\n\n- Update internal logic so that the storage_base_deposit does not\ninclude ED\n- add v16 migration to update ContractInfo struct with this change\n\nBefore:\n<img width=\"820\" alt=\"Screenshot 2024-03-21 at 11 23 29\"\nsrc=\"https://github.com/paritytech/polkadot-sdk/assets/521091/a0a8df0d-e743-42c5-9e16-cf2ec1aa949c\">\n\nAfter:\n![Screenshot 2024-03-21 at 11 23\n42](https://github.com/paritytech/polkadot-sdk/assets/521091/593235b0-b866-4915-b653-2071d793228b)\n\n---------\n\nCo-authored-by: Cyrill Leutwiler <cyrill@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-10T20:32:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/643aa2be2a2c0611eeb648cfc21eb4cb3c1c9cd8"
        },
        "date": 1712787587838,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013452884040000002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011552830093333332,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02652214223333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16118345518666677,
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
          "id": "69cc7f2090e169e736d9c998c29467040521881d",
          "message": "Fix ClaimQueue case of nothing scheduled on session boundary  (#4065)\n\nSame issue but about av-cores was fixed in\nhttps://github.com/paritytech/polkadot-sdk/pull/1403\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-11T08:37:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/69cc7f2090e169e736d9c998c29467040521881d"
        },
        "date": 1712828957821,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.926666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15496561769999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010515504713333339,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012450330513333335,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.025917381493333336,
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
          "id": "9ede4152ef0d539019875e6aff97dbe0744a4053",
          "message": "collation-generation: Avoid using `para_backing_state` if runtime is ancient (#4070)\n\nfixes https://github.com/paritytech/polkadot-sdk/issues/4067\n\nAlso add an early bail out for look ahead collator such that we don't\nwaste time if a CollatorFn is not set.\n\nTODO:\n- [x] add test.\n- [x] Polkadot System Parachain burn-in.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-11T10:36:30Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9ede4152ef0d539019875e6aff97dbe0744a4053"
        },
        "date": 1712837409601,
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
            "name": "test-environment",
            "value": 0.010327730780000006,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012279602473333333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02576731491333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15391874796666666,
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
          "id": "832570545b0311f533499ead57d764a2bc04145c",
          "message": "Fix link check (#4074)\n\nCloses #4041\n\nChanges:\n- Increase cache size and reduce retries.\n- Ignore Substrate SE links :(\n- Fix broken link.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-11T10:55:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/832570545b0311f533499ead57d764a2bc04145c"
        },
        "date": 1712843200548,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.926666666666,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010047565920000006,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01221242923333333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.025694231780000007,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15383937414,
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
          "id": "6ebf491b50a45d1814a72a6ac4287f4a15ba39ce",
          "message": "[ci] Divide subsystem-regression-tests into 2 jobs (#4076)\n\nCurrently `subsystem-regression-tests` job fails if the first benchmarks\nfail and there is no result for the second benchmark. Also dividing the\njob makes the pipeline faster (currently it's a longest job)\n\ncc https://github.com/paritytech/ci_cd/issues/969\ncc @AndreiEres\n\n---------\n\nCo-authored-by: Andrei Eres <eresav@me.com>",
          "timestamp": "2024-04-11T15:29:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6ebf491b50a45d1814a72a6ac4287f4a15ba39ce"
        },
        "date": 1712854517664,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.893333333326,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02650565074666668,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16218815036666664,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011889844726666668,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014496761419999997,
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
          "id": "25f038aa8e381911832450b2e2452d5cc64dfe37",
          "message": "Run subsystem-benchmark without network latency (#4068)\n\nImplements the idea from\nhttps://github.com/paritytech/polkadot-sdk/pull/3899\n- Removed latencies\n- Number of runs reduced from 50 to 5, according to local runs it's\nquite enough\n- Network message is always sent in a spawned task, even if latency is\nzero. Without it, CPU time sometimes spikes.\n- Removed the `testnet` profile because we probably don't need that\ndebug additions.\n\nAfter the local tests I can't say that it brings a significant\nimprovement in the stability of the results. However, I belive it is\nworth trying and looking at the results over time.",
          "timestamp": "2024-04-11T16:54:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/25f038aa8e381911832450b2e2452d5cc64dfe37"
        },
        "date": 1712858914257,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02453249373333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007717179133333335,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.015406796466666673,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16608715200000018,
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
          "id": "39b1f50f1c251def87c1625d68567ed252dc6272",
          "message": "Remove deprecated `TryRuntime` subcommand (#4017)\n\nCompletes the removal of `try-runtime-cli` logic from `polkadot-sdk`.",
          "timestamp": "2024-04-11T20:01:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/39b1f50f1c251def87c1625d68567ed252dc6272"
        },
        "date": 1712869821192,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02435384886666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0072147792666666655,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16209362493333354,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014615952733333326,
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
          "id": "a64009ae00f954acf907593309af2b1f1797e87d",
          "message": "Improve docs of broker pallet (#3980)\n\nSmall adjustments which should make understanding what is going on much\neasier for future readers.\n\nInitialization is a bit messy, the very least we should do is adding\ndocumentation to make it harder to use wrongly.\n\nI was thinking about calling `request_core_count` right from\n`start_sales`, but as explained in the docs, this is not necessarily\nwhat you want.\n\n---------\n\nCo-authored-by: eskimor <eskimor@no-such-url.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2024-04-12T10:23:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a64009ae00f954acf907593309af2b1f1797e87d"
        },
        "date": 1712935450358,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02465476213333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.015451214799999998,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1672521566666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.008059387733333336,
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
          "id": "2dfe5f745cd6daa362c6d8371e723fe4f0429b67",
          "message": "Runtime API: introduce `candidates_pending_availability` (#4027)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3576\n\nRequired by elastic scaling collators.\nDeprecates old API: `candidate_pending_availability`.\n\nTODO:\n- [x] PRDoc\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-12T10:50:13Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2dfe5f745cd6daa362c6d8371e723fe4f0429b67"
        },
        "date": 1712939939704,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16615990539999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007339489466666671,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024253042866666664,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.015224951000000028,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "wersfeds",
            "username": "wersfeds",
            "email": "wqq1479794@163.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "480d5d0feabcb68b9097bfd5cb2aa07523f2bfbc",
          "message": "chore: fix some typos (#4095)",
          "timestamp": "2024-04-12T14:32:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/480d5d0feabcb68b9097bfd5cb2aa07523f2bfbc"
        },
        "date": 1712945246356,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014660569200000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007316885866666666,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16431326646666677,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.0242192002,
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
          "id": "c963dc283af77824ceeeecc20e205f3a17968746",
          "message": "Synchronize templates (#4040)\n\n- Progresses https://github.com/paritytech/polkadot-sdk/issues/3155\n\n### What's inside\n\nA job, that will take each of the three\n[templates](https://github.com/paritytech/polkadot-sdk/tree/master/templates),\nyank them out of the monorepo workspace, and push to individual\nrepositories\n([1](https://github.com/paritytech/polkadot-sdk-minimal-template),\n[2](https://github.com/paritytech/polkadot-sdk-parachain-template),\n[3](https://github.com/paritytech/polkadot-sdk-solochain-template)).\n\nIn case the build/test does not succeed, a PR such as [this\none](https://github.com/paritytech-stg/polkadot-sdk-solochain-template/pull/2)\ngets created instead.\n\nI'm proposing a manual dispatch trigger for now - so we can test and\niterate faster - and change it to fully automatic triggered by releases\nlater.\n\nThe manual trigger looks like this:\n\n<img width=\"340px\"\nsrc=\"https://github.com/paritytech/polkadot-sdk/assets/12039224/e87e0fda-23a3-4735-9035-af801e8417fc\"/>\n\n### How it works\n\nThe job replaces dependencies [referenced by\ngit](https://github.com/paritytech/polkadot-sdk/blob/d733c77ee2d2e8e2d5205c552a5efb2e5b5242c8/templates/minimal/pallets/template/Cargo.toml#L25)\nwith a reference to released crates using\n[psvm](https://github.com/paritytech/psvm).\n\nIt creates a new workspace for the template, and adapts what's needed\nfrom the `polkadot-sdk` workspace.\n\n### See the results\n\nThe action has been tried out in staging, and the results can be\nobserved here:\n\n- [minimal\nstg](https://github.com/paritytech-stg/polkadot-sdk-minimal-template/)\n- [parachain\nstg](https://github.com/paritytech-stg/polkadot-sdk-parachain-template/)\n- [solochain\nstg](https://github.com/paritytech-stg/polkadot-sdk-solochain-template/)\n\nThese are based on the `1.9.0` release (using `release-crates-io-v1.9.0`\nbranch).",
          "timestamp": "2024-04-12T17:24:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c963dc283af77824ceeeecc20e205f3a17968746"
        },
        "date": 1712950904815,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.015608057799999997,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024714043466666665,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16749864786666663,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.008569035466666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Vedhavyas Singareddi",
            "username": "vedhavyas",
            "email": "vedhavyas.singareddi@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5b513cc0e995140b17e200d75442d7a3f2436243",
          "message": "define block hash provider and default impl using frame_system (#4080)\n\nThis PR introduces `BlockHashProvider` into `pallet_mmr::Config`\nThis type is used to get `block_hash` for a given `block_number` rather\nthan directly using `frame_system::Pallet::block_hash`\n\nThe `DefaultBlockHashProvider` uses `frame_system::Pallet::block_hash`\nto get the `block_hash`\n\nCloses: #4062",
          "timestamp": "2024-04-12T21:57:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5b513cc0e995140b17e200d75442d7a3f2436243"
        },
        "date": 1712965490338,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014485177533333332,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02430055693333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006878012866666672,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16385189939999983,
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
          "id": "8220c980084e70be55d956a69c5ebeebe47c9b9c",
          "message": "Fix zombienet-bridges-0001-asset-transfer-works (#4069)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3999\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-04-13T07:04:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8220c980084e70be55d956a69c5ebeebe47c9b9c"
        },
        "date": 1712996936249,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024639747866666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007197332400000004,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16856194333333335,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01473623273333334,
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
          "id": "7c698502d12b317c29838bfa6c0b928377477b19",
          "message": "sc_network_test: Announce only the highest block (#4111)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4100",
          "timestamp": "2024-04-13T08:40:10Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7c698502d12b317c29838bfa6c0b928377477b19"
        },
        "date": 1713001579623,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014327371000000005,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024315642399999997,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1660467554666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007398111666666677,
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
          "id": "aa437974376a6c862af4afff2a3b74b13cb3596e",
          "message": "Use Github Issue Sync to automate issues in Parachain board (#3694)\n\nThis workflow will automatically add issues related to async backing to\nthe Parachain team board, updating a custom \"meta\" field.\n\nRequested by @the-right-joyce",
          "timestamp": "2024-04-13T09:33:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aa437974376a6c862af4afff2a3b74b13cb3596e"
        },
        "date": 1713004832620,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.015046664399999998,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024570926466666664,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16698325959999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007574360466666678,
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
          "id": "1bca825cc27599dfea7b254d0ce00e3c51e632ea",
          "message": "Use `master` environment in the synchronize templates workflow (#4114)",
          "timestamp": "2024-04-13T11:00:45Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1bca825cc27599dfea7b254d0ce00e3c51e632ea"
        },
        "date": 1713008565680,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.1655572354666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013870248133333324,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024563040400000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007307677533333333,
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
          "id": "30c58fa22ad9fc6630e07e43ee2307675462995a",
          "message": "Deploy `pallet-parameters` to rococo and fix dynamic_params name expand (#4006)\n\nChanges:\n- Add pallet-parameters to Rococo to configure the NIS and preimage\npallet.\n- Fix names of expanded dynamic params. Apparently, `to_class_case`\nremoves suffix `s`, and `Nis` becomes `Ni` 😑. Now using\n`to_pascal_case`.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Alessandro Siniscalchi <asiniscalchi@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-13T11:20:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/30c58fa22ad9fc6630e07e43ee2307675462995a"
        },
        "date": 1713012082656,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02398070833333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012575168466666665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006566136133333339,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15674900779999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jonathan Udd",
            "username": "jonathanudd",
            "email": "jonathan@dwellir.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6688eac5ab21e7d9138b7ce6c90d1ce88d1f8962",
          "message": "Adding Dwellir bootnodes for Coretime Westend, People Westend and Paseo (#4066)\n\nVerified by running a node using `--reserved-only` and\n`--reserved-nodes`.",
          "timestamp": "2024-04-14T13:23:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6688eac5ab21e7d9138b7ce6c90d1ce88d1f8962"
        },
        "date": 1713104979823,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.008329658933333336,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014857028399999994,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.0243388844,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16739014040000003,
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
          "id": "88fe94516cf7c802e20aae3846d684c108765757",
          "message": "rococo_contracts: Adds missing migration (#4112)\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-04-14T20:39:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/88fe94516cf7c802e20aae3846d684c108765757"
        },
        "date": 1713131101682,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013013171666666665,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1553870986666666,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024031249999999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006353528399999996,
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
          "id": "6acf4787e168eea447b82b0a1f32e47bc794ae28",
          "message": "Bridge: slash destination may be an explicit account (#4106)\n\nExtracted to a separate PR as requested here:\nhttps://github.com/paritytech/parity-bridges-common/pull/2873#discussion_r1562459573",
          "timestamp": "2024-04-15T06:37:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6acf4787e168eea447b82b0a1f32e47bc794ae28"
        },
        "date": 1713167048421,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.023934282733333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006704480333333336,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012650073000000008,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15853513066666672,
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
          "id": "d1b0ef76a8b060437ec7dfc1bf6b400626cd6208",
          "message": "sp-api: Use macro to detect if `frame-metadata` is enabled (#4117)\n\nWhile `sp-api-proc-macro` isn't used directly and thus, it should have\nthe same features enabled as `sp-api`. However, I have seen issues\naround `frame-metadata` not being enabled for `sp-api`, but for\n`sp-api-proc-macro`. This can be prevented by using the\n`frame_metadata_enabled` macro from `sp-api` that ensures we have the\nsame feature set between both crates.",
          "timestamp": "2024-04-15T08:11:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d1b0ef76a8b060437ec7dfc1bf6b400626cd6208"
        },
        "date": 1713173286091,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16550066326666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007115363866666673,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.0242496582,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014542183666666677,
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
          "id": "8b4cfda7589325d1a34f70b3770ab494a9d4052c",
          "message": "added script to require a review post push (#3431)\n\nCloses https://github.com/paritytech/opstooling/issues/174\n\nAdded a new step in the action that triggers review bot to stop approval\nfrom new pushes.\n\nThis step works in the following way:\n- If the **author of the PR**, who **is not** a member of the org,\npushed a new commit then:\n- Review-Trigger requests new reviews from the reviewers and fails.\n\nIt *does not dismiss reviews*. It simply request them again, but they\nwill still be available.\n\nThis way, if the author changed something in the code, they will still\nneed to have this latest change approved to stop them from uploading\nmalicious code.\n\nFind the requested issue linked to this PR (it is from a private repo so\nI can't link it here)",
          "timestamp": "2024-04-15T13:46:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8b4cfda7589325d1a34f70b3770ab494a9d4052c"
        },
        "date": 1713190755866,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.023934027333333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1573839927333331,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006463533466666662,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012876985333333337,
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
          "id": "d1f9fe0a994febff13f21b0edd223e838754e2fc",
          "message": "logging(fix): Use the proper log target for logging (#4124)\n\nThis PR ensures the proper logging target (ie `libp2p_tcp` or `beefy`)\nis displayed.\n\nThe issue has been introduced in:\nhttps://github.com/paritytech/polkadot-sdk/pull/4059, which removes the\nnormalized metadata of logs.\n\nFrom\n[documentation](https://docs.rs/tracing-log/latest/tracing_log/trait.NormalizeEvent.html#tymethod.normalized_metadata):\n\n> In tracing-log, an Event produced by a log (through\n[AsTrace](https://docs.rs/tracing-log/latest/tracing_log/trait.AsTrace.html))\nhas an hard coded “log” target\n\n>\n[normalized_metadata](https://docs.rs/tracing-log/latest/tracing_log/trait.NormalizeEvent.html#tymethod.normalized_metadata):\nIf this Event comes from a log, this method provides a new normalized\nMetadata which has all available attributes from the original log,\nincluding file, line, module_path and target\n\nThis has low implications if a version was deployed containing the\nmentioned pull request, as we'll lose the ability to distinguish between\nlog targets.\n\n### Before this PR\n\n```\n2024-04-15 12:45:40.327  INFO main log: Parity Polkadot\n2024-04-15 12:45:40.328  INFO main log: ✌️  version 1.10.0-d1b0ef76a8b\n2024-04-15 12:45:40.328  INFO main log: ❤️  by Parity Technologies <admin@parity.io>, 2017-2024\n2024-04-15 12:45:40.328  INFO main log: 📋 Chain specification: Development\n2024-04-15 12:45:40.328  INFO main log: 🏷  Node name: yellow-eyes-2963\n2024-04-15 12:45:40.328  INFO main log: 👤 Role: AUTHORITY\n2024-04-15 12:45:40.328  INFO main log: 💾 Database: RocksDb at /tmp/substrated39i9J/chains/rococo_dev/db/full\n2024-04-15 12:45:44.508  WARN main log: Took active validators from set with wrong size\n...\n\n2024-04-15 12:45:45.805  INFO                 main log: 👶 Starting BABE Authorship worker\n2024-04-15 12:45:45.806  INFO tokio-runtime-worker log: 🥩 BEEFY gadget waiting for BEEFY pallet to become available...\n2024-04-15 12:45:45.806 DEBUG tokio-runtime-worker log: New listen address: /ip6/::1/tcp/30333\n2024-04-15 12:45:45.806 DEBUG tokio-runtime-worker log: New listen address: /ip4/127.0.0.1/tcp/30333\n```\n\n### After this PR\n\n```\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: Parity Polkadot\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: ✌️  version 1.10.0-d1b0ef76a8b\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: ❤️  by Parity Technologies <admin@parity.io>, 2017-2024\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: 📋 Chain specification: Development\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: 🏷  Node name: helpless-lizards-0550\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: 👤 Role: AUTHORITY\n...\n2024-04-15 12:59:50.204  INFO tokio-runtime-worker beefy: 🥩 BEEFY gadget waiting for BEEFY pallet to become available...\n2024-04-15 12:59:50.204 DEBUG tokio-runtime-worker libp2p_tcp: New listen address: /ip6/::1/tcp/30333\n2024-04-15 12:59:50.204 DEBUG tokio-runtime-worker libp2p_tcp: New listen address: /ip4/127.0.0.1/tcp/30333\n```\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-15T14:33:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d1f9fe0a994febff13f21b0edd223e838754e2fc"
        },
        "date": 1713197292180,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012598609133333332,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02397543026666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15452415646666656,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006269785733333327,
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
          "id": "0c9ad5306ce8bbc815d862121a42778c1ea734be",
          "message": "[pallet-broker] add tests for renewing leases (#4099)\n\nThe first test proves that parachains who were migrated over on a legacy\nlease can renew without downtime.\n\nThe exception is if their lease expires in period 0 - aka within\n`region_length` timeslices after `start_sales` is called. The second\ntest is designed such that it passes if the issue exists and should be\nfixed.\nThis will require an intervention on Kusama to add these renewals to\nstorage as it is too tight to schedule a runtime upgrade before the\nstart_sales call. All leases will still have at least two full regions\nof coretime.",
          "timestamp": "2024-04-15T16:28:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0c9ad5306ce8bbc815d862121a42778c1ea734be"
        },
        "date": 1713202493464,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16563714073333324,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007255639466666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024203189133333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.015274635466666681,
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
          "id": "a8f4f4f00f8fc0da512a09e1450bf4cda954d70d",
          "message": "pallet assets: Fix errors (#4118)\n\n`LiveAsset` is an error to be returned when an asset is not supposed to\nbe live.\nAnd `AssetNotLive` is an error to be returned when an asset is supposed\nto be live, I don't think frozen qualifies as live.",
          "timestamp": "2024-04-15T18:45:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a8f4f4f00f8fc0da512a09e1450bf4cda954d70d"
        },
        "date": 1713210856048,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.007733856733333329,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024406677,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1677993731999999,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.015201894666666656,
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
          "id": "4b5c3fd0cbb47f3484671ffce284b7586311126a",
          "message": "move fragment_tree module to its own folder (#4148)\n\nWill make https://github.com/paritytech/polkadot-sdk/pull/4035 easier to\nreview (the mentioned PR already does this move so the diff will be\nclearer).\n\nAlso called out as part of:\nhttps://github.com/paritytech/polkadot-sdk/pull/3233#discussion_r1490867383",
          "timestamp": "2024-04-16T07:25:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4b5c3fd0cbb47f3484671ffce284b7586311126a"
        },
        "date": 1713256202745,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012964074133333336,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15599579433333313,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006373802666666671,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024063455399999996,
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
          "id": "61d45ed72b2f8afade997e1a973327f2ada02aa0",
          "message": "Update review-trigger.yml (#4137)\n\nFollowup after https://github.com/paritytech/polkadot-sdk/pull/3431\nPer\nhttps://stackoverflow.com/questions/63188674/github-actions-detect-author-association\nand https://michaelheap.com/github-actions-check-permission/\nlooks like just checking NOT a MEMBER is not correct, Not a CONTRIBUTORs\ncheck should be included",
          "timestamp": "2024-04-16T10:11:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/61d45ed72b2f8afade997e1a973327f2ada02aa0"
        },
        "date": 1713266300631,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.01433293893333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16336106453333335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006589278933333333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02415798086666667,
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
          "id": "753bf2d860e083b5da25fe4171c0e540ddad4888",
          "message": "[prdoc] Update docs (#3998)\n\nUpdating the prdoc doc file to be a bit more useful for new contributors\nand adding a SemVer section.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-16T15:17:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/753bf2d860e083b5da25fe4171c0e540ddad4888"
        },
        "date": 1713284682875,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15924878893333327,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.0239467228,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007070786133333335,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013513785733333339,
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
          "id": "6f3d890ed35bfdee3e3f7d59018345635a62d1cd",
          "message": "FRAME: Unity Balance Conversion for Different IDs of Native Asset (#3659)\n\nIntroduce types to define 1:1 balance conversion for different relative\nasset ids/locations of native asset.\n\nExamples:\nnative asset on Asset Hub presented as `VersionedLocatableAsset` type in\nthe context of Relay Chain is\n```\n{\n  `location`: (0, Parachain(1000)),\n  `asset_id`: (1, Here),\n}\n```\nand it's balance should be converted 1:1 by implementations of\n`ConversionToAssetBalance` trait.\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-04-16T16:11:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6f3d890ed35bfdee3e3f7d59018345635a62d1cd"
        },
        "date": 1713288120046,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.007973351933333348,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024517849466666665,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1706664593333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.015842117399999987,
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
          "id": "e81322bc3e8192b536067fed3ef9e20f2752c376",
          "message": "Contracts verify benchmark block (#4130)\n\nAdd verify statement to ensure that benchmarks call do not revert\n\nAlso updated\n[benchmarks](https://weights.tasty.limo/compare?unit=time&ignore_errors=true&threshold=10&method=asymptotic&repo=polkadot-sdk&old=master&new=pg/verify-benchmarks&path_pattern=substrate%2Fframe%2Fcontracts%2Fsrc%2Fweights.rs)\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-16T23:38:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e81322bc3e8192b536067fed3ef9e20f2752c376"
        },
        "date": 1713314602756,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014389612800000007,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02426154706666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1665731892666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007233763266666667,
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
          "id": "4be9f93cd7e81c71ead1a8b5445bc695d98b10ba",
          "message": "Adjust `xcm-bridge-hub-router`'s `SendXcm::validate` behavior for `NotApplicable` (#4162)\n\nThis PR adjusts `xcm-bridge-hub-router` to be usable in the chain of\nrouters when a `NotApplicable` error occurs.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4133\n\n## TODO\n\n- [ ] backport to polkadot-sdk 1.10.0 crates.io release",
          "timestamp": "2024-04-17T09:09:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4be9f93cd7e81c71ead1a8b5445bc695d98b10ba"
        },
        "date": 1713346524551,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.01396255126666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007072937600000007,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024246400799999998,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16071496279999983,
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
          "id": "e6f3106d894277deba043a83e91565de24263a1b",
          "message": "XCM coretime region transfers (#3455)\n\nThis PR introduces changes enabling the transfer of coretime regions via\nXCM.\n\nTL;DR: There are two primary issues that are resolved in this PR:\n\n1. The `mint` and `burn` functions were not implemented for coretime\nregions. These operations are essential for moving assets to and from\nthe XCM holding register.\n2. The transfer of non-fungible assets through XCM was previously\ndisallowed. This was due to incorrectly benchmarking non-fungible asset\ntransfers via XCM, which led to assigning it a weight of `Weight::Max`,\neffectively preventing its execution.\n\n### `mint_into` and `burn` implementation\n\nThis PR addresses the issue with cross-chain transferring regions back\nto the Coretime chain. Remote reserve transfers are performed by\nwithdrawing and depositing the asset to and from the holding registry.\nThis requires the asset to support burning and minting functionality.\n\nThis PR adds burning and minting; however, they work a bit differently\nthan usual so that the associated region record is not lost when\nburning. Instead of removing all the data, burning will set the owner of\nthe region to `None`, and when minting it back, it will set it to an\nactual value. So, when cross-chain transferring, withdrawing into the\nregistry will remove the region from its original owner, and when\ndepositing it from the registry, it will set its owner to another\naccount\n\nThis was originally implemented in this PR: #3455, however we decided to\nmove all of it to this single PR\n(https://github.com/paritytech/polkadot-sdk/pull/3455#discussion_r1547324892)\n\n### Fixes made in this PR\n\n- Update the `XcmReserveTransferFilter` on coretime chain since it is\nmeant as a reserve chain for coretime regions.\n- Update the XCM benchmark to use `AssetTransactor` instead of assuming\n`pallet-balances` for fungible transfers.\n- Update the XCM benchmark to properly measure weight consumption for\nnonfungible reserve asset transfers. ATM reserve transfers via the\nextrinsic do not work since the weight for it is set to `Weight::max()`.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/865\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Dónal Murray <donalm@seadanda.dev>",
          "timestamp": "2024-04-17T09:25:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e6f3106d894277deba043a83e91565de24263a1b"
        },
        "date": 1713350327495,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16015083559999993,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012902917066666663,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.0240761686,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006582842399999994,
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
          "id": "4e10d3b0a6ec2eccf58c471e7739948c1a867acf",
          "message": "Asset Conversion: Pool Account ID derivation with additional Pallet ID seed (#3250)\n\nIntroduce `PalletId` as an additional seed parameter for pool's account\nid derivation.\n\nThe PR also introduces the `pallet_asset_conversion_ops` pallet with a\ncall to migrate a given pool to thew new account. Additionally\n`fungibles::lifetime::ResetTeam` and `fungible::lifetime::Refund`\ntraits, to facilitate the migration of pools.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-17T10:39:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4e10d3b0a6ec2eccf58c471e7739948c1a867acf"
        },
        "date": 1713356198646,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024105996666666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012960267533333342,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006071174999999995,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15773941326666646,
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
          "id": "bfbf7f5d6f5c491a820bb0a4fb9508ce52192a06",
          "message": "chainHead: Report unique hashes for pruned blocks (#3667)\n\nThis PR ensures that the reported pruned blocks are unique.\n\nWhile at it, ensure that the best block event is properly generated when\nthe last best block is a fork that will be pruned in the future.\n\nTo achieve this, the chainHead keeps a LRU set of reported pruned blocks\nto ensure the following are not reported twice:\n\n```bash\n\t finalized -> block 1 -> block 2 -> block 3\n\t\n\t                      -> block 2 -> block 4 -> block 5\n\t\n\t           -> block 1 -> block 2_f -> block 6 -> block 7 -> block 8\n```\n\nWhen block 7 is finalized the branch [block 2; block 3] is reported as\npruned.\nWhen block 8 is finalized the branch [block 2; block 4; block 5] should\nbe reported as pruned, however block 2 was already reported as pruned at\nthe previous step.\n\nThis is a side-effect of the pruned blocks being reported at level N -\n1. For example, if all pruned forks would be reported with the first\nencounter (when block 6 is finalized we know that block 3 and block 5\nare stale), we would not need the LRU cache.\n\ncc @paritytech/subxt-team  \n\nCloses https://github.com/paritytech/polkadot-sdk/issues/3658\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-04-17T15:29:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bfbf7f5d6f5c491a820bb0a4fb9508ce52192a06"
        },
        "date": 1713372538553,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.1551379109333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006031601866666661,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013205111066666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024163979933333333,
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
          "id": "305d311d5c732fcc4629f3295768f1ed44ef434c",
          "message": "Asset Conversion: Pool Touch Call (#3251)\n\nIntroduce `touch` call designed to address operational prerequisites\nbefore providing liquidity to a pool.\n\nThis function ensures that essential requirements, such as the presence\nof the pool's accounts, are fulfilled. It is particularly beneficial in\nscenarios where a pool creator removes the pool's accounts without\nproviding liquidity.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-17T16:45:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/305d311d5c732fcc4629f3295768f1ed44ef434c"
        },
        "date": 1713377987225,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.015449250133333328,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024397798666666668,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16665834713333338,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.008169633333333351,
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
          "id": "d591b16f6b1dec88003323cdae0c3abe3b5c9cbe",
          "message": "Remove NotConcrete error (#3867)\n\n# Description\n- Link to issue: https://github.com/paritytech/polkadot-sdk/issues/3651\n\npolkadot address: 19nSqFQorfF2HxD3oBzWM3oCh4SaCRKWt1yvmgaPYGCo71J",
          "timestamp": "2024-04-18T06:44:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d591b16f6b1dec88003323cdae0c3abe3b5c9cbe"
        },
        "date": 1713426980112,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013438111599999997,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15547001646666675,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006464405399999998,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024105921066666668,
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
          "id": "b6fab8046e42283d14e9fa6beda32c878b3e801e",
          "message": "[ci] Run `test-linux-stable-int` on self-hosted GitHub Runners (#4178)\n\nPR adds `test-linux-stable-int` and `quick-benchmarks` as github action\njobs. It's a copy of `test-linux-stable-int` and `quick-benchmarks` from\ngitlab ci and now it's needed to make a stress test for self-hosted\ngithub runners. `test-linux-stable-int` and `quick-benchmarks` in gitlab\nare still `Required` whereas this workflow is allowed to fail.\n\ncc https://github.com/paritytech/infrastructure/issues/46",
          "timestamp": "2024-04-18T07:40:45Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b6fab8046e42283d14e9fa6beda32c878b3e801e"
        },
        "date": 1713430275895,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024741817933333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.015304360733333324,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16824249440000008,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007628435933333319,
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
          "id": "76719da221d33117aadf6b7b9cc74e4fbeb25b34",
          "message": "[ci] Update ci image with rust 1.77 and 2024-04-10 (#4077)\n\ncc https://github.com/paritytech/ci_cd/issues/974\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2024-04-18T09:24:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/76719da221d33117aadf6b7b9cc74e4fbeb25b34"
        },
        "date": 1713436893267,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014474975200000007,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16338391513333353,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.0245329554,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0072775470000000075,
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
          "id": "ff906127ab513bb42a4288968e0f421f630809e0",
          "message": "Improve changelog in the release notes (#4179)\n\nThis PR adds description to each of the sections of the Changelog part.\nChanges are based on feedback that it wasn't that clear what exactly\n`Node Dev`, `Runtime Dev` etc. means. Now, the description for each of\nthose parts is taken directly from the `prdoc` schema.\nCloses https://github.com/paritytech/release-engineering/issues/197",
          "timestamp": "2024-04-18T10:30:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ff906127ab513bb42a4288968e0f421f630809e0"
        },
        "date": 1713440589755,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014763447199999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007468142266666674,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024539509333333334,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16534525813333317,
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
          "id": "91d4a207af43f8f81f56e4f24af74f7c6f590148",
          "message": "chain-selection: allow reverting current block (#4103)\n\nBlock reversion of the current block is technically possible as can be\nseen from\n\nhttps://github.com/paritytech/polkadot-sdk/blob/39b1f50f1c251def87c1625d68567ed252dc6272/polkadot/runtime/parachains/src/disputes.rs#L1215-L1223\n\n- [x] Fix the test",
          "timestamp": "2024-04-18T14:32:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/91d4a207af43f8f81f56e4f24af74f7c6f590148"
        },
        "date": 1713455703527,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013838641733333329,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16080508766666676,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0070274793333333346,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02434859886666666,
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
          "id": "c891fdabf4d519b25829490723fb70b1a2ffc0e5",
          "message": "tx: Remove tx_broadcast transaction from the pool (#4050)\n\nThis PR ensures that broadcast future cleans-up the submitted extrinsic\nfrom the pool, iff the `broadcast_stop` operation has been called.\n\nThis effectively cleans-up transactions from the pool when the\n`broadcast_stop` is called.\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-18T15:57:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c891fdabf4d519b25829490723fb70b1a2ffc0e5"
        },
        "date": 1713460827223,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012656718133333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.006379927666666679,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15810337746666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.0241669304,
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
          "id": "88a2f360238787bf5256cfdd14b40c08f519b38e",
          "message": "chainHead: Stabilize chainHead to version 1 (#4168)\n\nThis PR stabilizes the chainHead API to version 1.\n\nNeeds:\n- https://github.com/paritytech/polkadot-sdk/pull/3667\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-18T17:19:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/88a2f360238787bf5256cfdd14b40c08f519b38e"
        },
        "date": 1713465692802,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013666366733333324,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.024604184266666666,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16237995466666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.007063621733333335,
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
          "id": "98a364fe6e7abf10819f5fddd3de0588f7c38700",
          "message": "rpc-v2: Limit transactionBroadcast calls to 16 (#3772)\n\nThis PR limits the number of active calls to the transactionBroadcast\nAPIs to 16.\n\ncc @paritytech/subxt-team \n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/3081\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: James Wilson <james@jsdw.me>",
          "timestamp": "2024-04-19T04:34:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/98a364fe6e7abf10819f5fddd3de0588f7c38700"
        },
        "date": 1713506043108,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 440.3333333333333,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 20537.666666666668,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.00723506486666668,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02429190226666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014937964199999997,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16406809006666673,
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
          "id": "04a9071e2a5ba903648f8db19066e671659850fb",
          "message": "Use higher priority for PVF preparation in dispute/approval context (#4172)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/4126\ndiscussion\n\nCurrently all preparations have same priority and this is not ideal in\nall cases. This change should improve the finality time in the context\nof on-demand parachains and when `ExecutorParams` are updated on-chain\nand a rebuild of all artifacts is required. The desired effect is to\nspeed up approval and dispute PVF executions which require preparation\nand delay backing executions which require preparation.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-19T08:15:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/04a9071e2a5ba903648f8db19066e671659850fb"
        },
        "date": 1713516502083,
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
            "name": "availability-distribution",
            "value": 0.013317338399999996,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02285664656,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16472517582666657,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011046016453333331,
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
          "id": "21308d893ef0594538aee73cbdc3905189be0b7b",
          "message": "Fixed GrandpaConsensusLogReader::find_scheduled_change (#4208)",
          "timestamp": "2024-04-19T08:34:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/21308d893ef0594538aee73cbdc3905189be0b7b"
        },
        "date": 1713520027072,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.879999999994,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013195318133333332,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022799215106666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011179852533333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16436746543999997,
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
          "id": "eba3deca3e61855c237a33013e8a5e82c479e958",
          "message": "txWatch: Stabilize txWatch to version 1 (#4171)\n\nThis PR stabilizes the txBroadcast API to version 1.\n\nNeeds from spec:\n- https://github.com/paritytech/json-rpc-interface-spec/pull/153 \n- https://github.com/paritytech/json-rpc-interface-spec/pull/154\n\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-19T09:48:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/eba3deca3e61855c237a33013e8a5e82c479e958"
        },
        "date": 1713525433049,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16594548901333336,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013394704673333337,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02284134281999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011298552053333334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "maksimryndin",
            "username": "maksimryndin",
            "email": "maksim.ryndin@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4eabe5e0dddc4cd31ad9dab5645350360d4d36a5",
          "message": "Pvf refactor execute worker errors follow up (#4071)\n\nfollow up of https://github.com/paritytech/polkadot-sdk/pull/2604\ncloses https://github.com/paritytech/polkadot-sdk/pull/2604\n\n- [x] take relevant changes from Marcin's PR \n- [x] extract common duplicate code for workers (low-hanging fruits)\n\n~Some unpassed ci problems are more general and should be fixed in\nmaster (see https://github.com/paritytech/polkadot-sdk/pull/4074)~\n\nProposed labels: **T0-node**, **R0-silent**, **I4-refactor**\n\n-----\n\nkusama address: FZXVQLqLbFV2otNXs6BMnNch54CFJ1idpWwjMb3Z8fTLQC6\n\n---------\n\nCo-authored-by: s0me0ne-unkn0wn <48632512+s0me0ne-unkn0wn@users.noreply.github.com>",
          "timestamp": "2024-04-19T13:36:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4eabe5e0dddc4cd31ad9dab5645350360d4d36a5"
        },
        "date": 1713538151907,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.913333333327,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022856599180000003,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16663163157333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011357125946666671,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013875884859999996,
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
          "id": "e504c41a5adbd5e6d9a7764c07f6dcf47b2dae77",
          "message": "Allow privileged virtual bond in Staking pallet (#3889)\n\nThis is the first PR in preparation for\nhttps://github.com/paritytech/polkadot-sdk/issues/454.\n\n## Follow ups:\n- https://github.com/paritytech/polkadot-sdk/pull/3904.\n- https://github.com/paritytech/polkadot-sdk/pull/3905.\n\nOverall changes are documented here (lot more visual 😍):\nhttps://hackmd.io/@ak0n/454-np-governance\n\n[Maybe followup](https://github.com/paritytech/polkadot-sdk/issues/4217)\nwith migration of storage item `VirtualStakers` as a bool or enum in\n`Ledger`.\n\n## Context\nWe want to achieve a way for a user (`Delegator`) to delegate their\nfunds to another account (`Agent`). Delegate implies the funds are\nlocked in delegator account itself. Agent can act on behalf of delegator\nto stake directly on Staking pallet.\n\nThe delegation feature is added to Staking via another pallet\n`delegated-staking` worked on\n[here](https://github.com/paritytech/polkadot-sdk/pull/3904).\n\n## Introduces:\n### StakingUnchecked Trait\nAs the name implies, this trait allows unchecked (non-locked) mutation\nof staking ledger. These apis are only meant to be used by other pallets\nin the runtime and should not be exposed directly to user code path.\nAlso related: https://github.com/paritytech/polkadot-sdk/issues/3888.\n\n### Virtual Bond\nAllows other pallets to stake via staking pallet while managing the\nlocks on these accounts themselves. Introduces another storage\n`VirtualStakers` that whitelist these accounts.\n\nWe also restrict virtual stakers to set reward account as themselves.\nSince the account has no locks, we cannot support compounding of\nrewards. Conservatively, we require them to set a separate account\ndifferent from the staker. Since these are code managed, it should be\neasy for another pallet to redistribute reward and rebond them.\n\n### Slashes\nSince there is no actual lock maintained by staking-pallet for virtual\nstakers, this pallet does not apply any slashes. It is then important\nfor pallets managing virtual stakers to listen to slashing events and\napply necessary slashes.",
          "timestamp": "2024-04-20T00:05:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e504c41a5adbd5e6d9a7764c07f6dcf47b2dae77"
        },
        "date": 1713575676281,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.946666666663,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16463982230000013,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013230670046666665,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022750283846666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01112947089333334,
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
          "id": "f3c3ebb6a99295816ac4ee0a26364d736094c147",
          "message": "Fix case in type in macro generation (#4223)\n\nGenerated type is not camel case this generate some warnings from IDE\n\nlabel should be R0",
          "timestamp": "2024-04-20T08:20:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f3c3ebb6a99295816ac4ee0a26364d736094c147"
        },
        "date": 1713605188655,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.913333333334,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.1669615220866667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022889248073333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01147075324666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013845966280000005,
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
          "id": "253778c94dd64e6bc174ed1e03ac7e0b43990129",
          "message": "ci: disallow westend migration failure (#4205)\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-22T05:08:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/253778c94dd64e6bc174ed1e03ac7e0b43990129"
        },
        "date": 1713766591462,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.886666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013696153539999999,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16581050518666665,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02277566974666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011535437613333336,
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
          "id": "921265ca7889b9c9bc615af0eced9c6918c8af9f",
          "message": "Added prdoc for 4208 (#4239)",
          "timestamp": "2024-04-22T12:06:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/921265ca7889b9c9bc615af0eced9c6918c8af9f"
        },
        "date": 1713789867418,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.926666666663,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012422185153333331,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022538646013333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01034281784,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15987728674666654,
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
          "id": "a2a049db2bd669a88f6ab410b22b780ebcc8baee",
          "message": "[subsystem-benchmark] Add approval-voting benchmark to CI (#4216)\n\nCo-authored-by: alvicsam <alvicsam@gmail.com>",
          "timestamp": "2024-04-22T12:45:54Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a2a049db2bd669a88f6ab410b22b780ebcc8baee"
        },
        "date": 1713795159714,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.879999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16125345841333327,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012504797366666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010642607673333339,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022482682226666675,
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
          "id": "3380e21cd92690c2066f686164a954ba7cd17244",
          "message": "Use default branch of `psvm` when synchronizing templates (#4240)\n\nWe cannot lock to a specific version of `psvm`, because we will need to\nkeep it up-to-date - each release currently requires a change in `psvm`\nsuch as [this one](https://github.com/paritytech/psvm/pull/2/files).\n\nThere is no `stable` branch in `psvm` repo or anything so using the\ndefault branch.",
          "timestamp": "2024-04-22T16:34:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3380e21cd92690c2066f686164a954ba7cd17244"
        },
        "date": 1713808054094,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.91333333333,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022425173993333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16052324617333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012306953093333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010348097646666672,
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
        "date": 1713820652457,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022577859879999992,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16351843857999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010986655566666672,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013090886886666663,
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
        "date": 1713829143753,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012165718366666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009929021673333337,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15866809538,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022106961399999993,
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
          "id": "ac4f421f0b99b73bbf80710206e9ac1463e8cb0b",
          "message": "parachains_coretime: Expose `MaxXCMTransactWeight` (#4189)\n\nThis should be configured on the runtime level and not somewhere inside\nthe pallet.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-04-23T09:51:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ac4f421f0b99b73bbf80710206e9ac1463e8cb0b"
        },
        "date": 1713867498298,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.88666666666,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.1606442662666666,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01235991210666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010540011666666673,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022406134019999992,
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
        "date": 1713877896412,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15795849608666668,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022021966219999994,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012002253100000002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009665370780000009,
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
        "date": 1713882348041,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.933333333334,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022041316346666666,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15793205897333334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00989696036666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012016053693333336,
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
        "date": 1713892503791,
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
            "value": 0.021801540826666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.008522958546666671,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1532653423666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011679695160000003,
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
        "date": 1713944647800,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021911986333333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15452399792000007,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01169703560666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.008894100220000008,
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
        "date": 1713954090722,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.926666666666,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.011875082493333331,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021975996779999993,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15697767274666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009546830933333339,
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
        "date": 1713957014608,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.009999025913333338,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02224292026,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15852513885999997,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012116473853333334,
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
        "date": 1713975926015,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013526106813333334,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022815036033333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1666388968266666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01136655622,
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
        "date": 1714028629790,
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
            "value": 0.02294808060666666,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16878411043333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01158801021333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013946326326666671,
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
        "date": 1714035180952,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.893333333326,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022032969920000006,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011833041986666671,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15668121515999991,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009417726440000003,
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
          "id": "239a23d9cc712aab8c0a87eab7e558e5a149fd42",
          "message": "Fix polkadot parachains not producing blocks until next session (#4269)\n\n... a few sessions too late :(, this already happened on polkadot, so as\nof now there are no known relay-chains without async backing enabled in\nruntime, but let's fix it in case someone else wants to repeat our\nsteps.\n\nFixes: https://github.com/paritytech/polkadot-sdk/issues/4226\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-25T10:11:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/239a23d9cc712aab8c0a87eab7e558e5a149fd42"
        },
        "date": 1714041393543,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.91333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012993279913333329,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022350885366666674,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010692841026666676,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16218449430666662,
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
        "date": 1714052327458,
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
            "name": "test-environment",
            "value": 0.01163119069333334,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022668894840000006,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16774334200666657,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014125829873333329,
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
          "id": "8f5c8f735af9048b83957821db7fb363e89e919f",
          "message": "Update approval-voting banchmarks base values (#4283)",
          "timestamp": "2024-04-25T15:04:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8f5c8f735af9048b83957821db7fb363e89e919f"
        },
        "date": 1714058927654,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.91333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.1680170189266666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011823804346666673,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022694669880000004,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014330130940000001,
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
        "date": 1714063117874,
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
            "name": "availability-distribution",
            "value": 0.013381481233333334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011198012880000004,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16697290975999995,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022818989179999995,
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
        "date": 1714119099481,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013599766840000001,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02285059932000001,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1678232687333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011406888700000005,
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
        "date": 1714124923061,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013524566646666672,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011512533393333331,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16840664945999997,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022808047093333332,
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
        "date": 1714129014897,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666662,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022083998000000004,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15791758168,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01181590378,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009487834293333336,
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
        "date": 1714136084828,
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
            "name": "bitfield-distribution",
            "value": 0.022548969006666662,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010323059020000002,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012256915886666666,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16238486724666665,
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
        "date": 1714139941526,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.88666666666,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022199912006666662,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010766993600000005,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16684624379333335,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013410524326666664,
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
        "date": 1714144303833,
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
            "name": "availability-store",
            "value": 0.15768924471999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00924715232666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02198413662,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011792149840000002,
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
        "date": 1714151874483,
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
            "name": "availability-store",
            "value": 0.15768924471999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00924715232666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02198413662,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011792149840000002,
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
        "date": 1714154093018,
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
            "name": "test-environment",
            "value": 0.011614926000000001,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02293087921333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013882219959999999,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16908525746,
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
        "date": 1714313767701,
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
            "value": 0.010746237146666672,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16442013484666668,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012901068273333337,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02264743258,
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
        "date": 1714323814929,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.926666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.1578228254866667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011608030659999997,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02204717038,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009315662820000004,
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
        "date": 1714327976131,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.93333333333,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16931051438666656,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02299356515333332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01168649367333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01380836603999999,
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
        "date": 1714380784512,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.88666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022732553719999996,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013637075853333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011152923733333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16797939999333333,
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
        "date": 1714411799341,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.886666666658,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.011127700433333337,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16806507518666666,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013304694239999996,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02273404818,
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
        "date": 1714431379322,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.913333333327,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021934606326666665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00906760677333334,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15710597662000003,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011617852019999996,
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
        "date": 1714459150214,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.87333333333,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010804169453333339,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022310043473333332,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1665873662266667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01345162486666667,
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
        "date": 1714492836906,
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
            "value": 0.022305633859999993,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16238397479333336,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012403014413333328,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010334418866666669,
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
        "date": 1714589430772,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.93333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013462606326666664,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011416420880000005,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022761539919999997,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16746014383333332,
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
        "date": 1714599165932,
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
            "name": "availability-distribution",
            "value": 0.013034155933333336,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022618835306666668,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16586144116666673,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010953754920000001,
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
        "date": 1714640171698,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.913333333327,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.01207393814666666,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022379359086666658,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16159951193999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01015029263333334,
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
        "date": 1714647659054,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666662,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.01149748202666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013760565240000001,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022940044173333337,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16921953033333345,
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
        "date": 1714653380910,
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
            "name": "availability-distribution",
            "value": 0.01164191045333334,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15916281335999996,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021955609813333336,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00943789574666667,
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
        "date": 1714658291861,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.926666666666,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012359749259999997,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022144698113333337,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16106044409333328,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010285193560000008,
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
        "date": 1714664416103,
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
            "value": 0.02272878978,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16708010308666665,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013754613079999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011436568600000006,
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
        "date": 1714668795877,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.94,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.1602902751533333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022230245786666672,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010147947373333339,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012116246066666659,
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
        "date": 1714686341368,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.94,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.1602902751533333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022230245786666672,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010147947373333339,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012116246066666659,
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
        "date": 1714688581012,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16180018726000003,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012725930206666666,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.0224230634,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010673378926666673,
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
          "id": "871281783c1be03157319d5143096fd3dd860d0a",
          "message": "Bridge: fix zombienet tests (#4367)\n\nDue to recent bump of Rococo/Westend versions + the fact that\nhttps://github.com/paritytech/parity-bridges-common/pull/2894 has\nfinally reached this repo, tests now fail, because we've started\nchecking all client versions (even source) unless we specify\n`--source-version-mode Auto` in CLI arguments. This looks like an\noverkill, but all those version checks will be fixed by\nhttps://github.com/paritytech/polkadot-sdk/pull/4256, so now it makes\nsense just to add this CLI option. We also need to propagate it to\nrunning relays eventually.",
          "timestamp": "2024-05-03T11:49:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/871281783c1be03157319d5143096fd3dd860d0a"
        },
        "date": 1714738550117,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.93333333333,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022283963306666665,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1592591075733333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011945184453333334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010325890333333339,
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
        "date": 1714745293870,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.011446124580000005,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013567564799999998,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16541646515333336,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022830034046666663,
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
        "date": 1714750590259,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.91333333333,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022236672953333335,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01245481313333334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010304154513333339,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16135695286666668,
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
        "date": 1714756633780,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.933333333334,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010890741020000006,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022643188926666665,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012987877060000002,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1640376909333333,
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
        "date": 1714806554363,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16307975415333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012850937320000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010938801700000002,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02270302428666666,
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
        "date": 1714974207868,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.9,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012115400206666672,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15728025695333328,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02194111728666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009711134373333338,
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
        "date": 1715002176619,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666662,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.011055315660000004,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02275195401333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013274935653333325,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16429059974666665,
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
        "date": 1715012037035,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022728231060000003,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16474208622000003,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013145355306666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011289223520000002,
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
        "date": 1715076548313,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.926666666666,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02227081898,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16120044963333335,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012339322620000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010238682626666669,
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
        "date": 1715083166661,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.913333333327,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021942102459999998,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011958841686666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009726702800000004,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15745662511333333,
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
        "date": 1715096984834,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.873333333326,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02205671973333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010031089433333338,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012076777020000002,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15920589665333332,
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
        "date": 1715115073488,
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
            "value": 0.1589073715666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012187485553333335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010101875520000003,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022024686833333328,
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
        "date": 1715121664124,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.939999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.008440926573333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15533438469333333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02179736795333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011510209253333334,
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
        "date": 1715124160788,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.926666666666,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.011733087293333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009574275093333336,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15761594735333329,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021848100886666666,
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
        "date": 1715159518157,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.93333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.15713830498666662,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009382366686666668,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011807044006666665,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02187081322,
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
        "date": 1715162253692,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.88666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02183534283999999,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1556066974,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011858825546666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009287894293333335,
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
        "date": 1715175245017,
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
            "name": "test-environment",
            "value": 0.011888432413333339,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013755595126666662,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02291126784,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16939412632000003,
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
        "date": 1715190938865,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666662,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02268498238,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013122228380000003,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16447625909999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011241744526666663,
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
        "date": 1715245329133,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.93333333333,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022815363626666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01172858450666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013672441539999998,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16759343136666666,
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
        "date": 1715338678097,
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
            "name": "availability-store",
            "value": 0.17016258271333323,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022903868673333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014497435226666662,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011980256993333329,
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
        "date": 1715347971536,
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
            "name": "availability-store",
            "value": 0.17016258271333323,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022903868673333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014497435226666662,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011980256993333329,
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
        "date": 1715349718364,
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
            "name": "availability-store",
            "value": 0.17016258271333323,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022903868673333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014497435226666662,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011980256993333329,
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
        "date": 1715375680791,
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
            "value": 0.02241609306,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16246377111333335,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01247009012,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010482164940000004,
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
        "date": 1715382238268,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.9,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022808600126666675,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013012511019999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01099445450666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16454326094000002,
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
        "date": 1715534697453,
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
            "name": "test-environment",
            "value": 0.010278997080000005,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012338754953333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16067164566666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.023009661859999996,
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
        "date": 1715556212414,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013592200273333337,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011086830920000002,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1660359637933333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022604919359999996,
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
        "date": 1715559047396,
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
            "name": "availability-distribution",
            "value": 0.011774864246666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021939057493333324,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15700105829999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009530222553333339,
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
        "date": 1715596794973,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022192048966666666,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012061740779999998,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15922303507999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010393080826666668,
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
        "date": 1715619364920,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.011984120606666666,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15819738402666664,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021951596133333327,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009504680233333338,
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
        "date": 1715645370284,
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
            "value": 0.022642198326666675,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16518056376,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012860918453333332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011019291006666668,
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
        "date": 1715683217005,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.011159420260000004,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022619730246666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013190033906666671,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16508149357999993,
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
        "date": 1715855594992,
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
            "value": 0.012467508726666663,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022274249020000005,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01027797778666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16179165370666668,
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
        "date": 1715861751268,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.01222303986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010323962700000005,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022365538186666676,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16171045103333334,
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
        "date": 1715869368491,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.926666666663,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.011867008879999995,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021923196939999997,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15874332420666673,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009396756080000006,
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
        "date": 1715873490146,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022133626033333326,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009794639020000008,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16035781318666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012023172153333334,
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
        "date": 1715882437162,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.91333333333,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.011921292499999998,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16035359782,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009892475186666671,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02202798466,
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
        "date": 1715930676005,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021943088253333336,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15944524373333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00957631380000001,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011793417906666666,
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
        "date": 1715938725992,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.933333333334,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010129529700000006,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012469793173333339,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022101827286666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16189251474666666,
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
        "date": 1715953254719,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.011985559753333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16085110535333333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021990491853333335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009911813840000004,
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
        "date": 1715960835414,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.93333333333,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010471098560000008,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022200026986666663,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16333374604,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012481351900000002,
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
        "date": 1716138300583,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.86666666666,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014716581399999998,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02272844433333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.17338150680666672,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011848364740000008,
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
        "date": 1716147009828,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.87333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.1573437672733334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011570199106666665,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.0218602114,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009043280346666675,
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
        "date": 1716192629905,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.88666666666,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16370914627999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010545235573333336,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012507207533333328,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02261299362,
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
        "date": 1716284921114,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999998,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014137405259999998,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.17103518062666662,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02296263890666667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011874344833333335,
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
        "date": 1716295256704,
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
            "name": "bitfield-distribution",
            "value": 0.022881702806666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011789682280000007,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014011427193333337,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16955196893333338,
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
        "date": 1716305539888,
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
            "name": "bitfield-distribution",
            "value": 0.022966348620000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.012548989546666672,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.17363557151999998,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014730534393333335,
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
        "date": 1716314167161,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.93333333333,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.008720289539999999,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011452595820000007,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021879984066666663,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1538646869466667,
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
        "date": 1716333314668,
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
            "name": "test-environment",
            "value": 0.008818209753333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1548414695266667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01165810972666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02186274034666667,
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
        "date": 1716368411575,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.91333333333,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.011750876760000003,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16723582414000002,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.014747031786666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022776885233333332,
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
        "date": 1716379151571,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.939999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014178919739999995,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022857146199999998,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1693644356466667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.012365678393333336,
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
        "date": 1716384610288,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.16614394844000002,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022991895280000004,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013496233333333336,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010981154653333339,
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
        "date": 1716412316810,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.011359630726666665,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16649834856666668,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022734788646666657,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01340014166666666,
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
        "date": 1716457225756,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.913333333334,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.01311429113333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01131437085333334,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022516358013333328,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16383017571999997,
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
        "date": 1716464598150,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.893333333333,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.1559104204533333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009691771733333337,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011819740113333333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021872563413333337,
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
        "date": 1716473534821,
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
            "value": 0.1564036480266667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021889953026666662,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00948543293333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011920343719999999,
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
        "date": 1716503637976,
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
            "value": 0.022052363466666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009907612166666668,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15699832492666663,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011889911880000001,
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
        "date": 1716542318392,
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
            "value": 0.15647288464666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021983797179999997,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011736843540000002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009596366813333338,
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
        "date": 1716551389974,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.913333333334,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02226725409333333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010352311613333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012105763066666669,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15953374737333337,
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
        "date": 1716557486928,
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
            "value": 0.012146325219999999,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15935638152,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022265146533333332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010172919926666676,
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
        "date": 1716562682549,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02234104528,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010065171773333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012184031133333334,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15952021767333333,
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
        "date": 1716589885656,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.92,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02206091735333333,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012122788266666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1582996468466667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00996092410000001,
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
        "date": 1716796073661,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.906666666662,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012301556146666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01000346835333334,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1584147061933333,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022151191233333332,
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
        "date": 1716801665519,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.88666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012090893506666665,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.021980540553333337,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009944949433333336,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.1573213902,
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
        "date": 1716810736924,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.953333333335,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012116699346666666,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022106360926666672,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009967452086666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15727604116000002,
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
          "id": "16887b6fd5ea637f3c2891d4a41180e9534e63db",
          "message": "chain-spec-builder: help updated (#4597)\n\nAdded some clarification on output file.",
          "timestamp": "2024-05-27T15:10:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/16887b6fd5ea637f3c2891d4a41180e9534e63db"
        },
        "date": 1716824437322,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.87333333333,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.01035459082666667,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16117321578000002,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01241326081333334,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02245979702000001,
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
        "date": 1716835668876,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.88666666666,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012417942466666667,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022402971626666664,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16059003144666664,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010289349493333343,
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
        "date": 1716843809714,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.913333333334,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010700844219999998,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02223878704666666,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012469982533333339,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16050014161333337,
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
        "date": 1716851145037,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.899999999994,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022514186160000005,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012820402039999994,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16351356413999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011057344740000004,
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
        "date": 1716887748459,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18479.939999999995,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.011917855813333336,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02189783440666666,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15705227228666663,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.009202756853333337,
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
        "date": 1716901264006,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.17933795046000003,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.013717471633333334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01151638343333334,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022807574033333333,
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
        "date": 1716912654261,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022933977986666666,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.17956800999333325,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011934011760000003,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01384720616666667,
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
        "date": 1716919213026,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.015093704159999999,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022916573806666674,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.18291474769333335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01245715348,
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
        "date": 1716924871908,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "availability-store",
            "value": 0.18241232578666672,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.012132323333333332,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.01445393812,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022814796573333324,
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
        "date": 1716959673036,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012266621219999994,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02209747388,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16959264074666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010219286753333334,
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
        "date": 1716967639835,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022173055959999997,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012380142826666671,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010392649866666676,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16971708833333335,
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
        "date": 1716977470042,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022244561806666674,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.01046732711333334,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.17262746828666667,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012215049893333335,
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
        "date": 1716983492507,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.012227176886666675,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022083691359999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010172147253333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.17050273181333325,
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
        "date": 1717018510413,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.01245881762,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010566947746666673,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02238501147333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.17179402058000004,
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
        "date": 1717023507563,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02226769586666666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.010264958046666672,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012169118753333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.17081261121999997,
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
        "date": 1717067880522,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 18481.666666666653,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.010787633493333332,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.022482956473333333,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.17394743663333326,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.012542482113333331,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}