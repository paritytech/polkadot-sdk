const fs = require("fs");
const { exit } = require("process");
const { xxhashAsHex } = require("@polkadot/util-crypto");

// Utility script scraping a chain spec for the genesis keys and values and writing them out as a
// json array of pairs. Filters the keys for anything already present in a shell runtime and sorts
// the output for reproducibility.

if (!process.argv[2] || !process.argv[3]) {
  console.log("usage: node generate_keys <input chainspec> <output json>");
  exit();
}

const input = process.argv[2];
const output = process.argv[3];
fs.readFile(input, "utf8", (err, data) => {
  if (err) {
    console.log(`Error reading file from disk: ${err}`);
    exit(1);
  }

  const toHex = (str) => "0x" + Buffer.from(str, "ascii").toString("hex");
  const startsWith = (str, arr) => arr.some((test) => str.startsWith(test));

  const filter_prefixes = [
    // substrate well known keys
    ":code",
    ":heappages",
    ":extrinsic_index",
    ":changes_trie",
    ":child_storage",
  ]
    .map(toHex)
    .concat(
      // shell pallets
      ["System", "ParachainSystem", "ParachainInfo", "CumulusXcm"].map((str) =>
        xxhashAsHex(str)
      )
    )
    .concat([
      // polkadot well known keys; don't seem necessary, but just to make sure
      "0x06de3d8a54d27e44a9d5ce189618f22db4b49d95320d9021994c850f25b8e385",
      "0xf5207f03cfdce586301014700e2c2593fad157e461d71fd4c1f936839a5f1f3e",
      "0x6a0da05ca59913bc38a8630590f2627cb6604cff828a6e3f579ca6c59ace013d",
      "0x6a0da05ca59913bc38a8630590f2627c1d3719f5b0b12c7105c073c507445948",
      "0x6a0da05ca59913bc38a8630590f2627cf12b746dcf32e843354583c9702cc020",
      "0x63f78c98723ddc9073523ef3beefda0c4d7fefc408aac59dbfe80a72ac8e3ce5",
    ]);

  const spec = JSON.parse(data);

  const genesis =
    Object.entries(spec.genesis.raw.top).filter(
      ([key, value]) => !startsWith(key, filter_prefixes)
    );
  genesis.sort();

  fs.writeFileSync(output, JSON.stringify(genesis));
});
