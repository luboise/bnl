use std::io::{Read, Seek, SeekFrom};

use binrw::{BinReaderExt, BinWriterExt};

#[derive(Debug, Clone, serde::Serialize)]
pub struct VertexBufferResourceView {
    pub stride: u8,
    pub view_type: VertexBufferViewType,
    pub unknown_u16: u16,

    pub unknown_u32_1: u32,

    // 0x8
    pub unknown_u32_2: u32,
    pub unknown_u32_3: u32,

    resource_start: u32,
    pub resource: Vec<u8>,
}

impl binrw::BinRead for VertexBufferResourceView {
    type Args<'a> = crate::asset::model::nd::ModelReadContext<'a>;

    fn read_options<R: std::io::prelude::Read + std::io::prelude::Seek>(
        reader: &mut R,
        _endian: binrw::Endian,
        mrc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        let stride = reader.read_le()?;
        let view_type = reader.read_le()?;
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
    type Args<'a> = crate::asset::model::nd::ModelWriteContext;

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

        writer.write_le(stride)?;
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

            if u64::from(*resource_start) != res.stream_position()? {
                return Err(binrw::Error::AssertFail {
                    pos: res.stream_position().unwrap_or(0),
                    message: format!(
                        "unable to write resource at 0x{:x} (head is actually at 0x{:x})",
                        resource_start,
                        res.stream_position()?,
                    ),
                });
            }

            res.write_le(resource)?;
        }

        writer.write_le(resource_start)?;
        writer.write_le(&(resource.len() as u32))?;

        Ok(())
    }
}

impl VertexBufferResourceView {
    pub fn from_reader<R: std::io::Read + std::io::Seek>(
        reader: &mut R,
        mrc: crate::asset::model::nd::ModelReadContext<'_>,
    ) -> Result<Self, crate::Error> {
        Ok(reader.read_le_args(mrc)?)
    }

    pub fn add_to_gltf(
        &self,
        gltf: &mut gltf_writer::gltf::Gltf,
        buffer_view_index: gltf_writer::GltfIndex,
    ) -> Result<gltf_writer::GltfIndex, std::io::Error> {
        match self.view_type {
            VertexBufferViewType::Vertex => {
                let num_vertices = self.resource.len() / 12;

                Ok(gltf.add_accessor(gltf_writer::gltf::Accessor::new(
                    buffer_view_index,
                    // self.view_start as usize,
                    0,
                    gltf_writer::gltf::AccessorDataType::F32,
                    num_vertices as usize,
                    gltf_writer::gltf::AccessorComponentCount::VEC3,
                )))
            }
            VertexBufferViewType::UV => {
                let num_vertices = self.resource.len() / 8;

                Ok(gltf.add_accessor(gltf_writer::gltf::Accessor::new(
                    buffer_view_index,
                    // self.view_start as usize,
                    0,
                    gltf_writer::gltf::AccessorDataType::F32,
                    num_vertices as usize,
                    gltf_writer::gltf::AccessorComponentCount::VEC2,
                )))
            }
            VertexBufferViewType::Unknown10
            | VertexBufferViewType::Unknown11
            | VertexBufferViewType::Unknown12
            | VertexBufferViewType::SkinWeight
            | VertexBufferViewType::Unknown14
            | VertexBufferViewType::Unknown15
            | VertexBufferViewType::Unknown16
            | VertexBufferViewType::Skin
            | VertexBufferViewType::KnknownFF => Err(std::io::Error::other(format!(
                "VertexBufferViewType {:?} not implemented.",
                self.view_type
            ))),
        }
    }

    pub fn len(&self) -> usize {
        self.resource.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

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
        (self.resource.len() as u32 / u32::from(self.stride)) as usize
    }

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
)]
#[binrw::binrw]
#[brw(repr = u8)]
pub enum VertexBufferViewType {
    Skin = 0x0,
    SkinWeight = 0x8,
    Vertex = 0x9,
    Unknown10 = 0xa,
    Unknown11 = 0xb,
    Unknown12 = 0xc,
    UV = 0xd,
    Unknown14 = 0xe,
    Unknown15 = 0xf,
    Unknown16 = 0x10,
    KnknownFF = 0xff,
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

impl_vertex_buffer_view_marker!(Vertex, Vec<[f32; 3]>);
impl_vertex_buffer_view_marker!(Skin, Vec<[f32; 2]>);
impl_vertex_buffer_view_marker!(SkinWeight, Vec<[f32; 2]>);

/*
pub struct VertexView;
impl VertexBufferView for VertexView {
    type Data = Vec<[f32; 3]>;
    const VIEW_TYPE: VertexBufferViewType = VertexBufferViewType::Vertex;
}

pub struct SkinView;
*/
