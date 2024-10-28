import { test, expect } from "bun:test";
import { wnd, XcmV3Junctions, XcmV3Junction, XcmV3MultiassetFungibility } from "@polkadot-api/descriptors";
import { Enum, createClient } from "polkadot-api";
import { getWsProvider } from "polkadot-api/ws-provider/web";
import { withPolkadotSdkCompat } from "polkadot-api/polkadot-sdk-compat";

let client = createClient(
    withPolkadotSdkCompat(
        getWsProvider("ws://localhost:8001")
    )
);
let dotApi = client.getTypedApi(wnd);

const testCases = [
  {
    description: 'Test with trusted location',
    asset: Enum('V4', {
      id: {
        parents: 0,
        interior: XcmV3Junctions.Here(),
      },
      fun: XcmV3MultiassetFungibility.Fungible(1n),
    }),
    location: Enum('V4', {
      parents: 0,
      interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(1000))
    }),
    want: true
  },
  {
    description: 'Test with untrusted location',
    asset: Enum('V4', {
      id: {
        parents: 0,
        interior: XcmV3Junctions.Here(),
      },
      fun: XcmV3MultiassetFungibility.Fungible(2n),
    }),
    location: Enum('V4', {
      parents: 0,
      interior: XcmV3Junctions.X1(XcmV3Junction.Parachain(2000))
    }),
    want: false
  },
];

testCases.forEach(({ description, asset, location, want }) => {
  test(description, async () => {
    const resp = await dotApi.apis.TrustedQueryApi.is_trusted_teleporter(asset, location);

    expect(resp.success).toBe(true);
    expect(resp.value).toBe(want);

    console.log(`Test: ${description}, want: ${want}, got: ${resp.value}`);
  });
});
