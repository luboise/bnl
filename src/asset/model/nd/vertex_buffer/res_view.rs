use byteorder::{LittleEndian, ReadBytesExt as _};
use gltf_writer::gltf::GltfIndex;

#[derive(Debug, Clone, serde::Serialize)]
pub struct VertexBufferResourceView {
    stride: u8,
    view_type: VertexBufferViewType,
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
    pub fn from_cursor(cur: &mut std::io::Cursor<&[u8]>) -> Result<Self, std::io::Error> {
        Ok(VertexBufferResourceView {
            stride: cur.read_u8()?,
            view_type: cur.read_u8()?.into(),
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
        gltf: &mut gltf_writer::gltf::Gltf,
        buffer_view_index: GltfIndex,
    ) -> Result<GltfIndex, std::io::Error> {
        match self.view_type {
            VertexBufferViewType::Vertex => {
                let num_vertices = self.view_size / 12;

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
                let num_vertices = self.view_size / 8;

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

    /// Number of entries in this resource view
    /// Equal by length / stride
    pub fn num_entries(&self) -> usize {
        (self.view_size / u32::from(self.stride)) as usize
    }

    pub fn view_type(&self) -> VertexBufferViewType {
        self.view_type
    }
}

#[repr(u8)]
#[derive(Debug, PartialEq, Clone, Copy, serde::Serialize)]
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
