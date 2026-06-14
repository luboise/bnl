// pub mod gltf;
pub mod nd;
pub mod sub_colliders;
pub mod sub_main;
pub mod subresources;

pub use subresources::ModelSubresType;

use subresources::*;

use std::io::{Cursor, Seek, SeekFrom};

use binrw::{BinRead, BinReaderExt, binrw};

use crate::asset::{
    AssetData, AssetType,
    model::{sub_colliders::CollisionSubresource, sub_main::ModelSubresource},
    texture::Texture,
};

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

    #[brw(magic = 0u32)] // used at runtime to store data about model, always zero
    unknown_u32_2: u32,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub flags: u32,
    pub unknown_u32_1: u32,
    pub unknown_u32_2: u32,

    pub model_subresource: ModelSubresource,
    pub flags_subresource: Option<u32>,
    pub subresource0x2: Option<Subresource0x2>,
    pub subresource0x3: Option<Subresource0x3>,
    pub subresource0x4: Option<Subresource0x4>,
    pub subresource0x5: Option<Subresource0x5>,
    pub collision_subresource: Option<CollisionSubresource>,
    pub textures_subresource: Option<TexturesSubresource>,
    pub subresource0x8: Option<Subresource0x8>,
    pub subresource0x9: Option<Subresource0x9>,
    pub transforms_subresource: Option<Subresource0xa>,
    pub subresource0xb: Option<Subresource0xb>,
    pub subresource0xc: Option<Subresource0xc>,
    pub subresource0xd: Option<Subresource0xd>,
    pub subresource0xe: Option<Subresource0xe>,
    pub subresource0xf: Option<Subresource0xf>,
    pub subresource0x10: Option<Subresource0x10>,
    pub subresource0x11: Option<Subresource0x11>,
    pub tiles_subresource: Option<TilesSubresource>,
    pub subresource0x13: Option<Subresource0x13>,
    pub subresource0x14: Option<Subresource0x14>,
    pub subresource0x15: Option<Subresource0x15>,
}

impl Model {
    pub fn model_subresource(&self) -> &ModelSubresource {
        &self.model_subresource
    }

    pub fn has_subresource(&self, subres_type: ModelSubresType) -> bool {
        match subres_type {
            ModelSubresType::Mesh => true,
            ModelSubresType::Flags => self.flags_subresource.is_some(),
            ModelSubresType::Unknown0x02 => self.subresource0x2.is_some(),
            ModelSubresType::Unknown0x03 => self.subresource0x3.is_some(),
            ModelSubresType::Unknown0x04 => self.subresource0x4.is_some(),
            ModelSubresType::Matrices => self.subresource0x5.is_some(),
            ModelSubresType::Collision => self.collision_subresource.is_some(),
            ModelSubresType::Texture => self.textures_subresource.is_some(),
            ModelSubresType::Unknown8 => self.subresource0x8.is_some(),
            ModelSubresType::BoneIndices => self.subresource0x9.is_some(),
            ModelSubresType::Transforms => self.transforms_subresource.is_some(),
            ModelSubresType::Unknown0x0b => self.subresource0xb.is_some(),
            ModelSubresType::Unknown0x0c => self.subresource0xc.is_some(),
            ModelSubresType::Unknown0x0d => self.subresource0xd.is_some(),
            ModelSubresType::Unknown0x0e => self.subresource0xe.is_some(),
            ModelSubresType::Unknown0x0f => self.subresource0xf.is_some(),
            ModelSubresType::Unknown0x10 => self.subresource0x10.is_some(),
            ModelSubresType::Unknown0x11 => self.subresource0x11.is_some(),
            ModelSubresType::Tiles => self.tiles_subresource.is_some(),
            ModelSubresType::Unknown0x13 => self.subresource0x13.is_some(),
            ModelSubresType::Unknown0x14 => self.subresource0x14.is_some(),
            ModelSubresType::Unknown0x15 => self.subresource0x15.is_some(),
        }
    }

    #[deprecated(note = "use Model::properties")]
    pub fn key_value_map(&self) -> &indexmap::IndexMap<String, Vec<u8>> {
        &self.model_subresource.properties
    }
}

impl TryFrom<crate::RawAssetData> for Model {
    type Error = crate::Error;

    fn try_from(value: crate::RawAssetData) -> Result<Self, Self::Error> {
        let crate::RawAssetData {
            descriptor_bytes,
            resource_chunks,
        } = value;

        let resource = resource_chunks.into_iter().flatten().collect::<Vec<_>>();

        let RawModelDescriptor {
            footer_ptr,
            footer_entries,
            flags,
            unknown_u32_1,
            unknown_u32_2,
        } = RawModelDescriptor::read_le(&mut Cursor::new(&descriptor_bytes))?;

        let footer_offsets = footer_entries
            .iter()
            .filter_map(|entry| (entry.subres_type != ModelSubresType::Flags).then_some(entry.ptr))
            .collect::<Vec<_>>();

        let mut it = footer_entries.into_iter();
        let Some(model_footer_entry) = it.next() else {
            return Err("no footer entries in model".into());
        };

        if model_footer_entry.subres_type != ModelSubresType::Mesh {
            return Err("no mesh in model".into());
        }

        let model_subresource = {
            let ptr = model_footer_entry.ptr;

            let end_offset = footer_offsets
                .iter()
                .position(|v| *v == ptr)
                .and_then(|v| footer_offsets.get(v + 1).copied())
                .unwrap_or(footer_ptr);

            let start = ptr.try_into()?;
            let end = end_offset.try_into()?;

            let subres_bytes = descriptor_bytes
                .get(start..end)
                .ok_or("unable to get model subresource bytes")?;

            let mut cur = std::io::Cursor::new(&descriptor_bytes);
            cur.seek_relative(start.try_into()?)?;

            cur.read_le_args((resource.as_slice(), 0))?
        };

        let mut model = Model {
            flags,
            unknown_u32_1,
            unknown_u32_2,
            model_subresource,
            flags_subresource: None,
            subresource0x2: None,
            subresource0x3: None,
            subresource0x4: None,
            subresource0x5: None,
            collision_subresource: None,
            textures_subresource: None,
            subresource0x8: None,
            subresource0x9: None,
            transforms_subresource: None,
            subresource0xb: None,
            subresource0xc: None,
            subresource0xd: None,
            subresource0xe: None,
            subresource0xf: None,
            subresource0x10: None,
            subresource0x11: None,
            tiles_subresource: None,
            subresource0x13: None,
            subresource0x14: None,
            subresource0x15: None,
        };

        for ModelSubresHeader { subres_type, ptr } in it {
            if model.has_subresource(subres_type) {
                return Err(
                    format!("model subresource {subres_type} appears more than once").into(),
                );
            }

            let subresource_bytes = if subres_type == ModelSubresType::Flags {
                &ptr.to_le_bytes()
            } else {
                let end_offset = footer_offsets
                    .iter()
                    .position(|v| *v == ptr)
                    .and_then(|v| footer_offsets.get(v + 1).copied())
                    .unwrap_or(footer_ptr);

                let start = ptr as usize;
                let end = end_offset as usize;

                descriptor_bytes
                    .get(start..end)
                    .ok_or("unable to get desc bytes")?
            };

            match subres_type {
                // caught by duplicate check above
                ModelSubresType::Mesh => unreachable!(),
                ModelSubresType::Flags => {
                    model.flags_subresource =
                        Some(u32::from_le_bytes(subresource_bytes.try_into()?))
                }
                ModelSubresType::Matrices => {
                    model.subresource0x5 = Some(subresource_bytes.to_owned());
                }
                ModelSubresType::Collision => {
                    ()
                    // let mut cur = Cursor::new(&descriptor_bytes);
                    // cur.seek(SeekFrom::Start(ptr as u64))?;
                    //
                    // let collision_subresource = cur.read_le()?;
                    // model.collision_subresource = Some(collision_subresource);
                }
                ModelSubresType::Texture => {
                    let textures_subresource =
                        TexturesSubresource::new(subresource_bytes, ptr, &resource, 0)?;
                    model.textures_subresource = Some(textures_subresource);
                }
                ModelSubresType::Transforms => {
                    model.transforms_subresource =
                        Some(std::io::Cursor::new(subresource_bytes).read_le()?);
                }
                ModelSubresType::Tiles => {
                    todo!()
                }
                ModelSubresType::Unknown0x0c => {
                    todo!()
                }
                ModelSubresType::Unknown0x02
                | ModelSubresType::Unknown0x03
                | ModelSubresType::Unknown0x04
                | ModelSubresType::Unknown8
                | ModelSubresType::BoneIndices
                | ModelSubresType::Unknown0x0b
                | ModelSubresType::Unknown0x0d
                | ModelSubresType::Unknown0x0e
                | ModelSubresType::Unknown0x0f
                | ModelSubresType::Unknown0x10
                | ModelSubresType::Unknown0x11
                | ModelSubresType::Unknown0x13
                | ModelSubresType::Unknown0x14
                | ModelSubresType::Unknown0x15 => {
                    let opt = match subres_type {
                        ModelSubresType::Mesh
                        | ModelSubresType::Flags
                        | ModelSubresType::Matrices
                        | ModelSubresType::Collision
                        | ModelSubresType::Transforms
                        | ModelSubresType::Texture
                        | ModelSubresType::Unknown0x0c
                        | ModelSubresType::Tiles => unreachable!(),
                        ModelSubresType::Unknown0x02 => &mut model.subresource0x2,
                        ModelSubresType::Unknown0x03 => &mut model.subresource0x3,
                        ModelSubresType::Unknown0x04 => &mut model.subresource0x4,
                        ModelSubresType::Unknown8 => &mut model.subresource0x8,
                        ModelSubresType::BoneIndices => &mut model.subresource0x9,
                        ModelSubresType::Unknown0x0b => &mut model.subresource0xb,
                        ModelSubresType::Unknown0x0d => &mut model.subresource0xd,
                        ModelSubresType::Unknown0x0e => &mut model.subresource0xe,
                        ModelSubresType::Unknown0x0f => &mut model.subresource0xf,
                        ModelSubresType::Unknown0x10 => &mut model.subresource0x10,
                        ModelSubresType::Unknown0x11 => &mut model.subresource0x11,
                        ModelSubresType::Unknown0x13 => &mut model.subresource0x13,
                        ModelSubresType::Unknown0x14 => &mut model.subresource0x14,
                        ModelSubresType::Unknown0x15 => &mut model.subresource0x15,
                    };
                    // no need for dupe check since it should've been caught above
                    *opt = Some(subresource_bytes.to_owned());
                }
            };
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
    #[deprecated(note = "Just access the field `Model.textures_subresource.textures`")]
    /// Returns a list of textures if the model has any, and None otherwise.
    pub fn textures(&self) -> Option<&Vec<Texture>> {
        self.textures_subresource
            .as_ref()
            .map(|subres| &subres.textures)
    }
}

/*
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
                        TexturedModelSubresource::Textures(TexturesSubresource { textures }),
                    );
                }
                ModelSubresType::Flags => (),
                ModelSubresType::Mesh
                | ModelSubresType::Unknown0x02
                | ModelSubresType::Matrices
                | ModelSubresType::Collision
                | ModelSubresType::BoneIndices
                | ModelSubresType::Unknown0x03
                | ModelSubresType::Unknown0x04
                | ModelSubresType::Unknown8
                | ModelSubresType::Transforms
                | ModelSubresType::Unknown0x0b
                | ModelSubresType::Unknown0x0c
                | ModelSubresType::Unknown0x0d
                | ModelSubresType::Unknown0x0e
                | ModelSubresType::Unknown0x0f
                | ModelSubresType::Unknown0x10
                | ModelSubresType::Unknown0x11
                | ModelSubresType::Tiles
                | ModelSubresType::Unknown0x13
                | ModelSubresType::Unknown0x14
                | ModelSubresType::Unknown0x15 => {
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
                | ModelSubresType::Unknown0x02
                | ModelSubresType::Unknown0x03
                | ModelSubresType::Unknown0x04
                | ModelSubresType::Matrices
                | ModelSubresType::Collision
                | ModelSubresType::Unknown8
                | ModelSubresType::BoneIndices
                | ModelSubresType::Transforms
                | ModelSubresType::Unknown0x0b
                | ModelSubresType::Unknown0x0c
                | ModelSubresType::Unknown0x0d
                | ModelSubresType::Unknown0x0e
                | ModelSubresType::Unknown0x0f
                | ModelSubresType::Unknown0x10
                | ModelSubresType::Unknown0x11
                | ModelSubresType::Tiles
                | ModelSubresType::Unknown0x13
                | ModelSubresType::Unknown0x14
                | ModelSubresType::Unknown0x15 => {
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
*/

#[cfg(test)]
#[path = "./model_tests.rs"]
mod tests;
