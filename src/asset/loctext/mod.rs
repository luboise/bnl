mod serialisation;
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    io::{BufRead, Cursor, Read, Seek, SeekFrom},
};

use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::Serialize;
use serialisation::*;

use crate::asset::AssetParseError;

#[derive(Debug, Serialize)]
pub struct LoctextResource {
    #[serde(
        flatten,
        serialize_with = "serde_ordered_collections::map::sorted_serialize"
    )]
    values: HashMap<String, String>,
}

impl LoctextResource {
    pub fn hash_loctext_key<S: AsRef<[u8]>>(s: S) -> u16 {
        let bytes = s.as_ref();

        let mut hash: u32 = 0;

        bytes.iter().for_each(|b| {
            hash = hash.wrapping_mul(0x10) + (*b as u32);

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
                        "Sentinel not found after values in LSBL file (found 0x{:04x} instead)",
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

    pub fn from_hashmap(hashmap: HashMap<String, String>) -> Result<Self, AssetParseError> {
        // TODO: Validate the chars as UTF8 and UTF16LE
        Ok(Self { values: hashmap })
    }

    pub fn dump(&self) -> Result<Vec<u8>, AssetParseError> {
        let mut values_section: Vec<u8> = vec![];
        let mut keys_section: Vec<u8> = vec![];
        let mut unknown_section: Vec<u8> = vec![];
        let mut hash_list_section: Vec<u8> = vec![];

        #[repr(C)]
        struct ValueLocator {
            hash: u16,
            char_offset: u32,
        }

        let mut value_locators = vec![];
        let mut value_chars: Vec<u16> = vec![];

        #[repr(C)]
        struct KeyLocator {
            hash: u16,
            value_index: u16,
            char_offset: u32,
        }

        let mut key_locators: Vec<KeyLocator> = vec![];
        let mut key_chars: Vec<u8> = vec![];

        let mut hashes = HashSet::<u16>::new();

        #[repr(C)]
        struct HashCollision {
            name: String,
            original_hash: u16,
            substituted_hash: u16,
        }

        #[repr(C)]
        struct CollisionTableEntry {
            name_offset: u32,
            original_hash: u16,
            substituted_hash: u16,
        }

        let mut collisions = Vec::<HashCollision>::new();

        #[derive(Debug)]
        struct KeyPair {
            key: String,
            value: String,
        }

        let mut substituted_hash: u16 = 0;

        let mut hash_to_pair = HashMap::<u16, KeyPair>::new();
        for (k, v) in self.values.clone() {
            let mut key: Vec<u8> = k.chars().map(|c| c as u8).collect();
            let mut hash = LoctextResource::hash_loctext_key(&key);

            // Add null terminator
            key.push(0u8);

            if hash_to_pair.contains_key(&hash) {
                while hash_to_pair.contains_key(&substituted_hash) {
                    substituted_hash += 1;
                }

                println!(
                    "Key {} resolves to duplicate hash: 0x{:04x}. Using substituted hash 0x{:04x} instead.",
                    k, hash, substituted_hash
                );

                collisions.push(HashCollision {
                    name: k.clone(),
                    original_hash: hash,
                    substituted_hash,
                });

                hash = substituted_hash;
            }

            // If it STILL contains the hash, print an error
            if let Some(old_val) = hash_to_pair.insert(hash, KeyPair { key: k, value: v }) {
                eprintln!(
                    "Fatal hash collision on collision table insertion. Old value: {:?}",
                    old_val
                );
            }
        }

        // Sort by hashes (the game does binary search on the values table)
        let mut sorted_values: Vec<_> = hash_to_pair.iter().collect();
        sorted_values.sort_by(|(h1, _), (h2, _)| {
            if h1 < h2 {
                return Ordering::Less;
            } else if h1 > h2 {
                return Ordering::Greater;
            }

            Ordering::Equal
        });

        for (i, (hash, kp)) in sorted_values.iter().enumerate() {
            let mut key: Vec<u8> = kp.key.chars().map(|c| c as u8).collect();

            // Add null terminator
            key.push(0u8);

            // Insert fail => already in set
            if !hashes.insert(**hash) {
                return Err(AssetParseError::InvalidDataViews(
                    "Fatal hash collision when dumping file.".to_string(),
                ));
            }

            let mut value: Vec<u16> = kp.value.as_str().encode_utf16().collect();
            value.push(0u16);

            // Write value chars
            value_locators.push(ValueLocator {
                hash: **hash,
                char_offset: value_chars.len() as u32,
            });

            value_chars.extend(value);

            // Write key chars
            key_locators.push(KeyLocator {
                hash: **hash,
                value_index: (i + 1) as u16,
                char_offset: key_chars.len() as u32,
            });
            key_chars.extend(key);
        }

        // Write collision chars
        let mut col_table_entries = Vec::<CollisionTableEntry>::new();
        let mut collision_chars = Vec::<u8>::new();

        for collision in &collisions {
            let mut collision_key: Vec<u8> = collision.name.chars().map(|c| c as u8).collect();
            collision_key.push(0);

            col_table_entries.push(CollisionTableEntry {
                name_offset: collision_chars.len() as u32,
                original_hash: collision.original_hash,
                substituted_hash: collision.substituted_hash,
            });

            collision_chars.extend(collision_key);
        }

        let mut collisions_section: Vec<u8> = vec![];

        let collisions_section_base = 0x20;

        for entry in col_table_entries {
            collisions_section
                .write_u32::<LittleEndian>(entry.name_offset + collisions_section_base)?;
            collisions_section.write_u16::<LittleEndian>(entry.original_hash)?;
            collisions_section.write_u16::<LittleEndian>(entry.substituted_hash)?;
        }

        collisions_section.extend(collision_chars);

        {
            // The size
            values_section.write_u32::<LittleEndian>(0x00)?;

            // Create values section
            values_section.write_u32::<LittleEndian>(hashes.len() as u32)?;
            for value_locator in value_locators {
                values_section.write_u16::<LittleEndian>(value_locator.hash)?;
                values_section.write_u32::<LittleEndian>(value_locator.char_offset)?;
            }
            // Write end of locators sentinel
            values_section.write_u16::<LittleEndian>(0xFFFF)?;
            values_section.write_u32::<LittleEndian>(value_chars.len() as u32)?;
            values_section.extend(value_chars.iter().flat_map(|v| v.to_le_bytes()));
            // Write end of values sentinel (an extra empty wchar_t)
            values_section.write_u16::<LittleEndian>(0x0000)?;

            let len = values_section.len() as u32;
            values_section[0..4].copy_from_slice(&(len.to_le_bytes()));
        }

        if !hashes.is_empty() {
            // The size
            hash_list_section.write_u32::<LittleEndian>(0x00)?;

            hash_list_section.write_u32::<LittleEndian>(hashes.len() as u32)?;

            for hash in &hashes {
                hash_list_section.write_u16::<LittleEndian>(*hash)?;
            }

            let len = hash_list_section.len() as u32;
            hash_list_section[0..4].copy_from_slice(&(len.to_le_bytes()));
        }

        {
            // The size
            keys_section.write_u32::<LittleEndian>(0x00)?;

            // Create keys section
            keys_section.write_u32::<LittleEndian>(hashes.len() as u32)?;
            for key_locator in key_locators {
                keys_section.write_u16::<LittleEndian>(key_locator.hash)?;
                keys_section.write_u16::<LittleEndian>(key_locator.value_index)?;
                keys_section.write_u32::<LittleEndian>(key_locator.char_offset)?;
            }
            keys_section.extend(key_chars);

            let len = keys_section.len() as u32;
            keys_section[0..4].copy_from_slice(&(len.to_le_bytes()));
        }

        let mut lsbl_bytes: Vec<u8> = vec![b'L', b'S', b'B', b'L'];

        let mut section_ptr = 0x1c;

        // Values section ptr
        lsbl_bytes.write_u32::<LittleEndian>(section_ptr)?;
        lsbl_bytes.write_u32::<LittleEndian>(0x4)?;
        lsbl_bytes.write_u32::<LittleEndian>(section_ptr)?;
        section_ptr += values_section.len() as u32;

        // Keys section ptr
        lsbl_bytes.write_u32::<LittleEndian>(section_ptr)?;
        section_ptr += keys_section.len() as u32;

        // Unknown section ptr
        if unknown_section.is_empty() {
            lsbl_bytes.write_u32::<LittleEndian>(0x00)?;
        } else {
            lsbl_bytes.write_u32::<LittleEndian>(section_ptr)?;
        }
        section_ptr += unknown_section.len() as u32;

        // Hash list section ptr
        if hash_list_section.is_empty() {
            lsbl_bytes.write_u32::<LittleEndian>(0x00)?;
        } else {
            lsbl_bytes.write_u32::<LittleEndian>(section_ptr)?;
        }

        lsbl_bytes.extend(values_section);
        lsbl_bytes.extend(keys_section);
        lsbl_bytes.extend(unknown_section);
        lsbl_bytes.extend(hash_list_section);

        let mut out_bytes: Vec<u8> = Vec::new();

        out_bytes.write_u32::<LittleEndian>(0x10)?;
        out_bytes.write_u32::<BigEndian>(0x1d_62_a2_b1)?;
        out_bytes.write_u32::<BigEndian>(0x36_88_e5_48)?;
        out_bytes.write_u32::<LittleEndian>(0x2)?;

        // Offset of 20
        out_bytes.write_u32::<LittleEndian>(0x20)?;
        out_bytes.write_u32::<LittleEndian>(0xc + lsbl_bytes.len() as u32)?;
        out_bytes.write_u32::<LittleEndian>(0x20)?;
        out_bytes.write_u32::<LittleEndian>(0x0)?;

        if collisions.is_empty() {
            // LSBL file ptr
            out_bytes.write_u32::<LittleEndian>(0x0c)?;
            // Other section ptrs
            out_bytes.write_u32::<LittleEndian>(0x0)?;
            out_bytes.write_u32::<LittleEndian>(0x0)?;
        } else {
            // LSBL file ptr
            out_bytes.write_u32::<LittleEndian>((0x0c + collisions_section.len()) as u32)?;
            // Other section ptrs
            out_bytes.write_u32::<LittleEndian>(0x0c)?;
            out_bytes.write_u32::<LittleEndian>(collisions.len() as u32)?;
        }

        out_bytes.extend(collisions_section);
        out_bytes.extend(lsbl_bytes);

        Ok(out_bytes)
    }
}

#[cfg(test)]
mod tests {
    use crate::asset::loctext::LoctextResource;

    #[test]
    pub fn chapter_names_hash_correctly() -> Result<(), String> {
        assert_eq!(LoctextResource::hash_loctext_key("chaptername__1"), 0x1d1);
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

        assert_eq!(
            LoctextResource::hash_loctext_key("dialogs__challengeawards_scaredyspiders_bronze"),
            0xfa02
        );

        Ok(())
    }
}
