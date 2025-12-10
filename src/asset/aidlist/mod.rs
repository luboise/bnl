use std::{
    fs::File,
    io::{BufWriter, Cursor, Write},
    path::Path,
};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    VirtualResource,
    asset::{AssetDescriptor, AssetLike, AssetName, AssetParseError, AssetType, Dump},
};

#[derive(Debug, Clone)]
pub struct AidListDescriptor {
    asset_ids: Vec<AssetName>,
}

impl AidListDescriptor {}

#[derive(Debug, Clone)]
pub struct AidList {
    asset_ids: Vec<String>,
}

impl AidList {
    pub fn asset_ids_mut(&mut self) -> &mut Vec<String> {
        &mut self.asset_ids
    }
}

impl AssetDescriptor for AidListDescriptor {
    fn from_bytes(data: &[u8]) -> Result<Self, AssetParseError> {
        if data.len() % 128 != 0 {
            return Err(AssetParseError::InvalidDataViews(format!(
                "Input bytes were expected to be a multiple of 128 (received {})",
                data.len()
            )));
        }

        Ok(Self {
            asset_ids: data
                .chunks_exact(128)
                .map(|chunk| (chunk[0..128].try_into().unwrap()))
                .collect::<Vec<AssetName>>(),
        })
    }

    fn size(&self) -> usize {
        self.asset_ids.len() * size_of::<AssetName>()
    }

    fn asset_type() -> AssetType {
        AssetType::ResAidList
    }

    fn to_bytes(&self) -> Result<Vec<u8>, AssetParseError> {
        Ok(self.asset_ids.iter().flat_map(|id| id.to_vec()).collect())
    }
}

impl AssetLike for AidList {
    type Descriptor = AidListDescriptor;

    fn new(
        descriptor: &Self::Descriptor,
        _virtual_res: &VirtualResource,
    ) -> Result<Self, AssetParseError> {
        let mut strings = Vec::new();

        for asset_id in &descriptor.asset_ids {
            strings.push(
                String::from_utf8(asset_id.to_vec())
                    .map_err(|_| AssetParseError::ErrorParsingDescriptor)?,
            );
        }

        Ok(Self { asset_ids: strings })
    }

    fn get_descriptor(&self) -> Self::Descriptor {
        AidListDescriptor {
            asset_ids: self
                .asset_ids
                .iter()
                .map(|asset_id_str| {
                    let mut new_chars = [0u8; 128];

                    let len = asset_id_str.len();

                    new_chars[0..len].copy_from_slice(
                        &asset_id_str
                            .chars()
                            .take(len)
                            .map(|c| c as u8)
                            .collect::<Vec<u8>>(),
                    );

                    Ok(new_chars)
                })
                .collect::<Result<Vec<AssetName>, AssetParseError>>()
                .unwrap(),
        }
    }

    fn get_resource_chunks(&self) -> Option<Vec<Vec<u8>>> {
        None
    }
}

impl Dump for AidList {
    fn dump<P: AsRef<Path>>(&self, dump_path: P) -> Result<(), std::io::Error> {
        {
            let out_file = File::create(dump_path)?;

            let mut writer = BufWriter::new(out_file);

            writer.write_all(
                &self
                    .asset_ids
                    .join("\n")
                    .chars()
                    .map(|c| c as u8)
                    .collect::<Vec<u8>>(),
            )?;
        }

        Ok(())
    }
}
