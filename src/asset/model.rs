pub mod nd;
pub mod sub_colliders;
pub mod subresources;

use binrw::{BinRead, BinReaderExt, BinWriterExt, binrw};
use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{Read, Seek, SeekFrom, Write};
use strum::IntoEnumIterator;
pub use subresources::ModelSubresType;

use subresources::*;

use crate::asset::{
    AssetData, AssetType, model::sub_colliders::CollisionSubresource, texture::Texture,
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
    pub fn key_value_map(&self) -> &ModelProperties {
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
        } = RawModelDescriptor::read_le(&mut std::io::Cursor::new(&descriptor_bytes))?;

        let footer_offsets = footer_entries
            .iter()
            .filter_map(|entry| (entry.subres_type != ModelSubresType::Flags).then_some(entry.ptr))
            .collect::<Vec<_>>();

        let mrc = ModelReadContext {
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
                    let mut cur = std::io::Cursor::new(&descriptor_bytes);
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
                    let mut cur = std::io::Cursor::new(&descriptor_bytes);
                    cur.seek(SeekFrom::Start(ptr.into()))?;
                    model.tiles_subresource = Some(cur.read_le()?)
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

        let mwc = new_write_context();

        mwc.borrow_mut().properties = model_subresource.properties.clone();

        writer.write_le_args(&model_subresource, mwc.clone())?;

        // Align resource to 0x100 after writing model subresource
        {
            let res = &mut mwc.borrow_mut().resource;
            let modulo = res.len() % 0x100;
            if modulo != 0 {
                res.extend(&vec![0u8; 0x100 - modulo]);
            }
        }

        let ModelWriteContextInner {
            nd_heirarchy_ptrs,
            mut resource,
            rigid_entries,
            properties: _,
            property_counts: _,
            in_blend_shape: _,
        } = std::rc::Rc::try_unwrap(mwc)
            .map_err(|_| "model write context still in use after finishing export")?
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

#[derive(Clone, Debug)]
pub struct ModelSubresource {
    pub unknown1: u32,
    pub unknown2: u32,
    // nodes_ptr: u32,
    // num_nodes: u32,

    // key_values_ptr: u32,
    // map2_ptr: u32,
    pub floats: [f32; 4],
    pub next_ptr: u32,
    pub model_model_root: u32,
    pub nodes: Vec<nd::Nd>,
    pub properties: ModelProperties,
}

impl ModelSubresource {
    #[deprecated(note = "use BinReaderExt::read_le_args(ModelReadContext)")]
    pub fn from_bytes(
        bytes: &[u8],
        resource: &[u8],
        _resource_base: u32,
    ) -> Result<Self, crate::Error> {
        Ok(std::io::Cursor::new(bytes).read_le_args(ModelReadContext {
            properties: &Default::default(),
            resource,
        })?)
    }

    #[deprecated(note = "use ModelSubresource.nodes")]
    pub fn primitives(&self) -> &[nd::Nd] {
        &self.nodes
    }
}

impl binrw::BinRead for ModelSubresource {
    type Args<'a> = ModelReadContext<'a>;

    fn read_options<R: std::io::Read + Seek>(
        reader: &mut R,
        _endian: binrw::Endian,
        mrc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        let ModelReadContext {
            properties: _,
            resource,
        } = mrc;

        // FIXME: make this more efficient
        let mut reader = {
            // Model subres starts with 0x40, which points to 0x20 inside of the resource
            // => Skip 0x20 in and set that as zero
            let begin_ptr = reader.read_u32::<LittleEndian>()?;
            reader.seek(SeekFrom::Start(begin_ptr.into()))?;
            std::io::Cursor::new(reader.bytes().collect::<Result<Vec<_>, _>>()?)
        };

        let unknown1 = reader.read_u32::<LittleEndian>()?;
        let unknown2 = reader.read_u32::<LittleEndian>()?;
        let primitive_ptrs_start = reader.read_u32::<LittleEndian>()?;
        let primitive_count = reader.read_u32::<LittleEndian>()?;
        let properties_ptr = reader.read_u32::<LittleEndian>()?;
        let properties: ModelProperties = {
            if properties_ptr == 0 {
                Default::default()
            } else {
                let mut reader = reader.clone();
                reader.seek(SeekFrom::Start(properties_ptr.into()))?;
                reader
                    .read_le::<ModelProperties>()?
                    .try_into()
                    .map_err(|e| binrw::Error::Custom {
                        pos: reader.stream_position().unwrap_or_default(),
                        err: Box::new(format!(
                            "unable to convert model_properties to hashmap: {e}"
                        )),
                    })?
            }
        };

        let map2_ptr = reader.read_u32::<LittleEndian>()?;
        if map2_ptr != 0 {
            return Err(binrw::Error::AssertFail {
                pos: reader.stream_position().unwrap_or(0),
                message: "map2_ptr is not 0".to_owned(),
            });
        }

        let floats = reader.read_le()?;
        let next_ptr = reader.read_le()?;
        let model_model_root = reader.read_le()?;

        let stream_end_position = reader.stream_position()?;

        let primitive_ptrs: Vec<u32> = {
            let mut reader = reader.clone();
            reader.seek(SeekFrom::Start(primitive_ptrs_start as u64))?;

            (0..primitive_count as usize)
                .map(|_| reader.read_u32::<LittleEndian>())
                .collect::<Result<_, _>>()?
        };

        let mrc = ModelReadContext::new(&properties, resource);

        let mut nodes = vec![];
        for primitive_ptr in primitive_ptrs {
            let mut reader = reader.clone();
            reader.seek(SeekFrom::Start(primitive_ptr.into()))?;
            let nd: nd::Nd = reader.read_le_args((mrc,))?;
            nodes.push(nd);
        }

        reader.seek(SeekFrom::Start(stream_end_position))?;

        Ok(Self {
            unknown1,
            unknown2,
            floats,
            next_ptr,
            model_model_root,
            nodes,
            properties,
        })
    }
}

impl binrw::BinWrite for ModelSubresource {
    type Args<'a> = ModelWriteContext;

    fn write_options<W: std::io::prelude::Write + Seek>(
        &self,
        writer: &mut W,
        _endian: binrw::Endian,
        mwc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        let base = u32::try_from(writer.stream_position()?).map_err(nd::br_error(writer))?;

        let subres = {
            let mut subres = vec![];
            let mut cur = std::io::Cursor::new(&mut subres);

            let ModelSubresource {
                unknown1,
                unknown2,
                floats,
                next_ptr,
                model_model_root,
                nodes,
                properties: _,
            } = self;

            cur.write_le(&unknown1)?;
            cur.write_le(&unknown2)?;
            // nodes ptr
            cur.write_le(&0x30u32)?;
            cur.write_le(&(self.nodes.len() as u32))?;
            // properties_ptr
            cur.write_le(&0u32)?;
            // map2
            cur.write_le(&0u32)?;

            cur.write_le(&floats)?;
            cur.write_le(&next_ptr)?;
            cur.write_le(&model_model_root)?;

            {
                let ptrs_start = cur.stream_position()? as u32;
                let mut nd_ptr = ptrs_start + 4 * nodes.len() as u32;

                for node in nodes {
                    nd_ptr = {
                        cur.write_le(&nd_ptr)?;
                        let restore = cur.stream_position()?;
                        cur.seek(SeekFrom::Start(nd_ptr.into()))?;
                        cur.write_le_args(node, mwc.clone())?;
                        let write_end = cur.stream_position()?;
                        cur.seek(SeekFrom::Start(restore))?;
                        write_end as u32
                    };
                }

                // Skip to after the nodes after writing them
                cur.seek(SeekFrom::Start(nd_ptr.into()))?;
            }

            for (nd_offset, indices) in std::mem::take(&mut mwc.borrow_mut().rigid_entries) {
                let ptr = cur.stream_position()? as u32;

                cur.seek(SeekFrom::Start(nd_offset))?;
                cur.write_le(&ptr)?;
                cur.seek(SeekFrom::Start(ptr.into()))?;
                cur.write_all(&indices)?;
            }

            let properties = &mwc.borrow().properties;

            if !properties.is_empty() {
                let properties_pos = cur.stream_position()?;
                cur.write_le(&ModelProperties::from(properties.clone()))?;

                let restore = cur.stream_position()?;

                // update the properties pos
                cur.seek(SeekFrom::Start(0x10))?;
                cur.write_le(&(properties_pos as u32))?;

                cur.seek(SeekFrom::Start(restore))?;
            }

            subres
        };

        let subres = (base + 0x20)
            .to_le_bytes()
            .into_iter()
            .chain([0u8; 32 - 4])
            .chain(subres)
            .collect::<Vec<_>>();

        writer.write_all(&subres)?;

        // pad to 16
        while writer.stream_position()? % 16 != 0 {
            writer.write_le(&0u8)?;
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct ModelReadContext<'a> {
    pub properties: &'a ModelProperties,
    pub resource: &'a [u8],
}

impl<'a> ModelReadContext<'a> {
    pub fn new(properties: &'a ModelProperties, resource: &'a [u8]) -> Self {
        Self {
            properties,
            resource,
        }
    }

    pub fn get_bone_name(&self, bone_index: u32) -> Option<String> {
        self.properties.iter().find_map(|property| {
            let ModelProperty { key, value } = property;

            let key = key.to_string();

            if !is_bone_name(&key) || property.value.len() != 4 {
                return None;
            }

            if !u32::from_le_bytes(value.as_slice().try_into().unwrap()) == bone_index {
                return None;
            }

            Some(key)
        })
    }
}

pub type ModelWriteContext = std::rc::Rc<std::cell::RefCell<ModelWriteContextInner>>;

pub fn new_write_context() -> ModelWriteContext {
    ModelWriteContext::new(
        ModelWriteContextInner {
            nd_heirarchy_ptrs: vec![],
            resource: vec![],
            rigid_entries: vec![],
            properties: Default::default(),
            property_counts: Default::default(),
            in_blend_shape: false,
        }
        .into(),
    )
}

#[derive(Clone, Debug)]
pub struct ModelWriteContextInner {
    pub nd_heirarchy_ptrs: Vec<u32>,
    pub resource: Vec<u8>,
    pub rigid_entries: Vec<(u64, Vec<u8>)>,
    pub properties: ModelProperties,

    /// Tracks how many times a property has come up in the properties
    /// Used for shader properties which appear more than once
    pub property_counts: std::collections::HashMap<String, usize>,
    pub in_blend_shape: bool,
}

pub fn is_bone_name<S: AsRef<str>>(s: S) -> bool {
    let s = s.as_ref();
    s.to_lowercase().starts_with("joint") || ["BASE", "MID"].contains(&s)
}

#[binrw::binread]
#[br(little, stream = r)]
#[derive(Clone, Debug, Default)]
pub struct ModelProperties {
    #[br(temp)]
    num_key_values: u32,
    #[br(temp, assert(r.stream_position().is_ok_and(|v| v == u64::from(key_values_ptr))))]
    key_values_ptr: u32,
    #[br(count = num_key_values)]
    pub properties: Vec<ModelProperty>,
}

impl ModelProperties {
    pub fn get(&self, key: &str) -> Option<&[u8]> {
        self.properties
            .iter()
            .find(|property| property.key.0.as_slice() == key.as_bytes())
            .map(|property| property.value.as_slice())
    }

    pub fn get_all(&mut self, key: &str) -> Vec<&mut [u8]> {
        self.properties
            .iter_mut()
            .filter(|property| property.key.to_string() == key)
            .map(|property| property.value.as_mut_slice())
            .collect()
    }

    pub fn get_property(&self, index: usize) -> Option<&ModelProperty> {
        self.properties.get(index)
    }

    pub fn get_keys_from_u32(&self, value: u32) -> Vec<String> {
        self.properties
            .iter()
            .filter_map(|property| {
                (property.value.len() == 4
                    && value == u32::from_le_bytes(property.value.as_slice().try_into().unwrap()))
                .then(|| property.key.to_string())
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.properties.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = &ModelProperty> {
        self.properties.iter()
    }
}

impl IntoIterator for ModelProperties {
    type Item = ModelProperty;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.properties.into_iter()
    }
}

impl binrw::BinWrite for ModelProperties {
    type Args<'a> = ();

    fn write_options<W: std::io::prelude::Write + Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        _: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        writer.write_le(&(self.properties.len() as u32))?;

        if self.properties.is_empty() {
            writer.write_le(&0u32)?;
            return Ok(());
        } else {
            let entries_start = (writer.stream_position()? + 0x4) as u32;
            writer.write_le(&entries_start)?;
        }

        let mut data_bytes = vec![];

        let mut data_ptr = (self.properties.len() as u64 * 12 + writer.stream_position()?) as u32;
        for entry in &self.properties {
            let key_buf = {
                let mut buf = entry.key.0.clone();
                if buf.last() != Some(&0) {
                    buf.push(0);
                }
                while buf.len() % 4 != 0 {
                    buf.push(0xFD);
                }
                buf
            };

            // key_ptr
            writer.write_le(&data_ptr)?;
            data_ptr += key_buf.len() as u32;
            data_bytes.extend(key_buf);

            // value_ptr
            writer.write_le(&data_ptr)?;
            data_ptr += entry.value.len() as u32;
            data_bytes.extend_from_slice(&entry.value);

            // value_size
            writer.write_le(&(entry.value.len() as u32))?;
        }

        writer.write_le(&data_bytes)?;

        Ok(())
    }
}

#[binrw::binread]
#[br(little)]
#[derive(Clone, Debug, Default)]
pub struct ModelProperty {
    #[br(parse_with = binrw::FilePtr32::parse)]
    pub key: binrw::NullString,
    #[br(temp)]
    value_ptr: u32,
    #[br(temp)]
    value_size: u32,
    #[br(restore_position, count = value_size, seek_before = SeekFrom::Start(value_ptr as u64))]
    pub value: Vec<u8>,
}

#[cfg(test)]
#[path = "./model_tests.rs"]
mod tests;
