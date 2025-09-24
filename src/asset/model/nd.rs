use std::{
    fmt::Display,
    io::{self, Cursor, Read, Seek, SeekFrom},
    iter::{self},
};

use byteorder::{LittleEndian, ReadBytesExt};
use gltf_writer::gltf::{
    self, Accessor, AccessorComponentCount, AccessorDataType, Buffer, BufferView, Gltf, GltfIndex,
};
use indexmap::IndexMap;
use serde::{Serialize, ser::SerializeMap};

use crate::{
    VirtualResource,
    asset::{AssetParseError, model::gltf::NdGltfContext, param::KnownUnknown},
    d3d::{D3DPrimitiveType, PixelShaderConstant, VertexShaderConstant},
};

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
    fn children<'a>(&'a self) -> impl Iterator<Item = &'a Nd> + 'a {
        let child = self.header().first_child.as_deref();

        iter::successors(child, |node| node.header().next_sibling.as_deref())
    }

    fn header(&self) -> &NdHeader;

    fn add_gltf_node(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError>;

    fn insert_into_gltf_heirarchy(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        let node_index_opt = self.add_gltf_node(virtual_res, ctx)?;

        /*
        let mut parent = GltfIndex::MAX;
        let mut grandparent: Option<GltfIndex> = Some(GltfIndex::MAX);
        */

        if let Some(node_index) = node_index_opt {
            ctx.node_stack.push(node_index);

            /*
            grandparent = ctx.node_stack.last().copied();
            parent = node_index;
            */
        } else {
        }

        if let Some(child) = self.header().first_child() {
            child.add_gltf_node(virtual_res, ctx)?;
        }
        if let Some(next_sibling) = self.header().next_sibling() {
            next_sibling.add_gltf_node(virtual_res, ctx)?;
        }

        if node_index_opt.is_some() {
            ctx.node_stack.pop();
        }

        Ok(node_index_opt)
    }
}

impl From<KnownNdType> for String {
    fn from(value: KnownNdType) -> Self {
        match value {
            KnownNdType::VertexBuffer => "ndVertexBuffer",
            KnownNdType::PushBuffer => "ndPushBuffer",
            KnownNdType::BGPushBuffer => "ndBGPushBuffer",
            KnownNdType::ShaderParam2 => "ndShaderParam2",
            KnownNdType::Group => "ndGroup",
        }
        .to_string()
    }
}

const NDHEADER_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct NdHeader {
    pub name_ptr: u32,
    pub nd_type: NdType,
    pub unknown_u16: u16, // Possibly index
    pub unknown_ptr1: u32,
    pub unknown_ptr2: u32,
    pub unknown_u32: u32,
    pub first_child_ptr: u32,
    pub next_sibling_ptr: u32,
    pub parent_ptr: u32,

    // DO NOT SERIALISE
    first_child: Option<Box<Nd>>,
    next_sibling: Option<Box<Nd>>,
}

impl Serialize for NdHeader {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(None)?;

        map.serialize_entry("type", &self.nd_type.to_string())?;

        let children: Vec<&Nd> = self.children().collect();
        map.serialize_entry("children", &children)?;

        map.end()
    }
}

impl NdHeader {
    pub fn from_bytes(bytes: &[u8], header_start: u32) -> Result<NdHeader, NdError> {
        let mut cur = Cursor::new(bytes);

        cur.seek(SeekFrom::Start(header_start as u64))?;

        let name_ptr = cur.read_u32::<LittleEndian>()?;

        // TODO: Sanity check name against name ptr
        let type_u16 = cur.read_u16::<LittleEndian>()?;
        let unknown_u16 = cur.read_u16::<LittleEndian>()?;

        let unknown_ptr1 = cur.read_u32::<LittleEndian>()?;
        let unknown_ptr2 = cur.read_u32::<LittleEndian>()?;
        let unknown_u32 = cur.read_u32::<LittleEndian>()?;

        let first_child_ptr = cur.read_u32::<LittleEndian>()?;
        let next_sibling_ptr = cur.read_u32::<LittleEndian>()?;
        let parent_ptr = cur.read_u32::<LittleEndian>()?;

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

        let nd_type: NdType = name.into();

        let first_child = match first_child_ptr {
            0 => None,
            _ => Some(
                Nd::new(ModelSlice {
                    slice: bytes,
                    read_start: first_child_ptr as usize,
                })?
                .into(),
            ),
        };

        let next_sibling = match next_sibling_ptr {
            0 => None,
            _ => Some(
                Nd::new(ModelSlice {
                    slice: bytes,
                    read_start: next_sibling_ptr as usize,
                })?
                .into(),
            ),
        };

        Ok(NdHeader {
            name_ptr,
            nd_type,
            unknown_u16,
            unknown_ptr1,
            unknown_ptr2,
            unknown_u32,
            first_child_ptr,
            next_sibling_ptr,
            parent_ptr,
            //
            first_child,
            next_sibling,
        })
    }

    pub fn children(&self) -> impl Iterator<Item = &Nd> {
        iter::successors(self.first_child(), |nd| nd.header().next_sibling())
    }

    pub fn first_child(&self) -> Option<&Nd> {
        self.first_child.as_deref()
    }

    pub fn next_sibling(&self) -> Option<&Nd> {
        self.next_sibling.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum KnownNdType {
    VertexBuffer,
    PushBuffer,
    BGPushBuffer,
    ShaderParam2,
    Group,
}

impl Display for KnownNdType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KnownNdType::VertexBuffer => write!(f, "ndVertexBuffer"),
            KnownNdType::PushBuffer => write!(f, "ndPushBuffer"),
            KnownNdType::BGPushBuffer => write!(f, "ndBGPushBuffer"),
            KnownNdType::ShaderParam2 => write!(f, "ndShaderParam2"),
            KnownNdType::Group => write!(f, "ndGroup"),
        }
    }
}

impl TryFrom<String> for KnownNdType {
    type Error = NdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_ref() {
            "ndVertexBuffer" => Ok(KnownNdType::VertexBuffer),
            "ndPushBuffer" => Ok(KnownNdType::PushBuffer),
            "ndBGPushBuffer" => Ok(KnownNdType::BGPushBuffer),
            "ndShaderParam2" => Ok(KnownNdType::ShaderParam2),
            "ndGroup" => Ok(KnownNdType::Group),
            _ => Err(NdError::UnknownType),
        }
    }
}

type NdType = KnownUnknown<KnownNdType, String>;

impl ToString for NdType {
    fn to_string(&self) -> String {
        match self {
            KnownUnknown::Known(val) => val.to_string(),
            KnownUnknown::Unknown(val) => val.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdUnknown {
    header: NdHeader,
}

impl NdNode for NdUnknown {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        _virtual_res: &VirtualResource,
        _ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        Ok(None)
    }
}

impl NdUnknown {
    pub(crate) fn header(&self) -> &NdHeader {
        &self.header
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum Nd {
    VertexBuffer(NdVertexBuffer),
    PushBuffer(NdPushBuffer),
    BGPushBuffer(NdBGPushBuffer),
    Group(NdGroup),
    ShaderParam2(NdShaderParam2),
    Unknown(NdUnknown),
}

impl NdNode for Nd {
    fn header(&self) -> &NdHeader {
        match self {
            Nd::VertexBuffer(val) => val.header(),
            Nd::PushBuffer(val) => val.header(),
            Nd::BGPushBuffer(val) => val.header(),
            Nd::Group(val) => val.header(),
            Nd::Unknown(val) => val.header(),
            Nd::ShaderParam2(val) => val.header(),
        }
    }

    fn add_gltf_node(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        match self {
            Nd::VertexBuffer(nd) => nd.insert_into_gltf_heirarchy(virtual_res, ctx),
            Nd::PushBuffer(nd) => nd.insert_into_gltf_heirarchy(virtual_res, ctx),
            Nd::BGPushBuffer(nd) => nd.insert_into_gltf_heirarchy(virtual_res, ctx),
            Nd::Group(nd) => nd.insert_into_gltf_heirarchy(virtual_res, ctx),
            Nd::ShaderParam2(nd) => nd.insert_into_gltf_heirarchy(virtual_res, ctx),
            Nd::Unknown(nd) => nd.insert_into_gltf_heirarchy(virtual_res, ctx),
        }
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

impl Nd {
    pub fn new(model_slice: ModelSlice) -> Result<Nd, NdError> {
        let slice = model_slice.slice();

        let mut cur = Cursor::new(slice);

        let header = NdHeader::from_bytes(slice, model_slice.read_start as u32)?;

        cur.seek(SeekFrom::Start(32 + model_slice.read_start as u64))?;

        if let KnownUnknown::Known(nd_type) = &header.nd_type.clone() {
            match nd_type {
                KnownNdType::VertexBuffer => {
                    let resource_views_ptr = cur.read_u32::<LittleEndian>()?;
                    let num_resource_views = cur.read_u32::<LittleEndian>()?;

                    let mut resource_views = Vec::with_capacity(num_resource_views as usize);

                    for _ in 0..num_resource_views {
                        resource_views.push(VertexBufferResourceView::from_cursor(&mut cur)?);
                    }

                    Ok(Nd::VertexBuffer(NdVertexBuffer {
                        header,
                        resource_views_ptr,
                        num_resource_views,
                        resource_views,
                    }))
                }
                KnownNdType::PushBuffer | KnownNdType::BGPushBuffer => {
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

                    let buffer_bytes = slice
                        [push_buffer_base as usize..push_buffer_base as usize + push_buffer_size]
                        .to_vec();

                    let push_buffer = NdPushBuffer {
                        header,
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
                    };

                    if *nd_type == KnownNdType::BGPushBuffer {
                        let unknown_ptr_1 = cur.read_u32::<LittleEndian>()?;
                        let unknown_ptr_2 = cur.read_u32::<LittleEndian>()?;

                        Ok(Nd::BGPushBuffer(NdBGPushBuffer {
                            push_buffer,
                            unknown_ptr_1,
                            unknown_ptr_2,
                        }))
                    } else {
                        Ok(Nd::PushBuffer(push_buffer))
                    }
                }
                KnownNdType::Group => {
                    // NdGroup spotted
                    Ok(Nd::Group(NdGroup { header }))
                }
                KnownNdType::ShaderParam2 => {
                    let main_payload_ptr = cur.read_u32::<LittleEndian>()?;
                    let sub_payload_ptr = cur.read_u32::<LittleEndian>()?;

                    let main_payload = NdShaderParam2Payload::from_model_slice(
                        &model_slice.at(main_payload_ptr as usize),
                    )?;

                    let sub_payload = match sub_payload_ptr {
                        0 => None,
                        val => Some(NdShaderParam2Payload::from_model_slice(
                            &model_slice.at(val as usize),
                        )?),
                    };

                    Ok(Nd::ShaderParam2(NdShaderParam2 {
                        header,
                        main_payload,
                        sub_payload,
                    }))
                }
            }
        } else {
            Ok(Nd::Unknown(NdUnknown { header }))
        }
    }
}

#[repr(u8)]
#[derive(Debug, PartialEq, Clone, Copy, Serialize)]
pub enum VertexBufferViewType {
    Skin = 0x0,
    SkinWeight = 0x8,
    Vertex = 0x9,
    Unknown10 = 0xa,
    Unknown11 = 0xb,
    UV = 0xd,
    Unknown14 = 0xe,
    Unknown15 = 0xf,
    Unknown16 = 0x10,
    KnknownFF = 0xff,
}

impl From<u8> for VertexBufferViewType {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Skin,
            0x8 => Self::SkinWeight,
            0x9 => Self::Vertex,
            0xa => Self::Unknown10,
            0xb => Self::Unknown11,
            0xd => Self::UV,
            0xe => Self::Unknown14,
            0xf => Self::Unknown15,
            0x10 => Self::Unknown16,
            _ => Self::KnknownFF,
        }
    }
}

struct GLTFViewAttribs {
    base_type: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct VertexBufferResourceView {
    stride: u8,
    res_type: VertexBufferViewType,
    unknown_u16: u16,

    unknown_u32_1: u32,

    // 0x8
    unknown_u32_2: u32,
    unknown_u32_3: u32,

    // 0x16
    view_start: u32,
    view_size: u32,
}

impl VertexBufferResourceView {
    pub fn from_cursor(cur: &mut Cursor<&[u8]>) -> Result<Self, std::io::Error> {
        Ok(VertexBufferResourceView {
            stride: cur.read_u8()?,
            res_type: cur.read_u8()?.into(),
            unknown_u16: cur.read_u16::<LittleEndian>()?,
            unknown_u32_1: cur.read_u32::<LittleEndian>()?,
            unknown_u32_2: cur.read_u32::<LittleEndian>()?,
            unknown_u32_3: cur.read_u32::<LittleEndian>()?,
            view_start: cur.read_u32::<LittleEndian>()?,
            view_size: cur.read_u32::<LittleEndian>()?,
        })
    }

    pub(crate) fn add_to_gltf(
        &self,
        gltf: &mut Gltf,
        buffer_view_index: GltfIndex,
    ) -> Result<GltfIndex, std::io::Error> {
        match self.res_type {
            VertexBufferViewType::Vertex => {
                let num_vertices = self.view_size / 12;

                Ok(gltf.add_accessor(Accessor::new(
                    buffer_view_index,
                    self.view_start as usize,
                    AccessorDataType::F32,
                    num_vertices as usize,
                    AccessorComponentCount::VEC3,
                )))
            }
            VertexBufferViewType::UV => {
                let num_vertices = self.view_size / 8;

                Ok(gltf.add_accessor(Accessor::new(
                    buffer_view_index,
                    self.view_start as usize,
                    AccessorDataType::F32,
                    num_vertices as usize,
                    AccessorComponentCount::VEC2,
                )))
            }
            VertexBufferViewType::Unknown10
            | VertexBufferViewType::Unknown11
            | VertexBufferViewType::SkinWeight
            | VertexBufferViewType::Unknown14
            | VertexBufferViewType::Unknown15
            | VertexBufferViewType::Unknown16
            | VertexBufferViewType::Skin
            | VertexBufferViewType::KnknownFF => Err(std::io::Error::other(format!(
                "VertexBufferViewType {:?} not implemented.",
                self.res_type
            ))),
        }
    }

    pub fn len(&self) -> usize {
        self.view_size as usize
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn stride(&self) -> u8 {
        self.stride
    }

    pub fn start(&self) -> u32 {
        self.view_start
    }

    pub fn end(&self) -> u32 {
        self.view_start + self.view_size
    }

    pub fn res_type(&self) -> VertexBufferViewType {
        self.res_type
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdVertexBuffer {
    header: NdHeader,
    resource_views_ptr: u32,
    num_resource_views: u32,

    // DO NOT SERIALISE
    resource_views: Vec<VertexBufferResourceView>,
}

impl NdVertexBuffer {
    pub fn resource_views(&self) -> &[VertexBufferResourceView] {
        &self.resource_views
    }
}

impl NdNode for NdVertexBuffer {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        let mut min = u32::MAX;
        let mut max = u32::MIN;

        // Get the size of the buffer
        self.resource_views().iter().for_each(|view| {
            if view.start() < min {
                min = view.start();
            }

            if view.end() > max {
                max = view.end();
            }
        });

        let res_size = (max - min) as usize;

        let res_bytes = virtual_res
            .get_bytes(min as usize, res_size)
            .map_err(|e| AssetParseError::InvalidDataViews(e.to_string()))?;

        let gb = Buffer::new(res_bytes);
        let buffer_index = ctx.gltf.add_buffer(gb);

        for res_view in self.resource_views() {
            let buffer_view_index = ctx.gltf.add_buffer_view(gltf::BufferView::new(
                buffer_index,
                res_view.start() as usize,
                res_view.len(),
                Some(res_view.stride() as usize),
                None,
            ));

            if res_view.res_type() == VertexBufferViewType::Vertex
                && ctx.positions_accessor.is_none()
            {
                let accessor_index = ctx.gltf.add_accessor(Accessor::new(
                    buffer_view_index,
                    0,
                    AccessorDataType::F32,
                    res_view.len() / 12,
                    AccessorComponentCount::VEC3,
                ));

                ctx.positions_accessor = Some(accessor_index);
            }

            if let Err(e) = res_view.add_to_gltf(&mut ctx.gltf, buffer_view_index) {
                eprintln!(
                    "Unable to add bv {} to gltf file.\nError: {}",
                    buffer_view_index, e
                );

                return Ok(None);
            } else if res_view.res_type() == VertexBufferViewType::UV && ctx.uv_accessor.is_none() {
                let accessor_index = ctx.gltf.add_accessor(Accessor::new(
                    buffer_view_index,
                    0,
                    AccessorDataType::F32,
                    res_view.len() / 8,
                    AccessorComponentCount::VEC2,
                ));

                ctx.uv_accessor = Some(accessor_index);
            }

            if let Err(e) = res_view.add_to_gltf(&mut ctx.gltf, buffer_view_index) {
                eprintln!(
                    "Unable to add bv {} to gltf file.\nError: {}",
                    buffer_view_index, e
                );
            };
        }

        Ok(None)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DrawCall {
    pub(crate) data_ptr: u32,
    pub(crate) prim_type: D3DPrimitiveType,
    pub(crate) num_vertices: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NdPushBuffer {
    header: NdHeader,

    num_draws: u32,
    unknown_u32_1: u32,
    unknown_u32_2: u32,
    unknown_u32_3: u32,

    // File offsets
    data_pointers_start: u32,
    primitive_types_list_ptr: u32,
    vertex_counts_list_ptr: u32,

    prevent_culling_flag: u8,
    padding: [u8; 3],

    #[serde(skip_serializing)]
    pub(crate) buffer_bytes: Vec<u8>,

    pub(crate) push_buffer_base: u32,
    pub(crate) push_buffer_size: u32,

    draw_calls: Vec<DrawCall>,
}

impl NdPushBuffer {
    pub fn draw_calls(&self) -> &[DrawCall] {
        &self.draw_calls
    }
}

impl NdNode for NdPushBuffer {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        let mut mesh = gltf::Mesh::new("Idk Mesh".to_string());

        let index_buffer: &Vec<u8> = &self.buffer_bytes;

        let buffer_index = ctx.gltf.add_buffer(Buffer::new(index_buffer));
        let ib_view_index = ctx.gltf.add_buffer_view(BufferView {
            buffer_index,
            byte_offset: 0,
            byte_length: index_buffer.len(),
            byte_stride: None,
            target: None,
        });

        self.draw_calls().iter().for_each(|draw_call| {
            let ib_accessor_index = ctx.gltf.add_accessor(Accessor::new(
                ib_view_index,
                (draw_call.data_ptr - self.push_buffer_base) as usize,
                AccessorDataType::U16,
                draw_call.num_vertices as usize,
                AccessorComponentCount::SCALAR,
            ));

            let primitive = mesh.add_primitive(gltf::Primitive {
                indices_accessor: Some(ib_accessor_index),
                topology_type: match draw_call.prim_type.clone().try_into() {
                    Ok(val) => Some(val),
                    Err(e) => {
                        eprintln!("{}", e);
                        None
                    }
                },

                material: ctx.current_material,
                attributes: Default::default(),
            });

            if let Some(positions_accessor) = ctx.positions_accessor {
                primitive.set_attribute(gltf::VertexAttribute::Position, positions_accessor);
            } else {
                eprintln!("No positions accessor available.");
            }

            if let Some(uv_accessor) = ctx.uv_accessor {
                primitive.set_attribute(gltf::VertexAttribute::TexCoord(0), uv_accessor);
            } else {
                eprintln!("No texcoords accessor available.");
            }
        });

        let mesh_index = ctx.gltf.add_mesh(mesh);

        let mut node = gltf::Node::new(Some("node name".to_string()));
        node.set_mesh_index(Some(mesh_index));

        Ok(Some(ctx.gltf.add_node(node)))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdBGPushBuffer {
    push_buffer: NdPushBuffer,
    unknown_ptr_1: u32,
    unknown_ptr_2: u32,
}

impl NdBGPushBuffer {
    pub fn push_buffer(&self) -> &NdPushBuffer {
        &self.push_buffer
    }
}

impl NdNode for NdBGPushBuffer {
    fn header(&self) -> &NdHeader {
        self.push_buffer.header()
    }

    fn add_gltf_node(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        let push_buffer = self.push_buffer();
        push_buffer.insert_into_gltf_heirarchy(virtual_res, ctx)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdGroup {
    header: NdHeader,
}

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

pub const TEXTURE_ASSIGNMENT_SIZE: usize = 28;

#[derive(Debug, Clone, Serialize)]
pub struct TextureAssignment {
    pub(crate) texture_index: u32,
    pub(crate) count_1: u8,
    pub(crate) count_2: u8,
    pub(crate) count_3: u8,
    pub(crate) skip_diffuse_texture: bool,
    pub(crate) unknown_1: u32,
    pub(crate) unknown_2: u32,
    pub(crate) unknown_3: u32,
    pub(crate) unknown_4: u32,
    pub(crate) unknown_5: u32,
    // ORIGINAL FORMAT
    /*
       u32 textureIndex;
    u8 flag1;
    u8 flag2;
    u8 flag3;
    bool skipDiffuseTexture;
    u32 unknown3;
    u32 unknown4;
    u32 unknown5;
    u32 unknown6;
    u32 unknown7;
    */
}

impl TextureAssignment {
    fn from_model_slice(model_slice: ModelSlice) -> Result<Self, std::io::Error> {
        let mut cur = model_slice.new_cursor();

        let texture_index = cur.read_u32::<LittleEndian>()?;
        let count_1 = cur.read_u8()?;
        let count_2 = cur.read_u8()?;
        let count_3 = cur.read_u8()?;

        let skip_diffuse_texture: bool = !matches!(cur.read_u8()?, 0);

        let unknown_1 = cur.read_u32::<LittleEndian>()?;
        let unknown_2 = cur.read_u32::<LittleEndian>()?;
        let unknown_3 = cur.read_u32::<LittleEndian>()?;
        let unknown_4 = cur.read_u32::<LittleEndian>()?;
        let unknown_5 = cur.read_u32::<LittleEndian>()?;

        Ok(Self {
            texture_index,
            count_1,
            count_2,
            count_3,
            skip_diffuse_texture,
            unknown_1,
            unknown_2,
            unknown_3,
            unknown_4,
            unknown_5,
        })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AttributeValue {
    pub(crate) val1: u32,
    pub(crate) val2: u32,

    pub(crate) sentinel1: u8,
    pub(crate) sentinel2: u8,
    pub(crate) sentinel3: u8,
    pub(crate) sentinel4: u8,
}

fn serialize_index_map<S>(
    index_map: &IndexMap<String, AttributeValue>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut map = serializer.serialize_map(None)?;

    for (key, value) in index_map {
        map.serialize_entry(&key, &value)?;
    }

    map.end()
}

#[derive(Debug, Clone, Serialize)]
pub struct NdShaderParam2Payload {
    vertex_shader_constants: Vec<VertexShaderConstant>,
    pixel_shader_constants: Vec<[u8; 4]>,
    texture_assignments: Vec<TextureAssignment>,

    alpha_ref: u8, // Index to the alpha reference texture???
    count_1: u8,
    count_2: u8,
    some_count: u8,

    unknown_1: u32,
    next_payload: u32, // Pointer to next payload???

    #[serde(serialize_with = "serialize_index_map")]
    attribute_map: IndexMap<String, AttributeValue>,
    /*
    RawColour* pixelShaderConstants: u32 [[pointer_base("section1innersptr")]];
    u32* somePtr2: u32 [[pointer_base("section1innersptr")]];
    TextureAssignment* textureAssignments: u32 [[pointer_base("section1innersptr")]];
    u32 numTextureAssignments;
    u32 numBruhs;
    u32 numPixelShaderConstants;

    // 0x18
    u8 alphaReference;
    u8 flag1;
    u8 flag2;
    u8 someCount;

    u32 someU32_5;

    // 0x20
    u32* child: u32 [[pointer_base("section1innersptr")]];

    u32* assignmentsStart: u32 [[pointer_base("section1innersptr")]];
    u32 numAssignments;
        */
}

impl NdShaderParam2Payload {
    pub fn from_model_slice(model_slice: &ModelSlice) -> Result<Self, NdError> {
        let mut cur = Cursor::new(model_slice.slice);

        cur.seek(SeekFrom::Start(model_slice.read_start as u64))?;

        let pixel_shader_constants_start = cur.read_u32::<LittleEndian>()?;
        let vertex_shader_constants_start = cur.read_u32::<LittleEndian>()?;
        let texture_assignments_start = cur.read_u32::<LittleEndian>()?;
        let num_texture_assignments = cur.read_u32::<LittleEndian>()?;
        let num_vertex_shader_constants = cur.read_u32::<LittleEndian>()?;
        let num_pixel_shader_constants = cur.read_u32::<LittleEndian>()?;

        let alpha_ref = cur.read_u8()?;
        let count_1 = cur.read_u8()?;
        let count_2 = cur.read_u8()?;
        let some_count = cur.read_u8()?;

        let unknown_1 = cur.read_u32::<LittleEndian>()?;
        let next_payload_start = cur.read_u32::<LittleEndian>()?;
        let attributes_start = cur.read_u32::<LittleEndian>()?;
        let num_attributes = cur.read_u32::<LittleEndian>()?;

        let mut attribute_map = IndexMap::new();

        cur.seek(SeekFrom::Start(attributes_start as u64))?;

        for _ in 0..num_attributes {
            let name_ptr = cur.read_u32::<LittleEndian>()?;
            let val1 = cur.read_u32::<LittleEndian>()?;
            let val2 = cur.read_u32::<LittleEndian>()?;

            let sentinel1 = cur.read_u8()?;
            let sentinel2 = cur.read_u8()?;
            let sentinel3 = cur.read_u8()?;
            let sentinel4 = cur.read_u8()?;

            let mut name_cur = cur.clone();
            name_cur.seek(SeekFrom::Start(name_ptr as u64))?;

            let utf8_chars: Vec<u8> = name_cur
                .bytes()
                .map(|b| b.unwrap())
                .take_while(|b| *b != 0)
                .collect();

            let name = String::from_utf8(utf8_chars)
                .map_err(|e| NdError::CreationFailure(e.to_string()))?;

            if let Some(old_val) = attribute_map.insert(
                name.clone(),
                AttributeValue {
                    val1,
                    val2,
                    sentinel1,
                    sentinel2,
                    sentinel3,
                    sentinel4,
                },
            ) {
                println!(
                    "Overriding old entry in attribute map.\n{}: {:?}",
                    name, old_val
                );
            }
        }

        let vertex_constants_slice = &model_slice.slice[vertex_shader_constants_start as usize..];
        let vertex_shader_constants: Vec<VertexShaderConstant> = vertex_constants_slice
            .chunks_exact(size_of::<VertexShaderConstant>())
            .take(num_vertex_shader_constants as usize)
            .map(|chunk| {
                let mut constant: VertexShaderConstant = [0.0, 0.0, 0.0, 0.0];

                chunk.chunks_exact(4).enumerate().for_each(|(i, ch)| {
                    constant[i] = f32::from_le_bytes(ch.try_into().unwrap());
                });

                constant
            })
            .collect();

        let pixel_constants_slice = &model_slice.slice[pixel_shader_constants_start as usize..];
        let pixel_shader_constants: Vec<PixelShaderConstant> = pixel_constants_slice
            .chunks_exact(size_of::<PixelShaderConstant>())
            .take(num_pixel_shader_constants as usize)
            .map(|chunk| chunk.try_into().unwrap())
            .collect();

        let mut texture_assignments = vec![];

        for i in 0..num_texture_assignments as usize {
            texture_assignments.push(TextureAssignment::from_model_slice(
                model_slice.at(texture_assignments_start as usize + i * TEXTURE_ASSIGNMENT_SIZE),
            )?);
        }

        Ok(NdShaderParam2Payload {
            vertex_shader_constants,
            pixel_shader_constants,
            texture_assignments,
            alpha_ref,
            count_1,
            count_2,
            some_count,
            unknown_1,
            next_payload: next_payload_start,
            attribute_map,
        })
    }

    pub fn attribute_map(&self) -> &IndexMap<String, AttributeValue> {
        &self.attribute_map
    }

    pub fn texture_assignments(&self) -> &[TextureAssignment] {
        &self.texture_assignments
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdShaderParam2 {
    header: NdHeader,

    main_payload: NdShaderParam2Payload,
    sub_payload: Option<NdShaderParam2Payload>,
}

impl NdNode for NdShaderParam2 {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        let main_attribute_map = self.main_payload().attribute_map();

        let attrib_key = "colour0";

        if let Some(attrib) = main_attribute_map.get(attrib_key) {
            main_attribute_map
                .get_index_of(attrib_key)
                .expect("Unable to find index for key that was literally just found.");

            let texture_slot = attrib.val2;

            match self
                .main_payload()
                .texture_assignments()
                .get(texture_slot as usize)
            {
                Some(tex_assignment) => {
                    let material_index = ctx.gltf.add_material(gltf::Material {
                        name: "Some Material".to_string(),
                        pbr_metallic_roughness: Some(gltf::PBRMetallicRoughness {
                            base_color_texture: Some(gltf::TextureInfo {
                                texture_index: tex_assignment.texture_index,
                                texcoords_accessor: None,
                            }),
                            metallic_factor: Some(0.0),
                            ..Default::default()
                        }),
                    });

                    ctx.current_material = Some(material_index);
                }
                None => eprintln!(
                    "Texture slot {} is referenced by an ndShaderParam, but the param only assigns {} slots.",
                    texture_slot + 1,
                    self.main_payload().texture_assignments().len()
                ),
            };
        }

        Ok(None)
    }
}

impl NdShaderParam2 {
    pub fn num_bound_textures(&self) -> usize {
        self.main_payload.texture_assignments.len()
    }

    pub fn main_payload(&self) -> &NdShaderParam2Payload {
        &self.main_payload
    }

    pub fn sub_payload(&self) -> Option<&NdShaderParam2Payload> {
        self.sub_payload.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn get_test_bytes() -> Vec<u8> {
        let test_path = std::path::Path::new(file!())
            .parent()
            .expect("Unable to get parent directory of test.")
            .join("test_meshes")
            .join("test_mesh_0");

        fs::read(test_path).expect("Unable to read test input.")
    }

    fn get_test_file(filename: &str) -> Vec<u8> {
        let test_path = std::path::Path::new(file!())
            .parent()
            .expect("Unable to get parent directory of test.")
            .join("test_meshes")
            .join(filename);

        fs::read(&test_path).expect("Unable to get test file")
    }

    #[test]
    fn nd_header() {
        let bytes = get_test_bytes();
        NdHeader::from_bytes(&bytes, 0x34).expect("Unable to create NdHeader");
    }

    #[test]
    fn nd_parse_test() {
        let bytes = get_test_bytes();

        Nd::new(ModelSlice {
            slice: &bytes,
            read_start: 0x34,
        })
        .expect("Unable to create ND");
    }

    #[test]
    fn nd_shader_param2() {
        let bytes = get_test_file("test_ndShaderParam2_1");

        let nd = Nd::new(ModelSlice {
            slice: &bytes,
            read_start: 0,
        })
        .expect("Unable to create ND");

        if let Nd::ShaderParam2(sp2) = nd {
            let attribute_map = &sp2.main_payload.attribute_map;

            assert_eq!(attribute_map.len(), 2, "Attribute map is wrong size.");

            assert_eq!(
                sp2.num_bound_textures(),
                2,
                "Number of bound textures is wrong."
            );

            assert_eq!(attribute_map.len(), 2, "Attribute map is wrong size.");
        } else {
            panic!(
                "nd has wrong type {:?}, expected ndShaderParam2.",
                dbg!(&nd)
            );
        }
    }
}
