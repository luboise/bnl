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

        let gb = gltf::Buffer::new(res_bytes);
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
                None,
            ));

            match res_view.res_type {
                VertexBufferViewType::Vertex => {
                    let num_vertices = res_view.view_size / 12;

                    let accessor_index = ctx.gltf.add_accessor(gltf::Accessor::new(
                        buffer_view_index,
                        res_view.view_start as usize,
                        gltf::AccessorDataType::F32,
                        num_vertices as usize,
                        gltf::AccessorComponentCount::VEC3,
                    ));

                    ctx.positions_accessor = Some(accessor_index);
                }
                VertexBufferViewType::UV => {
                    let num_vertices = res_view.view_size / 8;

                    let accessor_index = ctx.gltf.add_accessor(gltf::Accessor::new(
                        buffer_view_index,
                        // res_view.view_start as usize,
                        0,
                        gltf::AccessorDataType::F32,
                        num_vertices as usize,
                        gltf::AccessorComponentCount::VEC2,
                    ));

                    if ctx.uv_accessor.is_none() {
                        ctx.uv_accessor = Some(accessor_index);
                    }
                }
                VertexBufferViewType::Unknown10
                | VertexBufferViewType::Unknown11
                | VertexBufferViewType::SkinWeight
                | VertexBufferViewType::Unknown14
                | VertexBufferViewType::Unknown15
                | VertexBufferViewType::Unknown16
                | VertexBufferViewType::Skin
                | VertexBufferViewType::KnknownFF => {
                    eprintln!(
                        "Unable to add bv {} to gltf file.\nError: {}",
                        buffer_view_index,
                        format!(
                            "VertexBufferViewType {:?} not implemented.",
                            res_view.res_type
                        )
                    );
                }
            };
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
