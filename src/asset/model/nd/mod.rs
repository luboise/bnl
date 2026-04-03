mod push_buffer;
mod shader;
mod vertex_buffer;

use binrw::binrw;
pub use push_buffer::{DrawCall, NdPushBufferData};
pub use vertex_buffer::*;

pub(crate) mod prelude {
    // External
    pub use gltf_writer::gltf::{self, GltfIndex};
    pub use serde::{Serialize, ser::SerializeMap};

    // Internal
    pub use super::ModelSlice;
    pub use crate::asset::AssetParseError;
    pub use crate::asset::model::gltf::NdGltfContext;
    pub use crate::asset::model::nd::NdNode;

    pub use super::NdError;

    pub(crate) use crate::VirtualResource;
    pub(crate) use byteorder::{LittleEndian, ReadBytesExt};
    pub(crate) use std::io::{Cursor, Read, Seek, SeekFrom};
}

use std::{
    collections::HashMap,
    io::{self},
    iter::{self},
};

use serde::{Serialize, ser::SerializeMap};

use crate::asset::model::nd::shader::NdShaderParam2Payload;

use prelude::*;

#[derive(Debug)]
pub enum NdError {
    UnknownType,
    CreationFailure(String),
}

impl From<io::Error> for NdError {
    fn from(e: io::Error) -> Self {
        Self::CreationFailure(e.to_string())
    }
}

pub trait NdNode {
    fn add_gltf_node(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError>;
}

/*
impl From<NdType> for String {
    fn from(value: NdType) -> Self {
        match value {
            NdType::VertexBuffer => "ndVertexBuffer",
            NdType::PushBuffer => "ndPushBuffer",
            NdType::BGPushBuffer => "ndBGPushBuffer",
            NdType::Shader2 => "ndShader2",
            NdType::VertexShader => "ndVertexShader",
            NdType::ShaderParam2 => "ndShaderParam2",
            NdType::Group => "ndGroup",
            NdType::Skeleton => "ndSkeleton",
        }
        .to_string()
    }
}
*/

impl Serialize for Nd {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(None)?;

        map.serialize_entry("type", &self.nd_type().to_string())?;

        let children: Vec<&Nd> = self.children().collect();
        map.serialize_entry("children", &children)?;

        map.end()
    }
}

impl Nd {
    pub fn new(ctx: &mut ModelReadContext, model_slice: ModelSlice) -> Result<Self, NdError> {
        let slice = model_slice.slice();
        Nd::from_bytes(ctx, slice, model_slice.read_start as u32)
    }

    pub fn from_bytes(
        ctx: &mut ModelReadContext,
        bytes: &[u8],
        nd_start_offset: u32,
    ) -> Result<Nd, NdError> {
        let mut cur = Cursor::new(bytes);

        cur.seek(SeekFrom::Start(nd_start_offset as u64))?;

        let name_ptr = cur.read_u32::<LittleEndian>()?;

        // TODO: Sanity check name against name ptr
        let (type_u16, unknown_u16) = (
            cur.read_u16::<LittleEndian>()?,
            cur.read_u16::<LittleEndian>()?,
        );

        let (
            unknown_ptr1,
            unknown_ptr2,
            unknown_u32,
            first_child_ptr,
            next_sibling_ptr,
            parent_ptr,
        ) = (
            cur.read_u32::<LittleEndian>()?,
            cur.read_u32::<LittleEndian>()?,
            cur.read_u32::<LittleEndian>()?,
            cur.read_u32::<LittleEndian>()?,
            cur.read_u32::<LittleEndian>()?,
            cur.read_u32::<LittleEndian>()?,
        );

        // Processing
        let mut name_cur = cur.clone();
        name_cur.seek(SeekFrom::Start(name_ptr as u64))?;

        let mut chars = vec![];

        let mut c = name_cur.read_u8()?;

        while c != 0 {
            chars.push(c);
            c = name_cur.read_u8()?;
        }

        let name = String::from_utf8(chars).map_err(|e| {
            NdError::CreationFailure(format!("Failed to parse nd string name\n{}", e))
        })?;

        let nd_type: NdType = name.parse().unwrap_or(NdType::Other(0));

        let first_child = match first_child_ptr {
            0 => None,
            _ => Some(
                Nd::new(
                    ctx,
                    ModelSlice {
                        slice: bytes,
                        read_start: first_child_ptr as usize,
                    },
                )?
                .into(),
            ),
        };

        let next_sibling = match next_sibling_ptr {
            0 => None,
            _ => Some(
                Nd::new(
                    ctx,
                    ModelSlice {
                        slice: bytes,
                        read_start: next_sibling_ptr as usize,
                    },
                )?
                .into(),
            ),
        };

        let data: Result<NdData, NdError> = match nd_type {
            NdType::VertexBuffer => {
                let resource_views_ptr = cur.read_u32::<LittleEndian>()?;
                let num_resource_views = cur.read_u32::<LittleEndian>()?;

                let mut resource_views = Vec::with_capacity(num_resource_views as usize);

                for _ in 0..num_resource_views {
                    resource_views.push(res_view::VertexBufferResourceView::from_cursor(&mut cur)?);
                }

                Ok(NdData::VertexBuffer {
                    resource_views_ptr,
                    num_resource_views,
                    resource_views,
                })
            }
            NdType::PushBuffer | NdType::BGPushBuffer => {
                let push_buffer = {
                    let num_draws = cur.read_u32::<LittleEndian>()?;
                    let unknown_u32_1 = cur.read_u32::<LittleEndian>()?;
                    let unknown_u32_2 = cur.read_u32::<LittleEndian>()?;
                    let unknown_u32_3 = cur.read_u32::<LittleEndian>()?;

                    let data_pointers_start = cur.read_u32::<LittleEndian>()?;
                    let primitive_types_list_ptr = cur.read_u32::<LittleEndian>()?;
                    let vertex_counts_list_ptr = cur.read_u32::<LittleEndian>()?;

                    let prevent_culling_flag = cur.read_u8()?;
                    let mut padding = [0u8; 3];

                    cur.read_exact(&mut padding)?;

                    let mut data_ptr_cur = cur.clone();
                    data_ptr_cur.seek(SeekFrom::Start(data_pointers_start as u64))?;

                    let mut prim_type_ptr = cur.clone();
                    prim_type_ptr.seek(SeekFrom::Start(primitive_types_list_ptr as u64))?;

                    let mut vertex_counts_ptr = cur.clone();
                    vertex_counts_ptr.seek(SeekFrom::Start(vertex_counts_list_ptr as u64))?;

                    let mut draw_calls = Vec::with_capacity(num_draws as usize);

                    // TODO: FIGURE OUT IF THIS GOES HERE
                    let mut min = u32::MAX;
                    let mut max = u32::MIN;

                    for _ in 0..num_draws as usize {
                        let data_ptr = data_ptr_cur.read_u32::<LittleEndian>()?;
                        let prim_type = prim_type_ptr.read_u32::<LittleEndian>()?.into();
                        let num_vertices = vertex_counts_ptr.read_u32::<LittleEndian>()?;
                        let data_size = num_vertices * size_of::<u16>() as u32;

                        if data_ptr < min {
                            min = data_ptr;
                        }
                        if data_ptr + data_size > max {
                            max = data_ptr + data_size;
                        }

                        draw_calls.push(DrawCall {
                            data_ptr,
                            prim_type,
                            num_vertices,
                        });
                    }

                    let push_buffer_base = min;
                    let push_buffer_size = (max - min) as usize;

                    let buffer_bytes = bytes
                        [push_buffer_base as usize..push_buffer_base as usize + push_buffer_size]
                        .to_vec();

                    NdPushBufferData {
                        num_draws,
                        unknown_u32_1,
                        unknown_u32_2,
                        unknown_u32_3,
                        //
                        data_pointers_start,
                        primitive_types_list_ptr,
                        vertex_counts_list_ptr,
                        //
                        prevent_culling_flag,
                        padding,
                        //
                        buffer_bytes,
                        push_buffer_base,
                        push_buffer_size: push_buffer_size as u32,

                        draw_calls,
                    }
                };

                if nd_type == NdType::BGPushBuffer {
                    let unknown_ptr_1 = cur.read_u32::<LittleEndian>()?;
                    let unknown_ptr_2 = cur.read_u32::<LittleEndian>()?;

                    Ok(NdData::BGPushBuffer {
                        push_buffer,
                        unknown_ptr_1,
                        unknown_ptr_2,
                    })
                } else {
                    Ok(NdData::PushBuffer(push_buffer))
                }
            }
            NdType::Group => {
                // NdGroup spotted
                Ok(NdData::Group)
            }
            NdType::ShaderParam2 => {
                let main_payload_ptr = cur.read_u32::<LittleEndian>()?;
                let sub_payload_ptr = cur.read_u32::<LittleEndian>()?;

                let main_payload = NdShaderParam2Payload::from_model_slice(&ModelSlice {
                    slice: bytes,
                    read_start: main_payload_ptr as usize,
                })?;

                let sub_payload = match sub_payload_ptr {
                    0 => None,
                    val => Some(NdShaderParam2Payload::from_model_slice(&ModelSlice {
                        slice: bytes,
                        read_start: val as usize,
                    })?),
                };

                Ok(NdData::ShaderParam2 {
                    main_payload,
                    sub_payload,
                })
            }
            NdType::Skeleton => {
                let num_bones = cur.read_u32::<LittleEndian>()?;
                let bones_ptr = cur.read_u32::<LittleEndian>()?;

                let bones = if bones_ptr != 0 && num_bones > 0 {
                    let mut bones = Vec::with_capacity(num_bones as usize);

                    cur.seek(SeekFrom::Start(bones_ptr as u64))?;

                    for i in 0..num_bones {
                        bones.push(Bone {
                            name: ctx.get_bone_name(i).map(|v| v.into()),
                            parent_id: cur.read_u16::<LittleEndian>()?,
                            id: cur.read_u16::<LittleEndian>()?,
                            local_transform: [
                                cur.read_f32::<LittleEndian>()?,
                                cur.read_f32::<LittleEndian>()?,
                                cur.read_f32::<LittleEndian>()?,
                            ],
                            global_transform: [
                                cur.read_f32::<LittleEndian>()?,
                                cur.read_f32::<LittleEndian>()?,
                                cur.read_f32::<LittleEndian>()?,
                            ],
                            sentinel: cur.read_u32::<LittleEndian>()?.to_le_bytes(),
                        });
                    }

                    bones
                } else {
                    vec![]
                };

                Ok(NdData::Skeleton { bones })
            }
            NdType::Shader2 => Ok(NdData::Shader2),
            NdType::VertexShader => Ok(NdData::VertexShader),
            NdType::RigidSkinIdx | NdType::MtxArray | NdType::BlendShape | NdType::Other(_) => Ok(
                NdData::Unknown(nd_type, nd_type.to_string(), Vec::default()),
            ),
        };

        /*
        let data = match nd_type {
            NdType::Group => {}
            NdType::Skeleton => todo!(),
            NdType::RigidSkinIdx => todo!(),
            NdType::MtxArray => todo!(),
            NdType::Shader2 => todo!(),
            NdType::ShaderParam2 => todo!(),
            NdType::VertexBuffer => todo!(),
            NdType::PushBuffer => todo!(),
            NdType::VertexShader => todo!(),
            NdType::BGPushBuffer => todo!(),
            NdType::BlendShape => todo!(),
            NdType::Other(_) => todo!(),
        };
        */

        Ok(Self {
            unknown_u16,
            unknown_ptr1,
            unknown_ptr2,
            unknown_u32,
            first_child_ptr,
            next_sibling_ptr,
            parent_ptr,
            first_child,
            next_sibling,
            data: Box::new(data?),
        })
    }

    pub fn children(&self) -> impl Iterator<Item = &Nd> {
        iter::successors(self.first_child(), |nd| nd.next_sibling())
    }

    pub fn first_child(&self) -> Option<&Nd> {
        self.first_child.as_deref()
    }

    pub fn next_sibling(&self) -> Option<&Nd> {
        self.next_sibling.as_deref()
    }

    #[inline]
    pub fn nd_type(&self) -> NdType {
        self.data.nd_type()
    }

    pub fn heirarchy(&self) -> impl Iterator<Item = &Nd> {
        NdIterator::new(self)
    }
}

#[binrw]
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, strum::EnumString, strum::Display)]
pub enum NdType {
    #[strum(serialize = "ndGroup")]
    Group = 0x01,
    #[strum(serialize = "ndSkeleton")]
    Skeleton = 0x02,
    #[strum(serialize = "ndRigidSkinIdx")]
    RigidSkinIdx = 0x0b,
    #[strum(serialize = "ndMtxArray")]
    MtxArray = 0x0c,
    #[strum(serialize = "ndShader2")]
    Shader2 = 0x11,
    #[strum(serialize = "ndShaderParam2")]
    ShaderParam2 = 0x12,
    #[strum(serialize = "ndVertexBuffer")]
    VertexBuffer = 0x13,
    #[strum(serialize = "ndPushBuffer")]
    PushBuffer = 0x14,
    #[strum(serialize = "ndVertexShader")]
    VertexShader = 0x15,
    #[strum(serialize = "ndBGPushBuffer")]
    BGPushBuffer = 0x16,
    #[strum(serialize = "ndBlendShape")]
    BlendShape = 0x17,
    #[strum(serialize = "ndUnknown")]
    Other(u32),
}

#[derive(Debug, Clone, Serialize)]
pub enum NdData {
    Skeleton {
        bones: Vec<Bone>,
    },
    VertexBuffer {
        resource_views_ptr: u32,
        num_resource_views: u32,

        #[serde(skip)]
        resource_views: Vec<res_view::VertexBufferResourceView>,
    },
    PushBuffer(NdPushBufferData),
    BGPushBuffer {
        push_buffer: NdPushBufferData,
        unknown_ptr_1: u32,
        unknown_ptr_2: u32,
    },
    Group,
    Shader2,
    VertexShader,
    ShaderParam2 {
        main_payload: NdShaderParam2Payload,
        sub_payload: Option<NdShaderParam2Payload>,
    },
    Unknown(NdType, String, Vec<u8>),
}

impl NdData {
    pub fn nd_type(&self) -> NdType {
        match self {
            NdData::Skeleton { .. } => NdType::Skeleton,
            NdData::VertexBuffer { .. } => NdType::VertexBuffer,
            NdData::PushBuffer(_) => NdType::PushBuffer,
            NdData::BGPushBuffer { .. } => NdType::BGPushBuffer,
            NdData::Group => NdType::Group,
            NdData::Shader2 => NdType::Shader2,
            NdData::VertexShader => NdType::VertexShader,
            NdData::ShaderParam2 { .. } => NdType::ShaderParam2,
            NdData::Unknown(nd_type, ..) => *nd_type,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Nd {
    // pub name_ptr: NullString,
    // pub nd_type: NdType,
    pub unknown_u16: u16, // Possibly index
    pub unknown_ptr1: u32,
    pub unknown_ptr2: u32,
    pub unknown_u32: u32,
    pub first_child_ptr: u32,
    pub next_sibling_ptr: u32,
    pub parent_ptr: u32,

    // DO NOT SERIALISE
    pub first_child: Option<Box<Self>>,
    pub next_sibling: Option<Box<Self>>,

    pub data: Box<NdData>,
}

struct NdIterator<'a> {
    base_nd: &'a Nd,
    current_nd: Option<&'a Nd>,
}

impl<'a> NdIterator<'a> {
    pub fn new(nd: &'a Nd) -> Self {
        Self {
            base_nd: nd,
            current_nd: Some(nd),
        }
    }
}

impl<'a> Iterator for NdIterator<'a> {
    type Item = &'a Nd;

    fn next(&mut self) -> Option<Self::Item> {
        // If sibling
        if let Some(x) = self.current_nd.and_then(|nd| nd.next_sibling.as_deref()) {
            self.current_nd = Some(x);
            return Some(x);
        }

        // If child
        if let Some(child) = self.base_nd.first_child() {
            self.current_nd = Some(child);
            self.base_nd = child;

            return Some(child);
        }

        unreachable!()
    }
}

/*
impl Serialize for Nd {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.header().serialize(serializer)
    }
}
*/

pub struct ModelReadContext<'a> {
    key_value_map: &'a HashMap<String, Vec<u8>>,
}

impl<'a> ModelReadContext<'a> {
    pub fn new(key_value_map: &'a HashMap<String, Vec<u8>>) -> Self {
        Self { key_value_map }
    }

    pub fn get_bone_name(&self, bone_index: u32) -> Option<&str> {
        self.key_value_map.iter().find_map(|(k, v)| {
            (is_bone_name(k)
                && v.len() == 4
                && u32::from_le_bytes(v.as_slice().try_into().unwrap()) == bone_index)
                .then_some(k.as_str())
        })
    }
}

pub fn is_bone_name<S: AsRef<str>>(s: S) -> bool {
    ["BASE", "MID", "joint3"].contains(&s.as_ref())
}

pub struct ModelSlice<'a> {
    pub(crate) slice: &'a [u8],
    pub(crate) read_start: usize,
}

impl<'a> ModelSlice<'a> {
    pub fn slice(&self) -> &'a [u8] {
        self.slice
    }

    pub fn nd_start(&self) -> usize {
        self.read_start
    }

    pub fn at(&self, read_start: usize) -> Self {
        ModelSlice {
            slice: self.slice,
            read_start,
        }
    }

    pub fn new_cursor(&self) -> Cursor<&[u8]> {
        let mut cur = Cursor::new(self.slice);
        cur.seek(SeekFrom::Start(self.read_start as u64)).unwrap();

        cur
    }
}

/*
impl NdNode for NdGroup {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        _virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        Ok(Some(
            ctx.gltf
                .add_node(gltf::Node::new(Some("ndGroup".to_string()))),
        ))
    }
}
*/

#[derive(Debug, Clone, Serialize)]
pub struct Bone {
    pub name: Option<String>,
    pub parent_id: u16,
    pub id: u16,
    pub local_transform: [f32; 3],
    pub global_transform: [f32; 3],
    pub sentinel: [u8; 4],
}

#[path = "./tests.rs"]
#[cfg(test)]
mod tests;
