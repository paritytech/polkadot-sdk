use sha2::Sha512;
use hmac::Hmac;
use pbkdf2::pbkdf2;
use schnorrkel::keys::{MiniSecretKey, SecretKey};

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
pub fn secret_from_entropy(entropy: &[u8], password: &str) -> Result<SecretKey, Error> {
    let seed = seed_from_entropy(entropy, password)?;
    let mini_secret_key = MiniSecretKey::from_bytes(&seed[..32]).expect("Length is always correct; qed");

    Ok(mini_secret_key.expand::<Sha512>())
}

fn seed_from_entropy(entropy: &[u8], password: &str) -> Result<[u8; 64], Error> {
    if entropy.len() < 16 || entropy.len() > 32 || entropy.len() % 4 != 0 {
        return Err(Error::InvalidEntropy);
    }

    let salt = format!("mnemonic{}", password);

    let mut seed = [0u8; 64];

    pbkdf2::<Hmac<Sha512>>(entropy, salt.as_bytes(), 2048, &mut seed);

    Ok(seed)
}

#[cfg(test)]
mod test {
    use super::*;
    use bip39::{Mnemonic, Language};
    use rustc_hex::FromHex;

    // phrase, entropy, seed, expanded secret_key
    //
    // ALL SEEDS GENERATED USING "Substrate" PASSWORD!
    static VECTORS: &[[&str; 4]] = &[
        [
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
            "00000000000000000000000000000000",
            "44e9d125f037ac1d51f0a7d3649689d422c2af8b1ec8e00d71db4d7bf6d127e33f50c3d5c84fa3e5399c72d6cbbbbc4a49bf76f76d952f479d74655a2ef2d453",
            "b0b3174fe43c15938bb0d0cc5b6f7ac7295f557ee1e6fdeb24fb73f4e0cb2b6ec40ffb9da4af6d411eae8e292750fd105ff70fe93f337b5b590f5a9d9030c750"
        ],
        [
            "legal winner thank year wave sausage worth useful legal winner thank yellow",
            "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
            "4313249608fe8ac10fd5886c92c4579007272cb77c21551ee5b8d60b780416850f1e26c1f4b8d88ece681cb058ab66d6182bc2ce5a03181f7b74c27576b5c8bf",
            "20666c9dd63c5b04a6a14377579af14aba60707752d134726304d0804992e26f9092c47fbb9e14c02fd53c702c8a3cfca4735638599da5c4362e0d0560dceb58"
        ],
        [
            "letter advice cage absurd amount doctor acoustic avoid letter advice cage above",
            "80808080808080808080808080808080",
            "27f3eb595928c60d5bc91a4d747da40ed236328183046892ed6cd5aa9ae38122acd1183adf09a89839acb1e6eaa7fb563cc958a3f9161248d5a036e0d0af533d",
            "709e8254d0a9543c6b35b145dd23349e6369d487a1d10b0cfe09c05ff521f4691ad8bb8221339af38fc48510ec2dfc3104bb94d38f1fa241ceb252943df7b6b5"
        ],
        [
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong",
            "ffffffffffffffffffffffffffffffff",
            "227d6256fd4f9ccaf06c45eaa4b2345969640462bbb00c5f51f43cb43418c7a753265f9b1e0c0822c155a9cabc769413ecc14553e135fe140fc50b6722c6b9df",
            "88206f4b4102ad30ee40b4b5943c5259db77fd576d95d79eeea00160197e406308821814dea9442675a5d3fa375b3bd65ffe92be43e07dbf6bb4ab84e9d4449d"
        ],
        [
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon agent",
            "000000000000000000000000000000000000000000000000",
            "44e9d125f037ac1d51f0a7d3649689d422c2af8b1ec8e00d71db4d7bf6d127e33f50c3d5c84fa3e5399c72d6cbbbbc4a49bf76f76d952f479d74655a2ef2d453",
            "b0b3174fe43c15938bb0d0cc5b6f7ac7295f557ee1e6fdeb24fb73f4e0cb2b6ec40ffb9da4af6d411eae8e292750fd105ff70fe93f337b5b590f5a9d9030c750"
        ],
        [
            "legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth useful legal will",
            "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
            "cb1d50e14101024a88905a098feb1553d4306d072d7460e167a60ccb3439a6817a0afc59060f45d999ddebc05308714733c9e1e84f30feccddd4ad6f95c8a445",
            "50dcb74f223740d6a256000a2f1ccdb60044b39ce3aad71a3bd7761848d5f55d5a34f96e0b96ecb45d7a142e07ddfde734f9525f9f88310ab50e347da5789d3e"
        ],
        [
            "letter advice cage absurd amount doctor acoustic avoid letter advice cage absurd amount doctor acoustic avoid letter always",
            "808080808080808080808080808080808080808080808080",
            "9ddecf32ce6bee77f867f3c4bb842d1f0151826a145cb4489598fe71ac29e3551b724f01052d1bc3f6d9514d6df6aa6d0291cfdf997a5afdb7b6a614c88ab36a",
            "88112947a30e864b511838b6daf6e1e13801ae003d6d9b73eb5892c355f7e37ab3bb71200092d004467b06fe67bc153ee4e2bb7af2f544815b0dde276d2dae75"
        ],
        [
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo when",
            "ffffffffffffffffffffffffffffffffffffffffffffffff",
            "8971cb290e7117c64b63379c97ed3b5c6da488841bd9f95cdc2a5651ac89571e2c64d391d46e2475e8b043911885457cd23e99a28b5a18535fe53294dc8e1693",
            "4859cdeda3f957b7ffcd2d59257c30e43996796f38e1be5c6136c9bf3744e047ce9a52c11793c98d0dc8caee927576ce46ef2e5f4b3f1d5e4b1344b2c31ebe8e"
        ],
        [
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "44e9d125f037ac1d51f0a7d3649689d422c2af8b1ec8e00d71db4d7bf6d127e33f50c3d5c84fa3e5399c72d6cbbbbc4a49bf76f76d952f479d74655a2ef2d453",
            "b0b3174fe43c15938bb0d0cc5b6f7ac7295f557ee1e6fdeb24fb73f4e0cb2b6ec40ffb9da4af6d411eae8e292750fd105ff70fe93f337b5b590f5a9d9030c750"
        ],
        [
            "legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth useful legal winner thank year wave sausage worth title",
            "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
            "3037276a5d05fcd7edf51869eb841bdde27c574dae01ac8cfb1ea476f6bea6ef57ab9afe14aea1df8a48f97ae25b37d7c8326e49289efb25af92ba5a25d09ed3",
            "e8962ace15478f69fa42ddd004aad2c285c9f5a02e0712860e83fbec041f89489046aa57b21db2314d93aeb4a2d7cbdab21d4856e8e151894abd17fb04ae65e1"
        ],
        [
            "letter advice cage absurd amount doctor acoustic avoid letter advice cage absurd amount doctor acoustic avoid letter advice cage absurd amount doctor acoustic bless",
            "8080808080808080808080808080808080808080808080808080808080808080",
            "2c9c6144a06ae5a855453d98c3dea470e2a8ffb78179c2e9eb15208ccca7d831c97ddafe844ab933131e6eb895f675ede2f4e39837bb5769d4e2bc11df58ac42",
            "78667314bf1e52e38d29792cdf294efcaddadc4fa9ce48c5f2bef4daad7ed95d1db960d6f6f895c1a9d2a3ddcc0398ba6578580ea1f03f65ea9a68e97cf3f840"
        ],
        [
            "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo vote",
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
            "047e89ef7739cbfe30da0ad32eb1720d8f62441dd4f139b981b8e2d0bd412ed4eb14b89b5098c49db2301d4e7df4e89c21e53f345138e56a5e7d63fae21c5939",
            "e09f4ae0d4e22f6c9bbc4251c880e73d93d10fdf7f152c393ce36e4e942b3155a6f4b48e6c6ebce902a10d4ab46ef59cd154c15aeb8f7fcd4e1e26342d40806d"
        ],
        [
            "ozone drill grab fiber curtain grace pudding thank cruise elder eight picnic",
            "9e885d952ad362caeb4efe34a8e91bd2",
            "f4956be6960bc145cdab782e649a5056598fd07cd3f32ceb73421c3da27833241324dc2c8b0a4d847eee457e6d4c5429f5e625ece22abaa6a976e82f1ec5531d",
            "b0eb046f48eacb7ad6c4da3ff92bfc29c9ad471bae3e554d5d63e58827160c70c7e5165598761f96b5659ab28c474f50e89ee13c67e30bca40fdcf4335835649"
        ],
        [
            "gravity machine north sort system female filter attitude volume fold club stay feature office ecology stable narrow fog",
            "6610b25967cdcca9d59875f5cb50b0ea75433311869e930b",
            "fbcc5229ade0c0ff018cb7a329c5459f91876e4dde2a97ddf03c832eab7f26124366a543f1485479c31a9db0d421bda82d7e1fe562e57f3533cb1733b001d84d",
            "d8da734285a13967647dd906288e1ac871e1945d0c6b72fa259de4051a9b75431be0c4eb40b1ca38c780f445e3e2809282b9efef4dcc3538355e68094f1e79fa"
        ],
        [
            "hamster diagram private dutch cause delay private meat slide toddler razor book happy fancy gospel tennis maple dilemma loan word shrug inflict delay length",
            "68a79eaca2324873eacc50cb9c6eca8cc68ea5d936f98787c60c7ebc74e6ce7c",
            "7c60c555126c297deddddd59f8cdcdc9e3608944455824dd604897984b5cc369cad749803bb36eb8b786b570c9cdc8db275dbe841486676a6adf389f3be3f076",
            "883b72fa7fb06b6abd0fc2cdb0018b3578e086e93074256bbbb8c68e53c04a56391bc0d19d7b2fa22a8148ccbe191d969c4323faca1935a576cc1b24301f203a"
        ],
        [
            "scheme spot photo card baby mountain device kick cradle pact join borrow",
            "c0ba5a8e914111210f2bd131f3d5e08d",
            "c12157bf2506526c4bd1b79a056453b071361538e9e2c19c28ba2cfa39b5f23034b974e0164a1e8acd30f5b4c4de7d424fdb52c0116bfc6a965ba8205e6cc121",
            "7039a64150089f8d43188af964c7b8e2b8c9ba20aede085baca5672e978a47576c7193c3e557f37cdeeee5e5131b854e4309efc55259b050474e1f0884a7a621"
        ],
        [
            "horn tenant knee talent sponsor spell gate clip pulse soap slush warm silver nephew swap uncle crack brave",
            "6d9be1ee6ebd27a258115aad99b7317b9c8d28b6d76431c3",
            "23766723e970e6b79dec4d5e4fdd627fd27d1ee026eb898feb9f653af01ad22080c6f306d1061656d01c4fe9a14c05f991d2c7d8af8730780de4f94cd99bd819",
            "e07a1f3073edad5b63585cdf1d5e6f8e50e3145de550fc8eb1fb430cce62d76d251904272c5d25fd634615d413bb31a2bc7b5d6eeb2f6ddc68a2b95ac6bd49bc"
        ],
        [
            "panda eyebrow bullet gorilla call smoke muffin taste mesh discover soft ostrich alcohol speed nation flash devote level hobby quick inner drive ghost inside",
            "9f6a2878b2520799a44ef18bc7df394e7061a224d2c33cd015b157d746869863",
            "f4c83c86617cb014d35cd87d38b5ef1c5d5c3d58a73ab779114438a7b358f457e0462c92bddab5a406fe0e6b97c71905cf19f925f356bc673ceb0e49792f4340",
            "607f8595266ac0d4aa91bf4fddbd2a868889317f40099979be9743c46c418976e6ff3717bd11b94b418f91c8b88eae142cecb19104820997ddf5a379dd9da5ae"
        ],
        [
            "cat swing flag economy stadium alone churn speed unique patch report train",
            "23db8160a31d3e0dca3688ed941adbf3",
            "719d4d4de0638a1705bf5237262458983da76933e718b2d64eb592c470f3c5d222e345cc795337bb3da393b94375ff4a56cfcd68d5ea25b577ee9384d35f4246",
            "d078b66bb357f1f06e897a6fdfa2f3dfb0da05836ded1fd0793373068b7e854e783a548a6d194f142e1ba78bf42a49fa58e3673b363ba6f6494efffa28f168df"
        ],
        [
            "light rule cinnamon wrap drastic word pride squirrel upgrade then income fatal apart sustain crack supply proud access",
            "8197a4a47f0425faeaa69deebc05ca29c0a5b5cc76ceacc0",
            "7ae1291db32d16457c248567f2b101e62c5549d2a64cd2b7605d503ec876d58707a8d663641e99663bc4f6cc9746f4852e75e7e54de5bc1bd3c299c9a113409e",
            "5095fe4d0144b06e82aa4753d595fd10de9bba3733eba8ce0784417182317e725fac31b2fb53f4856a5e38866501425b485f4d2eaf2666a9f20ae68f4331ed2c"
        ],
        [
            "all hour make first leader extend hole alien behind guard gospel lava path output census museum junior mass reopen famous sing advance salt reform",
            "066dca1a2bb7e8a1db2832148ce9933eea0f3ac9548d793112d9a95c9407efad",
            "a911a5f4db0940b17ecb79c4dcf9392bf47dd18acaebdd4ef48799909ebb49672947cc15f4ef7e8ef47103a1a91a6732b821bda2c667e5b1d491c54788c69391",
            "8844cb50f3ba8030ab61afee623534836d4ea3677d42bae470fc5e251ea0ca7ec9ea65c8c40be191c7c8683165848279ced81f3a121c9450078a496b6c59f610"
        ],
        [
            "vessel ladder alter error federal sibling chat ability sun glass valve picture",
            "f30f8c1da665478f49b001d94c5fc452",
            "4e2314ca7d9eebac6fe5a05a5a8d3546bc891785414d82207ac987926380411e559c885190d641ff7e686ace8c57db6f6e4333c1081e3d88d7141a74cf339c8f",
            "18917f0c7480c95cd4d98bdc7df773c366d33590252707da1358eb58b43a7b765e3c513878541bfbfb466bb4206f581edf9bf601409c72afac130bcc8b5661b5"
        ],
        [
            "scissors invite lock maple supreme raw rapid void congress muscle digital elegant little brisk hair mango congress clump",
            "c10ec20dc3cd9f652c7fac2f1230f7a3c828389a14392f05",
            "7a83851102849edc5d2a3ca9d8044d0d4f00e5c4a292753ed3952e40808593251b0af1dd3c9ed9932d46e8608eb0b928216a6160bd4fc775a6e6fbd493d7c6b2",
            "b0bf86b0955413fc95144bab124e82042d0cce9c292c1bfd0874ae5a95412977e7bc109aeef33c7c90be952a83f3fe528419776520de721ef6ec9e814749c3fc"
        ],
        [
            "void come effort suffer camp survey warrior heavy shoot primary clutch crush open amazing screen patrol group space point ten exist slush involve unfold",
            "f585c11aec520db57dd353c69554b21a89b20fb0650966fa0a9d6f74fd989d8f",
            "938ba18c3f521f19bd4a399c8425b02c716844325b1a65106b9d1593fbafe5e0b85448f523f91c48e331995ff24ae406757cff47d11f240847352b348ff436ed",
            "c07ba4a979657576f4f7446e3bd2672c87131fa0f472a8bc1f2e9b28c11fb04c66da12cd280662196a5888d8a77178dab8034ed42b11d1654a31db6e1ff4d4c5"
        ]
    ];

    #[test]
    fn vectors_are_correct() {
        for vector in VECTORS {
            let phrase = vector[0];

            let expected_entropy: Vec<u8> = vector[1].from_hex().unwrap();
            let expected_seed: Vec<u8> = vector[2].from_hex().unwrap();
            let expected_secret: Vec<u8> = vector[3].from_hex().unwrap();

            let mnemonic = Mnemonic::from_phrase(phrase, Language::English).unwrap();
            let seed = seed_from_entropy(mnemonic.entropy(), "Substrate").unwrap();
            let secret = secret_from_entropy(mnemonic.entropy(), "Substrate").unwrap().to_bytes();

            assert_eq!(mnemonic.entropy(), &expected_entropy[..], "Entropy is incorrect for {}", phrase);
            assert_eq!(&seed[..], &expected_seed[..], "Seed is incorrect for {}", phrase);
            assert_eq!(&secret[..], &expected_secret[..], "Secret is incorrect for {}", phrase);
        }
    }
}
