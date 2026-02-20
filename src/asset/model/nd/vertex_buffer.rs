use gltf_writer::gltf::{Accessor, BufferView};
use res_view::VertexBufferViewType;

use super::prelude::*;

pub mod res_view;

#[derive(Debug, Clone, Serialize)]
pub struct NdVertexBuffer {
    pub(crate) header: NdHeader,
    pub(crate) resource_views_ptr: u32,
    pub(crate) num_resource_views: u32,

    // DO NOT SERIALISE
    pub(crate) resource_views: Vec<res_view::VertexBufferResourceView>,
}

impl NdVertexBuffer {
    pub fn resource_views(&self) -> &[res_view::VertexBufferResourceView] {
        &self.resource_views
    }

    // TODO: Bounds checking on resource views
    pub fn get_positions(&self, resource: &[u8]) -> Option<Vec<[f32; 3]>> {
        self.resource_views.iter().find_map(|view| {
            (view.view_type() == res_view::VertexBufferViewType::Vertex).then(|| {
                resource[view.start() as usize..view.end() as usize]
                    .chunks_exact(12)
                    .map(|chunk| {
                        [
                            f32::from_le_bytes(chunk[0..4].try_into().unwrap()),
                            f32::from_le_bytes(chunk[4..8].try_into().unwrap()),
                            f32::from_le_bytes(chunk[8..12].try_into().unwrap()),
                        ]
                    })
                    .collect()
            })
        })
    }

    fn get_resource<V: res_view::VertexBufferView>(&self, resource: &[u8]) -> Option<Vec<f32>> {
        let view = self
            .resource_views
            .iter()
            .find(|view| view.view_type() == V::VIEW_TYPE)?;

        if view.len() > resource.len() || view.start() as usize + view.len() > resource.len() {
            return None;
        }

        Some(
            resource[view.start() as usize..view.start() as usize + view.len()]
                .to_owned()
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
                .collect(),
        )
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

            if res_view.view_type() == VertexBufferViewType::Vertex
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
                match res_view.add_to_gltf(&mut ctx.gltf, buffer_view_index) {
                    Ok(accessor_index) => {
                        if res_view.view_type() == VertexBufferViewType::UV
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
                        } else if res_view.view_type() == VertexBufferViewType::Skin {
                            ctx.skin_accessor = Some(accessor_index)
                        } else if res_view.view_type() == VertexBufferViewType::SkinWeight {
                            ctx.skin_weight_accessor = Some(accessor_index)
                        } /*
                        else if res_view.view_type() == VertexBufferViewType::Normal {
                        ctx.normal_accessor = Some(accessor_index)
                        }
                         */
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
