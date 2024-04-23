window.BENCHMARK_DATA = {
  "lastUpdate": 1713867221549,
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
      }
    ]
  }
}