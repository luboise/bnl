pub mod sub_main;

use std::io::{Cursor, Seek, SeekFrom};

use byteorder::{LittleEndian, ReadBytesExt};

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    VirtualResource,
    asset::{
        Asset, AssetDescription, AssetDescriptor, AssetParseError,
        texture::{TextureData, TextureDescriptor},
    },
    game::AssetType,
};

#[derive(Debug)]
pub struct Model {
    description: AssetDescription,
    descriptor: ModelDescriptor,
    // subresource_descriptors: Vec<ModelSubresourceDescriptor>,
    textures: Vec<TextureData>,
}

#[repr(u32)]
#[derive(Debug, Clone, TryFromPrimitive, IntoPrimitive)]
pub enum ModelSubresType {
    Model = 0x00,
    Unknown1 = 0x01,
    Unknown2 = 0x02,
    Unknown3 = 0x03,
    Unknown4 = 0x04,
    Unknown5 = 0x05,
    Unknown6 = 0x06,
    Texture = 0x07,
    Unknown8 = 0x08,
    Unknown9 = 0x09,
    Unknown10 = 0x0a,
    Unknown11 = 0x0b,
    Unknown12 = 0x0c,
    Unknown13 = 0x0d,
    Unknown14 = 0x0e,
    Unknown15 = 0x0f,
    Unknown16 = 0x10,
    Unknown17 = 0x11,
    Unknown18 = 0x12,
    Unknown19 = 0x13,
    Unknown20 = 0x14,
    Unknown21 = 0x15,
}

#[derive(Debug, Clone)]
pub(crate) struct RawModelSubresource {
    subres_type: ModelSubresType,
    subres_param: u32,
}

#[derive(Debug, Clone)]
pub struct ModelDescriptor {
    subresources_offset: u32,
    subresource_count: u32,
    raw_subresources: Vec<RawModelSubresource>,
    texture_descriptors: Vec<TextureDescriptor>,
}

impl AssetDescriptor for ModelDescriptor {
    fn from_bytes(data: &[u8]) -> Result<Self, AssetParseError> {
        let data_size = data.len() as u32;

        if data_size < size_of::<ModelDescriptor>() as u32 {
            return Err(AssetParseError::InputTooSmall);
        }

        if data_size < 8 {
            return Err(AssetParseError::InputTooSmall);
        }

        let subresources_offset = u32::from_le_bytes(data[0..4].try_into().unwrap_or_default());
        let subresource_count = u32::from_le_bytes(data[4..8].try_into().unwrap_or_default());

        if subresources_offset > data_size
            || (subresource_count * 8) > data_size - subresources_offset
        {
            return Err(AssetParseError::InputTooSmall);
        }

        let mut cur = Cursor::new(data);

        cur.seek(SeekFrom::Start(subresources_offset as u64))?;

        let mut raw_subresources = vec![];

        let mut texture_descriptors = vec![];

        for _ in 0..subresource_count {
            let subres_type: ModelSubresType = cur
                .read_u32::<LittleEndian>()
                .map_err(|_| AssetParseError::ErrorParsingDescriptor)?
                .try_into()
                .map_err(|_| AssetParseError::ErrorParsingDescriptor)?;

            let subres_param = cur
                .read_u32::<LittleEndian>()
                .map_err(|_| AssetParseError::ErrorParsingDescriptor)?;

            raw_subresources.push(RawModelSubresource {
                subres_type: subres_type
                    .clone()
                    .try_into()
                    .map_err(|_| AssetParseError::ErrorParsingDescriptor)?,
                subres_param,
            });

            match subres_type {
                ModelSubresType::Texture => {
                    let mut tex_cur = Cursor::new(data);
                    tex_cur.seek(SeekFrom::Start(subres_param as u64))?;

                    let texture_list_count = tex_cur.read_u32::<LittleEndian>()?;
                    let texture_list_offset = tex_cur.read_u32::<LittleEndian>()?;

                    tex_cur.seek(SeekFrom::Start(texture_list_offset as u64))?;

                    for _ in 0..texture_list_count {
                        let ptr = tex_cur.read_u32::<LittleEndian>()? as usize;

                        let slice = &data[ptr..];
                        let tex_desc = TextureDescriptor::from_bytes(slice)?;

                        texture_descriptors.push(tex_desc);
                    }
                }
                _ => {}
            };
        }

        Ok(ModelDescriptor {
            subresources_offset,
            subresource_count,
            raw_subresources,
            texture_descriptors,
        })
    }

    fn to_bytes(&self) -> Result<Vec<u8>, AssetParseError> {
        todo!()
    }

    fn size(&self) -> usize {
        todo!()
    }

    fn asset_type() -> AssetType {
        AssetType::ResModel
    }
}

impl Asset for Model {
    type Descriptor = ModelDescriptor;

    fn new(
        description: &AssetDescription,
        descriptor: &Self::Descriptor,
        virtual_res: &VirtualResource,
    ) -> Result<Self, AssetParseError> {
        if virtual_res.is_empty() {
            return Err(AssetParseError::InvalidDataViews(
                "Unable to create a Model using 0 data views".to_string(),
            ));
        }

        let mut model = Model {
            description: description.clone(),
            descriptor: descriptor.clone(),
            textures: vec![],
        };

        for subtex_desc in &model.descriptor.texture_descriptors {
            model.textures.push(TextureData::new(
                subtex_desc.clone().into(),
                virtual_res.get_all_bytes(),
            ));
        }

        Ok(model)
    }

    fn descriptor(&self) -> &Self::Descriptor {
        &self.descriptor
    }

    fn description(&self) -> &super::AssetDescription {
        todo!()
    }

    fn as_bnl_asset(&self) -> crate::BNLAsset {
        todo!()
    }
}

pub trait Subresource {}

impl Model {
    /// Returns a list of textures if the model has any, and None otherwise.
    pub fn textures(&self) -> Option<&Vec<TextureData>> {
        Some(&self.textures)
    }
}
