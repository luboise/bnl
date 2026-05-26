use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use crate::asset::{AssetName, AssetParseError};

#[derive(Debug, Clone)]
pub struct AidList {
    asset_ids: Vec<AssetName>,
}

impl AidList {
    pub fn asset_ids(&self) -> Vec<&str> {
        self.asset_ids
            .iter()
            .map(|v| unsafe { str::from_utf8_unchecked(v) })
            .collect()
    }
}

impl TryFrom<crate::RawAssetData> for AidList {
    type Error = crate::Error;

    fn try_from(value: crate::RawAssetData) -> Result<Self, Self::Error> {
        let crate::RawAssetData {
            descriptor_bytes,
            resource_chunks: _,
        } = value;

        if descriptor_bytes.len() % 128 != 0 {
            return Err(format!(
                "aidlist length {} is not divisible by 0x80",
                descriptor_bytes.len()
            )
            .into());
        }

        Ok(Self {
            asset_ids: descriptor_bytes
                .chunks_exact(128)
                .map(|chunk| {
                    chunk[0..128]
                        .try_into()
                        .map_err(|_| AssetParseError::ErrorParsingDescriptor)
                })
                .collect::<Result<Vec<AssetName>, _>>()?,
        })
    }
}

impl TryFrom<AidList> for crate::RawAssetData {
    type Error = crate::Error;

    fn try_from(value: AidList) -> Result<Self, Self::Error> {
        Ok(crate::RawAssetData {
            descriptor_bytes: value.asset_ids.into_iter().flatten().collect(),
            resource_chunks: vec![],
        })
    }
}

impl super::AssetData for AidList {
    const ASSET_TYPE: super::AssetType = super::AssetType::AidList;
}

impl super::Dump for AidList {
    fn dump<P: AsRef<Path>>(&self, dump_path: P) -> Result<(), Box<dyn std::error::Error>> {
        let out_file = File::create(dump_path)?;
        let mut writer = BufWriter::new(out_file);
        writer.write_all(self.asset_ids().join("\n").as_bytes())?;

        Ok(())
    }
}

impl super::Parse for AidList {
    fn parse<P: AsRef<Path>>(parse_path: P) -> Result<Self, AssetParseError> {
        let asset_ids = std::fs::read_to_string(parse_path)?
            .lines()
            .filter(|line| !line.is_empty())
            .map(|asset_id| {
                if asset_id.len() > super::MAX_ASSET_NAME_LENGTH {
                    return Err(AssetParseError::InvalidDataViews("input too large".into()));
                }

                let mut v: AssetName = [0u8; 128];
                v[..asset_id.len()].copy_from_slice(&asset_id.as_bytes()[..]);

                Ok(v)
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Self { asset_ids })
    }
}
