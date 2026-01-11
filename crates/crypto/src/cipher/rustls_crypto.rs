use rand_core::{CryptoRng, RngCore};
use aead::{
    generic_array::typenum::Unsigned, AeadCore, AeadInPlace, Key, KeyInit, KeySizeUser, Nonce, Tag
};
use serde::{Deserialize, Serialize};
use bytes::{BufMut, BytesMut};
use aes_gcm::Aes256Gcm;
use chacha20poly1305::ChaCha20Poly1305;

pub type CipherAes256Gcm = Box<CipherBase<Aes256Gcm>>;
pub type CipherChaCha20Poly1305 = CipherBase<ChaCha20Poly1305>;

#[derive(Clone, Copy, Deserialize, Serialize)]
pub enum CipherType {
    Aes256Gcm,
    ChaCha20Poly1305
}

impl CipherType
{
    pub fn nonce_size(&self) -> usize {
        match self {
            Self::Aes256Gcm => <Aes256Gcm as AeadCore>::NonceSize::to_usize(),
            Self::ChaCha20Poly1305 => <ChaCha20Poly1305 as AeadCore>::NonceSize::to_usize(),
        }
    }

    pub fn tag_size(&self) -> usize {
        match self {
            Self::Aes256Gcm => <Aes256Gcm as AeadCore>::TagSize::to_usize(),
            Self::ChaCha20Poly1305 => <ChaCha20Poly1305 as AeadCore>::TagSize::to_usize(),
        }
    }

    pub fn key_size(&self) -> usize {
        match self {
            Self::Aes256Gcm => Aes256Gcm::key_size(),
            Self::ChaCha20Poly1305 => ChaCha20Poly1305::key_size(),
        }
    }
}

pub struct CipherBase<C>
where
    C: AeadCore + KeySizeUser + KeyInit + AeadInPlace,
{
    cipher: C,
    nonce: Nonce<C>,
}

impl<C> CipherBase<C>
where
    C: AeadCore + KeySizeUser + KeyInit + AeadInPlace,
{
    pub fn new(key: &[u8], mut rng: impl CryptoRng + RngCore) -> Self {
        let key = Key::<C>::from_slice(key);

        Self {
            cipher: C::new(key),
            nonce: C::generate_nonce(&mut rng),
        }
    }

    pub fn new_with_nonce(key: &[u8], nonce: &[u8]) -> Self {
        let key = Key::<C>::from_slice(key);

        Self {
            cipher: C::new(key),
            nonce: Nonce::<C>::clone_from_slice(nonce),
        }
    }

    pub fn nonce(&self) -> &[u8] {
        &self.nonce
    }

    pub fn nonce_mut(&mut self) -> &mut [u8] {
        &mut self.nonce
    }

    pub fn nonce_size(&self) -> usize {
        <C as AeadCore>::NonceSize::to_usize()
    }

    pub fn tag_size(&self) -> usize {
        <C as AeadCore>::TagSize::to_usize()
    }

    pub fn encrypt(&mut self, data_buffer: &mut BytesMut, start_pos: usize) {
        let (_, data) = data_buffer.split_at_mut(start_pos);
        
        let tag = self.cipher
            .encrypt_in_place_detached(&self.nonce, &[], data)
            .unwrap();
        data_buffer.put(tag.as_slice());
    }

    pub fn decrypt(&mut self, data_buffer: &mut BytesMut) -> bool {
        let tag_pos = data_buffer.len() - self.tag_size();

        let (data, tag) = data_buffer.split_at_mut(tag_pos);
        self.cipher
            .decrypt_in_place_detached(&self.nonce, &[], data, Tag::<C>::from_slice(tag))
            .is_ok()
    }    
}
