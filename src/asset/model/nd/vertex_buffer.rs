use gltf_writer::gltf::{Accessor, BufferView};

use super::prelude::*;

#[derive(Debug, Clone, Serialize)]
pub struct NdVertexBuffer {
    pub(crate) header: NdHeader,
    pub(crate) resource_views_ptr: u32,
    pub(crate) num_resource_views: u32,

    // DO NOT SERIALISE
    pub(crate) resource_views: Vec<VertexBufferResourceView>,
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

        let gb = gltf::Buffer::new(&res_bytes);
        let buffer_index = ctx.gltf.add_buffer(gb);

        for res_view in self.resource_views() {
            if res_view.is_empty() {
                continue;
            }

            let buffer_view_index = ctx.gltf.add_buffer_view(gltf::BufferView::new(
                buffer_index,
                res_view.start() as usize,
                res_view.len(),
                Some(res_view.stride() as usize),
                Some(34962),
            ));

            if res_view.res_type() == VertexBufferViewType::Vertex
                && ctx.positions_accessor.is_none()
            {
                let accessor_index = ctx.gltf.add_accessor(gltf::Accessor::new(
                    buffer_view_index,
                    0,
                    gltf::AccessorDataType::F32,
                    res_view.num_entries(),
                    gltf::AccessorComponentCount::VEC3,
                ));

                ctx.positions_accessor = Some(accessor_index);
            } else {
                match res_view.add_to_gltf(&mut ctx.gltf, &res_bytes, buffer_view_index) {
                    Ok(accessor_index) => {
                        if res_view.res_type() == VertexBufferViewType::UV
                            && ctx.uv_accessor.is_none()
                        {
                            /*
                            let accessor_index = ctx.gltf.add_accessor(gltf::Accessor::new(
                                buffer_view_index,
                                0,
                                gltf::AccessorDataType::F32,
                                res_view.len() / 8,
                                gltf::AccessorComponentCount::VEC2,
                            ));
                            */

                            ctx.uv_accessor = Some(accessor_index);
                        } else if res_view.res_type() == VertexBufferViewType::Skin {
                            ctx.skin_accessor = Some(accessor_index)
                        } else if res_view.res_type() == VertexBufferViewType::SkinWeight {
                            ctx.skin_weight_accessor = Some(accessor_index)
                        } else if res_view.res_type() == VertexBufferViewType::Normal {
                            ctx.normal_accessor = Some(accessor_index)
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Unable to add bv {} to gltf file.\nError: {}",
                            buffer_view_index, e
                        );
                    }
                };
            }
        }

        Ok(None)
    }
}

#[repr(u8)]
#[derive(Debug, PartialEq, Clone, Copy, Serialize)]
pub enum VertexBufferViewType {
    Skin = 0x0,
    SkinWeight = 0x8,
    Vertex = 0x9,
    Normal = 0xa,
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
            0xa => Self::Normal,
            0xb => Self::Unknown11,
            0xd => Self::UV,
            0xe => Self::Unknown14,
            0xf => Self::Unknown15,
            0x10 => Self::Unknown16,
            _ => Self::KnknownFF,
        }
    }
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
        gltf: &mut gltf::Gltf,
        buffer_bytes: &[u8],
        buffer_view_index: GltfIndex,
    ) -> Result<GltfIndex, std::io::Error> {
        match self.res_type {
            VertexBufferViewType::Vertex => {
                let num_vertices = self.view_size / 12;

                Ok(gltf.add_accessor(gltf::Accessor::new(
                    buffer_view_index,
                    // self.view_start as usize,
                    0,
                    gltf::AccessorDataType::F32,
                    num_vertices as usize,
                    gltf::AccessorComponentCount::VEC3,
                )))
            }
            VertexBufferViewType::UV => {
                let num_vertices = self.view_size / 8;

                let accessor_index = gltf.add_accessor(gltf::Accessor::new(
                    buffer_view_index,
                    // self.view_start as usize,
                    0,
                    gltf::AccessorDataType::F32,
                    num_vertices as usize,
                    gltf::AccessorComponentCount::VEC2,
                ));

                println!("Vertices accessor at index {accessor_index}");
                Ok(accessor_index)
            }

            // Skin must be converted to u8 or u16 stream for gltf according to specification
            //
            // "JOINTS_n: unsigned byte or unsigned short"
            // https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html#skinned-mesh-attributes
            VertexBufferViewType::Skin => {
                // Convert [f32, f32] to [u16, u16, u16, u16], padding extra missing indices with 0
                let u16_bytes = buffer_bytes
                    [self.view_start as usize..(self.view_start + self.view_size) as usize]
                    .chunks_exact(self.stride.into())
                    .flat_map(|chunk| {
                        let mut skin_indices = [0u16; 4];
                        chunk
                            .chunks_exact(4)
                            .take(4)
                            .enumerate()
                            .for_each(|(i, c)| {
                                // TODO: Remove this and actually figure out bone mapping
                                let val = f32::from_le_bytes(c.try_into().unwrap()) as u16;
                                skin_indices[i] = if val == 24 {
                                    1
                                } else if val == 27 {
                                    2
                                } else {
                                    0
                                };
                            });

                        skin_indices
                    })
                    .flat_map(|short| short.to_le_bytes())
                    .collect::<Vec<u8>>();

                let buf_index = gltf.add_buffer(gltf_writer::gltf::Buffer::new(&u16_bytes));
                let bv = gltf.add_buffer_view(BufferView::new(
                    buf_index,
                    0,
                    u16_bytes.len(),
                    None,
                    Some(34962),
                ));

                let accessor_index = gltf.add_accessor(Accessor::new(
                    bv,
                    0,
                    gltf::AccessorDataType::U16,
                    self.num_entries(),
                    gltf::AccessorComponentCount::VEC4,
                ));
                println!("Skin accessor at index {accessor_index}");
                Ok(accessor_index)
            }

            VertexBufferViewType::SkinWeight => {
                let skin_weight_bytes = buffer_bytes
                    [(self.view_start) as usize..(self.view_start + self.view_size) as usize]
                    .chunks_exact(8)
                    .flat_map(|chunk| {
                        let mut v = vec![0u8; 16];

                        v[0..8].copy_from_slice(chunk);
                        v
                    })
                    .collect::<Vec<_>>();

                let buf_index = gltf.add_buffer(gltf_writer::gltf::Buffer::new(&skin_weight_bytes));
                let bv = gltf.add_buffer_view(BufferView::new(
                    buf_index,
                    0,
                    skin_weight_bytes.len(),
                    None,
                    Some(34962),
                ));

                let accessor_index = gltf.add_accessor(Accessor::new(
                    bv,
                    0,
                    gltf::AccessorDataType::F32,
                    self.num_entries(),
                    gltf::AccessorComponentCount::VEC4,
                ));
                println!("Skin weights accessor at index {accessor_index}");
                Ok(accessor_index)
            }
            VertexBufferViewType::Normal => {
                let num_vertices = self.view_size / 12;

                let accessor_index = gltf.add_accessor(gltf::Accessor::new(
                    buffer_view_index,
                    0,
                    gltf::AccessorDataType::F32,
                    num_vertices as usize,
                    gltf::AccessorComponentCount::VEC3,
                ));

                println!("Vertices accessor at index {accessor_index}");
                Ok(accessor_index)
            }
            VertexBufferViewType::Unknown11
            | VertexBufferViewType::Unknown14
            | VertexBufferViewType::Unknown15
            | VertexBufferViewType::Unknown16
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

    /// Number of entries in this resource view
    /// Equal by length / stride
    pub fn num_entries(&self) -> usize {
        (self.view_size / u32::from(self.stride)) as usize
    }

    pub fn res_type(&self) -> VertexBufferViewType {
        self.res_type
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdVertexShader {
    pub(crate) header: NdHeader,
}

impl NdNode for NdVertexShader {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        let mesh_node_index = ctx
            .gltf
            .add_node(gltf::Node::new(Some("ndVertexShader".to_string())));

        Ok(Some(mesh_node_index))
    }
}
