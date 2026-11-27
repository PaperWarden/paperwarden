//! The PDF standard security handler (ISO 32000-2, 7.6.4), only so encrypted
//! streams can be measured. Decryption happens in memory, with the empty user
//! password only, and nothing decrypted leaves the scan.
//!
//! Supported: revisions 2 to 4 (RC4 and AES-128) and 5 to 6 (AES-256), with
//! crypt filters. A document that needs a real password, or uses another
//! handler (certificates, custom handlers), is refused.

use crate::syntax::{Dict, Object};
use aes::cipher::{BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use aes::{Aes128, Aes256, Block};
use md5::{Digest, Md5};
use sha2::{Sha256, Sha384, Sha512};

/// Algorithm 2's padding string.
const PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// Why a document's encryption cannot be measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CryptError {
    /// The empty user password does not open it.
    NeedsPassword,
    /// Another security handler, an unknown revision or a malformed
    /// encryption dictionary.
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Method {
    /// The stream is not encrypted.
    Identity,
    Rc4,
    Aes128,
    Aes256,
}

/// Decrypts a document's streams with the key the empty password gives.
#[derive(Debug)]
pub(crate) struct Decryptor {
    key: Vec<u8>,
    streams: Method,
    encrypt_metadata: bool,
}

fn bytes<'d>(dict: &'d Dict, key: &[u8]) -> Option<&'d [u8]> {
    match dict.get(key) {
        Some(Object::Str(Some(value))) => Some(value),
        _ => None,
    }
}

/// The crypt method a crypt filter name stands for (V4 and V5).
fn filter_method(encrypt: &Dict, name: Option<&[u8]>) -> Result<Method, CryptError> {
    let name = name.unwrap_or(b"Identity");
    if name == b"Identity" {
        return Ok(Method::Identity);
    }
    let Some(Object::Dict(filters)) = encrypt.get(b"CF") else {
        return Err(CryptError::Unsupported);
    };
    let Some(Object::Dict(filter)) = filters.get(name) else {
        return Err(CryptError::Unsupported);
    };
    match filter.name(b"CFM").unwrap_or(b"None") {
        b"None" => Ok(Method::Identity),
        b"V2" => Ok(Method::Rc4),
        b"AESV2" => Ok(Method::Aes128),
        b"AESV3" => Ok(Method::Aes256),
        _ => Err(CryptError::Unsupported),
    }
}

impl Decryptor {
    /// Opens the standard security handler with the empty password.
    /// `id` is the first element of the trailer's `/ID`.
    pub(crate) fn new(encrypt: &Dict, id: &[u8]) -> Result<Self, CryptError> {
        if encrypt.name(b"Filter") != Some(&b"Standard"[..]) {
            return Err(CryptError::Unsupported);
        }
        let version = encrypt.int(b"V").unwrap_or(0);
        let revision = encrypt.int(b"R").ok_or(CryptError::Unsupported)?;
        let encrypt_metadata =
            !matches!(encrypt.get(b"EncryptMetadata"), Some(Object::Bool(false)));
        let streams = match version {
            1 | 2 => Method::Rc4,
            4 | 5 => filter_method(encrypt, encrypt.name(b"StmF"))?,
            _ => return Err(CryptError::Unsupported),
        };
        let owner = bytes(encrypt, b"O").ok_or(CryptError::Unsupported)?;
        let user = bytes(encrypt, b"U").ok_or(CryptError::Unsupported)?;
        let key = match revision {
            2..=4 => {
                let length = match (version, streams) {
                    (1, _) => 5,
                    (4, Method::Aes128) => 16,
                    _ => {
                        let bits = encrypt.int(b"Length").unwrap_or(40);
                        usize::try_from(bits / 8).map_err(|_| CryptError::Unsupported)?
                    }
                };
                // Revisions 2 to 4 store 32-byte keys; anything shorter is
                // malformed, not a password question.
                if !(5..=16).contains(&length) || owner.len() < 32 || user.len() < 32 {
                    return Err(CryptError::Unsupported);
                }
                let permissions = encrypt.int(b"P").ok_or(CryptError::Unsupported)?;
                // `/P` is a 32-bit field; writers store it signed or unsigned.
                let permissions = (permissions as u32).to_le_bytes();
                let key = file_key(revision, length, owner, permissions, id, encrypt_metadata);
                if !user_password_is_empty(revision, &key, user, id) {
                    return Err(CryptError::NeedsPassword);
                }
                key
            }
            5 | 6 => {
                let encrypted_key = bytes(encrypt, b"UE").ok_or(CryptError::Unsupported)?;
                let (Some(hash), Some(validation), Some(key_salt)) =
                    (user.get(..32), user.get(32..40), user.get(40..48))
                else {
                    return Err(CryptError::Unsupported);
                };
                if hash_2b(revision, b"", validation, b"") != hash {
                    return Err(CryptError::NeedsPassword);
                }
                let intermediate = hash_2b(revision, b"", key_salt, b"");
                let key = aes256_cbc_decrypt_raw(&intermediate, &[0; 16], encrypted_key)
                    .ok_or(CryptError::Unsupported)?;
                if key.len() != 32 {
                    return Err(CryptError::Unsupported);
                }
                key
            }
            _ => return Err(CryptError::Unsupported),
        };
        Ok(Self {
            key,
            streams,
            encrypt_metadata,
        })
    }

    /// Whether a stream with `dict` is encrypted at all.
    pub(crate) fn applies_to(&self, dict: &Dict) -> bool {
        if self.streams == Method::Identity {
            return false;
        }
        match dict.name(b"Type") {
            // Cross-reference streams are never encrypted.
            Some(b"XRef") => return false,
            Some(b"Metadata") if !self.encrypt_metadata => return false,
            _ => {}
        }
        // A stream's own `/Crypt` filter with the identity handler.
        let first_is_crypt = match dict.get(b"Filter") {
            Some(Object::Name(name)) => name.as_slice() == b"Crypt",
            Some(Object::Array(items)) => {
                matches!(items.first(), Some(Object::Name(name)) if name.as_slice() == b"Crypt")
            }
            _ => false,
        };
        if first_is_crypt {
            let parms = match dict.get(b"DecodeParms") {
                Some(Object::Dict(parms)) => Some(parms),
                Some(Object::Array(items)) => match items.first() {
                    Some(Object::Dict(parms)) => Some(parms),
                    _ => None,
                },
                _ => None,
            };
            let name = parms
                .and_then(|parms| parms.name(b"Name"))
                .unwrap_or(b"Identity");
            if name == b"Identity" {
                return false;
            }
        }
        true
    }

    /// Decrypts one stream of object `number`, generation `generation`.
    /// Returns `None` for data that cannot be decrypted (corrupt).
    pub(crate) fn decrypt(&self, number: u32, generation: u16, data: &[u8]) -> Option<Vec<u8>> {
        match self.streams {
            Method::Identity => Some(data.to_vec()),
            Method::Rc4 => Some(rc4(&self.object_key(number, generation, false), data)),
            Method::Aes128 => aes_cbc_decrypt(&self.object_key(number, generation, true), data),
            Method::Aes256 => aes_cbc_decrypt(&self.key, data),
        }
    }

    /// Algorithm 1: the per-object key for RC4 and AES-128.
    fn object_key(&self, number: u32, generation: u16, aes: bool) -> Vec<u8> {
        let mut hasher = Md5::new();
        hasher.update(&self.key);
        hasher.update(number.to_le_bytes().get(..3).unwrap_or_default());
        hasher.update(generation.to_le_bytes());
        if aes {
            hasher.update(b"sAlT");
        }
        let digest = hasher.finalize();
        let length = (self.key.len() + 5).min(16);
        digest.get(..length).unwrap_or_default().to_vec()
    }
}

/// Algorithm 2: the file key from the (empty) user password.
fn file_key(
    revision: i64,
    length: usize,
    owner: &[u8],
    permissions: [u8; 4],
    id: &[u8],
    encrypt_metadata: bool,
) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(PAD);
    hasher.update(owner.get(..32).unwrap_or(owner));
    hasher.update(permissions);
    hasher.update(id);
    if revision >= 4 && !encrypt_metadata {
        hasher.update([0xFF; 4]);
    }
    let mut digest: Vec<u8> = hasher.finalize().to_vec();
    if revision >= 3 {
        for _ in 0..50 {
            digest = Md5::digest(digest.get(..length).unwrap_or_default()).to_vec();
        }
    }
    digest.truncate(length);
    digest
}

/// Algorithms 4 and 5: whether `key` (from the empty password) matches `/U`.
fn user_password_is_empty(revision: i64, key: &[u8], user: &[u8], id: &[u8]) -> bool {
    if revision == 2 {
        rc4(key, &PAD) == user.get(..32).unwrap_or_default()
    } else {
        user_password_is_empty_r3(key, user, id)
    }
}

fn user_password_is_empty_r3(key: &[u8], user: &[u8], id: &[u8]) -> bool {
    let mut hasher = Md5::new();
    hasher.update(PAD);
    hasher.update(id);
    let mut value = rc4(key, &hasher.finalize());
    for round in 1..=19u8 {
        let round_key: Vec<u8> = key.iter().map(|byte| byte ^ round).collect();
        value = rc4(&round_key, &value);
    }
    user.get(..16) == Some(value.as_slice())
}

/// RC4, for revisions 2 to 4.
pub(crate) fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut state: [u8; 256] = std::array::from_fn(|index| index as u8);
    if key.is_empty() {
        return data.to_vec();
    }
    let mut j: u8 = 0;
    for i in 0..256usize {
        let key_byte = key.get(i % key.len()).copied().unwrap_or(0);
        j = j
            .wrapping_add(state.get(i).copied().unwrap_or(0))
            .wrapping_add(key_byte);
        state.swap(i, usize::from(j));
    }
    let (mut i, mut j) = (0u8, 0u8);
    data.iter()
        .map(|&byte| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(state.get(usize::from(i)).copied().unwrap_or(0));
            state.swap(usize::from(i), usize::from(j));
            let index = state
                .get(usize::from(i))
                .copied()
                .unwrap_or(0)
                .wrapping_add(state.get(usize::from(j)).copied().unwrap_or(0));
            byte ^ state.get(usize::from(index)).copied().unwrap_or(0)
        })
        .collect()
}

fn block(chunk: &[u8]) -> Option<Block> {
    let array: [u8; 16] = chunk.try_into().ok()?;
    Some(Block::from(array))
}

/// AES-CBC with the IV in the first 16 bytes and PKCS#7 padding, as PDF
/// streams use it. Bad padding is kept, as PDFium does.
fn aes_cbc_decrypt(key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let (iv, ciphertext) = data.split_at_checked(16)?;
    let whole = ciphertext.len() / 16 * 16;
    let mut plain = match key.len() {
        16 => {
            let cipher = Aes128::new_from_slice(key).ok()?;
            cbc_decrypt(|b| cipher.decrypt_block(b), iv, ciphertext.get(..whole)?)?
        }
        32 => aes256_cbc_decrypt_raw(key, iv, ciphertext.get(..whole)?)?,
        _ => return None,
    };
    if let Some(&pad) = plain.last() {
        let pad = usize::from(pad);
        if (1..=16).contains(&pad)
            && pad <= plain.len()
            && plain
                .iter()
                .rev()
                .take(pad)
                .all(|&byte| usize::from(byte) == pad)
        {
            plain.truncate(plain.len() - pad);
        }
    }
    Some(plain)
}

fn aes256_cbc_decrypt_raw(key: &[u8], iv: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256::new_from_slice(key).ok()?;
    cbc_decrypt(|b| cipher.decrypt_block(b), iv, data)
}

fn cbc_decrypt(decrypt: impl Fn(&mut Block), iv: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let (blocks, rest) = data.as_chunks::<16>();
    if !rest.is_empty() {
        return None;
    }
    let mut previous = block(iv)?;
    let mut out = Vec::with_capacity(data.len());
    for chunk in blocks {
        let mut current = Block::from(*chunk);
        let saved = current;
        decrypt(&mut current);
        out.extend(current.iter().zip(previous.iter()).map(|(a, b)| a ^ b));
        previous = saved;
    }
    Some(out)
}

fn aes128_cbc_encrypt_raw(key: &[u8], iv: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes128::new_from_slice(key).ok()?;
    let mut previous = block(iv)?;
    let mut out = Vec::with_capacity(data.len());
    let (blocks, _) = data.as_chunks::<16>();
    for chunk in blocks {
        let mixed: Vec<u8> = chunk
            .iter()
            .zip(previous.iter())
            .map(|(a, b)| a ^ b)
            .collect();
        let mut current = block(&mixed)?;
        cipher.encrypt_block(&mut current);
        out.extend_from_slice(&current);
        previous = current;
    }
    Some(out)
}

/// Algorithm 2.B (revision 6), or plain SHA-256 (revision 5): the hash of a
/// password with an 8-byte salt and optional user data.
fn hash_2b(revision: i64, password: &[u8], salt: &[u8], udata: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(password);
    hasher.update(salt);
    hasher.update(udata);
    let mut k: Vec<u8> = hasher.finalize().to_vec();
    if revision == 5 {
        return k;
    }
    let mut round: u32 = 0;
    loop {
        let one: Vec<u8> = [password, k.as_slice(), udata].concat();
        let k1 = one.repeat(64);
        let (Some(key), Some(iv)) = (k.get(..16), k.get(16..32)) else {
            return Vec::new();
        };
        let Some(e) = aes128_cbc_encrypt_raw(key, iv, &k1) else {
            return Vec::new();
        };
        let selector = e.iter().take(16).map(|&byte| u32::from(byte)).sum::<u32>() % 3;
        k = match selector {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };
        round += 1;
        let last = u32::from(e.last().copied().unwrap_or(0));
        if round >= 64 && last <= round - 32 {
            break;
        }
    }
    k.truncate(32);
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
            .collect()
    }

    #[test]
    fn rc4_matches_the_classic_test_vectors() {
        assert_eq!(rc4(b"Key", b"Plaintext"), hex("bbf316e8d940af0ad3"));
        assert_eq!(rc4(b"Wiki", b"pedia"), hex("1021bf0420"));
        assert_eq!(
            rc4(b"Secret", b"Attack at dawn"),
            hex("45a01f645fc35b383552544b9bf5")
        );
    }

    #[test]
    fn aes_cbc_matches_nist_sp800_38a() {
        // F.2.1 and F.2.5: CBC-AES128 and CBC-AES256 encrypt, first block.
        let iv = hex("000102030405060708090a0b0c0d0e0f");
        let plain = hex("6bc1bee22e409f96e93d7e117393172a");
        let key128 = hex("2b7e151628aed2a6abf7158809cf4f3c");
        let cipher128 = hex("7649abac8119b246cee98e9b12e9197d");
        assert_eq!(
            aes128_cbc_encrypt_raw(&key128, &iv, &plain),
            Some(cipher128.clone())
        );
        let key256 = hex("603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4");
        let cipher256 = hex("f58c4c04d6e5f1ba779eabfb5f7bfbd6");
        assert_eq!(
            aes256_cbc_decrypt_raw(&key256, &iv, &cipher256),
            Some(plain.clone())
        );
        // As a PDF stream: IV first, no padding block, so nothing is removed.
        let stream = [iv.as_slice(), cipher128.as_slice()].concat();
        assert_eq!(aes_cbc_decrypt(&key128, &stream), Some(plain));
    }

    #[test]
    fn short_or_ragged_data_is_not_a_panic() {
        assert_eq!(aes_cbc_decrypt(&[0; 16], &[1, 2, 3]), None);
        assert_eq!(aes_cbc_decrypt(&[0; 16], &[0; 16]), Some(Vec::new()));
        assert!(aes_cbc_decrypt(&[0; 16], &[0; 40]).is_some());
        assert_eq!(rc4(b"", b"abc"), b"abc".to_vec());
    }
}
