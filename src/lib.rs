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
	if entropy.len() < 16 || entropy.len() > 32 || entropy.len() % 4 != 0 {
		return Err(Error::InvalidEntropy);
	}

    let salt = format!("mnemonic{}", password);

	let mut seed = [0u8; 64];

    pbkdf2::<Hmac<Sha512>>(entropy, salt.as_bytes(), 2048, &mut seed);

    let mini_secret_key = MiniSecretKey::from_bytes(&seed[..32]).expect("Length is always correct; qed");

    Ok(mini_secret_key.expand::<Sha512>())
}
