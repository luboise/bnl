mod push_buffer;
mod shader;
mod shader_param_2;
mod vertex_buffer;

use binrw::{BinReaderExt, BinWriterExt, binrw};
pub use push_buffer::{DrawCall, NdPushBufferData};
pub use vertex_buffer::*;

pub(crate) mod prelude {
    pub(crate) use byteorder::{LittleEndian, ReadBytesExt};
}

use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Seek, SeekFrom},
};

use serde::{Serialize, ser::SerializeMap};

use crate::asset::model::nd::shader_param_2::NdShaderParam2Data;

use {
    push_buffer::NdBGPushBufferData,
    shader::{NdShader2Data, NdVertexShaderData},
};

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
pub struct NdHeader {}

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

            String::from_utf8(chars).map_err(br_error(reader))?
        };

        let nd_type: NdType = name.parse().map_err(br_error(reader))?;

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
            NdType::VertexBuffer => Ok(NdData::VertexBuffer(reader.read_le()?)),
            NdType::PushBuffer => Ok(NdData::PushBuffer(reader.read_le()?)),
            NdType::BGPushBuffer => Ok(NdData::BGPushBuffer(reader.read_le()?)),
            NdType::Group => Ok(NdData::Group),
            NdType::ShaderParam2 => Ok(NdData::ShaderParam2(reader.read_le()?)),
            NdType::Skeleton => Ok(NdData::Skeleton(reader.read_le()?)),
            NdType::Shader2 => Ok(NdData::Shader2(reader.read_le()?)),
            NdType::VertexShader => Ok(NdData::VertexShader(reader.read_le()?)),
            NdType::MtxArray => Ok(NdData::MtxArray(reader.read_le()?)),
            NdType::RigidSkinIdx | NdType::BlendShape => Ok(
                todo!(), // NdData::Unknown(nd_type, nd_type.to_string(), Vec::default()),
            ),
        };

        let data = data.map_err(|e| binrw::Error::Custom {
            pos: reader.stream_position().unwrap_or(0),
            err: Box::new(format!("failed to get data for Nd: {e}")),
        })?;

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
            data: Box::new(data),
        })
    }
}

pub(crate) fn br_error<E: std::error::Error>(
    seeker: &mut impl std::io::Seek,
) -> impl FnMut(E) -> binrw::Error {
    |e: E| binrw::Error::Custom {
        pos: seeker.stream_position().unwrap_or(0),
        err: Box::new(format!("failed to get data for Nd: {e}")),
    }
}

pub(crate) fn br_get_stream_pos(
    seeker: &mut impl std::io::Seek,
    struct_offset: u32,
) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
    seeker
        .stream_position()
        .ok()
        .and_then(|v| u32::try_from(v).ok())
        .map(|v| v - struct_offset)
        .ok_or("bad conversion".into())
}

impl binrw::BinWrite for Nd {
    type Args<'a> = ();

    fn write_options<W: std::io::prelude::Write + Seek>(
        &self,
        writer: &mut W,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        let base = writer.stream_position()?;

        let Nd {
            unknown_u16,
            unknown_ptr1,
            unknown_ptr2,
            unknown_u32,
            first_child_ptr,
            next_sibling_ptr,
            parent_ptr,
            first_child,
            next_sibling,
            data,
        } = &self;

        let name_offset =
            base + 0x20 + u64::try_from(data.name_offset()).map_err(br_error(writer))?;
        let val = u32::try_from(name_offset).map_err(br_error(writer))?;

        writer.write_le(&val)?;
        writer.write_le(&self.nd_type())?;
        writer.write_le(&unknown_u16)?;
        writer.write_le(&unknown_ptr1)?;
        writer.write_le(&unknown_ptr2)?;
        writer.write_le(&unknown_u32)?;
        // first child, sibling, prev
        writer.write_le(&0u32)?;
        writer.write_le(&0u32)?;
        writer.write_le(&0u32)?;
        writer.write_le(data)?;

        Ok(())
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
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, strum::EnumString, strum::Display,
)]
#[repr(u16)]
#[brw(little, repr = u16)]
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
}

#[derive(Debug, Clone)]
#[binrw::binrw]
pub enum NdData {
    Skeleton(NdSkeletonData),
    VertexBuffer(NdVertexBufferData),
    PushBuffer(NdPushBufferData),
    BGPushBuffer(NdBGPushBufferData),
    Group,
    Shader2(NdShader2Data),
    VertexShader(NdVertexShaderData),
    ShaderParam2(NdShaderParam2Data),
    MtxArray(NdMtxArrayData),
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
            NdData::Shader2(_) => NdType::Shader2,
            NdData::VertexShader(_) => NdType::VertexShader,
            NdData::ShaderParam2(_) => NdType::ShaderParam2,
            NdData::MtxArray(_) => NdType::MtxArray,
            NdData::Unknown(nd_type, ..) => *nd_type,
        }
    }

    pub fn name_offset(&self) -> i64 {
        match self {
            NdData::Skeleton { .. } => 0x8,
            NdData::VertexBuffer(nd_vertex_buffer_data) => {
                8 + nd_vertex_buffer_data.resource_views.len() as i64 * 0x18
            }
            NdData::PushBuffer(..) => 0x20,
            NdData::BGPushBuffer(..) => todo!(),
            NdData::Group => todo!(),
            NdData::Shader2(data) => data.name_offset(),
            NdData::VertexShader(..) => 0x48,
            NdData::ShaderParam2(data) => data.name_offset(),
            NdData::Unknown(..) => todo!(),
            NdData::MtxArray(data) => data.name_offset(),
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

#[binrw::binrw]
#[bw(stream = w)]
#[derive(Debug, Clone)]
pub struct NdSkeletonData {
    #[br(temp)]
    #[bw(try_calc = bones.len().try_into())]
    num_bones: u32,
    #[br(temp)]
    #[bw(try_calc = u32::try_from(w.stream_position()?)
        .map(|v| v + (0x8 + 0xc - 4)))]
    bones_ptr: u32,
    #[brw(magic = b"ndSkeleton\x00\x00")]
    _name: (),
    #[br(count = num_bones)]
    bones: Vec<Bone>,
}

#[derive(Debug, Clone, Serialize)]
#[binrw::binrw]
#[expect(clippy::manual_non_exhaustive)]
pub struct Bone {
    pub parent_id: u16,
    pub id: u16,
    pub local_transform: [f32; 3],
    pub global_transform: [f32; 3],
    #[brw(magic = b"\xff\xff\x01\xcd")]
    _sentinel: (),
}

#[binrw::binrw]
#[derive(Debug, Clone)]
struct NdMtxArrayEntry {
    index: u16,
    idk1: u16,
    idk2: u8,
    flags: u8,
    idk3: u16,
}

#[binrw::binrw]
#[derive(Debug, Clone)]
#[br(stream = r)]
#[bw(stream = w)]
pub struct NdMtxArrayData {
    #[br(temp)]
    #[bw(try_calc =
        w.stream_position().ok()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or("failed to convert").map(|v| v + 0x14))]
    entries_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = entries.len().try_into())]
    num_entries: u32,
    count2: u16,
    calc1: u16, // calculated at runtime
    some_float: f32,
    some_u32: u32,

    // Fake fields
    #[br(if(num_entries > 0 && entries_ptr > 0),
        count = num_entries,
        seek_before = SeekFrom::Start(entries_ptr.into())
        )]
    entries: Vec<NdMtxArrayEntry>,
    #[brw(magic = b"ndMtxArray\x00\x00")]
    _magic: (),
}

impl NdMtxArrayData {
    pub fn name_offset(&self) -> i64 {
        let v = 0x14 + self.entries.len() * 8;
        v as i64
    }
}

#[path = "./nd_tests.rs"]
#[cfg(test)]
mod tests;
