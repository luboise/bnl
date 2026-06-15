// pub mod gltf;
pub mod nd;
pub mod sub_colliders;
pub mod sub_main;
pub mod subresources;

use strum::IntoEnumIterator;
pub use subresources::ModelSubresType;

use subresources::*;

use std::io::{Cursor, Seek, SeekFrom, Write};

use binrw::{BinRead, BinReaderExt, BinWriterExt, binrw};

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

    pub fn num_subresources(&self) -> usize {
        ModelSubresType::iter()
            .filter(|v| self.has_subresource(*v))
            .count()
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
            ModelSubresType::Unknown0x08 => self.subresource0x8.is_some(),
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

        let mrc = nd::ModelReadContext {
            properties: &Default::default(),
            resource: resource.as_slice(),
        };

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

            let start = usize::try_from(ptr)?;
            let end = end_offset.try_into()?;

            let subres_bytes = descriptor_bytes
                .get(start..end)
                .ok_or("unable to get model subresource bytes")?;

            let mut cur = std::io::Cursor::new(&descriptor_bytes);
            cur.seek_relative(start.try_into()?)?;

            // TODO: pull properties in from the parent struct and use it to name the bones
            cur.read_le_args(mrc)?
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
                    let mut cur = Cursor::new(&descriptor_bytes);
                    cur.seek(SeekFrom::Start(ptr as u64))?;

                    let collision_subresource = cur.read_le()?;
                    model.collision_subresource = Some(collision_subresource);
                }
                ModelSubresType::Texture => {
                    let mut cur = std::io::Cursor::new(&descriptor_bytes);
                    cur.seek(SeekFrom::Start(ptr.into()))?;
                    model.textures_subresource = Some(cur.read_le_args(mrc)?)
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
                | ModelSubresType::Unknown0x08
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
                        ModelSubresType::Unknown0x08 => &mut model.subresource0x8,
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

    fn try_from(model: Model) -> Result<Self, Self::Error> {
        let mut descriptor_bytes = vec![];

        let num_subresources = model.num_subresources();
        let mut footer_entries = vec![];

        let Model {
            flags,
            unknown_u32_1,
            unknown_u32_2,
            model_subresource,
            flags_subresource,
            subresource0x2,
            subresource0x3,
            subresource0x4,
            subresource0x5,
            collision_subresource,
            textures_subresource,
            subresource0x8,
            subresource0x9,
            transforms_subresource,
            subresource0xb,
            subresource0xc,
            subresource0xd,
            subresource0xe,
            subresource0xf,
            subresource0x10,
            subresource0x11,
            tiles_subresource,
            subresource0x13,
            subresource0x14,
            subresource0x15,
        } = model;

        let mut writer = std::io::Cursor::new(&mut descriptor_bytes);

        // footer_ptr
        writer.write_le(&0u32)?;
        writer.write_le(&(num_subresources as u32))?;
        writer.write_le(&flags)?;
        writer.write_le(&unknown_u32_1)?;
        writer.write_le(&unknown_u32_2)?;

        // padding
        writer.write_le(&0u32)?;
        writer.write_le(&0u32)?;
        writer.write_le(&0u32)?;

        footer_entries.push((ModelSubresType::Mesh, writer.stream_position()? as u32));

        let mwc = nd::new_write_context();
        writer.write_le_args(&model_subresource, mwc.clone())?;

        // Align resource to 0x100 after writing model subresource
        {
            let res = &mut mwc.borrow_mut().resource;
            let modulo = res.len() % 0x100;
            if modulo != 0 {
                res.extend(&vec![0u8; 0x100 - modulo]);
            }
        }

        let nd::ModelWriteContextInner {
            nd_heirarchy_ptrs,
            mut resource,
            rigid_entries,
        } = std::rc::Rc::try_unwrap(mwc)
            .map_err(|e| "model write context still in use after finishing export")?
            .into_inner();

        if !nd_heirarchy_ptrs.is_empty() {
            return Err("heirarchy is not empty after exiting the model".into());
        }
        if !rigid_entries.is_empty() {
            return Err("rigid_entries is not empty after exiting the model, ndRigidSkinIdx indices were lost".into());
        }
        if let Some(flags_subresource) = flags_subresource {
            footer_entries.push((ModelSubresType::Flags, flags_subresource));
        }

        for (subres_type, subres) in [
            (ModelSubresType::Unknown0x02, subresource0x2),
            (ModelSubresType::Unknown0x03, subresource0x3),
            (ModelSubresType::Unknown0x04, subresource0x4),
            (ModelSubresType::Matrices, subresource0x5),
        ] {
            let Some(subres) = subres else {
                continue;
            };

            let pos = writer.stream_position()?;

            footer_entries.push((subres_type, u32::try_from(pos)?));
            writer.write_all(&subres)?;
        }

        if let Some(collision_subresource) = collision_subresource {
            footer_entries.push((
                ModelSubresType::Collision,
                writer.stream_position()?.try_into()?,
            ));

            writer.write_le(&collision_subresource)?;
        }

        if let Some(textures_subresource) = textures_subresource {
            let base = writer.stream_position()?.try_into()?;

            footer_entries.push((ModelSubresType::Texture, base));

            let (tex, res) =
                textures_subresource.serialize(base, resource.len().try_into()?, 0x20)?;

            writer.write_all(&tex)?;
            resource.extend(res);
        }

        for (subres_type, subres) in [
            (ModelSubresType::Unknown0x08, subresource0x8),
            (ModelSubresType::BoneIndices, subresource0x9),
            (ModelSubresType::Transforms, subresource0x10),
            (ModelSubresType::Unknown0x0b, subresource0xb),
        ] {
            let Some(subres) = subres else {
                continue;
            };

            let pos = writer.stream_position()?;

            footer_entries.push((subres_type, u32::try_from(pos)?));
            writer.write_all(&subres)?;
        }

        if let Some(_subresource0xc) = subresource0xc {
            todo!("subresource 0xc export not implemented yet")
        }

        //            (ModelSubresType::Unknown0x02, subresource0xd),
        //            (ModelSubresType::Unknown0x02, subresource0xe),
        //            (ModelSubresType::Unknown0x02, subresource0xf),
        //            (ModelSubresType::Unknown0x02, subresource0x10),
        //            (ModelSubresType::Unknown0x02, subresource0x11),
        //            (ModelSubresType::Tiles, tiles_subresource),
        //            (ModelSubresType::Unknown0x02, subresource0x13),
        //            (ModelSubresType::Unknown0x02, subresource0x14),
        //            (ModelSubresType::Unknown0x02, subresource0x15),

        let footer_entries_ptr = u32::try_from(writer.stream_position()?)?;

        for (footer_entry_type, footer_u32) in footer_entries {
            writer.write_le(&footer_entry_type)?;
            writer.write_le(&footer_u32)?;
        }

        writer.rewind()?;
        writer.write_le(&footer_entries_ptr)?;

        Ok(Self {
            descriptor_bytes,
            resource_chunks: vec![resource],
        })
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

#[cfg(test)]
#[path = "./model_tests.rs"]
mod tests;
