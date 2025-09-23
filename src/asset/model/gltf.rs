use std::path::{self, Path};

use gltf_writer::gltf::{
    self, Accessor, AccessorComponentCount, AccessorDataType, Buffer, BufferView, Gltf, GltfIndex,
    Mesh, Node, Primitive, VertexAttribute, serialisation::GltfExportType,
};

use crate::{
    VirtualResource,
    asset::{
        Asset, AssetDescription, AssetParseError, Dump, DumpToDir,
        model::{
            ModelDescriptor,
            nd::{Nd, NdNode, VertexBufferViewType},
        },
        texture::TextureData,
    },
};

#[derive(Debug)]
pub struct GLTFModel {
    description: AssetDescription,
    descriptor: ModelDescriptor,
    // subresource_descriptors: Vec<ModelSubresourceDescriptor>,
    gltf: Gltf,
}

impl GLTFModel {
    pub fn gltf(&self) -> &Gltf {
        &self.gltf
    }

    pub fn to_gltf_bytes(&self) -> serde_json::Result<Vec<u8>> {
        serde_json::to_vec_pretty(&self.gltf)
    }
}

impl DumpToDir for GLTFModel {
    fn dump_to_dir<P: AsRef<Path>>(&self, dump_dir: P) -> Result<(), std::io::Error> {
        self.dump(dump_dir.as_ref().join(format!("{}.gltf", self.name())))
    }
}

impl Dump for GLTFModel {
    fn dump<P: AsRef<Path>>(&self, dump_path: P) -> Result<(), std::io::Error> {
        let export_path = path::absolute(dump_path.as_ref())?;

        self.gltf
            .export(&export_path, GltfExportType::JSON)
            .map_err(|e| std::io::Error::other(format!("Error dumping GLTF model: {:?}", e)))?;

        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct NdGltfContext {
    gltf: Gltf,
    positions_accessor: Option<GltfIndex>,
    uv_accessor: Option<GltfIndex>,

    node_stack: Vec<GltfIndex>,
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

        // Load all textures first, because we need to assign them based on index
        for (i, tex_desc) in descriptor.texture_descriptors.iter().enumerate() {
            let image_bytes = virtual_res
                .get_bytes(
                    tex_desc.texture_offset() as usize,
                    tex_desc.texture_size() as usize,
                )
                .map_err(|e| AssetParseError::InvalidDataViews(e.to_string()))?;

            let tex_data = TextureData::new(tex_desc.clone(), image_bytes);
            let rgba_image = tex_data.to_rgba_image()?;

            let mut png = vec![];
            rgba_image
                .dump_png_bytes(&mut png)
                .map_err(|e| AssetParseError::InvalidDataViews(format!("{:?}", e)))?;

            let image_index = gltf.add_image(gltf::Image {
                uri: Some(format!("image{}.png", i)),
                data: png,
                name: format!("Image {}", i),
                // Empty values
                mime_type: None,
                buffer_view_index: None,
            });

            gltf.add_texture(gltf::Texture {
                image_index: Some(image_index),
                name: format!("texture{}", i),
            });
        }

        /*

            let material_index = gltf.add_material(Material {
                name: format!("material{}", i),
                pbr_metallic_roughness: Some(PBRMetallicRoughness {
                    base_color_texture: TextureInfo {
                        texture_index,
                        texcoords_accessor: todo!(),
                    },
                }),
            });
        */

        for (i, mesh_desc) in descriptor.mesh_descriptors.iter().enumerate() {
            let scene_name = format!("{}_{}", description.name(), i + 1);

            gltf.add_scene(gltf::Scene::new(scene_name));

            for nd in &mesh_desc.primitives {
                let mut ctx = NdGltfContext::default();

                insert_nd_into_gltf(nd, virtual_res, &mut gltf, &mut ctx)?;
            }

            /*
            gltf.validate().map_err(|e| {
                AssetParseError::InvalidDataViews(format!(
                    "GLTF file was parsed, but could not validate correctly.\nError: {}",
                    e
                ))
            })?;
            */
        }

        gltf.prepare_for_export()
            .map_err(|e| AssetParseError::InvalidDataViews(format!("{:?}", e)))?;

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

            let gb = Buffer::new(res_bytes);
            let index = gltf.add_buffer(gb);

            for res_view in buf.resource_views() {
                let buffer_view_index = gltf.add_buffer_view(BufferView::new(
                    index,
                    res_view.start() as usize,
                    res_view.len(),
                    Some(res_view.stride() as usize),
                    None,
                ));

                if res_view.res_type() == VertexBufferViewType::Vertex
                    && ctx.positions_accessor.is_none()
                {
                    let accessor_index = gltf.add_accessor(Accessor::new(
                        buffer_view_index,
                        0,
                        AccessorDataType::F32,
                        res_view.len() / 12,
                        AccessorComponentCount::VEC3,
                    ));

                    ctx.positions_accessor = Some(accessor_index);
                }

                if let Err(e) = res_view.add_to_gltf(gltf, buffer_view_index) {
                    eprintln!(
                        "Unable to add bv {} to gltf file.\nError: {}",
                        buffer_view_index, e
                    );
                } else if res_view.res_type() == VertexBufferViewType::UV
                    && ctx.uv_accessor.is_none()
                {
                    let accessor_index = gltf.add_accessor(Accessor::new(
                        buffer_view_index,
                        0,
                        AccessorDataType::F32,
                        res_view.len() / 8,
                        AccessorComponentCount::VEC2,
                    ));

                    ctx.uv_accessor = Some(accessor_index);
                }

                if let Err(e) = res_view.add_to_gltf(gltf, buffer_view_index) {
                    eprintln!(
                        "Unable to add bv {} to gltf file.\nError: {}",
                        buffer_view_index, e
                    );
                };
            }
        }
        Nd::PushBuffer(buf) => {
            let mut mesh = Mesh::new("Idk Mesh".to_string());

            let index_buffer: &Vec<u8> = &buf.buffer_bytes;

            let buffer_index = gltf.add_buffer(Buffer::new(index_buffer));
            let ib_view_index = gltf.add_buffer_view(BufferView {
                buffer_index,
                byte_offset: 0,
                byte_length: index_buffer.len(),
                byte_stride: None,
                target: None,
            });

            buf.draw_calls().iter().for_each(|draw_call| {
                let ib_accessor_index = gltf.add_accessor(Accessor::new(
                    ib_view_index,
                    (draw_call.data_ptr - buf.push_buffer_base) as usize,
                    AccessorDataType::U16,
                    draw_call.num_vertices as usize,
                    AccessorComponentCount::SCALAR,
                ));

                let primitive = mesh.add_primitive(Primitive {
                    indices_accessor: Some(ib_accessor_index),
                    topology_type: match draw_call.prim_type.clone().try_into() {
                        Ok(val) => Some(val),
                        Err(e) => {
                            eprintln!("{}", e);
                            None
                        }
                    },
                    attributes: Default::default(),
                });

                if let Some(positions_accessor) = ctx.positions_accessor {
                    primitive.set_attribute(VertexAttribute::Position, positions_accessor);
                } else {
                    eprintln!("No positions accessor available.");
                }

                if let Some(uv_accessor) = ctx.uv_accessor {
                    primitive.set_attribute(VertexAttribute::TexCoord(0), uv_accessor);
                } else {
                    eprintln!("No texcoords accessor available.");
                }
            });

            let mesh_index = gltf.add_mesh(mesh);

            let mut node = Node::new(Some("node name".to_string()));
            node.set_mesh_index(Some(mesh_index));

            let _node_index = gltf.add_node(node);
        }
        Nd::BGPushBuffer(bg_buf) => {
            let buf = bg_buf.push_buffer();
            let mut mesh = Mesh::new("Idk Mesh".to_string());

            let index_buffer: &Vec<u8> = &buf.buffer_bytes;

            let buffer_index = gltf.add_buffer(Buffer::new(index_buffer));
            let ib_view_index = gltf.add_buffer_view(BufferView {
                buffer_index,
                byte_offset: 0,
                byte_length: index_buffer.len(),
                byte_stride: None,
                target: None,
            });

            buf.draw_calls().iter().for_each(|draw_call| {
                let ib_accessor_index = gltf.add_accessor(Accessor::new(
                    ib_view_index,
                    (draw_call.data_ptr - buf.push_buffer_base) as usize,
                    AccessorDataType::U16,
                    draw_call.num_vertices as usize,
                    AccessorComponentCount::SCALAR,
                ));

                let primitive = mesh.add_primitive(Primitive {
                    indices_accessor: Some(ib_accessor_index),
                    topology_type: match draw_call.prim_type.clone().try_into() {
                        Ok(val) => Some(val),
                        Err(e) => {
                            eprintln!("{}", e);
                            None
                        }
                    },
                    attributes: Default::default(),
                });

                if let Some(positions_accessor) = ctx.positions_accessor {
                    primitive.set_attribute(VertexAttribute::Position, positions_accessor);
                } else {
                    eprintln!("No positions accessor available.");
                }
            });

            let mesh_index = gltf.add_mesh(mesh);

            let mut node = Node::new(Some("node name".to_string()));
            node.set_mesh_index(Some(mesh_index));

            let _node_index = gltf.add_node(node);
        }
        Nd::ShaderParam2(val) => {
            //
        }
        Nd::Group(_val) => {}
        Nd::Unknown(_val) => (),
    };

    let header = nd_node.header();

    if let Some(child) = header.first_child() {
        insert_nd_into_gltf(child, virtual_res, gltf, ctx)?;
    }

    if let Some(next_sibling) = header.next_sibling() {
        insert_nd_into_gltf(next_sibling, virtual_res, gltf, ctx)?;
    }

    Ok(())
}
