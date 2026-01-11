use rand_core::{CryptoRng, RngCore};
use bytes::BytesMut;
use anyhow::Result;
use super::kdf::Kdf;

#[cfg(any(feature = "aws_lc_rs", feature = "ring"))]
mod ring_like_crypto;
#[cfg(any(feature = "aws_lc_rs", feature = "ring"))]
use ring_like_crypto as crypto;

#[cfg(feature = "rust_crypto")]
mod rustls_crypto;
#[cfg(feature = "rust_crypto")]
use rustls_crypto as crypto;

use crypto::{CipherAes256Gcm, CipherChaCha20Poly1305, CipherBase};
pub use crypto::CipherType;

pub enum Cipher {
    Aes256Gcm(CipherAes256Gcm),
    ChaCha20Poly1305(CipherChaCha20Poly1305)
}

impl Cipher {
    #[allow(clippy::useless_conversion)]
    pub fn new(cipher: CipherType, key: &[u8], rng: impl CryptoRng + RngCore) -> Self {
        match cipher {
            CipherType::Aes256Gcm => Cipher::Aes256Gcm(CipherBase::new(key, rng).into()),
            CipherType::ChaCha20Poly1305 => Cipher::ChaCha20Poly1305(CipherBase::new(key, rng).into()),
        }
    }

    #[allow(clippy::useless_conversion)]
    pub fn new_with_nonce(cipher: CipherType, key: &[u8], nonce: &[u8]) -> Self {
        match cipher {
            CipherType::Aes256Gcm => Cipher::Aes256Gcm(CipherBase::new_with_nonce(key, nonce).into()),
            CipherType::ChaCha20Poly1305 => Cipher::ChaCha20Poly1305(CipherBase::new_with_nonce(key, nonce).into()),
        }
    }

    pub fn new_client_server(cipher: CipherType, kdf: Kdf, pass: &str, salt: &[u8]) -> Result<(Cipher, Cipher)> {
        let key_size = cipher.key_size();
        let nonce_size = cipher.nonce_size();

        // client stream cipher
        let mut client_key = BytesMut::zeroed(key_size);
        kdf.derive_client_key(pass.as_bytes(), salt, &mut client_key)?;
        let client_cipher = Cipher::new_with_nonce(cipher, &client_key, &salt[0..nonce_size]);

        // server stream cipher
        let mut server_key = BytesMut::zeroed(key_size);
        kdf.derive_server_key(pass.as_bytes(), salt, &mut server_key)?;
        let server_cipher = Cipher::new_with_nonce(cipher, &server_key, &salt[key_size - nonce_size..key_size]);

        Ok((client_cipher, server_cipher))
    }

    pub fn nonce(&self) -> &[u8] {
        match self {
            Self::Aes256Gcm(c) => c.nonce(),
            Self::ChaCha20Poly1305(c) => c.nonce(),
        }
    }

    pub fn nonce_size(&self) -> usize {
        match self {
            Self::Aes256Gcm(c) => c.nonce_size(),
            Self::ChaCha20Poly1305(c) => c.nonce_size(),
        }
    }

    pub fn tag_size(&self) -> usize {
        match self {
            Self::Aes256Gcm(c) => c.tag_size(),
            Self::ChaCha20Poly1305(c) => c.tag_size(),
        }
    }

    pub fn encrypt(&mut self, data_buffer: &mut BytesMut, start_pos: usize) {
        match self {
            Self::Aes256Gcm(c) => c.encrypt(data_buffer, start_pos),
            Self::ChaCha20Poly1305(c) => c.encrypt(data_buffer, start_pos),
        }
    }

    pub fn decrypt(&mut self, data_buffer: &mut BytesMut) -> bool {
        match self {
            Self::Aes256Gcm(c) => c.decrypt(data_buffer),
            Self::ChaCha20Poly1305(c) => c.decrypt(data_buffer),
        }
    }

    pub fn inc_nonce(&mut self, value: u16) {
        let mut add = value as u32;
        let mut rest = self.nonce_mut();
        while add > 0 && !rest.is_empty() {
            let word;
            (word, rest) = rest.split_at_mut(2);
            let sum = u16::from_le_bytes(word.try_into().unwrap()) as u32 + add;
            word.copy_from_slice(&u16::to_le_bytes(sum as u16));
            add = sum >> 16;
        }
    }

    fn nonce_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Aes256Gcm(c) => c.nonce_mut(),
            Self::ChaCha20Poly1305(c) => c.nonce_mut(),
        }
    }
}

#[cfg(test)]
mod tests {

    use super::{Cipher, CipherType};

    #[test]
    fn inc_nonce() {
        check_nonce(0, 1);
        check_nonce(0, u8::MAX as u16);
        check_nonce(0, u16::MAX);
        check_nonce(1, 1);
        check_nonce(1, u8::MAX as u16);
        check_nonce(1, u16::MAX);
        check_nonce2(0);
        check_nonce2((u8::MAX as u128) << 16);
        check_nonce2((u16::MAX as u128) << 16);
        check_nonce2((u32::MAX as u128) << 16);
        check_nonce2((u64::MAX as u128) << 16);
        check_nonce2(((u64::MAX as u128) << 32) + ((u16::MAX as u128) << 16));
    }

    fn check_nonce(nonce: u128, inc: u16) {
        let key = [0u8;32];
        let nonce_array = u128::to_le_bytes(nonce);

        let mut cipher = Cipher::new_with_nonce(CipherType::Aes256Gcm, &key, &nonce_array[..12]);
        cipher.inc_nonce(inc);

        let mut res_array = [0u8;16];
        res_array.as_mut()[..12].copy_from_slice(cipher.nonce());
        let res = u128::from_le_bytes(res_array);

        let mut correct_result = nonce + inc as u128;
        if correct_result >= ((1 as u128) << 96) {
            correct_result &= ((1 as u128) << 96) - 1;
        }

        assert_eq!(res, correct_result);
    }

    fn check_nonce2(nonce_upper_part: u128) {
        check_nonce(u8::MAX as u128 + nonce_upper_part, 1);
        check_nonce(u8::MAX as u128 + nonce_upper_part, u8::MAX as u16);
        check_nonce(u8::MAX as u128 + nonce_upper_part, u16::MAX);
        check_nonce(u16::MAX as u128 + nonce_upper_part, 1);
        check_nonce(u16::MAX as u128 + nonce_upper_part, u8::MAX as u16);
        check_nonce(u16::MAX as u128 + nonce_upper_part, u16::MAX);
    }
}