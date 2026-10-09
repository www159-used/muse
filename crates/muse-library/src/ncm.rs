// SPDX-License-Identifier: AGPL-3.0-only
// Format reference: taurusxin/ncmdump 1.5.1, src/ncmcrypt.cpp.
// The reference project's MIT notice is in licenses/ncmdump-MIT.txt.

use aes::{
    Aes128,
    cipher::{Block, BlockDecrypt, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::Value;

const CORE_KEY: &[u8; 16] = b"hzHRAmso5kInbaxW";
const META_KEY: &[u8; 16] = b"#14ljk_!\\]&0U<'(";
const META_PREFIX: &[u8] = b"163 key(Don't modify):";

pub(crate) struct Ncm<'a> {
    pub metadata: Option<Value>,
    pub metadata_error: Option<String>,
    pub cover: &'a [u8],
    encrypted_audio: &'a [u8],
    mask: [u8; 256],
}

impl<'a> Ncm<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let mut reader = Reader { bytes, offset: 0 };
        if reader.take(8)? != b"CTENFDAM" {
            return Err("invalid NCM file header".into());
        }
        reader.take(2)?;
        let key_size = reader.u32()?;
        let encoded_key: Vec<u8> = reader
            .take(key_size)?
            .iter()
            .map(|byte| byte ^ 0x64)
            .collect();
        let key = decrypt(CORE_KEY, &encoded_key)?;
        let key = key
            .strip_prefix(b"neteasecloudmusic")
            .filter(|key| !key.is_empty())
            .ok_or("invalid NCM audio key")?;
        let mut key_box = std::array::from_fn::<_, 256, _>(|index| index as u8);
        let mut last = 0u8;
        for index in 0..256 {
            last = key_box[index]
                .wrapping_add(last)
                .wrapping_add(key[index % key.len()]);
            key_box.swap(index, usize::from(last));
        }
        let mask = std::array::from_fn(|index| {
            let j = ((index + 1) & 255) as u8;
            let value = key_box[usize::from(j)];
            key_box[usize::from(value.wrapping_add(key_box[usize::from(value.wrapping_add(j))]))]
        });
        let metadata_size = reader.u32()?;
        let encoded_metadata: Vec<u8> = reader
            .take(metadata_size)?
            .iter()
            .map(|byte| byte ^ 0x63)
            .collect();
        let (metadata, metadata_error) = if encoded_metadata.is_empty() {
            (None, None)
        } else {
            match decode_metadata(&encoded_metadata) {
                Ok(metadata) => (Some(metadata), None),
                Err(error) => (None, Some(error)),
            }
        };
        reader.take(5)?; // CRC and image version.
        let cover_frame_size = reader.u32()?;
        let cover_size = reader.u32()?;
        if cover_size > cover_frame_size {
            return Err("NCM cover length exceeds the container bounds".into());
        }
        let cover_frame = reader.take(cover_frame_size)?;
        let cover = &cover_frame[..cover_size];
        let encrypted_audio = &bytes[reader.offset..];
        if encrypted_audio.is_empty() {
            return Err("NCM contains no audio".into());
        }
        Ok(Self {
            metadata,
            metadata_error,
            cover,
            encrypted_audio,
            mask,
        })
    }

    pub fn decode_audio(&self) -> Vec<u8> {
        self.encrypted_audio
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ self.mask[index & 255])
            .collect()
    }
}

pub(crate) fn decode_metadata(bytes: &[u8]) -> Result<Value, String> {
    let encoded = bytes
        .strip_prefix(META_PREFIX)
        .ok_or("invalid NetEase metadata prefix")?;
    let encrypted = STANDARD
        .decode(encoded)
        .map_err(|_| "invalid NetEase metadata Base64")?;
    let decrypted = decrypt(META_KEY, &encrypted)?;
    let json = decrypted
        .strip_prefix(b"music:")
        .ok_or("invalid NetEase music metadata prefix")?;
    serde_json::from_slice(json).map_err(|error| format!("invalid NetEase metadata JSON: {error}"))
}

fn decrypt(key: &[u8; 16], encrypted: &[u8]) -> Result<Vec<u8>, String> {
    if encrypted.is_empty() || !encrypted.len().is_multiple_of(16) {
        return Err("invalid NCM AES data length".into());
    }
    let aes = Aes128::new_from_slice(key).map_err(|_| "invalid NCM AES key length")?;
    let mut bytes = encrypted.to_vec();
    for chunk in bytes.chunks_exact_mut(16) {
        let mut block = Block::<Aes128>::default();
        block.copy_from_slice(chunk);
        aes.decrypt_block(&mut block);
        chunk.copy_from_slice(&block);
    }
    let padding = usize::from(*bytes.last().ok_or("NCM AES data is empty")?);
    if !(1..=16).contains(&padding)
        || !bytes[bytes.len() - padding..]
            .iter()
            .all(|byte| usize::from(*byte) == padding)
    {
        return Err("invalid NCM AES padding".into());
    }
    bytes.truncate(bytes.len() - padding);
    Ok(bytes)
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(count)
            .filter(|end| *end <= self.bytes.len())
            .ok_or("NCM file is truncated or contains an invalid field length")?;
        let bytes = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<usize, String> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| "invalid NCM field length")?;
        Ok(u32::from_le_bytes(bytes) as usize)
    }
}
