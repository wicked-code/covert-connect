use rand_core::{CryptoRng, RngCore};
#[cfg(feature = "aws_lc_rs")]
use aws_lc_rs::aead::{Aad, Algorithm, Nonce, LessSafeKey, UnboundKey, AES_256_GCM, CHACHA20_POLY1305, NONCE_LEN};
#[cfg(feature = "ring")]
use ring::aead::{Aad, Algorithm, Nonce, LessSafeKey, UnboundKey, AES_256_GCM, CHACHA20_POLY1305, NONCE_LEN};
use serde::{Deserialize, Serialize};
use bytes::{BufMut, BytesMut};
use num_enum::{TryFromPrimitive, IntoPrimitive};
use std::marker::PhantomData;

pub struct Aes256Algorithm;
pub struct ChaCha20Poly1305Algorithm;

pub type CipherAes256Gcm = CipherBase<Aes256Algorithm>;
pub type CipherChaCha20Poly1305 = CipherBase<ChaCha20Poly1305Algorithm>;

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum CipherType {
    Aes256Gcm,
    ChaCha20Poly1305
}

impl CipherType
{
    pub fn nonce_size(&self) -> usize {
        NONCE_LEN
    }

    pub fn tag_size(&self) -> usize {
        match self {
            Self::Aes256Gcm => AES_256_GCM.tag_len(),
            Self::ChaCha20Poly1305 => AES_256_GCM.tag_len(),
        }
    }

    pub fn key_size(&self) -> usize {
        match self {
            Self::Aes256Gcm => AES_256_GCM.key_len(),
            Self::ChaCha20Poly1305 => AES_256_GCM.key_len(),
        }
    }
}

pub struct CipherBase<C: GetAlgo>
{
    key: LessSafeKey,
    nonce: [u8; NONCE_LEN],
    algo: PhantomData<C>,
}

impl<C> CipherBase<C>
where
    C : GetAlgo
{
    pub fn new(key: &[u8], mut rng: impl CryptoRng + RngCore) -> Self {
        let unbound_key = UnboundKey::new(C::algo(), key).unwrap();

        let mut nonce = [0u8; NONCE_LEN];
        rng.fill_bytes(&mut nonce);

        Self {
            key: LessSafeKey::new(unbound_key),
            nonce,
            algo: PhantomData,
        }
    }

    pub fn new_with_nonce(key: &[u8], nonce: &[u8]) -> Self {
        let unbound_key = UnboundKey::new(C::algo(), key).unwrap();

        Self {
            key: LessSafeKey::new(unbound_key),
            nonce: nonce.try_into().unwrap(),
            algo: PhantomData,
        }
    }

    pub fn nonce(&self) -> &[u8] {
        &self.nonce
    }

    pub fn nonce_mut(&mut self) -> &mut [u8] {
        &mut self.nonce
    }

    pub fn nonce_size(&self) -> usize {
        NONCE_LEN
    }

    pub fn tag_size(&self) -> usize {
        C::algo().tag_len()
    }

    pub fn encrypt(&mut self, data_buffer: &mut BytesMut, start_pos: usize) {
        let (_, data) = data_buffer.split_at_mut(start_pos);

        let nonce = Nonce::try_assume_unique_for_key(self.nonce()).unwrap();
        let tag = self.key
            .seal_in_place_separate_tag(nonce, Aad::empty(), data)
            .unwrap();
        data_buffer.put(tag.as_ref());
    }

    pub fn decrypt(&mut self, data_buffer: &mut BytesMut) -> bool {
        let nonce = Nonce::try_assume_unique_for_key(self.nonce()).unwrap();
        self.key.open_in_place(nonce, Aad::empty(), data_buffer).is_ok()
    }    
}

pub trait GetAlgo {
    fn algo() -> &'static Algorithm;
}

impl GetAlgo for Aes256Algorithm {
    fn algo() -> &'static Algorithm {
        &AES_256_GCM
    }
}

impl GetAlgo for ChaCha20Poly1305Algorithm {
    fn algo() -> &'static Algorithm {
        &CHACHA20_POLY1305
    }
}
