mod push_buffer;
mod shader;
mod shader_param_2;
mod vertex_buffer;

use binrw::{BinReaderExt, BinWriterExt};
pub use push_buffer::{DrawCall, NdPushBufferData};
pub use vertex_buffer::*;

pub(crate) mod prelude {
    pub(crate) use byteorder::{LittleEndian, ReadBytesExt};
}

use std::{
    collections::{VecDeque},
    io::{Seek, SeekFrom},
};

use serde::{Serialize, ser::SerializeMap};

pub use crate::asset::model::nd::shader_param_2::NdShaderParam2Data;

pub use {
    push_buffer::NdBGPushBufferData,
    shader::{NdShader2Data, NdVertexShaderData},
};

impl Serialize for Nd {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(None)?;

        if let Some(name) = &self.name {
            map.serialize_entry("name", name)?;
        }

        map.serialize_entry("type", &self.nd_type().to_string())?;
        map.serialize_entry("data", self.data.as_ref())?;

        let children: Vec<&Nd> = self.children().collect();
        map.serialize_entry("children", &children)?;

        map.end()
    }
}

fn serialize_nullstring<S: serde::Serializer>(str: &binrw::NullString, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(str::from_utf8(&str.0).unwrap_or("failed to parse nullstring"))
}

fn serialize_vec_len<T, S: serde::Serializer>(v: &[T], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_u32(v.len() as u32)
}

#[binrw::binread]
#[br(stream = r, import(mrc: ModelReadContext<'_>))]
#[brw(little)]
#[derive(Debug, Clone)]
pub struct Nd {
    #[br(temp, try_calc = {r.stream_position()?.try_into().map_err(br_error(r))})]
    _base: u32,
    #[br(calc = mrc.properties.iter()
            .find_map(|(key, value)| {
                if value.len() != 4 {
                    return None;
                } 

                let value = u32::from_le_bytes(value.as_slice().try_into().unwrap());
                if value != _base {
                    return None;
                }

                Some(key.clone())
    }))]
    #[bw(ignore)]
    pub name: Option<String>,
    #[br(temp, assert(nd_type_str_ptr != 0))]
    nd_type_str_ptr: u32,
    #[br(restore_position, seek_before = SeekFrom::Start(nd_type_str_ptr.into()))]
    pub nd_type_str: binrw::NullString,
    pub nd_type: NdType,
    pub unknown_u16: u16, // Possibly index
    pub unknown_ptr1: u32,
    pub unknown_ptr2: u32,
    pub unknown_u32: u32,
    #[br(temp)]
    pub first_child_ptr: u32,
    #[br(temp)]
    pub next_sibling_ptr: u32,
    #[br(temp)]
    pub parent_ptr: u32,

    #[br(args(mrc, nd_type))]
    pub data: Box<NdData>,

    #[br(if(first_child_ptr != 0),
        seek_before = SeekFrom::Start(first_child_ptr.into()), 
        restore_position,
        args(mrc))]
    pub first_child: Option<Box<Self>>,

    #[br(if(next_sibling_ptr != 0),
        seek_before = SeekFrom::Start(next_sibling_ptr.into()), 
        restore_position,
        args(mrc),
        )]
    pub next_sibling: Option<Box<Self>>,
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


pub type ModelWriteContext = std::rc::Rc<std::cell::RefCell<ModelWriteContextInner>>;

pub fn new_write_context() -> ModelWriteContext {
    ModelWriteContext::new(ModelWriteContextInner {
        nd_heirarchy_ptrs: vec![],
        resource: vec![],
        rigid_entries: vec![],
        properties: Default::default()
    }.into())
}

#[derive(Clone, Debug)]
pub struct ModelWriteContextInner {
    pub nd_heirarchy_ptrs: Vec<u32>,
    pub resource: Vec<u8>,
    pub rigid_entries: Vec<(u64, Vec<u8>)>,
    pub properties: indexmap::IndexMap<String, Vec<u8>>
}

impl binrw::BinWrite for Nd {
    type Args<'a> = ModelWriteContext;

    fn write_options<W: std::io::prelude::Write + Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        mwc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        let base = writer.stream_position()?;

        let Nd {
            name,
            nd_type_str: _,
            nd_type,
            unknown_u16,
            unknown_ptr1,
            unknown_ptr2,
            unknown_u32,
            data,
            first_child,
            next_sibling,
        } = &self;


        // if has name, update the property to current offset
        if let Some(name) = name {
            let bytes_to_write = u32::try_from(writer.stream_position()?).unwrap().to_le_bytes().to_vec();

            let mut borrowed = mwc.borrow_mut();

            let entry = borrowed.properties.entry(name.clone()).or_insert(bytes_to_write.clone());
            *entry = bytes_to_write;
        }

        let name_ptr =
            base + 0x20 + u64::try_from(data.name_offset()).map_err(br_error(writer))?;
        let name_ptr = u32::try_from(name_ptr).map_err(br_error(writer))?;

        writer.write_le(&name_ptr)?;
        writer.write_le(nd_type)?;
        writer.write_le(unknown_u16)?;
        writer.write_le(unknown_ptr1)?;
        writer.write_le(unknown_ptr2)?;
        writer.write_le(unknown_u32)?;

        // first child, sibling, prev
        writer.write_le(&0u32)?;
        writer.write_le(&0u32)?;

        let parent = mwc.borrow().nd_heirarchy_ptrs.last().copied().unwrap_or(0);
        writer.write_le(&parent)?;

        writer.write_le_args(data, mwc.clone())?;

        if let Some(first_child) = first_child {
            mwc.borrow_mut().nd_heirarchy_ptrs.push(base as u32);
            // go write the pointer before writing the child
            let first_child_ptr = writer.stream_position()? as u32;
            seek_and_write(writer, SeekFrom::Start(base + 4 * 5), &first_child_ptr)?;
            first_child.write_le_args(writer, mwc.clone())?;
            mwc.borrow_mut().nd_heirarchy_ptrs.pop();
        }
        if let Some(next_sibling) = next_sibling {
            let next_sibling_ptr = writer.stream_position()? as u32;
            seek_and_write(writer, SeekFrom::Start(base + 4 * 6), &next_sibling_ptr)?;
            next_sibling.write_le_args(writer, mwc.clone())?;
        }

        Ok(())
    }
}

/// Seeks, writes, gets position of writer then restores
pub fn seek_and_write<W, T>(writer: &mut W, seek_from: SeekFrom, data: &T) -> Result<u64, binrw::error::Error> 
where
    W: std::io::Seek + std::io::Write,
    T: binrw::BinWrite, for<'a> <T as binrw::BinWrite>::Args<'a>: std::default::Default {
    let cur = writer.stream_position()?;

    writer.seek(seek_from)?;
    data.write_le(writer)?;
    let write_end = writer.stream_position()?;
    writer.seek(SeekFrom::Start(cur))?;

    Ok(write_end)
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

    pub fn heirarchy_mut(&mut self) -> impl Iterator<Item = &mut Nd> {
        NdIteratorMut::new(self)
    }
}

#[binrw::binrw]
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

#[derive(Debug, Clone, serde::Serialize)]
#[binrw::binread]
#[br(import(mrc: ModelReadContext<'_>, nd_type: NdType))]
#[bw(import(mwc: ModelWriteContext))]
pub enum NdData {
    #[br(pre_assert(nd_type == NdType::Skeleton))]
    Skeleton(NdSkeletonData),
    #[br(pre_assert(nd_type == NdType::VertexBuffer))]
    VertexBuffer(
        #[br(args(mrc))]
        NdVertexBufferData
    ),
    #[br(pre_assert(nd_type == NdType::PushBuffer))]
    PushBuffer(NdPushBufferData),
    #[br(pre_assert(nd_type == NdType::BGPushBuffer))]
    BGPushBuffer(NdBGPushBufferData),
    // #[br(pre_assert(nd_type == NdType::Group))]
    // Group,
    #[br(pre_assert(nd_type == NdType::Shader2))]
    Shader2(NdShader2Data),
    #[br(pre_assert(nd_type == NdType::VertexShader))]
    VertexShader(NdVertexShaderData),
    #[br(pre_assert(nd_type == NdType::ShaderParam2))]
    ShaderParam2(NdShaderParam2Data),
    #[br(pre_assert(nd_type == NdType::MtxArray))]
    MtxArray(NdMtxArrayData),
    #[br(pre_assert(nd_type == NdType::RigidSkinIdx))]
    RigidSkin(NdRigidSkinIdxData),
    #[br(pre_assert(nd_type == NdType::BlendShape))]
    BlendShape(
        #[br(args_raw(mrc))]
        NdBlendShapeData
     ),
}

impl binrw::BinWrite for NdData {
    type Args<'a> = ModelWriteContext;

    fn write_options<W: std::io::prelude::Write + Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        mwc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        match self {
            NdData::Skeleton(data) => data.write_le(writer)?,
            NdData::VertexBuffer(data) => data.write_le_args(writer, (mwc,))?,
            NdData::PushBuffer(data) => data.write_le(writer)?,
            NdData::BGPushBuffer(data) => data.write_le(writer)?,
            NdData::Shader2(data) => data.write_le(writer)?,
            NdData::VertexShader(data) => data.write_le(writer)?,
            NdData::ShaderParam2(data) => data.write_le(writer)?,
            NdData::MtxArray(data) => data.write_le(writer)?,
            NdData::RigidSkin(data) => data.write_le_args(writer, mwc)?,
            NdData::BlendShape(data) => data.write_le_args(writer, mwc)?,            // NdData::Group => data.write_le(writer)?,
        }

        Ok(())
    }
}

impl NdData {
    pub fn nd_type(&self) -> NdType {
        match self {
            NdData::Skeleton { .. } => NdType::Skeleton,
            NdData::VertexBuffer { .. } => NdType::VertexBuffer,
            NdData::PushBuffer(_) => NdType::PushBuffer,
            NdData::BGPushBuffer { .. } => NdType::BGPushBuffer,
            // NdData::Group => NdType::Group,
            NdData::Shader2(_) => NdType::Shader2,
            NdData::VertexShader(_) => NdType::VertexShader,
            NdData::ShaderParam2(_) => NdType::ShaderParam2,
            NdData::MtxArray(_) => NdType::MtxArray,
            NdData::RigidSkin(_) => NdType::RigidSkinIdx,
            NdData::BlendShape(_) => NdType::BlendShape
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
            // NdData::Group => todo!(),
            NdData::Shader2(data) => data.name_offset(),
            NdData::VertexShader(..) => 0x48,
            NdData::ShaderParam2(data) => data.name_offset(),
            NdData::MtxArray(data) => data.name_offset(),
            NdData::RigidSkin(..) => 8,
            NdData::BlendShape(data) => data.name_offset()
        }
    }
}

struct NdIterator<'a> {
    stack: VecDeque<&'a Nd>,
}

struct NdIteratorMut<'a> {
    stack: VecDeque<*mut Nd>,
    // needed to tie lifetime of ref to iterator
    _marker: std::marker::PhantomData<&'a mut Nd>
}

impl<'a> NdIterator<'a> {
    pub fn new(nd: &'a Nd) -> Self {
        let stack = {
            let mut stack = VecDeque::new();
            Self::add_to_stack(nd, &mut stack);
            stack
        };
        Self { stack }
    }

    fn add_to_stack(node: &'a Nd, stack: &mut VecDeque<&'a Nd>) {
        stack.push_back(node);

        if let Some(child) = &node.first_child {
            Self::add_to_stack(child, stack);
        }

        if let Some(sibling) = &node.next_sibling {
            Self::add_to_stack(sibling, stack);
        }
    }
}

impl<'a> Iterator for NdIterator<'a> {
    type Item = &'a Nd;

    fn next(&mut self) -> Option<Self::Item> {
        self.stack.pop_front()
    }
}


impl<'a> NdIteratorMut<'a> {
    pub fn new(nd: &'a mut Nd) -> Self {
        let stack = {
            let mut stack = VecDeque::new();
            Self::add_to_stack(nd, &mut stack);
            stack
        };
        Self { stack, _marker: Default::default() }
    }

    fn add_to_stack(node: &'a mut Nd, stack: &mut VecDeque<*mut Nd>) {
        stack.push_back(node as *mut Nd);

        if let Some(child) = &mut node.first_child {
            Self::add_to_stack(child, stack);
        }

        if let Some(sibling) = &mut node.next_sibling {
            Self::add_to_stack(sibling, stack);
        }
    }
}



impl<'a> Iterator for NdIteratorMut<'a> {
    type Item = &'a mut Nd;

    fn next(&mut self) -> Option<Self::Item> {
         self.stack.pop_front().map(|v| unsafe { &mut *v })
    }
}

#[derive(Clone, Copy)]
pub struct ModelReadContext<'a> {
    pub properties: &'a indexmap::IndexMap<String, Vec<u8>>,
    pub resource: &'a [u8],
}

impl<'a> ModelReadContext<'a> {
    pub fn new(properties: &'a indexmap::IndexMap<String, Vec<u8>>, resource: &'a [u8]) -> Self {
        Self {
            properties,
            resource,
        }
    }

    pub fn get_bone_name(&self, bone_index: u32) -> Option<&str> {
        self.properties.iter().find_map(|(k, v)| {
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
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
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
    #[serde(rename = "numBones")]
    #[serde(serialize_with = "serialize_vec_len")]
    pub bones: Vec<Bone>,
}

// TODO: Bone Names in import
#[derive(Debug, Clone, Serialize)]
#[binrw::binrw]
#[expect(clippy::manual_non_exhaustive)]
pub struct Bone {
    pub parent_id: u16,
    pub id: u16,
    pub local_transform: [f32; 3],
    pub global_transform: [f32; 3],


    pub some_i8_1: i8,
    pub some_i8_2: i8,
    pub some_i8_3: i8,

    #[brw(magic = b"\xcd")]
    _sentinel2: ()
}

#[binrw::binrw]
#[derive(Debug, Clone, serde::Serialize)]
struct NdMtxArrayEntry {
    index: u16,
    idk1: u16,
    idk2: u8,
    flags: u8,
    idk3: u16,
}

#[binrw::binrw]
#[derive(Debug, Clone, serde::Serialize)]
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

#[binrw::parser(reader)]
fn parse_rigid_indices() -> binrw::BinResult<Vec<u8>> {
    let indices_ptr = reader.read_le::<u32>()?;
    let num_indices = reader.read_le::<u32>()?;

    if indices_ptr == 0 {
        return Err(binrw::Error::AssertFail { 
            pos: reader.stream_position().unwrap_or(0), 
            message: "indices_ptr is 0".to_owned() 
        });
    }
    if num_indices == 0 {
        return Err(binrw::Error::AssertFail { 
            pos: reader.stream_position().unwrap_or(0), 
            message: "num_indices is 0".to_owned()
        });
    }

    let pos = reader.stream_position()?;

    reader.seek(SeekFrom::Start(indices_ptr.into()))?;


    let mut indices = vec![0u8; num_indices as usize];
    reader.read_exact(&mut indices)?;
    reader.seek(SeekFrom::Start(pos))?;

    Ok(indices)
}

#[derive(Clone, Debug, serde::Serialize)]
#[binrw::binread]
#[expect(clippy::manual_non_exhaustive)]
pub struct NdRigidSkinIdxData {
    #[br(parse_with = parse_rigid_indices)]
    pub indices: Vec<u8>,
    #[brw(magic = b"ndRigidSkinIdx\x00\x00")]
    _name: ()
}

impl binrw::BinWrite for NdRigidSkinIdxData {
    type Args<'a> = ModelWriteContext;

    fn write_options<W: std::io::prelude::Write + Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        mwc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        let pos = writer.stream_position()?;
        mwc.borrow_mut().rigid_entries.push((pos, self.indices.clone()));

        writer.write_le(&0u32)?;
        writer.write_le(&(self.indices.len() as u32))?;

        writer.write_all(b"ndRigidSkinIdx\x00\x00")?;

        Ok(())
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct NdBlendShapeData {
     // #[br(temp)]   nd_ptr: u32,
     // #[br(temp)]   num_shapes: u32,
     // #[br(temp, count = num_shapes)]   vertex_buffer_ptrs: Vec<u32>,

    pub nodes: Vec<Nd>,
}

impl NdBlendShapeData {
    pub fn name_offset(&self) -> i64 {
            // goes after all of the vertex buffers, realistically need to serialise (or calculate)
            // the full size 
        

        let mut v = vec![];


        let mut cur = std::io::Cursor::new(&mut v);


        let mwc = new_write_context();

        for node in &self.nodes {
            cur.write_le_args(node, mwc.clone()).unwrap();
        }

        // TODO: CHECK THAT THIS IS CORRECT
        (4      // nd_ptr
         + 4    // num_shapes
         + 4 * self.nodes.len()     // node pointers
         + v.len() // vertex buffer nodes
         ) as i64
    }
}

impl binrw::BinRead for NdBlendShapeData {
    type Args<'a> = ModelReadContext<'a>;

    fn read_options<R: std::io::Read + Seek>(
        reader: &mut R,
        _: binrw::Endian,
        mrc: Self::Args<'_>,
    ) -> binrw::BinResult<Self> {
        // TODO: Read morph target names
        let nd_ptr: u32 = reader.read_le()?;
        let num_shapes: u32 = reader.read_le()?;

        let nd_ptrs = {
            let mut v = Vec::<u32>::with_capacity(num_shapes as usize);
            for _ in 0..num_shapes {
                v.push(reader.read_le()?);
            }
            v
        };

        let nodes = (0..num_shapes).map(|_|{
            reader.read_le_args((mrc,))
        }).collect::<Result<Vec<Nd>, binrw::Error>>()?;


        Ok(Self { nodes })
    }
}

impl binrw::BinWrite for NdBlendShapeData {
    type Args<'a> = ModelWriteContext;

    fn write_options<W: std::io::Write + Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        mwc: Self::Args<'_>,
    ) -> binrw::BinResult<()> {
        Ok(())
        // todo!()
    }
}

#[path = "./nd_tests.rs"]
#[cfg(test)]
mod tests;
