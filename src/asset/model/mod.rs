pub mod gltf;
pub mod nd;
pub mod sub_main;

use std::{collections::HashMap, io::{Cursor, Seek, SeekFrom}};

use byteorder::{LittleEndian, ReadBytesExt};

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::{
    VirtualResource,
    asset::{
         AssetDescriptor, AssetLike, AssetParseError, AssetType,
        model::sub_main::{Mesh, MeshDescriptor},
        texture::{Texture, TextureDescriptor},
    },
};

#[derive(Debug)]
pub struct Model {
    descriptor: ModelDescriptor,
    // subresource_descriptors: Vec<ModelSubresourceDescriptor>,
    meshes: Vec<Mesh>,
    textures: Vec<Texture>,
}

#[repr(u32)]
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

#[derive(Debug, Clone)]
pub struct ModelDescriptor {
    subresources_offset: u32,
    subresource_count: u32,
    raw_subresources: Vec<RawModelSubresource>,
    texture_descriptors: Vec<TextureDescriptor>,
    mesh_descriptors: Vec<MeshDescriptor>,
}

impl ModelDescriptor {
    pub fn mesh_descriptors(&self) -> &[MeshDescriptor] {
        &self.mesh_descriptors
    }

    pub fn key_value_map(&self) -> Option<&HashMap<String, Vec<u8>>> {
        self.mesh_descriptors.iter().find_map(|mesh|{
            (!mesh.key_value_map.is_empty()).then_some(&mesh.key_value_map)
        })
    }

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
        let mut mesh_descriptors = vec![];

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
                ModelSubresType::Mesh => {
                    let mut mesh_cur = cur.clone();

                    mesh_cur.seek(SeekFrom::Start(subres_param as u64))?;

                    let mut mesh_ptrs = Vec::new();

                    loop {
                        let ptr = mesh_cur.read_u32::<LittleEndian>()? as usize;
                        if ptr == 0 {
                            break;
                        }
                        mesh_ptrs.push(ptr);
                    }

                    for ptr in mesh_ptrs {
                        // TODO: Bounds check for ptr
                        mesh_descriptors.push(MeshDescriptor::from_bytes(&data[ptr..])?);
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
            mesh_descriptors,
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
            meshes: vec![],
        };

        for subtex_desc in &model.descriptor.texture_descriptors {
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
        // TODO: Implement this
        todo!()
    }
}

impl Model {
    /// Returns a list of textures if the model has any, and None otherwise.
    pub fn textures(&self) -> Option<&Vec<Texture>> {
        Some(&self.textures)
    }
}
