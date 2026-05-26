use std::io::Read;

use byteorder::{LittleEndian, ReadBytesExt};

#[derive(Debug, Clone)]
pub struct Cutscene {
    pub count_1: u8,
    pub count_2: u8,
    pub num_cameras: u8,
    pub num_animations: u8,
    pub length: f32,
    pub rest_raw: Vec<u8>,
}

impl TryFrom<crate::RawAssetData> for Cutscene {
    type Error = crate::Error;

    fn try_from(value: crate::RawAssetData) -> Result<Self, Self::Error> {
        let crate::RawAssetData {
            descriptor_bytes,
            resource_chunks: _,
        } = value;

        let mut cur = std::io::Cursor::new(descriptor_bytes);
        let count_1 = cur.read_u8()?;
        let count_2 = cur.read_u8()?;
        let num_cameras = cur.read_u8()?;
        let num_animations = cur.read_u8()?;

        let length = cur.read_f32::<LittleEndian>()?;

        let mut raw = vec![];

        cur.read_to_end(&mut raw)?;

        Ok(Self {
            count_1,
            count_2,
            num_cameras,
            num_animations,
            length,
            rest_raw: raw,
        })
    }
}

impl TryFrom<Cutscene> for crate::RawAssetData {
    type Error = crate::Error;

    fn try_from(value: Cutscene) -> Result<Self, Self::Error> {
        let mut ret = vec![
            value.count_1,
            value.count_2,
            value.num_cameras,
            value.num_animations,
        ];

        ret.extend(value.length.to_le_bytes());
        ret.extend(&value.rest_raw);

        Ok(crate::RawAssetData {
            descriptor_bytes: ret,
            resource_chunks: vec![],
        })
    }
}

impl super::AssetData for Cutscene {
    const ASSET_TYPE: super::AssetType = super::AssetType::Cutscene;
}
