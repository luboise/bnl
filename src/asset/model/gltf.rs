use std::slice;

use base64::{Engine, prelude::BASE64_STANDARD};
use mod3d_base::{BufferElementType, VertexAttr};
use mod3d_gltf::{
    AccessorIndex, BufferIndex, Gltf, GltfAsset, GltfBuffer, GltfMesh, GltfNode, GltfPrimitive,
    GltfScene, Indexable, ViewIndex,
};

use crate::{
    VirtualResource,
    asset::{
        Asset, AssetDescription, AssetParseError,
        model::{
            ModelDescriptor,
            nd::{Nd, NdNode, VertexBufferViewType},
        },
    },
};

#[derive(Debug)]
pub struct GLTFModel {
    description: AssetDescription,
    descriptor: ModelDescriptor,
    // subresource_descriptors: Vec<ModelSubresourceDescriptor>,
    gltf: Gltf,
}

#[derive(Debug, Clone, Default)]
pub struct NdGltfContext {
    positions_accessor: Option<AccessorIndex>,
}

fn insert_nd_into_gltf(
    nd_node: &Nd,
    virtual_res: &VirtualResource,
    gltf: &mut Gltf,
    ctx: &mut NdGltfContext,
) -> Result<(), AssetParseError> {
    match nd_node {
        Nd::VertexBuffer(buf) => {
            let mut min = u32::MAX;
            let mut max = u32::MIN;

            // Get the size of the buffer
            buf.resource_views().iter().for_each(|view| {
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

            let b64_bytes = BASE64_STANDARD.encode(res_bytes);
            // .map_err(|e| AssetParseError::InvalidDataViews(e.to_string()))?;

            let gb = GltfBuffer::of_base64(b64_bytes);
            let index = gltf.add_buffer(gb);

            for res_view in buf.resource_views() {
                let bv_index = gltf.add_view(
                    index,
                    res_view.start() as usize,
                    res_view.len(),
                    Some(res_view.stride() as usize),
                );

                if res_view.res_type() == VertexBufferViewType::Vertex {
                    if ctx.positions_accessor.is_none() {
                        let accessor_index = gltf.add_accessor(
                            bv_index,
                            0,
                            res_view.len() as u32 / 12,
                            BufferElementType::Float32,
                            3,
                        );

                        ctx.positions_accessor = Some(accessor_index);
                    }
                }

                if let Err(e) = res_view.add_to_gltf(gltf, bv_index) {
                    eprintln!("Unable to add bv {} to gltf file.\nError: {}", bv_index, e);
                };
            }
        }
        Nd::PushBuffer(buf) => {
            let mut mesh = GltfMesh::default();

            let indices: Vec<u16> = (0u16..2000u16).collect();

            let index_buffer: Vec<u8> = indices.iter().flat_map(|val| val.to_le_bytes()).collect();

            let buffer_index =
                gltf.add_buffer(GltfBuffer::of_base64(BASE64_STANDARD.encode(&index_buffer)));

            let ib_view_index = gltf.add_view(buffer_index, 0, index_buffer.len(), Some(1));

            buf.draw_calls().iter().for_each(|draw_call| {
                let accessor_index = gltf.add_accessor(
                    ib_view_index,
                    draw_call.data_ptr - buf.push_buffer_base,
                    draw_call.data_size / 2,
                    mod3d_base::BufferElementType::UInt16,
                    1,
                );

                let primitive_index = mesh.add_primitive(
                    draw_call.prim_type.clone().into(),
                    Some(accessor_index),
                    None,
                );

                let primitives = mesh.primitives();

                let primitive_ptr: *mut GltfPrimitive = primitives.as_ptr() as *mut GltfPrimitive;

                unsafe {
                    (*primitive_ptr).add_attribute(VertexAttr::Position, accessor_index);
                }

                /*
                unsafe {
                    // Cast &[Primitive] -> *const Primitive -> *mut Primitive -> &mut [Primitive]
                    let mut primitives = slice::from_raw_parts_mut(
                        mesh.primitives().as_ptr() as *mut GltfPrimitive,
                        num_primitives,
                    );

                    primitives.get_mut(0).unwrap();
                }
                */
            });

            let mesh_index = gltf.add_mesh(mesh);

            let mut node = GltfNode::default();
            node.set_mesh(mesh_index);

            let _node_index = gltf.add_node(node);
        }
        Nd::Unknown(_val) => (),
    };

    let header = nd_node.header();

    if let Some(child) = header.first_child() {
        insert_nd_into_gltf(&child, virtual_res, gltf, ctx)?;
    }

    if let Some(next_sibling) = header.next_sibling() {
        insert_nd_into_gltf(&next_sibling, virtual_res, gltf, ctx)?;
    }

    Ok(())
}

impl Asset for GLTFModel {
    type Descriptor = ModelDescriptor;

    fn descriptor(&self) -> &Self::Descriptor {
        &self.descriptor
    }

    fn new(
        description: &AssetDescription,
        descriptor: &Self::Descriptor,
        virtual_res: &VirtualResource,
    ) -> Result<Self, AssetParseError> {
        let mut gltf = Gltf::default();
        gltf.set_asset(GltfAsset::new("Idk".to_string()));

        for (i, mesh_desc) in descriptor.mesh_descriptors.iter().enumerate() {
            let nodes = vec![];

            for nd in &mesh_desc.primitives {
                let mut ctx = NdGltfContext::default();

                insert_nd_into_gltf(nd, virtual_res, &mut gltf, &mut ctx)?;
            }

            gltf.add_scene(GltfScene {
                name: format!("{}_{}", description.name(), i + 1),
                nodes,
            });

            gltf.validate().map_err(|e| {
                AssetParseError::InvalidDataViews(format!(
                    "GLTF file was parsed, but could not validate correctly.\nError: {}",
                    e
                ))
            })?;
        }

        Ok(Self {
            description: description.clone(),
            descriptor: descriptor.clone(),
            gltf,
        })
    }

    fn description(&self) -> &AssetDescription {
        &self.description
    }

    fn as_bnl_asset(&self) -> crate::BNLAsset {
        todo!()
    }
}

impl GLTFModel {
    pub fn gltf(&self) -> &Gltf {
        &self.gltf
    }

    pub fn to_gltf_bytes(&self) -> serde_json::Result<Vec<u8>> {
        serde_json::to_vec_pretty(&self.gltf)
    }
}
