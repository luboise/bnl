use gltf_writer::gltf::{self, Gltf, GltfIndex};

use crate::asset::{
    AssetParseError,
    model::{
        Model,
        nd::{Nd, NdData, res_view::VertexBufferViewType},
    },
};

impl TryFrom<Model> for Gltf {
    type Error = crate::Error;

    fn try_from(value: Model) -> Result<Self, Self::Error> {
        let Model {
            flags,
            unknown_u32_1,
            unknown_u32_2,
            model_subresource,
            flags_subresource: _,
            subresource0x2: _,
            subresource0x3: _,
            subresource0x4: _,
            subresource0x5,
            collision_subresource: _,
            textures_subresource,
            subresource0x8: _,
            subresource0x9: _,
            transforms_subresource: _,
            subresource0xb: _,
            subresource0xc: _,
            subresource0xd: _,
            subresource0xe: _,
            subresource0xf: _,
            subresource0x10: _,
            subresource0x11: _,
            tiles_subresource: _,
            subresource0x13: _,
            subresource0x14: _,
            subresource0x15: _,
        } = value;

        let mut gltf = Gltf::default();

        if let Some(textures_subresource) = textures_subresource {
            // Load all textures first, because we need to assign them based on index
            for (i, texture) in textures_subresource.textures.iter().enumerate() {
                let rgba_image = texture.to_rgba_image()?;

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
                    name: format!("image{}", i),
                });
            }
        }

        let mut ctx = NdGltfContext {
            gltf,
            properties: model_subresource.properties,
            current_scene: 0,
            ..Default::default()
        };

        let mut scene = gltf::Scene {
            name: "Main Scene".to_owned(),
            root_nodes: vec![],
        };

        for nd in model_subresource.nodes {
            let Some(new_index) = insert_into_gltf_heirarchy(&nd, &mut ctx)? else {
                return Err("Failed to add node into gltf heirarchy".into());
            };

            scene.add_node(new_index);
        }

        let NdGltfContext { mut gltf, .. } = ctx;

        let scene_index = gltf.add_scene(scene);
        gltf.set_default_scene(Some(scene_index));

        gltf.prepare_for_export()
            .map_err(|e| AssetParseError::InvalidDataViews(format!("{:?}", e)))?;
        Ok(gltf)
    }
}

#[derive(Debug, Clone, Default)]
pub struct NdGltfContext {
    pub(crate) current_node_name: Option<String>,
    pub(crate) properties: indexmap::IndexMap<String, Vec<u8>>,

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

pub fn insert_into_gltf_heirarchy(
    nd: &Nd,
    ctx: &mut NdGltfContext,
) -> Result<Option<GltfIndex>, crate::Error> {
    let node_index_opt = nd.create_gltf_node(ctx)?;

    if nd.nd_type() == crate::asset::model::nd::NdType::ShaderParam2
        && ctx.current_material.is_none()
    {
        eprintln!(
            "failed to add material from ndShaderParam2 {}. skipping this subtree.",
            nd.name
                .as_ref()
                .map(|name| format!("({name})"))
                .unwrap_or_default()
        );
        return Ok(node_index_opt);
    }

    let type_string = nd.nd_type().to_string();

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
        insert_into_gltf_heirarchy(child, ctx)?;
    }

    if let Some(node_index) = node_index_opt {
        ctx.pop_node();

        println!(
            "{}Removing {} {} from stack.",
            indentation, type_string, node_index
        );
    }

    if let Some(next_sibling) = &nd.next_sibling {
        insert_into_gltf_heirarchy(next_sibling, ctx)?;
    }

    Ok(node_index_opt)
}

// TODO: Clean these up into one trait maybe?
pub trait NdNode {
    fn add_gltf_node(
        &self,
        virtual_res: &crate::VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError>;
}

pub trait NdGltfAdd {
    fn create_gltf_node(&self, ctx: &mut NdGltfContext) -> Result<Option<GltfIndex>, crate::Error>;
}

impl NdGltfAdd for Nd {
    fn create_gltf_node(&self, ctx: &mut NdGltfContext) -> Result<Option<GltfIndex>, crate::Error> {
        ctx.current_node_name = self.name.clone();

        let new_index = match self.data.as_ref() {
            NdData::Skeleton(data) => data.create_gltf_node(ctx),
            NdData::VertexBuffer(data) => data.create_gltf_node(ctx),
            NdData::PushBuffer(data) => data.create_gltf_node(ctx),
            NdData::BGPushBuffer(_data) => todo!(),
            NdData::ShaderParam2(data) => data.create_gltf_node(ctx),
            NdData::Shader2(_)
            | NdData::VertexShader(_)
            | NdData::MtxArray(_)
            | NdData::RigidSkin(_) => Ok(None),
        }?;

        if let Some(name) = &self.name
            && let Some(index) = new_index
        {
            ctx.gltf.nodes_mut().get_mut(index as usize).unwrap().name = Some(name.clone())
        }

        Ok(new_index)
    }
}

impl NdGltfAdd for crate::asset::model::nd::NdSkeletonData {
    fn create_gltf_node(&self, ctx: &mut NdGltfContext) -> Result<Option<GltfIndex>, crate::Error> {
        if ctx.current_skin.is_some() {
            return Err("no skin during pass".into());
        }

        let skeleton_index = ctx
            .gltf
            .add_node(gltf::Node::new(Some("ndSkeleton".to_owned())));

        let root_index = ctx.gltf.add_node(gltf::Node::new(Some("BASE".to_string())));

        let mut new_skin = gltf::Skin::default();
        new_skin.joints.push(root_index);

        for (i, bone) in self.bones.iter().enumerate().skip(1) {
            // If bone doesn't match expected index
            if bone.id as usize != i {
                return Err(format!("Bone mismatch (expected {i}, got {})", bone.id).into());
            }

            if bone.parent_id as usize >= new_skin.joints.len() {
                return Err("parent bone doesn't exist".into());
            }

            let mut bone_node = gltf::Node::new(Some(
                // TODO: Put name on bone
                format!("bone{i}"),
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
}

impl NdGltfAdd for crate::asset::model::nd::NdPushBufferData {
    fn create_gltf_node(&self, ctx: &mut NdGltfContext) -> Result<Option<GltfIndex>, crate::Error> {
        let index_buffer = self
            .draw_calls
            .iter()
            .flat_map(|draw_call| draw_call.indices.iter())
            .flat_map(|index| index.to_le_bytes())
            .collect::<Vec<u8>>();

        let buffer_index = ctx.gltf.add_buffer(gltf::Buffer::new(&index_buffer));
        let ib_view_index = ctx.gltf.add_buffer_view(gltf::BufferView {
            buffer_index,
            byte_offset: 0,
            byte_length: index_buffer.len(),
            byte_stride: None,
            // 34963 -> ELEMENT_ARRAY_BUFFER
            target: Some(34963),
        });

        let mut primitives = Vec::new();
        let mut index_start = 0;

        for draw_call in &self.draw_calls {
            let ib_accessor_index = ctx.gltf.add_accessor(gltf::Accessor::new(
                ib_view_index,
                index_start * 2,
                gltf::AccessorDataType::U16,
                draw_call.indices.len(),
                gltf::AccessorComponentCount::SCALAR,
            ));

            index_start += draw_call.indices.len();

            let mut primitive = gltf::Primitive {
                indices_accessor: Some(ib_accessor_index),
                topology_type: match draw_call.prim_type.try_into() {
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

            if let Some(normal_accessor) = ctx.normal_accessor {
                primitive.set_attribute(gltf::VertexAttribute::Normal, normal_accessor);
            } else {
                // eprintln!("No normals accessor available.");
            }

            primitives.push(primitive);
        }

        let index = ctx.current_node_index().unwrap() as usize;

        let mesh: &mut gltf::Mesh = match ctx.gltf.meshes_mut().get_mut(index) {
            Some(val) => val,
            None => {
                let new_mesh_index = {
                    let new_mesh = gltf::Mesh::new("New Mesh".to_owned());
                    ctx.gltf.add_mesh(new_mesh)
                };

                let new_node_index = {
                    let new_node = gltf::Node::new(Some("Mesh Node".to_owned()));
                    ctx.gltf.add_node(new_node)
                };

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

                if let Some(node) = ctx.current_node() {
                    node.add_child(new_node_index);
                } else {
                    return Err("node missing which was just inserted".into());
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

impl NdGltfAdd for crate::asset::model::nd::NdShaderParam2Data {
    fn create_gltf_node(&self, ctx: &mut NdGltfContext) -> Result<Option<GltfIndex>, crate::Error> {
        // if ctx
        //     .current_node_name
        //     .as_ref()
        //     .is_none_or(|v| v != "shader_lambert6")
        // {
        //     return Ok(None);
        // }

        let Self {
            main_payload,
            sub_payload,
            ..
        } = self;

        let attrib_key = "colour0";

        let Some(attrib) = &main_payload
            .param_assignments
            .assignments
            .iter()
            .find_map(|v| (str::from_utf8(&v.name.0).ok()? == attrib_key).then_some(v))
        else {
            ctx.current_material = None;
            return Ok(None);
        };

        main_payload
            .param_assignments
            .assignments
            .iter()
            .position(|p| p.name.0 == attrib_key.as_bytes())
            .expect("Unable to find index for key that was literally just found.");

        let material = {
            let name = ctx
                .current_node_name
                .clone()
                .unwrap_or_else(|| "Some Material".to_owned());

            let base_color_texture = if let Some(bound_texture_index) =
                attrib.texture_assignment_index.0
                && let Some(bound_texture) = main_payload
                    .texture_assignments
                    .get(bound_texture_index as usize)
            {
                Some(gltf::TextureInfo {
                    texture_index: bound_texture
                        .texture_index
                        .0
                        .ok_or("texture index not available for bound texture")?,
                    texcoords_accessor: None,
                })
            } else {
                eprintln!(
                    "Texture slot {:?} is referenced by an ndShaderParam, but the param only assigns {} slots.",
                    attrib.texture_assignment_index.0,
                    main_payload.texture_assignments.len()
                );

                None
            };

            gltf::Material {
                name,
                pbr_metallic_roughness: Some(gltf::PBRMetallicRoughness {
                    base_color_texture,
                    metallic_factor: Some(0.0),
                    ..Default::default()
                }),
            }
        };

        let material_index = ctx.gltf.add_material(material);
        ctx.current_material = Some(material_index);

        Ok(None)
    }
}

impl NdGltfAdd for crate::asset::model::nd::NdVertexBufferData {
    fn create_gltf_node(&self, ctx: &mut NdGltfContext) -> Result<Option<GltfIndex>, crate::Error> {
        // Get size of buffer
        let (min, max) =
            self.resource_views
                .iter()
                .fold((u32::MAX, u32::MIN), |(min, max), view| {
                    (
                        min.min(view.start()), //
                        max.max(view.end()),
                    )
                });

        let res_size = (max - min) as usize;

        let res_bytes = self
            .resource_views
            .iter()
            .flat_map(|res_view| res_view.resource.iter().cloned())
            .collect::<Vec<_>>();

        if res_bytes.len() != res_size {
            return Err(format!(
                "res size {} does not match expected size {res_size}",
                res_bytes.len()
            )
            .into());
        }

        let gb = gltf::Buffer::new(&res_bytes);
        let buffer_index = ctx.gltf.add_buffer(gb);

        for res_view in &self.resource_views {
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
