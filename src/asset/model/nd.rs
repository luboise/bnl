use std::{
    io::{self, Cursor, Read, Seek, SeekFrom},
    iter::{self},
};

use byteorder::{LittleEndian, ReadBytesExt};
use mod3d_base::BufferElementType;
use mod3d_gltf::{AccessorIndex, Gltf, ViewIndex};

use crate::{asset::param::KnownUnknown, d3d::D3DPrimitiveType};

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
}

impl TryFrom<String> for KnownNdType {
    type Error = NdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_ref() {
            "ndVertexBuffer" => Ok(KnownNdType::VertexBuffer),
            "ndPushBuffer" => Ok(KnownNdType::PushBuffer),
            _ => Err(NdError::UnknownType),
        }
    }
}

impl From<KnownNdType> for String {
    fn from(value: KnownNdType) -> Self {
        match value {
            KnownNdType::VertexBuffer => "ndVertexBuffer",
            KnownNdType::PushBuffer => "ndPushBuffer",
        }
        .to_string()
    }
}

const NDHEADER_SIZE: usize = 32;

#[derive(Debug, Clone)]
pub struct NdHeader {
    name_ptr: u32,
    nd_type: NdType,
    unknown_u16: u16, // Possibly index
    unknown_ptr1: u32,
    unknown_ptr2: u32,
    unknown_u32: u32,
    first_child_ptr: u32,
    next_sibling_ptr: u32,
    parent_ptr: u32,

    // DO NOT SERIALISE
    first_child: Option<Box<Nd>>,
    next_sibling: Option<Box<Nd>>,
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

        // TODO: Move this somewhere else
        let nd_type: NdType = match name.as_ref() {
            "ndVertexBuffer" => NdType::Known(KnownNdType::VertexBuffer),
            "ndPushBuffer" => NdType::Known(KnownNdType::PushBuffer),
            _ => NdType::Unknown(name),
        };

        let first_child = match first_child_ptr {
            0 => None,
            _ => Some(Nd::new(bytes, first_child_ptr as usize)?.into()),
        };

        let next_sibling = match next_sibling_ptr {
            0 => None,
            _ => Some(Nd::new(bytes, next_sibling_ptr as usize)?.into()),
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

    pub fn first_child(&self) -> Option<&Box<Nd>> {
        self.first_child.as_ref()
    }

    pub fn next_sibling(&self) -> Option<&Box<Nd>> {
        self.next_sibling.as_ref()
    }
}

#[derive(Debug, Clone)]
pub enum KnownNdType {
    VertexBuffer,
    PushBuffer,
}

type NdType = KnownUnknown<KnownNdType, String>;

#[derive(Debug, Clone)]
pub(crate) struct NdUnknown {
    header: NdHeader,
}
impl NdUnknown {
    pub(crate) fn header(&self) -> &NdHeader {
        &self.header
    }
}

#[derive(Debug, Clone)]
pub enum Nd {
    VertexBuffer(NdVertexBuffer),
    PushBuffer(NdPushBuffer),
    Unknown(NdUnknown),
}

impl Nd {
    pub fn new(slice: &[u8], nd_start: usize) -> Result<Nd, NdError> {
        let mut cur = Cursor::new(slice);

        let header = NdHeader::from_bytes(slice, nd_start as u32)?;

        cur.seek(SeekFrom::Start(32 + nd_start as u64))?;

        if let KnownUnknown::Known(nd_type) = &header.nd_type {
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
                KnownNdType::PushBuffer => {
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
                        let data_size = vertex_counts_ptr.read_u32::<LittleEndian>()?;

                        if data_ptr < min {
                            min = data_ptr;
                        }
                        if data_ptr + data_size > max {
                            max = data_ptr + data_size;
                        }

                        draw_calls.push(DrawCall {
                            data_ptr,
                            prim_type,
                            data_size,
                        });
                    }

                    Ok(Nd::PushBuffer(NdPushBuffer {
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
                        push_buffer_base: min,
                        push_buffer_size: max - min,

                        draw_calls,
                    }))
                }
            }
        } else {
            Ok(Nd::Unknown(NdUnknown { header }))
        }
    }
}

impl NdNode for Nd {
    fn header(&self) -> &NdHeader {
        match self {
            Nd::VertexBuffer(val) => val.header(),
            Nd::PushBuffer(val) => val.header(),
            Nd::Unknown(val) => val.header(),
        }
    }
}

#[repr(u8)]
#[derive(Debug, PartialEq, Clone, Copy)]
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

#[derive(Debug, Clone)]
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
        buffer_index: ViewIndex,
    ) -> Result<AccessorIndex, std::io::Error> {
        match self.res_type {
            VertexBufferViewType::Vertex => {
                let num_vertices = self.view_size / (size_of::<mod3d_base::Vec3>() as u32);

                return Ok(gltf.add_accessor(
                    buffer_index,
                    self.view_start,
                    num_vertices,
                    BufferElementType::Float32,
                    3, // 3 elements in pos
                ));
            }
            VertexBufferViewType::UV
            | VertexBufferViewType::Unknown10
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

#[derive(Debug, Clone)]
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
}

#[derive(Debug, Clone)]
pub struct DrawCall {
    pub(crate) data_ptr: u32,
    pub(crate) prim_type: D3DPrimitiveType,
    pub(crate) data_size: u32,
}

#[derive(Debug, Clone)]
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

    // DO NOT SERIALISE
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

    #[test]
    fn nd_header() {
        let bytes = get_test_bytes();
        NdHeader::from_bytes(&bytes, 0x34).expect("Unable to create NdHeader");
    }

    #[test]
    fn nd_parse_test() {
        let bytes = get_test_bytes();

        Nd::new(&bytes, 0x34).expect("Unable to create ND");
    }
}
