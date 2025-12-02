mod serialisation;
use std::{
    collections::HashMap,
    io::{BufRead, Cursor, Read, Seek, SeekFrom},
};

use byteorder::{LittleEndian, ReadBytesExt};
use serde::Serialize;
use serialisation::*;

use crate::asset::AssetParseError;

#[derive(Debug, Serialize)]
pub struct LoctextResource {
    #[serde(flatten)]
    values: HashMap<String, String>,
}

impl LoctextResource {
    pub fn hash_loctext_key<S: AsRef<[u8]>>(s: S) -> u16 {
        let bytes = s.as_ref();

        let mut hash: u32 = 0;

        bytes.iter().for_each(|b| {
            hash = hash * 0x10 + (*b as u32);

            let masked: u32 = hash & 0xf000;

            if masked & 0xffff > 0 {
                hash ^= masked >> 8 | masked;
            }
        });

        hash as u16
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<LoctextResource, AssetParseError> {
        let mut cur = Cursor::new(bytes);
        let demand_header = DemandHeader::from_cursor(&mut cur)?;

        cur.seek(SeekFrom::Start(
            demand_header.loctext_resource_header_ptr as u64,
        ))?;

        let lsbl_ptr = cur.read_u32::<LittleEndian>()?;

        let lsbl_slice =
            &bytes[demand_header.loctext_resource_header_ptr as usize + lsbl_ptr as usize..];

        let mut hashes = vec![];

        let mut loctext_map: HashMap<String, String> = HashMap::new();

        let mut keys_map: HashMap<String, u16> = HashMap::new();
        let mut values_map: HashMap<u16, String> = HashMap::new();

        {
            let mut cur = Cursor::new(lsbl_slice);

            let mut lsbl_signature = [0u8; 4];
            cur.read_exact(&mut lsbl_signature)?;

            if lsbl_signature != ['L', 'S', 'B', 'L'].map(|v| v as u8) {
                return Err(AssetParseError::InvalidDataViews(
                    "LSBL file signature does not match".to_string(),
                ));
            }

            let values_ptr = cur.read_u32::<LittleEndian>()?;

            // lsbl                 4 bytes
            // values_start         4 bytes

            // unknown_count_1      2 bytes
            // unknown_count_2      2 bytes
            // unknown_u32_1        4 bytes
            cur.seek_relative(8)?; // Skip 8 bytes

            let keys_ptr = cur.read_u32::<LittleEndian>()?;

            // unknown_u32_2        4 bytes
            let _unused_u32 = cur.read_u32::<LittleEndian>()?;

            let hash_list_ptr = cur.read_u32::<LittleEndian>()?;

            let mut hash_list_cur = cur.clone();

            hash_list_cur.seek(SeekFrom::Start(hash_list_ptr as u64))?;

            // Find all hashes first
            let hash_list_size_bytes = hash_list_cur.read_u32::<LittleEndian>()?;
            let hash_list_length = hash_list_cur.read_u32::<LittleEndian>()?;

            let expected_size: u32 = 8 + (size_of::<u16>() * hash_list_length as usize) as u32;

            if hash_list_size_bytes != expected_size {
                return Err(AssetParseError::InvalidDataViews(format!(
                    "Hash list in LSBL file has {} entries, but {} bytes (expected {} bytes)",
                    hash_list_length, hash_list_size_bytes, expected_size
                )));
            }

            hashes.resize(hash_list_length as usize, 0);

            for i in 0..hash_list_length as usize {
                hashes[i] = cur.read_u16::<LittleEndian>()?;
            }

            // Find all values and the associated hash for each one
            cur.seek(SeekFrom::Start(values_ptr as u64))?;

            let values_section_size = cur.read_u32::<LittleEndian>()?;
            let num_values = cur.read_u32::<LittleEndian>()?;

            let mut chars: Vec<u16> = vec![];

            // Get the chars out of the file
            {
                let mut chars_cur = cur.clone();
                chars_cur.seek_relative((num_values * 6) as i64)?;

                let sentinel = chars_cur.read_u16::<LittleEndian>()?;
                if sentinel != 0xFFFF {
                    return Err(AssetParseError::InvalidDataViews(format!(
                        "Sentinel not found after values in LSBL file (found 0x{:4x} instead)",
                        sentinel
                    )));
                }

                let num_chars = chars_cur.read_u32::<LittleEndian>()?;

                let mut raw_chars = vec![0u8; (num_chars * 2) as usize];

                chars_cur.read_exact(&mut raw_chars)?;

                chars = raw_chars
                    .chunks_exact(2)
                    .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
                    .collect();
            }

            for _ in 0..num_values {
                let hash = cur.read_u16::<LittleEndian>()?;
                let chars_offset = cur.read_u32::<LittleEndian>()?;

                // TODO: Add bounds check
                let val = String::from_utf16(&chars[(chars_offset as usize)..]).map_err(|e| {
                    AssetParseError::InvalidDataViews(format!(
                        "Failed to read UTF16 LE string from value bytes. Error: {}",
                        e
                    ))
                })?;

                values_map.insert(hash, val.split_once('\0').unwrap().0.to_string());
            }

            // Find all keys and make sure each hash is matched
            cur.seek(SeekFrom::Start(keys_ptr as u64))?;

            let keys_section_size = cur.read_u32::<LittleEndian>()?;
            let keys_list_length = cur.read_u32::<LittleEndian>()?;

            let minimum_size = keys_list_length * 8 + 8;
            if keys_section_size < minimum_size {
                return Err(AssetParseError::InvalidDataViews(format!(
                    "Keys list in LSBL file has {} entries, but only {} bytes (expected at least {} bytes)",
                    keys_list_length, keys_section_size, minimum_size
                )));
            }

            let mut key_chars = vec![0u8; (keys_section_size - 8 - keys_list_length * 8) as usize];

            let mut str_cur = cur.clone();

            str_cur.seek_relative((keys_list_length * 8) as i64)?;
            str_cur.read_exact(&mut key_chars)?;

            keys_map = (0..keys_list_length as usize)
                .map(|i| -> Result<_, AssetParseError> {
                    let hash = cur.read_u16::<LittleEndian>()?;
                    let value_index = cur.read_u16::<LittleEndian>()?;
                    let chars_offset = cur.read_u32::<LittleEndian>()?;

                    let mut str_cur = Cursor::new(&key_chars);
                    str_cur.seek_relative(chars_offset as i64)?;
                    let mut new_str: Vec<u8> = vec![];
                    str_cur.read_until(0u8, &mut new_str)?;

                    match new_str.len() {
                        0 => unreachable!(),
                        1 => {
                            return Err(AssetParseError::InvalidDataViews(
                                "Failed to read key string (null terminated instantly)."
                                    .to_string(),
                            ));
                        }
                        _ => (),
                    }

                    new_str.pop();

                    let key = String::from_utf8(new_str).map_err(|e| {
                        AssetParseError::InvalidDataViews(format!(
                            "Failed to read key string from loctext. Error: {}",
                            e
                        ))
                    })?;

                    Ok((key, hash))
                })
                .collect::<Result<HashMap<_, _>, AssetParseError>>()?;
        }

        Ok(Self {
            values: keys_map
                .into_iter()
                .map(|(key, hash)| {
                    values_map
                        .remove(&hash)
                        .map(|val| (key.clone(), val))
                        .ok_or(AssetParseError::InvalidDataViews(format!(
                            "Key {} with hash {} does not have an accompanying value.",
                            key, hash
                        )))
                })
                .collect::<Result<HashMap<_, _>, _>>()?,
        })

        // Parse keys first, and get their hashes
    }
}

#[cfg(test)]
mod tests {
    use crate::asset::loctext::LoctextResource;

    #[test]
    pub fn chapter_names_hash_correctly() -> Result<(), String> {
        assert_eq!(LoctextResource::hash_loctext_key("chaptername__1"), 0x1d1);
        assert_eq!(LoctextResource::hash_loctext_key("chaptername__2"), 0x1d2);
        assert_eq!(LoctextResource::hash_loctext_key("chaptername__3"), 0x1d3);
        assert_eq!(LoctextResource::hash_loctext_key("chaptername__4"), 0x1d4);
        assert_eq!(LoctextResource::hash_loctext_key("chaptername__5"), 0x1d5);
        assert_eq!(LoctextResource::hash_loctext_key("chapternumber__1"), 0xe21);
        assert_eq!(LoctextResource::hash_loctext_key("chapternumber__2"), 0xe22);
        assert_eq!(LoctextResource::hash_loctext_key("chapternumber__3"), 0xe23);
        assert_eq!(LoctextResource::hash_loctext_key("chapternumber__4"), 0xe24);
        assert_eq!(LoctextResource::hash_loctext_key("chapternumber__5"), 0xe25);
        Ok(())
    }
}
