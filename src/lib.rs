// Copyright 2019-2020 Parity Technologies (UK) Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at

//     http://www.apache.org/licenses/LICENSE-2.0

// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::string::String;

use hmac::Hmac;
use pbkdf2::pbkdf2;
use schnorrkel::keys::MiniSecretKey;
use sha2::Sha512;
use zeroize::Zeroize;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    InvalidEntropy,
}

/// `entropy` should be a byte array from a correctly recovered and checksumed BIP39.
///
/// This function accepts slices of different length for different word lengths:
///
/// + 16 bytes for 12 words.
/// + 20 bytes for 15 words.
/// + 24 bytes for 18 words.
/// + 28 bytes for 21 words.
/// + 32 bytes for 24 words.
///
/// Any other length will return an error.
///
/// `password` is analog to BIP39 seed generation itself, with an empty string being defalt.
pub fn mini_secret_from_entropy(entropy: &[u8], password: &str) -> Result<MiniSecretKey, Error> {
    let seed = seed_from_entropy(entropy, password)?;
    Ok(MiniSecretKey::from_bytes(&seed[..32]).expect("Length is always correct; qed"))
}

/// Similar to `mini_secret_from_entropy`, except that it provides the 64-byte seed directly.
pub fn seed_from_entropy(entropy: &[u8], password: &str) -> Result<[u8; 64], Error> {
    if entropy.len() < 16 || entropy.len() > 32 || entropy.len() % 4 != 0 {
        return Err(Error::InvalidEntropy);
    }

    let mut salt = String::with_capacity(8 + password.len());
    salt.push_str("mnemonic");
    salt.push_str(password);

    let mut seed = [0u8; 64];

    pbkdf2::<Hmac<Sha512>>(entropy, salt.as_bytes(), 2048, &mut seed);

    salt.zeroize();

    Ok(seed)
}

#[cfg(test)]
mod test {
    use super::*;

    #[cfg(not(feature = "std"))]
    use alloc::vec::Vec;

    use bip39::{Language, Mnemonic};
    use rustc_hex::FromHex;

    // phrase, entropy, seed, expanded secret_key
    //
    // ALL SEEDS GENERATED USING "Substrate" PASSWORD!
    static VECTORS: &[[&str; 3]] = &[
        [
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            "00000000000000000000000000000000",
            "44e9d125f037ac1d51f0a7d3649689d422c2af8b1ec8e00d71db4d7bf6d127e33f50c3d5c84fa3e5399c72d6cbbbbc4a49bf76f76d952f479d74655a2ef2d453",
        ],
        [
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
            "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
            "4313249608fe8ac10fd5886c92c4579007272cb77c21551ee5b8d60b780416850f1e26c1f4b8d88ece681cb058ab66d6182bc2ce5a03181f7b74c27576b5c8bf",
        ],
        [
            "letter advice cage absurd amount doctor acoustic avoid letter advice cage above",
            "80808080808080808080808080808080",
            "27f3eb595928c60d5bc91a4d747da40ed236328183046892ed6cd5aa9ae38122acd1183adf09a89839acb1e6eaa7fb563cc958a3f9161248d5a036e0d0af533d",
        ],
        [
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong",
            "ffffffffffffffffffffffffffffffff",
            "227d6256fd4f9ccaf06c45eaa4b2345969640462bbb00c5f51f43cb43418c7a753265f9b1e0c0822c155a9cabc769413ecc14553e135fe140fc50b6722c6b9df",
        ],
        [
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon agent",
            "000000000000000000000000000000000000000000000000",
            "44e9d125f037ac1d51f0a7d3649689d422c2af8b1ec8e00d71db4d7bf6d127e33f50c3d5c84fa3e5399c72d6cbbbbc4a49bf76f76d952f479d74655a2ef2d453",
        ],
        [
            "legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth useful legal will",
            "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
            "cb1d50e14101024a88905a098feb1553d4306d072d7460e167a60ccb3439a6817a0afc59060f45d999ddebc05308714733c9e1e84f30feccddd4ad6f95c8a445",
        ],
        [
            "letter advice cage absurd amount doctor acoustic avoid letter advice cage absurd amount doctor acoustic avoid letter always",
            "808080808080808080808080808080808080808080808080",
            "9ddecf32ce6bee77f867f3c4bb842d1f0151826a145cb4489598fe71ac29e3551b724f01052d1bc3f6d9514d6df6aa6d0291cfdf997a5afdb7b6a614c88ab36a",
        ],
        [
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo when",
            "ffffffffffffffffffffffffffffffffffffffffffffffff",
            "8971cb290e7117c64b63379c97ed3b5c6da488841bd9f95cdc2a5651ac89571e2c64d391d46e2475e8b043911885457cd23e99a28b5a18535fe53294dc8e1693",
        ],
        [
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "44e9d125f037ac1d51f0a7d3649689d422c2af8b1ec8e00d71db4d7bf6d127e33f50c3d5c84fa3e5399c72d6cbbbbc4a49bf76f76d952f479d74655a2ef2d453",
        ],
        [
            "legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth title",
            "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
            "3037276a5d05fcd7edf51869eb841bdde27c574dae01ac8cfb1ea476f6bea6ef57ab9afe14aea1df8a48f97ae25b37d7c8326e49289efb25af92ba5a25d09ed3",
        ],
        [
            "letter advice cage absurd amount doctor acoustic avoid letter advice cage absurd amount doctor acoustic avoid letter advice cage absurd amount doctor acoustic bless",
            "8080808080808080808080808080808080808080808080808080808080808080",
            "2c9c6144a06ae5a855453d98c3dea470e2a8ffb78179c2e9eb15208ccca7d831c97ddafe844ab933131e6eb895f675ede2f4e39837bb5769d4e2bc11df58ac42",
        ],
        [
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo vote",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "047e89ef7739cbfe30da0ad32eb1720d8f62441dd4f139b981b8e2d0bd412ed4eb14b89b5098c49db2301d4e7df4e89c21e53f345138e56a5e7d63fae21c5939",
        ],
        [
            "ozone drill grab fiber curtain grace pudding thank cruise elder eight picnic",
            "9e885d952ad362caeb4efe34a8e91bd2",
            "f4956be6960bc145cdab782e649a5056598fd07cd3f32ceb73421c3da27833241324dc2c8b0a4d847eee457e6d4c5429f5e625ece22abaa6a976e82f1ec5531d",
        ],
        [
            "gravity machine north sort system female filter attitude volume fold club stay feature office ecology stable narrow fog",
            "6610b25967cdcca9d59875f5cb50b0ea75433311869e930b",
            "fbcc5229ade0c0ff018cb7a329c5459f91876e4dde2a97ddf03c832eab7f26124366a543f1485479c31a9db0d421bda82d7e1fe562e57f3533cb1733b001d84d",
        ],
        [
            "hamster diagram private dutch cause delay private meat slide toddler razor book happy fancy gospel tennis maple dilemma loan word shrug inflict delay length",
            "68a79eaca2324873eacc50cb9c6eca8cc68ea5d936f98787c60c7ebc74e6ce7c",
            "7c60c555126c297deddddd59f8cdcdc9e3608944455824dd604897984b5cc369cad749803bb36eb8b786b570c9cdc8db275dbe841486676a6adf389f3be3f076",
        ],
        [
            "scheme spot photo card baby mountain device kick cradle pact join borrow",
            "c0ba5a8e914111210f2bd131f3d5e08d",
            "c12157bf2506526c4bd1b79a056453b071361538e9e2c19c28ba2cfa39b5f23034b974e0164a1e8acd30f5b4c4de7d424fdb52c0116bfc6a965ba8205e6cc121",
        ],
        [
            "horn tenant knee talent sponsor spell gate clip pulse soap slush warm silver nephew swap uncle crack brave",
            "6d9be1ee6ebd27a258115aad99b7317b9c8d28b6d76431c3",
            "23766723e970e6b79dec4d5e4fdd627fd27d1ee026eb898feb9f653af01ad22080c6f306d1061656d01c4fe9a14c05f991d2c7d8af8730780de4f94cd99bd819",
        ],
        [
            "panda eyebrow bullet gorilla call smoke muffin taste mesh discover soft ostrich alcohol speed nation flash devote level hobby quick inner drive ghost inside",
            "9f6a2878b2520799a44ef18bc7df394e7061a224d2c33cd015b157d746869863",
            "f4c83c86617cb014d35cd87d38b5ef1c5d5c3d58a73ab779114438a7b358f457e0462c92bddab5a406fe0e6b97c71905cf19f925f356bc673ceb0e49792f4340",
        ],
        [
            "cat swing flag economy stadium alone churn speed unique patch report train",
            "23db8160a31d3e0dca3688ed941adbf3",
            "719d4d4de0638a1705bf5237262458983da76933e718b2d64eb592c470f3c5d222e345cc795337bb3da393b94375ff4a56cfcd68d5ea25b577ee9384d35f4246",
        ],
        [
            "light rule cinnamon wrap drastic word pride squirrel upgrade then income fatal apart sustain crack supply proud access",
            "8197a4a47f0425faeaa69deebc05ca29c0a5b5cc76ceacc0",
            "7ae1291db32d16457c248567f2b101e62c5549d2a64cd2b7605d503ec876d58707a8d663641e99663bc4f6cc9746f4852e75e7e54de5bc1bd3c299c9a113409e",
        ],
        [
            "all hour make first leader extend hole alien behind guard gospel lava path output census museum junior mass reopen famous sing advance salt reform",
            "066dca1a2bb7e8a1db2832148ce9933eea0f3ac9548d793112d9a95c9407efad",
            "a911a5f4db0940b17ecb79c4dcf9392bf47dd18acaebdd4ef48799909ebb49672947cc15f4ef7e8ef47103a1a91a6732b821bda2c667e5b1d491c54788c69391",
        ],
        [
            "vessel ladder alter error federal sibling chat ability sun glass valve picture",
            "f30f8c1da665478f49b001d94c5fc452",
            "4e2314ca7d9eebac6fe5a05a5a8d3546bc891785414d82207ac987926380411e559c885190d641ff7e686ace8c57db6f6e4333c1081e3d88d7141a74cf339c8f",
        ],
        [
            "scissors invite lock maple supreme raw rapid void congress muscle digital elegant little brisk hair mango congress clump",
            "c10ec20dc3cd9f652c7fac2f1230f7a3c828389a14392f05",
            "7a83851102849edc5d2a3ca9d8044d0d4f00e5c4a292753ed3952e40808593251b0af1dd3c9ed9932d46e8608eb0b928216a6160bd4fc775a6e6fbd493d7c6b2",
        ],
        [
            "void come effort suffer camp survey warrior heavy shoot primary clutch crush open amazing screen patrol group space point ten exist slush involve unfold",
            "f585c11aec520db57dd353c69554b21a89b20fb0650966fa0a9d6f74fd989d8f",
            "938ba18c3f521f19bd4a399c8425b02c716844325b1a65106b9d1593fbafe5e0b85448f523f91c48e331995ff24ae406757cff47d11f240847352b348ff436ed",
        ]
    ];

    #[test]
    fn vectors_are_correct() {
        for vector in VECTORS {
            let phrase = vector[0];

            let expected_entropy: Vec<u8> = vector[1].from_hex().unwrap();
            let expected_seed: Vec<u8> = vector[2].from_hex().unwrap();

            let mnemonic = Mnemonic::from_phrase(phrase, Language::English).unwrap();
            let seed = seed_from_entropy(mnemonic.entropy(), "Substrate").unwrap();
            let secret = mini_secret_from_entropy(mnemonic.entropy(), "Substrate")
                .unwrap()
                .to_bytes();

            assert_eq!(
                mnemonic.entropy(),
                &expected_entropy[..],
                "Entropy is incorrect for {}",
                phrase
            );
            assert_eq!(
                &seed[..],
                &expected_seed[..],
                "Seed is incorrect for {}",
                phrase
            );
            assert_eq!(
                &secret[..],
                &expected_seed[..32],
                "Secret is incorrect for {}",
                phrase
            );
        }
    }
}
