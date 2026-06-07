mod push_buffer;
mod shader;
mod vertex_buffer;

use binrw::{BinReaderExt, binrw};
pub use push_buffer::{DrawCall, NdPushBufferData};
pub use vertex_buffer::*;

pub(crate) mod prelude {
    pub use serde::{Serialize, ser::SerializeMap};

    // Internal
    pub use super::ModelSlice;

    pub(crate) use byteorder::{LittleEndian, ReadBytesExt};
}

use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Seek, SeekFrom},
};

use serde::{Serialize, ser::SerializeMap};

use crate::asset::model::nd::{res_view::VertexBufferResourceView, shader::NdShaderParam2Payload};

use prelude::*;

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

impl binrw::BinRead for Nd {
    type Args<'a> = &'a ModelReadContext<'a>;

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        ctx: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        let name_ptr = reader.read_u32::<LittleEndian>()?;

        // TODO: Sanity check name against name ptr
        let (_type_u16, unknown_u16) = (
            reader.read_u16::<LittleEndian>()?,
            reader.read_u16::<LittleEndian>()?,
        );

        let (
            unknown_ptr1,
            unknown_ptr2,
            unknown_u32,
            first_child_ptr,
            next_sibling_ptr,
            parent_ptr,
        ) = (
            reader.read_u32::<LittleEndian>()?,
            reader.read_u32::<LittleEndian>()?,
            reader.read_u32::<LittleEndian>()?,
            reader.read_u32::<LittleEndian>()?,
            reader.read_u32::<LittleEndian>()?,
            reader.read_u32::<LittleEndian>()?,
        );

        let name = {
            let old_pos = reader.stream_position()?;

            // Processing
            reader.seek(SeekFrom::Start(name_ptr as u64))?;

            let mut chars = vec![];

            let mut c = reader.read_u8()?;

            while c != 0 {
                chars.push(c);
                c = reader.read_u8()?;
            }

            reader.seek(SeekFrom::Start(old_pos))?;

            String::from_utf8(chars).map_err(|e| binrw::Error::Custom {
                pos: reader.stream_position().unwrap_or(0),
                err: Box::new(e),
            })?
        };

        let nd_type: NdType = name.parse().unwrap_or(NdType::Other(0));

        let first_child = match first_child_ptr {
            0 => None,
            _ => {
                let pos = reader.stream_position()?;
                reader.seek(SeekFrom::Start(first_child_ptr.into()))?;
                let child = Some(reader.read_le_args(ctx)?);
                reader.seek(SeekFrom::Start(pos))?;
                child
            }
        };

        let next_sibling = match next_sibling_ptr {
            0 => None,
            _ => {
                let pos = reader.stream_position()?;
                reader.seek(SeekFrom::Start(next_sibling_ptr.into()))?;
                let sibling = Some(reader.read_le_args(ctx)?);
                reader.seek(SeekFrom::Start(pos))?;
                sibling
            }
        };

        let data: Result<NdData, crate::Error> = match nd_type {
            NdType::VertexBuffer => {
                let resource_views_ptr = reader.read_u32::<LittleEndian>()?;
                let num_resource_views = reader.read_u32::<LittleEndian>()?;

                let mut resource_views = Vec::with_capacity(num_resource_views as usize);

                for _ in 0..num_resource_views {
                    resource_views.push(VertexBufferResourceView::from_reader(&mut reader)?);
                }

                Ok(NdData::VertexBuffer {
                    resource_views_ptr,
                    num_resource_views,
                    resource_views,
                })
            }
            NdType::PushBuffer | NdType::BGPushBuffer => {
                let push_buffer = reader.read_le()?;

                if nd_type == NdType::BGPushBuffer {
                    let unknown_ptr_1 = reader.read_u32::<LittleEndian>()?;
                    let unknown_ptr_2 = reader.read_u32::<LittleEndian>()?;

                    Ok(NdData::BGPushBuffer {
                        push_buffer: reader.read_le()?,
                        unknown_ptr_1: reader.read_le()?,
                        unknown_ptr_2: reader.read_le()?,
                        floats: reader.read_le()?,
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
                let main_payload_ptr = reader.read_u32::<LittleEndian>()?;
                let sub_payload_ptr = reader.read_u32::<LittleEndian>()?;

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
                let num_bones = reader.read_u32::<LittleEndian>()?;
                let bones_ptr = reader.read_u32::<LittleEndian>()?;

                let bones = if bones_ptr != 0 && num_bones > 0 {
                    let mut bones = Vec::with_capacity(num_bones as usize);

                    reader.seek(SeekFrom::Start(bones_ptr as u64))?;

                    for i in 0..num_bones {
                        bones.push(Bone {
                            name: ctx.get_bone_name(i).map(|v| v.into()),
                            parent_id: reader.read_u16::<LittleEndian>()?,
                            id: reader.read_u16::<LittleEndian>()?,
                            local_transform: [
                                reader.read_f32::<LittleEndian>()?,
                                reader.read_f32::<LittleEndian>()?,
                                reader.read_f32::<LittleEndian>()?,
                            ],
                            global_transform: [
                                reader.read_f32::<LittleEndian>()?,
                                reader.read_f32::<LittleEndian>()?,
                                reader.read_f32::<LittleEndian>()?,
                            ],
                            sentinel: reader.read_u32::<LittleEndian>()?.to_le_bytes(),
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
            data: Box::new(data.map_err(|e| binrw::Error::Custom {
                pos: reader.stream_position().unwrap_or_default(),
                err: Box::new(format!("failed to get data for Nd: {e}")),
            })?),
        })
    }
}

impl Nd {
    pub fn children(&self) -> impl Iterator<Item = &Nd> {
        std::iter::successors(self.first_child(), |nd| nd.next_sibling())
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
#[binrw::binread]
// TODO: binrw::binwrite
pub enum NdData {
    Skeleton {
        #[br(parse_with = binrw::helpers::until_eof)]
        bones: Vec<Bone>,
    },
    VertexBuffer {
        resource_views_ptr: u32,
        num_resource_views: u32,

        #[serde(skip)]
        #[br(count = num_resource_views,
            seek_before = SeekFrom::Start(resource_views_ptr.into()),
            restore_position
        )]
        resource_views: Vec<VertexBufferResourceView>,
    },
    PushBuffer(NdPushBufferData),
    BGPushBuffer {
        push_buffer: NdPushBufferData,
        unknown_ptr_1: u32,
        unknown_ptr_2: u32,
        floats: [f32; 6],
    },
    Group,
    Shader2,
    VertexShader,
    ShaderParam2 {
        main_payload: NdShaderParam2Payload,
        sub_payload: Option<NdShaderParam2Payload>,
    },
    Unknown(
        NdType,
        #[br(parse_with = binrw::helpers::until_eof)] Vec<u8>,
    ),
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

struct NdIterator<'a> {
    stack: VecDeque<&'a Nd>,
}

fn add_to_stack<'a>(node: &'a Nd, stack: &mut VecDeque<&'a Nd>) {
    stack.push_back(node);

    if let Some(child) = &node.first_child {
        add_to_stack(child, stack);
    }

    if let Some(sibling) = &node.next_sibling {
        add_to_stack(sibling, stack);
    }
}

impl<'a> NdIterator<'a> {
    pub fn new(nd: &'a Nd) -> Self {
        let stack = {
            let mut stack = VecDeque::new();
            add_to_stack(nd, &mut stack);
            stack
        };

        Self { stack }
    }
}

impl<'a> Iterator for NdIterator<'a> {
    type Item = &'a Nd;

    fn next(&mut self) -> Option<Self::Item> {
        self.stack.pop_front()
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
    resource: &'a [u8],
}

impl<'a> ModelReadContext<'a> {
    pub fn new(key_value_map: &'a HashMap<String, Vec<u8>>, resource: &'a [u8]) -> Self {
        Self {
            key_value_map,
            resource,
        }
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

    pub fn new_cursor(&self) -> std::io::Cursor<&[u8]> {
        let mut cur = std::io::Cursor::new(self.slice);
        cur.seek(SeekFrom::Start(self.read_start as u64)).unwrap();

        cur
    }
}

#[derive(Debug, Clone, Serialize)]
#[binrw::binrw]
pub struct Bone {
    pub parent_id: u16,
    pub id: u16,
    pub local_transform: [f32; 3],
    pub global_transform: [f32; 3],
    pub sentinel: [u8; 4],
}

#[path = "./nd_tests.rs"]
#[cfg(test)]
mod tests;
