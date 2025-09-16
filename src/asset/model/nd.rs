use std::{
    io::{self, Cursor, Read, Seek, SeekFrom},
    iter::{self},
    task::Wake,
};

use byteorder::{LittleEndian, ReadBytesExt};

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
            "ndVertexBuffer" => {
                return Ok(KnownNdType::VertexBuffer);
            }
            "ndPushBuffer" => {
                return Ok(KnownNdType::PushBuffer);
            }
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
}

#[derive(Debug, Clone)]
pub enum KnownNdType {
    VertexBuffer,
    PushBuffer,
}

type NdType = KnownUnknown<KnownNdType, String>;

#[derive(Debug, Clone)]
pub enum Nd {
    VertexBuffer(NdVertexBuffer),
    PushBuffer(NdPushBuffer),
    Other(),
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
                KnownNdType::PushBuffer => todo!(),
            }
        } else {
            Ok(Nd::Other())
        }
    }
}

impl NdNode for Nd {
    fn header(&self) -> &NdHeader {
        match self {
            Nd::VertexBuffer(val) => val.header(),
            Nd::PushBuffer(val) => val.header(),
            Nd::Other() => todo!(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VertexBufferResourceView {
    stride: u8,
    res_type: u8,
    unknown_u16: u16,

    // 0x8
    unknown_u32_1: u32,
    unknown_u32_2: u32,

    // 0x16
    view_start: u32,
    view_size: u32,
}

impl VertexBufferResourceView {
    pub fn from_cursor(cur: &mut Cursor<&[u8]>) -> Result<Self, std::io::Error> {
        Ok(VertexBufferResourceView {
            stride: cur.read_u8()?,
            res_type: cur.read_u8()?,
            unknown_u16: cur.read_u16::<LittleEndian>()?,
            unknown_u32_1: cur.read_u32::<LittleEndian>()?,
            unknown_u32_2: cur.read_u32::<LittleEndian>()?,
            view_start: cur.read_u32::<LittleEndian>()?,
            view_size: cur.read_u32::<LittleEndian>()?,
        })
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
    data_ptr: u32,
    prim_type: D3DPrimitiveType,
    data_size: u32,
}

#[derive(Debug, Clone)]
pub struct NdPushBuffer {
    header: NdHeader,

    num_draws: u32,
    unknown_u32_1: u32,
    unknown_u32_2: u32,
    unknown_u32_3: u32,

    push_data_list_ptr: u32,
    primitive_types_list_ptr: u32,
    vertex_counts_list_ptr: u32,

    draw_calls: Vec<DrawCall>,

    prevent_culling_flag: u8,
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
