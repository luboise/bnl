pub mod gltf;
pub mod nd;
pub mod sub_colliders;
pub mod sub_main;

use std::{
    collections::HashMap,
    io::{Cursor, Seek, SeekFrom},
};

use binrw::{BinRead, BinReaderExt, BinWrite, binrw};
use byteorder::{LittleEndian, ReadBytesExt};

use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::asset::{
    AssetData, AssetParseError, AssetType,
    model::{sub_colliders::CollisionSubresource, sub_main::ModelSubresource},
    texture::{Texture, TextureDescriptor},
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, TryFromPrimitive, IntoPrimitive)]
pub enum ModelSubresType {
    Mesh = 0x00,
    Flags = 0x01,
    Unknown2 = 0x02,
    Unknown3 = 0x03,
    Unknown4 = 0x04,
    Matrices = 0x05,
    Collision = 0x06,
    Texture = 0x07,
    Unknown8 = 0x08,
    BoneIndices = 0x09,
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
    pub model_subresource: Option<ModelSubresource>,
    pub texture_subresource: Vec<TextureDescriptor>,
    pub collision_subresource: Option<CollisionSubresource>,
    pub other_subresources: Vec<RawModelSubresource>,
}

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

impl ModelDescriptor {
    fn from_bytes(data: &[u8]) -> Result<Self, crate::Error> {
        let RawModelDescriptor {
            footer_ptr: _,
            footer_entries,
            flags,
            unknown_u32_1,
            model_runtime_context: _,
            unknown_u32_2,
        } = RawModelDescriptor::read_le(&mut Cursor::new(data))
            .map_err(|_| AssetParseError::ErrorParsingDescriptor)?;

        let data_size = data.len() as u32;

        if data_size < size_of::<ModelDescriptor>() as u32 {
            return Err("data smaller than ModelDescriptor".into());
        }

        if data_size < 8 {
            return Err(AssetParseError::InputTooSmall.into());
        }

        let mut model_subresource = None;
        let mut texture_subresource = vec![];
        let mut collision_subresource = None;
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
                        texture_subresource.push(
                            Cursor::new(&data.get(ptr..).ok_or("unable to get tex subres bytes")?)
                                .read_le()?,
                        );
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
                ModelSubresType::Collision => {
                    let mut cur = Cursor::new(data);
                    cur.seek(SeekFrom::Start(ptr as u64))?;

                    collision_subresource = match cur
                        .read_le()
                        .map_err(|e| AssetParseError::InvalidDataViews(e.to_string()))
                    {
                        Ok(subres) => Some(subres),
                        Err(e) => {
                            eprintln!("error parsing model collision subres: {e}");
                            None
                        }
                    };
                }
                ModelSubresType::Flags
                | ModelSubresType::Unknown2
                | ModelSubresType::Unknown3
                | ModelSubresType::Unknown4
                | ModelSubresType::Matrices
                | ModelSubresType::Unknown8
                | ModelSubresType::BoneIndices
                | ModelSubresType::Unknown10
                | ModelSubresType::Unknown11
                | ModelSubresType::Unknown12
                | ModelSubresType::Unknown13
                | ModelSubresType::Unknown14
                | ModelSubresType::Unknown15
                | ModelSubresType::Unknown16
                | ModelSubresType::Unknown17
                | ModelSubresType::Unknown18
                | ModelSubresType::Unknown19
                | ModelSubresType::Unknown20
                | ModelSubresType::Unknown21 => {
                    other_subresources.push(RawModelSubresource {
                        subres_type,
                        subres_param: ptr,
                    });
                }
            };
        }

        Ok(Self {
            flags,
            unknown_u32_1,
            unknown_u32_2,
            model_subresource,
            other_subresources,
            texture_subresource,
            collision_subresource,
        })
    }
}

impl TryFrom<crate::RawAssetData> for Model {
    type Error = crate::Error;

    fn try_from(value: crate::RawAssetData) -> Result<Self, Self::Error> {
        let crate::RawAssetData {
            descriptor_bytes,
            resource_chunks,
        } = value;

        let descriptor = ModelDescriptor::from_bytes(&descriptor_bytes)?;

        let resource = resource_chunks.into_iter().flatten().collect::<Vec<_>>();

        if resource.is_empty() {
            return Err("no resource for model".into());
        }

        let mut model = Model {
            descriptor: descriptor.clone(),
            textures: vec![],
            resource: resource.clone(),
        };

        for subtex_desc in &model.descriptor.texture_subresource {
            let subres_bytes = resource
                .get(subtex_desc.texture_offset as usize..subtex_desc.texture_size as usize)
                .ok_or_else(|| {
                    format!(
                        "failed to get model tex resource [{}..{}] in subres of size {}",
                        subtex_desc.texture_offset as usize,
                        subtex_desc.texture_size as usize,
                        resource.len()
                    )
                })?
                .to_vec();

            model
                .textures
                .push(Texture::new(subtex_desc.clone(), subres_bytes));
        }
        Ok(model)
    }
}

impl TryFrom<Model> for crate::RawAssetData {
    type Error = crate::Error;

    fn try_from(_value: Model) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl AssetData for Model {
    const ASSET_TYPE: AssetType = AssetType::Model;
}

impl Model {
    /// Returns a list of textures if the model has any, and None otherwise.
    pub fn textures(&self) -> Option<&Vec<Texture>> {
        Some(&self.textures)
    }
}

#[derive(Clone, Debug)]
pub enum TexturedModelSubresource {
    Flags(u32),
    Textures(TexturesSubresource),
    Other {
        subresource: Vec<u8>,
        resource: Option<Vec<u8>>,
        original_size: usize,
    },
}

#[derive(Clone, Debug)]
pub struct TexturedModel {
    pub top_flags: u32,

    pub subresources: std::collections::BTreeMap<ModelSubresType, TexturedModelSubresource>,
}

impl TexturedModel {
    pub fn new(
        descriptor_bytes: &[u8],
        resource_bytes: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let rmd: RawModelDescriptor =
            RawModelDescriptor::read_le(&mut Cursor::new(descriptor_bytes))?;

        let offsets = rmd
            .footer_entries
            .iter()
            .filter_map(|entry| (entry.subres_type != ModelSubresType::Flags).then_some(entry.ptr))
            .collect::<Vec<_>>();

        let mut model = Self {
            top_flags: rmd.flags,
            subresources: Default::default(),
        };

        let mut textures_start_ptr: Option<u32> = None;

        for entry in &rmd.footer_entries {
            let base = entry.ptr;

            let chunk = if entry.subres_type == ModelSubresType::Flags {
                // Base is a u32, not a ptr
                model.subresources.insert(
                    ModelSubresType::Flags,
                    TexturedModelSubresource::Flags(base),
                );
                continue;
            } else {
                let end_offset = offsets
                    .iter()
                    .position(|v| *v == entry.ptr)
                    .and_then(|v| offsets.get(v + 1).copied())
                    .unwrap_or(rmd.footer_ptr);

                let start = entry.ptr as usize;
                let end = end_offset as usize;

                descriptor_bytes[start..end].to_vec()
            };

            match entry.subres_type {
                ModelSubresType::Texture => {
                    let mut cur = Cursor::new(&chunk);

                    let (num_textures, ptrs_ptr) = (
                        cur.read_u32::<LittleEndian>()?,
                        cur.read_u32::<LittleEndian>()? - base,
                    );

                    let mut cur = Cursor::new(&chunk[ptrs_ptr.try_into()?..]);
                    let texture_ptrs = (0..num_textures)
                        .map(|_| cur.read_u32::<LittleEndian>())
                        .collect::<Result<Vec<_>, _>>()?;

                    let textures = texture_ptrs
                        .into_iter()
                        .map(|texture_ptr| {
                            let descriptor = std::io::Cursor::new(
                                &chunk
                                    .get((texture_ptr - base).try_into()?..)
                                    .ok_or_else(|| "unable to get tex descriptor")?,
                            )
                            .read_le::<TextureDescriptor>()?;

                            let start = descriptor.texture_offset.try_into()?;
                            let end = start + usize::try_from(descriptor.texture_size)?;

                            let resource = resource_bytes[start..end].to_vec();

                            if textures_start_ptr.is_none() {
                                textures_start_ptr = Some(start.try_into()?);
                            }

                            Ok(Texture::new(descriptor, resource))
                        })
                        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;

                    model.subresources.insert(
                        ModelSubresType::Texture,
                        TexturedModelSubresource::Textures(TexturesSubresource {
                            textures,
                            original_size: chunk.len(),
                        }),
                    );
                }
                ModelSubresType::Flags => (),
                ModelSubresType::Mesh
                | ModelSubresType::Unknown2
                | ModelSubresType::Matrices
                | ModelSubresType::Collision
                | ModelSubresType::BoneIndices
                | ModelSubresType::Unknown3
                | ModelSubresType::Unknown4
                | ModelSubresType::Unknown8
                | ModelSubresType::Unknown10
                | ModelSubresType::Unknown11
                | ModelSubresType::Unknown12
                | ModelSubresType::Unknown13
                | ModelSubresType::Unknown14
                | ModelSubresType::Unknown15
                | ModelSubresType::Unknown16
                | ModelSubresType::Unknown17
                | ModelSubresType::Unknown18
                | ModelSubresType::Unknown19
                | ModelSubresType::Unknown20
                | ModelSubresType::Unknown21 => {
                    model.subresources.insert(
                        entry.subres_type,
                        TexturedModelSubresource::Other {
                            original_size: chunk.len(),
                            subresource: chunk,
                            resource: None,
                        },
                    );
                }
            }
        }

        let TexturedModelSubresource::Other {
            resource,
            subresource: _,
            original_size: _,
        } = model
            .subresources
            .get_mut(&ModelSubresType::Mesh)
            .ok_or_else(|| "texture subres but no model subres".to_owned())?
        else {
            return Err("No model subres in model".into());
        };

        if let Some(textures_start_ptr) = textures_start_ptr {
            *resource = Some(resource_bytes[..textures_start_ptr.try_into()?].to_vec());
        } else {
            *resource = Some(resource_bytes.to_vec());
        }

        Ok(model)
    }

    pub fn serialize(&self) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
        const ALIGNMENT: usize = 0x20;

        let mut model_bytes = vec![0u8; ALIGNMENT];
        let mut resource_bytes = vec![0u8; 0];

        let mut footer = vec![];

        let num_subresources: u32 = self.subresources.len().try_into()?;

        model_bytes[4..8].copy_from_slice(&num_subresources.to_le_bytes());
        model_bytes[8..12].copy_from_slice(&self.top_flags.to_le_bytes());

        // Pack the subresources back into the model (tracking offset as we go)

        for (subres_type, subres) in self.subresources.clone() {
            match subres_type {
                ModelSubresType::Flags => {
                    let TexturedModelSubresource::Flags(flags) = subres else {
                        return Err("subres type mismatch".into());
                    };

                    footer.push((ModelSubresType::Flags, flags));
                }
                ModelSubresType::Texture => {
                    let TexturedModelSubresource::Textures(textures_subres) = subres else {
                        return Err("subres type mismatch".into());
                    };

                    if !textures_subres.textures.is_empty() {
                        footer.push((ModelSubresType::Texture, u32::try_from(model_bytes.len())?));
                        let (mut subresource, mut resource) = textures_subres.serialize(
                            model_bytes.len().try_into()?,
                            resource_bytes.len().try_into()?,
                            ALIGNMENT,
                        )?;

                        if subresource.len() < textures_subres.original_size {
                            eprintln!(
                                "warning: subresource of size 0x{subres_len:x} is shorter than original size 0x{original_size:x}",
                                subres_len = subresource.len(),
                                original_size = textures_subres.original_size
                            );
                            subresource.resize(textures_subres.original_size, 0u8);
                        }

                        model_bytes.append(&mut subresource);
                        resource_bytes.append(&mut resource);
                    }
                }
                ModelSubresType::Mesh
                | ModelSubresType::Unknown2
                | ModelSubresType::Unknown3
                | ModelSubresType::Unknown4
                | ModelSubresType::Matrices
                | ModelSubresType::Collision
                | ModelSubresType::Unknown8
                | ModelSubresType::BoneIndices
                | ModelSubresType::Unknown10
                | ModelSubresType::Unknown11
                | ModelSubresType::Unknown12
                | ModelSubresType::Unknown13
                | ModelSubresType::Unknown14
                | ModelSubresType::Unknown15
                | ModelSubresType::Unknown16
                | ModelSubresType::Unknown17
                | ModelSubresType::Unknown18
                | ModelSubresType::Unknown19
                | ModelSubresType::Unknown20
                | ModelSubresType::Unknown21 => {
                    let TexturedModelSubresource::Other {
                        mut subresource,
                        resource,
                        original_size,
                    } = subres
                    else {
                        return Err("subres type mismatch".into());
                    };

                    if !subresource.is_empty() {
                        footer.push((subres_type, u32::try_from(model_bytes.len())?));
                    }

                    if subresource.len() < original_size {
                        eprintln!(
                            "warning: subresource of size 0x{subres_len:x} is shorter than original size 0x{original_size:x}",
                            subres_len = subresource.len()
                        );
                        subresource.resize(original_size, 0u8);
                    }

                    model_bytes.append(&mut subresource);
                    if let Some(mut resource) = resource {
                        resource_bytes.append(&mut resource);
                    }
                }
            }
        }

        let base_len = u32::try_from(model_bytes.len())?;
        model_bytes[0..4].copy_from_slice(&base_len.to_le_bytes());

        for (subres_type, ptr) in footer {
            model_bytes.extend_from_slice(&u32::from(subres_type).to_le_bytes());
            model_bytes.extend_from_slice(&ptr.to_le_bytes());
        }

        Ok((model_bytes, resource_bytes))
    }
}

#[derive(Clone, Debug, Default)]
pub struct TexturesSubresource {
    pub textures: Vec<Texture>,
    pub original_size: usize,
}

impl TexturesSubresource {
    pub fn serialize(
        &self,
        descriptor_base: u32,
        resource_base: u32,
        alignment: usize,
    ) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
        let mut descriptor_bytes = vec![0u8; alignment];
        let mut resource_bytes = vec![];

        let ptrs_list_size = {
            let mut size = self.textures.len() * 4;

            // If not aligned, align it
            if !size.is_multiple_of(alignment) {
                size += alignment - (size % alignment);
            }

            size
        };

        let mut ptrs_list = vec![0u8; ptrs_list_size];

        descriptor_bytes[0..4].copy_from_slice(&u32::try_from(self.textures.len())?.to_le_bytes());
        descriptor_bytes[4..8]
            .copy_from_slice(&(descriptor_base + u32::try_from(alignment)?).to_le_bytes());

        for i in 0..self.textures.len() {
            ptrs_list[i * 4..i * 4 + 4].copy_from_slice(
                &(u32::try_from(
                    i * 0x40
                        + usize::try_from(descriptor_base)?
                        + descriptor_bytes.len()
                        + ptrs_list_size,
                )?)
                .to_le_bytes(),
            );
        }

        descriptor_bytes.extend_from_slice(&ptrs_list);

        for texture in &self.textures {
            let mut descriptor = texture.descriptor().clone();
            descriptor.texture_offset = resource_base + u32::try_from(resource_bytes.len())?;
            descriptor.texture_size = texture.bytes().len().try_into()?;

            resource_bytes.extend_from_slice(texture.bytes());

            let bytes = {
                let mut bytes = vec![];
                descriptor.write_le(&mut Cursor::new(&mut bytes))?;

                if bytes.len() < 0x40 {
                    bytes.resize(0x40, 0u8);
                }

                bytes
            };

            descriptor_bytes.extend_from_slice(&bytes);
        }

        Ok((descriptor_bytes, resource_bytes))
    }
}
