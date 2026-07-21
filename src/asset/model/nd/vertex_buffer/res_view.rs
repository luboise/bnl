use std::io::{Read, Seek, SeekFrom};

use binrw::{BinReaderExt, BinWriterExt};
use serde::ser::SerializeMap;

#[derive(Debug, Clone)]
pub struct VertexBufferResourceView {
    pub stride: u8,
    pub view_type: VertexBufferViewType,
    pub unknown_u16: u16,

    pub unknown_u32_1: u32,

    // 0x8
    pub unknown_u32_2: u32,
    pub unknown_u32_3: u32,

    pub resource_start: u32,
    pub resource: Vec<f32>,
}

impl serde::Serialize for VertexBufferResourceView {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = s.serialize_map(None)?;

        let Self {
            stride: _,
            view_type,
            unknown_u16,
            unknown_u32_1,
            unknown_u32_2,
            unknown_u32_3,
            resource_start,
            resource,
        } = self;

        macro_rules! serialize_val {
            ($val:ident) => {
                map.serialize_entry(stringify!($val), $val)?
            };
        }

        serialize_val!(view_type);
        if !resource.is_empty() {
            serialize_val!(unknown_u16);
            serialize_val!(unknown_u32_1);
            serialize_val!(unknown_u32_2);
            serialize_val!(unknown_u32_3);
            serialize_val!(resource_start);
        }

        map.serialize_entry("resource", &format!("{} bytes", resource.len() * 4))?;

        map.end()
    }
}

impl binrw::BinRead for VertexBufferResourceView {
    type Args<'a> = crate::asset::model::ModelReadContext<'a>;

    fn read_options<R: std::io::prelude::Read + std::io::prelude::Seek>(
        reader: &mut R,
        _endian: binrw::Endian,
        mrc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        let stride: u8 = reader.read_le()?;
        let view_type = reader.read_le::<VertexBufferViewType>()?;

        if !view_type.is_valid_stride(stride) {
            return Err(binrw::Error::AssertFail {
                pos: reader.stream_position().unwrap_or(0),
                message: format!("stride {stride} is invalid for {view_type}"),
            });
        }

        let unknown_u16 = reader.read_le()?;
        let unknown_u32_1 = reader.read_le()?;
        let unknown_u32_2 = reader.read_le()?;
        let unknown_u32_3 = reader.read_le()?;

        let resource_start = reader.read_le::<u32>()?;
        let resource_size = reader.read_le::<u32>()?;

        let mut res_cur = std::io::Cursor::new(mrc.resource);

        res_cur.seek(SeekFrom::Start(resource_start.into()))?;

        let mut resource = vec![0u8; resource_size as usize];
        res_cur.read_exact(&mut resource)?;

        let resource = resource
            .as_chunks()
            .0
            .iter()
            .copied()
            .map(f32::from_le_bytes)
            .collect();

        Ok(Self {
            stride,
            view_type,
            unknown_u16,
            unknown_u32_1,
            unknown_u32_2,
            unknown_u32_3,
            resource_start,
            resource,
        })
    }
}

impl binrw::BinWrite for VertexBufferResourceView {
    type Args<'a> = crate::asset::model::ModelWriteContext;

    fn write_options<W: std::io::prelude::Write + Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        mwc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<()> {
        let Self {
            stride,
            view_type,
            unknown_u16,
            unknown_u32_1,
            unknown_u32_2,
            unknown_u32_3,
            resource_start,
            resource,
        } = self;

        let mut resource_start = *resource_start;

        if !view_type.is_valid_stride(*stride) {
            return Err(binrw::Error::AssertFail {
                pos: writer.stream_position().unwrap_or(0),
                message: format!("invalid stride {stride} for view_type {view_type}"),
            });
        }
        writer.write_le(&stride)?;
        writer.write_le(view_type)?;
        writer.write_le(unknown_u16)?;
        writer.write_le(unknown_u32_1)?;
        writer.write_le(unknown_u32_2)?;
        writer.write_le(unknown_u32_3)?;

        if !resource.is_empty() {
            let mut mwc = mwc.borrow_mut();

            let cur_start = mwc.resource.len() as u64;

            let mut res = std::io::Cursor::new(&mut mwc.resource);

            res.seek(SeekFrom::Start(cur_start))?;

            if u64::from(resource_start) != res.stream_position()? {
                // TODO: Print warning here
                resource_start = res.stream_position()? as u32;
                /*
                return Err(binrw::Error::AssertFail {
                    pos: res.stream_position().unwrap_or(0),
                    message: format!(
                        "unable to write resource at 0x{:x} (head is actually at 0x{:x})",
                        resource_start,
                        res.stream_position()?,
                    ),
                });
                */
            }

            res.write_le(resource)?;
        }

        writer.write_le(&resource_start)?;
        writer.write_le(&(self.len() as u32))?;

        Ok(())
    }
}

impl VertexBufferResourceView {
    pub fn from_reader<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        mrc: crate::asset::model::ModelReadContext<'_>,
    ) -> Result<Self, crate::Error> {
        Ok(reader.read_le_args(mrc)?)
    }

    /// The length of the resource in bytes
    pub fn len(&self) -> usize {
        4 * self.resource.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[deprecated(note = "field is public")]
    pub fn stride(&self) -> u8 {
        self.stride
    }

    pub fn start(&self) -> u32 {
        self.resource_start
    }

    pub fn end(&self) -> u32 {
        self.resource_start + self.resource.len() as u32
    }

    /// Number of entries in this resource view
    /// Equal by length / stride
    pub fn num_entries(&self) -> usize {
        (self.resource.len() as u32 / u32::from(self.stride / 4)) as usize
    }

    #[deprecated(note = "field is public")]
    pub fn view_type(&self) -> VertexBufferViewType {
        self.view_type
    }
}

#[repr(u8)]
#[derive(
    Debug,
    PartialEq,
    Clone,
    Copy,
    serde::Serialize,
    num_enum::IntoPrimitive,
    num_enum::TryFromPrimitive,
    strum::Display,
)]
#[binrw::binrw]
#[brw(repr = u8)]
pub enum VertexBufferViewType {
    Skin = 0x0,
    SkinWeight = 0x8,
    Position = 0x9,
    Normal = 0xa,
    Colour = 0xb,
    Unknown12 = 0xc,
    UV = 0xd,
    Unknown14 = 0xe,
    Unknown15 = 0xf,
    Unknown16 = 0x10,
    /// morph target 1?
    Unknown0x1b = 0x1b,
    /// morph target 2?
    Unknown0x1c = 0x1c,
    /// morph target 3?
    Unknown0x1d = 0x1d,
    /// morph target 4?
    Unknown0x1e = 0x1e,
    /// morph target 5?
    Unknown0x1f = 0x1f,
    /// morph target 6?
    Unknown0x20 = 0x20,
    /// morph target 6?
    Unknown0x21 = 0x21,
    /// morph target 7?
    Unknown0x22 = 0x22,
    KnknownFF = 0xff,
}

impl VertexBufferViewType {
    pub const fn is_valid_stride(&self, stride: u8) -> bool {
        match self {
            VertexBufferViewType::Position
            | VertexBufferViewType::Normal
            | VertexBufferViewType::Unknown0x1b
            | VertexBufferViewType::Unknown0x1c
            | VertexBufferViewType::Unknown0x1d
            | VertexBufferViewType::Unknown0x1e
            | VertexBufferViewType::Unknown0x1f
            | VertexBufferViewType::Unknown0x20
            | VertexBufferViewType::Unknown0x21
            | VertexBufferViewType::Unknown0x22 => stride == 0xc,
            VertexBufferViewType::Skin | VertexBufferViewType::SkinWeight => {
                stride == 0x4 || stride == 0x8
            }
            VertexBufferViewType::UV
            | VertexBufferViewType::Unknown14
            | VertexBufferViewType::Unknown16
            | VertexBufferViewType::Unknown15 => stride == 0x8,
            VertexBufferViewType::Unknown12 | VertexBufferViewType::Colour => stride == 0x4,
            VertexBufferViewType::KnknownFF => todo!(),
        }
    }
}

/// Marker trait for a vertex buffer resource view
pub trait VertexBufferView {
    type Data;
    const VIEW_TYPE: VertexBufferViewType;
}

macro_rules! impl_vertex_buffer_view_marker {
    ($t:ident, $v:ty) => {
        pub struct $t;
        impl VertexBufferView for $t {
            type Data = $v;
            const VIEW_TYPE: VertexBufferViewType = VertexBufferViewType::$t;
        }
    };
}

impl_vertex_buffer_view_marker!(Skin, Vec<[f32; 2]>);
impl_vertex_buffer_view_marker!(SkinWeight, Vec<[f32; 2]>);
impl_vertex_buffer_view_marker!(Position, Vec<[f32; 3]>);
impl_vertex_buffer_view_marker!(Normal, Vec<[f32; 3]>);
impl_vertex_buffer_view_marker!(Colour, Vec<f32>);

/*
pub struct VertexView;
impl VertexBufferView for VertexView {
    type Data = Vec<[f32; 3]>;
    const VIEW_TYPE: VertexBufferViewType = VertexBufferViewType::Vertex;
}

pub struct SkinView;
*/
