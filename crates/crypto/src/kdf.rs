use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use argon2::Argon2;
use blake2::{Blake2b512, Digest};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use num_enum::{TryFromPrimitive, IntoPrimitive};
use rand::prelude::*;
use rand_chacha::ChaCha20Rng;
use hex_literal::hex;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, TryFromPrimitive, IntoPrimitive)]
pub enum Kdf {
    Argon2,
    Blake3
}

const KEY_LEN : usize = 32;

const TIME_SALT: &[u8;KEY_LEN] = &hex!("5c980be021981e1f3c17af9b8230c09df4a8315d2879ca0aae50c0b97a113567");
const SERVER_SALT: &[u8;KEY_LEN] = &hex!("b67c9161f48f1aa8cea536ee2a733ad7b72d2465fe109af8ffe1f883f7576df6");
const CLIENT_SALT: &[u8;KEY_LEN] = &hex!("d182a1c62e0008bacb12d22ea14738b7eb997faa56f0f08a4270f1d19fcf87e3");
const HTTPS_PATH_SALT: &[u8;KEY_LEN] = &hex!("c08d712e6ba79cdeb83769f3bc9cd7ee6a2777e11beb3b96691fad255dad12b8");
const PROTOCOL_RESPONSE_SALT: &[u8;KEY_LEN] = &hex!("3368714db61844018dbb0cd7214425800c1d87ea9ae6edeb97e5bd5d462c3808");

impl Kdf {
    // u16 mean 65s+ max, that should be more than enough (default is 10000ms)
    pub fn derive_key_from_timestamp(&self, key: &[u8], timestamp: i64, out: &mut [u8]) -> Result<()> {
        // time just for replay protection
        // it's some kind of know data thus we don't need a stronger hasher
        let mut hasher = Blake2b512::new();
        hasher.update(timestamp.to_le_bytes());
        hasher.update(TIME_SALT);
        let time_hash = hasher.finalize();

        self.derive_key(key, &time_hash, out)
    }

    pub fn derive_key(&self, key: &[u8], salt: &[u8], out: &mut [u8]) -> Result<()> {
        match self {
            Kdf::Argon2 => {
                Argon2::default()
                    .hash_password_into(key, salt, out)
                    .map_err(|err| anyhow!("{err}"))
            },
            Kdf::Blake3 => {
                let mut hasher = blake3::Hasher::new();
                hasher.update(key);
                hasher.update(salt);
                out.copy_from_slice(hasher.finalize().as_bytes());
                Ok(())
            }
        }
    }

    pub fn derive_client_key(&self, key: &[u8], salt: &[u8], out: &mut [u8]) -> Result<()> {
        self.derive_key2(key, salt, SERVER_SALT, out)
    }

    pub fn derive_server_key(&self, key: &[u8], salt: &[u8], out: &mut [u8]) -> Result<()> {
        self.derive_key2(key, salt, CLIENT_SALT, out)
    }

    pub fn derive_protocol_response_key(&self, key: &[u8], salt: &[u8], out: &mut [u8]) -> Result<()> {
        self.derive_key2(key, salt, PROTOCOL_RESPONSE_SALT, out)
    }

    pub fn derive_url_path(key: &str) -> Result<String> {
        // use Aragon since this used only once per start
        let mut url_path_data = [0u8; KEY_LEN];
        Kdf::Argon2.derive_key(key.as_bytes(), HTTPS_PATH_SALT, &mut url_path_data)?;

        Ok(URL_SAFE_NO_PAD.encode(url_path_data))
    }

    pub fn generate_new_key() -> String {
        let mut key_data = [0u8; KEY_LEN];

        let mut rng = ChaCha20Rng::from_entropy();
        rng.fill_bytes(&mut key_data);

        URL_SAFE_NO_PAD.encode(key_data)
    }

    fn derive_key2(&self, key: &[u8], salt1: &[u8], salt2: &[u8], out: &mut [u8]) -> Result<()> {
        let mut salt = [0u8;32];
        self.derive_key(salt1, salt2, &mut salt)?;
        self.derive_key(key, &salt, out)
    }
}