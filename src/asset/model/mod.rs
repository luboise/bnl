pub mod gltf;
pub mod nd;
pub mod sub_main;

use std::{
    collections::HashMap,
    io::{Cursor, Seek, SeekFrom},
};

use binrw::{BinRead, binrw};
use byteorder::{LittleEndian, ReadBytesExt};

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    VirtualResource,
    asset::{
        AssetDescriptor, AssetLike, AssetParseError, AssetType,
        model::sub_main::ModelSubresource,
        texture::{Texture, TextureDescriptor},
    },
};

#[derive(Debug)]
pub struct Model {
    descriptor: ModelDescriptor,
    // subresource_descriptors: Vec<ModelSubresourceDescriptor>,
    // meshes: Vec<Mesh>,
    textures: Vec<Texture>,
    resource: Vec<u8>,
}

#[binrw]
#[bw(repr = u32)]
#[repr(u32)]
#[br(repr = u32)]
#[derive(Debug, Clone, TryFromPrimitive, IntoPrimitive)]
pub enum ModelSubresType {
    Mesh = 0x00,
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

#[binrw]
#[derive(Debug, Clone)]
struct ModelSubresHeader {
    subres_type: ModelSubresType,
    ptr: u32,
}

#[binrw]
#[derive(Debug)]
pub struct RawModelDescriptor {
    // TODO: Use bw calc
    #[br(temp)]
    #[bw(ignore)]
    footer_ptr: u32,

    #[br(temp)]
    #[bw(ignore)]
    num_footer_entries: u32,

    #[br(count = num_footer_entries, seek_before(SeekFrom::Start(footer_ptr.into())), restore_position)]
    footer_entries: Vec<ModelSubresHeader>,

    flags: u32,
    unknown_u32_1: u32,
    model_runtime_context: u32,
    unknown_u32_2: u32,
}

#[derive(Debug, Clone)]
pub struct ModelDescriptor {
    flags: u32,
    unknown_u32_1: u32,
    unknown_u32_2: u32,
    model_subresource: Option<ModelSubresource>,
    texture_subresource: Vec<TextureDescriptor>,
    other_subresources: Vec<RawModelSubresource>,
}

/*
#[derive(Debug, Clone)]
pub struct ModelDescriptor {
    subresources_offset: u32,
    subresource_count: u32,
    raw_subresources: Vec<RawModelSubresource>,
    texture_descriptors: Vec<TextureDescriptor>,
    mesh_descriptors: Vec<MeshDescriptor>,
}
*/

impl ModelDescriptor {
    pub fn model_subresource(&self) -> Option<&ModelSubresource> {
        self.model_subresource.as_ref()
    }

    pub fn key_value_map(&self) -> Option<&HashMap<String, Vec<u8>>> {
        self.model_subresource
            .iter()
            .find_map(|mesh| (!mesh.key_value_map.is_empty()).then_some(&mesh.key_value_map))
    }
}

impl AssetDescriptor for ModelDescriptor {
    fn from_bytes(data: &[u8]) -> Result<Self, AssetParseError> {
        let RawModelDescriptor {
            footer_entries,
            flags,
            unknown_u32_1,
            model_runtime_context: _,
            unknown_u32_2,
        } = RawModelDescriptor::read_le(&mut Cursor::new(data))
            .map_err(|_| AssetParseError::ErrorParsingDescriptor)?;

        let data_size = data.len() as u32;

        if data_size < size_of::<ModelDescriptor>() as u32 {
            return Err(AssetParseError::InputTooSmall);
        }

        if data_size < 8 {
            return Err(AssetParseError::InputTooSmall);
        }

        let mut model_subresource = None;
        let mut texture_subresource = vec![];
        let mut other_subresources = vec![];

        for ModelSubresHeader { subres_type, ptr } in footer_entries {
            let mut cur = Cursor::new(data);
            cur.seek(SeekFrom::Start(ptr.into()))?;

            match subres_type {
                ModelSubresType::Texture => {
                    let texture_list_count = cur.read_u32::<LittleEndian>()?;
                    let texture_list_offset = cur.read_u32::<LittleEndian>()?;

                    cur.seek(SeekFrom::Start(texture_list_offset as u64))?;

                    for _ in 0..texture_list_count {
                        let ptr = cur.read_u32::<LittleEndian>()? as usize;
                        texture_subresource.push(TextureDescriptor::from_bytes(&data[ptr..])?);
                    }
                }
                ModelSubresType::Mesh => {
                    cur.seek(SeekFrom::Start(ptr as u64))?;

                    let mut mesh_ptrs = Vec::new();

                    loop {
                        let ptr = cur.read_u32::<LittleEndian>()? as usize;
                        if ptr == 0 {
                            break;
                        }
                        mesh_ptrs.push(ptr);
                    }

                    for ptr in mesh_ptrs {
                        // TODO: Bounds check for ptr
                        model_subresource = Some(ModelSubresource::from_bytes(&data[ptr..])?);
                    }
                }
                _ => {
                    other_subresources.push(RawModelSubresource {
                        subres_type,
                        subres_param: ptr,
                    });
                }
            };
        }

        Ok(ModelDescriptor {
            flags,
            unknown_u32_1,
            unknown_u32_2,
            model_subresource,
            other_subresources,
            texture_subresource,
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

impl AssetLike for Model {
    type Descriptor = ModelDescriptor;

    fn new(
        descriptor: &Self::Descriptor,
        virtual_res: &VirtualResource,
    ) -> Result<Self, AssetParseError> {
        if virtual_res.is_empty() {
            return Err(AssetParseError::InvalidDataViews(
                "Unable to create a Model using 0 data views".to_string(),
            ));
        }

        let mut model = Model {
            descriptor: descriptor.clone(),
            textures: vec![],
            resource: virtual_res.get_all_bytes(),
        };

        for subtex_desc in &model.descriptor.texture_subresource {
            model.textures.push(Texture::new(
                subtex_desc.clone(),
                virtual_res
                    .get_bytes(
                        subtex_desc.texture_offset() as usize,
                        subtex_desc.texture_size() as usize,
                    )
                    .map_err(|e| {
                        AssetParseError::InvalidDataViews(
                            format!("Unable to get section of Virtual Resource required for texture. Error: {}", e)
                        )
                    })?,
            ));
        }
        Ok(model)
    }

    fn get_descriptor(&self) -> Self::Descriptor {
        self.descriptor.clone()
    }

    fn get_resource_chunks(&self) -> Option<Vec<Vec<u8>>> {
        Some(vec![self.resource.clone()])
    }
}

impl Model {
    /// Returns a list of textures if the model has any, and None otherwise.
    pub fn textures(&self) -> Option<&Vec<Texture>> {
        Some(&self.textures)
    }
}
