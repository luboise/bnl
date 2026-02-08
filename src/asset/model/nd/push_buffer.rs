use super::prelude::*;
use crate::d3d::D3DPrimitiveType;

#[derive(Debug, Clone, Serialize)]
pub struct DrawCall {
    pub(crate) data_ptr: u32,
    pub(crate) prim_type: D3DPrimitiveType,
    pub(crate) num_vertices: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NdPushBuffer {
    pub(crate) header: NdHeader,

    pub(crate) num_draws: u32,
    pub(crate) unknown_u32_1: u32,
    pub(crate) unknown_u32_2: u32,
    pub(crate) unknown_u32_3: u32,

    // File offsets
    pub(crate) data_pointers_start: u32,
    pub(crate) primitive_types_list_ptr: u32,
    pub(crate) vertex_counts_list_ptr: u32,

    pub(crate) prevent_culling_flag: u8,
    pub(crate) padding: [u8; 3],

    #[serde(skip_serializing)]
    pub(crate) buffer_bytes: Vec<u8>,

    pub(crate) push_buffer_base: u32,
    pub(crate) push_buffer_size: u32,

    pub(crate) draw_calls: Vec<DrawCall>,
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
        _virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        // let mut mesh = gltf::Mesh::new("Idk Mesh".to_string());

        let index_buffer: &Vec<u8> = &self.buffer_bytes;

        let buffer_index = ctx.gltf.add_buffer(gltf::Buffer::new(index_buffer));
        let ib_view_index = ctx.gltf.add_buffer_view(gltf::BufferView {
            buffer_index,
            byte_offset: 0,
            byte_length: index_buffer.len(),
            byte_stride: None,
            // 34963 -> ELEMENT_ARRAY_BUFFER
            target: Some(34963),
        });

        let mut primitives = Vec::new();

        println!("Adding {} draw calls.", self.draw_calls().len());

        self.draw_calls().iter().for_each(|draw_call| {
            let ib_accessor_index = ctx.gltf.add_accessor(gltf::Accessor::new(
                ib_view_index,
                (draw_call.data_ptr - self.push_buffer_base) as usize,
                gltf::AccessorDataType::U16,
                draw_call.num_vertices as usize,
                gltf::AccessorComponentCount::SCALAR,
            ));

            let mut primitive = gltf::Primitive {
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
            };

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

            if let Some(skin_accessor) = ctx.skin_accessor {
                primitive.set_attribute(gltf::VertexAttribute::Joints(0), skin_accessor);
            }

            if let Some(skin_weight_accessor) = ctx.skin_weight_accessor {
                primitive.set_attribute(gltf::VertexAttribute::Weights(0), skin_weight_accessor);
            }

            primitives.push(primitive);
        });

        let index = ctx.current_node_index().unwrap() as usize;

        let mesh: &mut gltf::Mesh = match ctx.gltf.meshes_mut().get_mut(index) {
            Some(val) => val,
            None => {
                let new_mesh = gltf::Mesh::new("New Mesh".to_string());
                let new_mesh_index = ctx.gltf.add_mesh(new_mesh);

                let new_node = gltf::Node::new(Some("Mesh Node".to_string()));
                let new_node_index = ctx.gltf.add_node(new_node);

                ctx.gltf
                    .nodes_mut()
                    .get_mut(new_node_index as usize)
                    .unwrap()
                    .set_mesh_index(Some(new_mesh_index));

                if let Some(skin_index) = ctx.current_skin {
                    ctx.gltf
                        .nodes_mut()
                        .get_mut(new_node_index as usize)
                        .unwrap()
                        .set_skin_index(Some(skin_index));
                }

                ctx.gltf
                    .meshes_mut()
                    .get_mut(new_mesh_index as usize)
                    .unwrap()
            }
        };

        for primitive in primitives {
            mesh.add_primitive(primitive);
        }

        Ok(None)

        // Ok(Some(ctx.gltf.add_node(node)))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct NdBGPushBuffer {
    pub(crate) push_buffer: NdPushBuffer,
    pub(crate) unknown_ptr_1: u32,
    pub(crate) unknown_ptr_2: u32,
}

impl NdBGPushBuffer {
    pub fn push_buffer(&self) -> &NdPushBuffer {
        &self.push_buffer
    }

    pub fn unknown_ptr_2(&self) -> u32 {
        self.unknown_ptr_2
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
