window.BENCHMARK_DATA = {
  "lastUpdate": 1713819804787,
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
      }
    ]
  }
}