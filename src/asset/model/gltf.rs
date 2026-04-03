use std::{
    collections::HashMap,
    path::{self, Path},
};

use gltf_writer::gltf::{self, Gltf, GltfIndex, serialisation::GltfExportType};

use crate::{
    VirtualResource,
    asset::{
        AssetLike, AssetParseError, Dump,
        model::{
            ModelDescriptor,
            nd::{Nd, NdData, res_view::VertexBufferViewType},
        },
        texture::Texture,
    },
};

#[derive(Debug)]
pub struct GLTFModel {
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
    pub(crate) key_value_map: HashMap<String, Vec<u8>>,

    pub(crate) gltf: Gltf,
    pub(crate) positions_accessor: Option<GltfIndex>,
    pub(crate) uv_accessor: Option<GltfIndex>,
    pub(crate) skin_accessor: Option<GltfIndex>,
    pub(crate) skin_weight_accessor: Option<GltfIndex>,
    pub(crate) normal_accessor: Option<GltfIndex>,

    pub(crate) current_skin: Option<GltfIndex>,

    pub(crate) current_material: Option<GltfIndex>,
    pub(crate) current_scene: GltfIndex,

    pub(crate) node_stack: Vec<GltfIndex>,
}

impl NdGltfContext {
    pub fn push_node(&mut self, child_index: GltfIndex) {
        // If the scene is not empty, add the new one as a child
        if let Some(node) = self.current_node() {
            node.add_child(child_index);
        }

        self.node_stack.push(child_index);
    }

    pub fn pop_node(&mut self) -> Option<&mut gltf::Node> {
        if let Some(popped) = self.node_stack.pop() {
            return Some(self.gltf.nodes_mut().get_mut(popped as usize).unwrap());
        }

        None
    }

    pub fn current_node(&mut self) -> Option<&mut gltf::Node> {
        match self.node_stack.last() {
            Some(index) => self.gltf.nodes_mut().get_mut(*index as usize),
            None => None,
        }
    }

    pub fn current_node_index(&self) -> Option<GltfIndex> {
        if self.node_stack.is_empty() {
            return None;
        }

        Some(*self.node_stack.last().unwrap())
    }
}

impl AssetLike for GLTFModel {
    type Descriptor = ModelDescriptor;

    fn get_descriptor(&self) -> Self::Descriptor {
        self.descriptor.clone()
    }

    fn new(
        descriptor: &Self::Descriptor,
        virtual_res: &VirtualResource,
    ) -> Result<Self, AssetParseError> {
        let mut gltf = Gltf::default();

        // Load all textures first, because we need to assign them based on index
        for (i, tex_desc) in descriptor.texture_subresource.iter().enumerate() {
            let image_bytes = virtual_res
                .get_bytes(
                    tex_desc.texture_offset() as usize,
                    tex_desc.texture_size() as usize,
                )
                .map_err(|e| AssetParseError::InvalidDataViews(e.to_string()))?;

            let tex = Texture::new(tex_desc.clone(), image_bytes);
            let rgba_image = tex.to_rgba_image()?;

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

        let mut ctx = NdGltfContext {
            gltf,
            key_value_map: descriptor.key_value_map().cloned().unwrap_or_default(),
            ..Default::default()
        };

        for (i, mesh_desc) in descriptor.model_subresource.iter().enumerate() {
            let scene_name = format!("model_{}", i + 1);

            let mut scene = gltf::Scene::new(scene_name);

            ctx.current_scene = ctx.gltf.scenes().len() as u32;

            for nd in &mesh_desc.primitives {
                println!("FOUND ND");

                if let Some(new_index) = insert_into_gltf_heirarchy(nd, virtual_res, &mut ctx)? {
                    scene.add_node(new_index);
                }
            }
            ctx.gltf.add_scene(scene);
        }

        ctx.gltf
            .prepare_for_export()
            .map_err(|e| AssetParseError::InvalidDataViews(format!("{:?}", e)))?;

        Ok(Self {
            descriptor: descriptor.clone(),
            gltf: ctx.gltf,
        })
    }

    fn get_resource_chunks(&self) -> Option<Vec<Vec<u8>>> {
        // TODO: Create this function
        todo!();
    }
}

pub fn create_gltf_node(
    nd: &Nd,
    virtual_res: &VirtualResource,
    ctx: &mut NdGltfContext,
) -> Result<Option<GltfIndex>, AssetParseError> {
    match nd.data.as_ref() {
        NdData::Skeleton { bones } => {
            if ctx.current_skin.is_some() {
                return Err(AssetParseError::ErrorParsingDescriptor);
            }

            let skeleton_index = ctx
                .gltf
                .add_node(gltf::Node::new(Some(nd.nd_type().to_string())));

            let root_index = ctx.gltf.add_node(gltf::Node::new(Some("BASE".to_string())));

            let mut new_skin = gltf::Skin::default();
            new_skin.joints.push(root_index);

            for (i, bone) in bones.iter().enumerate().skip(1) {
                // If bone doesn't match expected index
                if bone.id as usize != i {
                    return Err(AssetParseError::InvalidDataViews(format!(
                        "Bone mismatch (expected {i}, got {})",
                        bone.id
                    )));
                }

                // If the parent doesn't exist
                if bone.parent_id as usize >= new_skin.joints.len() {
                    return Err(AssetParseError::ErrorParsingDescriptor);
                }

                let mut bone_node = gltf::Node::new(Some(
                    bone.name.clone().unwrap_or(format!("unnamed_joint_{i}")),
                ));
                bone_node.set_transform(Some(gltf::NodeTransform::TRS(
                    bone.local_transform,
                    [0f32, 0f32, 0f32],
                    [1f32, 1f32, 1f32],
                )));

                // Add the new child node (bone), and parent it to its parent
                let bone_index = ctx.gltf.add_node(bone_node);
                ctx.gltf
                    .nodes_mut()
                    .get_mut(
                        new_skin
                            .joints
                            .get(bone.parent_id as usize)
                            .cloned()
                            .ok_or(AssetParseError::ErrorParsingDescriptor)?
                            as usize,
                    )
                    .ok_or(AssetParseError::ErrorParsingDescriptor)?
                    .add_child(bone_index);

                new_skin.joints.push(bone_index);
            }

            let new_skin_index = ctx.gltf.add_skin(new_skin);

            ctx.current_skin = Some(new_skin_index);

            Ok(Some(skeleton_index))
        }
        NdData::VertexBuffer {
            resource_views_ptr: _,
            num_resource_views: _,
            resource_views,
        } => {
            // Get size of buffer
            let (min, max) =
                resource_views
                    .iter()
                    .fold((u32::MAX, u32::MIN), |(min, max), view| {
                        (
                            min.min(view.start()), //
                            max.max(view.end()),
                        )
                    });

            let res_size = (max - min) as usize;

            let res_bytes = virtual_res
                .get_bytes(min as usize, res_size)
                .map_err(|e| AssetParseError::InvalidDataViews(e.to_string()))?;

            let gb = gltf::Buffer::new(&res_bytes);
            let buffer_index = ctx.gltf.add_buffer(gb);

            for res_view in resource_views {
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
        NdData::PushBuffer(nd_push_buffer_data) => {
            nd_push_buffer_data.create_gltf_node(virtual_res, ctx)

            // TODO: Figure out whats up with these
            // insert_into_gltf_heirarchy(nd, virtual_res, ctx)
            // push_buffer.insert_into_gltf_heirarchy(virtual_res, ctx)
        }
        NdData::BGPushBuffer {
            push_buffer,
            unknown_ptr_1: _,
            unknown_ptr_2: _,
        } => {
            push_buffer.create_gltf_node(virtual_res, ctx)
            // insert_into_gltf_heirarchy(nd, virtual_res, ctx)
            // push_buffer.insert_into_gltf_heirarchy(virtual_res, ctx)
        }

        NdData::ShaderParam2 {
            main_payload,
            sub_payload: _,
        } => {
            let main_attribute_map = main_payload.attribute_map();

            let attrib_key = "colour0";

            if let Some(attrib) = main_attribute_map.get(attrib_key) {
                main_attribute_map
                    .get_index_of(attrib_key)
                    .expect("Unable to find index for key that was literally just found.");

                let texture_slot = attrib.val2;

                match main_payload
                    .texture_assignments()
                    .get(texture_slot as usize)
                {
                    Some(tex_assignment) => {
                        let material_index = ctx.gltf.add_material(gltf::Material {
                            name: "Some Material".to_string(),
                            pbr_metallic_roughness: Some(gltf::PBRMetallicRoughness {
                                base_color_texture: Some(gltf::TextureInfo {
                                    texture_index: tex_assignment.texture_index,
                                    texcoords_accessor: None,
                                }),
                                metallic_factor: Some(0.0),
                                ..Default::default()
                            }),
                        });

                        ctx.current_material = Some(material_index);
                    }
                    None => eprintln!(
                        "Texture slot {} is referenced by an ndShaderParam, but the param only assigns {} slots.",
                        texture_slot + 1,
                        main_payload.texture_assignments().len()
                    ),
                };
            } else {
                ctx.current_material = None;
            }

            Ok(Some(ctx.gltf.add_node(gltf::Node::new(Some(
                "ndShaderParam2".to_string(),
            )))))
        }
        NdData::Group | NdData::Shader2 | NdData::VertexShader | NdData::Unknown(..) => {
            let mesh_node_index = ctx
                .gltf
                .add_node(gltf::Node::new(Some(nd.nd_type().to_string())));

            Ok(Some(mesh_node_index))
        }
    }
}

pub fn insert_into_gltf_heirarchy(
    nd: &Nd,
    virtual_res: &VirtualResource,
    ctx: &mut NdGltfContext,
) -> Result<Option<GltfIndex>, AssetParseError> {
    let node_index_opt = create_gltf_node(nd, virtual_res, ctx)?;

    let type_string = nd.nd_type().to_string();

    /*
    let mut parent = GltfIndex::MAX;
    let mut grandparent: Option<GltfIndex> = Some(GltfIndex::MAX);
    */

    let indentation = String::from_utf8(vec![b' '; 4 * ctx.node_stack.len()]).unwrap();

    // Push node, then handle child, then unpush node
    if let Some(node_index) = &node_index_opt {
        ctx.push_node(*node_index);

        println!(
            "{}Pushing {} {}, onto stack.",
            &indentation, type_string, node_index
        );
    }

    if let Some(child) = &nd.first_child {
        insert_into_gltf_heirarchy(child, virtual_res, ctx)?;
    }

    if let Some(node_index) = node_index_opt {
        ctx.pop_node();

        println!(
            "{}Removing {} {} from stack.",
            indentation, type_string, node_index
        );
    }

    if let Some(next_sibling) = &nd.next_sibling {
        insert_into_gltf_heirarchy(next_sibling, virtual_res, ctx)?;
    }

    Ok(node_index_opt)
}
