use binrw::{BinWriterExt};
use gltf_writer::GltfIndex;
use image::EncodableLayout;

use crate::asset::{
    AssetParseError,
    model::{
        Model,
        nd::{Nd, NdData, res_view::VertexBufferViewType},
    },
};

impl TryFrom<Model> for gltf_writer::Gltf {
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

        let mut gltf = Self::default();

        if let Some(textures_subresource) = textures_subresource {
            // Load all textures first, because we need to assign them based on index
            for (i, texture) in textures_subresource.textures.iter().enumerate() {
                let rgba_image = texture.to_rgba_image()?;

                let mut png = vec![];
                rgba_image
                    .dump_png_bytes(&mut png)
                    .map_err(|e| AssetParseError::InvalidDataViews(format!("{:?}", e)))?;

                let image_index = gltf.add_image(gltf_writer::Image {
                    uri: Some(format!("image{}.png", i)),
                    data: png,
                    name: format!("Image {}", i),
                    // Empty values
                    mime_type: None,
                    buffer_view_index: None,
                });

                gltf.add_texture(gltf_writer::Texture {
                    image_index: Some(image_index),
                    name: format!("image{}", i),
                });
            }
        }

        let current_material = (!gltf.textures().is_empty()).then(|| {
            gltf.add_material(gltf_writer::Material {
                name: "Default Material".to_owned(),
                pbr_metallic_roughness: Some(gltf_writer::PBRMetallicRoughness {
                    base_color_texture: Some(gltf_writer::TextureInfo {
                        texture_index: 0,
                        texcoords_accessor: None,
                    }),
                    ..Default::default()
                }),
            })
        });

        /*
        let default_material = {
            let base_color_texture = if let Some(bound_texture_index) =
                attrib.texture_assignment_index.0
                && let Some(bound_texture) = main_payload
                    .texture_assignments
                    .get(bound_texture_index as usize)
            {
            } else {
                eprintln!(
                    "Texture slot {:?} is referenced by an ndShaderParam, but the param only assigns {} slots.",
                    attrib.texture_assignment_index.0,
                    main_payload.texture_assignments.len()
                );

                None
            };

            gltf_writer::Material {
                name,
                pbr_metallic_roughness: Some(gltf_writer::PBRMetallicRoughness {
                    base_color_texture,
                    metallic_factor: Some(0.0),
                    ..Default::default()
                }),
            }
        };
        */

        let mut ctx = NdGltfContext {
            gltf,
            properties: model_subresource.properties,
            current_scene: 0,
            current_material,
            ..Default::default()
        };

        let mut scene = gltf_writer::Scene {
            name: "Main Scene".to_owned(),
            root_nodes: vec![],
        };

        for nd in model_subresource.nodes {
            let Some(new_index) = nd.create_gltf_node(&mut ctx)? else {
                return Err("Failed to add node into gltf heirarchy".into());
            };

            scene.add_node(new_index);
        }

        let NdGltfContext { mut gltf, .. } = ctx;

        let scene_index = gltf.add_scene(scene);
        gltf.set_default_scene(Some(scene_index));

        gltf.prepare_for_export().map_err(|e| {
            AssetParseError::InvalidDataViews(format!("failed to prepare gltf for export: {e:?}"))
        })?;
        Ok(gltf)
    }
}

#[derive(Debug, Clone, Default)]
pub struct NdGltfContext {
    pub(crate) current_node_name: Option<String>,
    pub(crate) properties: indexmap::IndexMap<String, Vec<u8>>,

    pub(crate) gltf: gltf_writer::Gltf,
    pub(crate) positions_accessor: Option<GltfIndex>,
    pub(crate) uv_accessor: Option<GltfIndex>,
    pub(crate) skin_accessor: Option<GltfIndex>,
    pub(crate) skin_weights_accessor: Option<GltfIndex>,
    pub(crate) normals_accessor: Option<GltfIndex>,
    pub(crate) colours_accessor: Option<GltfIndex>,

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

    pub fn pop_node(&mut self) -> Option<&mut gltf_writer::Node> {
        if let Some(popped) = self.node_stack.pop() {
            return Some(self.gltf.nodes_mut().get_mut(popped as usize).unwrap());
        }

        None
    }

    pub fn current_node(&mut self) -> Option<&mut gltf_writer::Node> {
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

    pub fn set_accessor(
        &mut self,
        view_type: VertexBufferViewType,
        index: GltfIndex,
    ) -> Result<Option<GltfIndex>, crate::Error> {
        Ok(match view_type {
            VertexBufferViewType::Skin => self.skin_accessor.replace(index),
            VertexBufferViewType::SkinWeight => self.skin_weights_accessor.replace(index),
            VertexBufferViewType::Position => self.positions_accessor.replace(index),
            VertexBufferViewType::Normal => self.normals_accessor.replace(index),
            VertexBufferViewType::Colour => self.colours_accessor.replace(index),
            VertexBufferViewType::UV => self.uv_accessor.replace(index),
            VertexBufferViewType::Unknown12
            | VertexBufferViewType::Unknown14
            | VertexBufferViewType::Unknown15
            | VertexBufferViewType::Unknown16
            | VertexBufferViewType::Unknown0x1b
            | VertexBufferViewType::Unknown0x1c
            | VertexBufferViewType::Unknown0x1d
            | VertexBufferViewType::Unknown0x1e 
            | VertexBufferViewType::Unknown0x1f
            | VertexBufferViewType::Unknown0x20 
            | VertexBufferViewType::Unknown0x21
            | VertexBufferViewType::Unknown0x22
            | VertexBufferViewType::KnknownFF => {
                return Err(format!("{view_type} accessor unimplemented").into());
            }
        })
    }
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
        let new_index_opt = {
            ctx.current_node_name = self.name.clone();

            let new_index = match &*self.data {
                NdData::Skeleton(data) => data.create_gltf_node(ctx),
                NdData::VertexBuffer(data) => data.create_gltf_node(ctx),
                NdData::PushBuffer(data) => data.create_gltf_node(ctx),
                NdData::BGPushBuffer(_data) => todo!(),
                NdData::ShaderParam2(data) => data.create_gltf_node(ctx).inspect_err(|e| {
                    eprintln!(
                        "failed to add material from ndShaderParam2 {}. unsetting current material: {e}",
                        self.name
                            .as_ref()
                            .map(|name| format!("({name})"))
                            .unwrap_or_default()
                    );

                    ctx.current_material = None;
                }),
                NdData::Shader2(_)
                | NdData::VertexShader(_)
                | NdData::MtxArray(_)
                | NdData::BlendShape(_) 
                | NdData::RigidSkin(_) => Ok(None),
            }?;

            if let Some(name) = &self.name
                && let Some(index) = new_index
            {
                let old_name = &mut ctx.gltf.nodes_mut().get_mut(index as usize).unwrap().name; 
                if let Some(old_name) = old_name.replace(name.clone()) {
                    println!("renaming {old_name} ({}) to {}", self.nd_type, name);
                }
            }

            new_index
        };

        // Exit early on bad material
        if self.nd_type() == crate::asset::model::nd::NdType::ShaderParam2
            && ctx.current_material.is_none()
        {
            eprintln!(
                "unable to add ndShaderParam2 {} with no material set. skipping this subtree.",
                self.name
                    .as_ref()
                    .map(|name| format!("({name})"))
                    .unwrap_or_default()
            );

            return Ok(new_index_opt);
        }

        if self.nd_type() == crate::asset::model::nd::NdType::BlendShape {
            return Ok(None)
        }

        let type_string = self.nd_type().to_string();

        let indentation = String::from_utf8(vec![b' '; 4 * ctx.node_stack.len()]).unwrap();

        // Push node, then handle child, then unpush node
        if let Some(node_index) = &new_index_opt {
            ctx.push_node(*node_index);

            println!(
                "{}Pushing {} {}, onto stack.",
                &indentation, type_string, node_index
            );
        }

        if let Some(child) = &self.first_child {
            child.create_gltf_node(ctx)?;
        }

        if let Some(node_index) = new_index_opt {
            ctx.pop_node();

            println!(
                "{}Removing {} {} from stack.",
                indentation, type_string, node_index
            );
        }

        if let Some(next_sibling) = &self.next_sibling {
            next_sibling.create_gltf_node(ctx)?;
        }

        Ok(new_index_opt)

    }
}

impl NdGltfAdd for crate::asset::model::nd::NdSkeletonData {
    fn create_gltf_node(&self, ctx: &mut NdGltfContext) -> Result<Option<GltfIndex>, crate::Error> {
        if ctx.current_skin.is_some() {
            return Err("no skin during pass".into());
        }

        let mut ret_index = None;

        // if we are at the root, create a new thing to parent the skeleton under and add it 
        // (need the mesh NEXT to the skeleton, not under it)
        if ctx.node_stack.is_empty() {
            let root = ctx.gltf.add_node(gltf_writer::Node::new(Some("Root".to_owned())));
            ret_index = Some(root);
        }

        let skeleton_index = ctx
            .gltf
            .add_node(gltf_writer::Node::new(Some("ndSkeleton".to_owned())));

        let ret_index = match ret_index {
            Some(root) => {
                ctx.gltf.node_mut(root).ok_or("no node")?.add_child(skeleton_index);
                root
            }
            None => skeleton_index
        };

        // TODO: Get this bone name from the properties instead?
        let root_bone_index = {
            let root = ctx
                .gltf
                .add_node(gltf_writer::Node::new(Some("BASE".to_string())));

            // add root to skeleton as
            ctx.gltf
                .node_mut(skeleton_index)
                .ok_or("no skeleton")?
                .add_child(root);

            root
        };

        let mut new_skin = gltf_writer::Skin::default();

        let mut inverse_bind_bytes = Vec::with_capacity(self.bones.len() * (4 * 16));
        let mut inverse_bind_cur = std::io::Cursor::new(&mut inverse_bind_bytes);

        let bone_names = ctx.properties.iter()
            .skip_while(|(k, v)| *k != "BASE")
            .take(self.bones.len())
            .map(|(k, v)| k)
            .collect::<Vec<_>>();


        for (i, bone) in self.bones.iter().enumerate() {
            // If bone doesn't match expected index
            if bone.id as usize != i {
                return Err(format!("Bone mismatch (expected {i}, got {})", bone.id).into());
            }

            if (bone.parent_id != u16::MAX)  // ignore root bone
                && bone.parent_id as usize >= new_skin.joints.len()
            {
                return Err("parent bone doesn't exist".into());
            }

            let name = bone_names.get(i).map(|v| String::from(*v)).unwrap_or_else(|| {
                if i == 0 {
                    "BASE".to_owned()
                } else {
                    // TODO: Put name on bone
                    format!("bone{i}")
                }
            });

            let mut bone_node = gltf_writer::Node::new(Some(name));
            bone_node.set_transform(Some(gltf_writer::NodeTransform::TRS(
                bone.local_transform,
                [0f32, 0f32, 0f32],
                [1f32, 1f32, 1f32],
            )));

            // Add the new child node (bone), and parent it to its parent
            let bone_index = ctx.gltf.add_node(bone_node);

            let parent_index = if bone.parent_id == u16::MAX {
                root_bone_index
            } else {
                *new_skin
                    .joints
                    .get(bone.parent_id as usize)
                    .ok_or_else(|| {
                        format!("failed to get node for bone's parent {}", bone.parent_id)
                    })?
            };

            let [tx, ty, tz] = bone.global_transform;

            #[rustfmt::skip]
            let inverse_bind_matrix = [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [-tx, -ty, -tz, 1.0],
            ];

            inverse_bind_cur.write_le(&inverse_bind_matrix)?;

            ctx.gltf
                .node_mut(parent_index)
                .ok_or(format!("failed to get parent node {parent_index}"))?
                .add_child(bone_index);

            new_skin.joints.push(bone_index);
        }

        assert_eq!(inverse_bind_bytes.len(), inverse_bind_bytes.capacity());

        let buffer_index = ctx.gltf.add_buffer(gltf_writer::Buffer::new(&inverse_bind_bytes));
        let bvi = ctx.gltf.add_buffer_view(gltf_writer::BufferView{
            buffer_index,
            byte_offset: 0, 
            byte_length: inverse_bind_bytes.len(),
            byte_stride: None,
            target: None
         });

        // TODO: Assert num inverse binds == num bones
        
        let ibm_accessor = ctx.gltf.add_accessor(gltf_writer::Accessor::new(bvi, 0, gltf_writer::AccessorDataType::F32, inverse_bind_bytes.len() / (4 * 16), gltf_writer::AccessorComponentCount::MAT4));

        new_skin.inverse_bind_matrices = Some(ibm_accessor);

        let new_skin_index = ctx.gltf.add_skin(new_skin);

        ctx.current_skin = Some(new_skin_index);

        Ok(Some(ret_index))
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

        let buffer_index = ctx.gltf.add_buffer(gltf_writer::Buffer::new(&index_buffer));
        let ib_view_index = ctx.gltf.add_buffer_view(gltf_writer::BufferView {
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
            let ib_accessor_index = ctx.gltf.add_accessor(gltf_writer::Accessor::new(
                ib_view_index,
                index_start * 2,
                gltf_writer::AccessorDataType::U16,
                draw_call.indices.len(),
                gltf_writer::AccessorComponentCount::SCALAR,
            ));

            index_start += draw_call.indices.len();

            let mut primitive = gltf_writer::Primitive {
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
                primitive.set_attribute(gltf_writer::VertexAttribute::Position, positions_accessor);
            } else {
                eprintln!("No positions accessor available.");
            }

            if let Some(uv_accessor) = ctx.uv_accessor {
                primitive.set_attribute(gltf_writer::VertexAttribute::TexCoord(0), uv_accessor);
            } else {
                eprintln!("No texcoords accessor available.");
            }

            if let Some(skin_accessor) = ctx.skin_accessor {
                primitive.set_attribute(gltf_writer::VertexAttribute::Joints(0), skin_accessor);
            }

            if let Some(skin_weight_accessor) = ctx.skin_weights_accessor {
                primitive.set_attribute(
                    gltf_writer::VertexAttribute::Weights(0),
                    skin_weight_accessor,
                );
            }

            if let Some(normal_accessor) = ctx.normals_accessor {
                primitive.set_attribute(gltf_writer::VertexAttribute::Normal, normal_accessor);
            } else {
                // eprintln!("No normals accessor available.");
            }

            primitives.push(primitive);
        }

        let index = ctx.current_node_index().unwrap() as usize;

        let mesh: &mut gltf_writer::Mesh = match ctx.gltf.meshes_mut().get_mut(index) {
            Some(val) => val,
            None => {
                let new_mesh_index = {
                    let new_mesh = gltf_writer::Mesh::new("New Mesh".to_owned());
                    ctx.gltf.add_mesh(new_mesh)
                };

                let new_node_index = {
                    let new_node = gltf_writer::Node::new(Some("Mesh Node".to_owned()));
                    ctx.gltf.add_node(new_node)
                };

                {
                    let node = ctx
                        .gltf
                        .nodes_mut()
                        .get_mut(new_node_index as usize)
                        .unwrap();

                    node.set_mesh_index(Some(new_mesh_index));

                    if let Some(skin_index) = ctx.current_skin.as_ref() {
                        node.set_skin_index(Some(*skin_index));
                    }
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
                Some(gltf_writer::TextureInfo {
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

            gltf_writer::Material {
                name,
                pbr_metallic_roughness: Some(gltf_writer::PBRMetallicRoughness {
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
        let buffer_i = ctx.gltf.add_buffer(gltf_writer::Buffer::new([]));

        for resource_view in &self.resource_views {
            if resource_view.is_empty() {
                continue;
            }

            let num_vertices = resource_view.len() / usize::from(resource_view.stride());

            let buffer_len = ctx.gltf.buffer_mut(buffer_i).ok_or("no buffer")?.data.len();

            let accessor_index = match resource_view.view_type {
                VertexBufferViewType::Position
                | VertexBufferViewType::Normal
                | VertexBufferViewType::UV
                | VertexBufferViewType::Colour => {
                    let component_count = match resource_view.view_type {
                        VertexBufferViewType::Position | VertexBufferViewType::Normal => {
                            gltf_writer::AccessorComponentCount::VEC3
                        }
                        VertexBufferViewType::UV => gltf_writer::AccessorComponentCount::VEC2,
                        VertexBufferViewType::Colour => gltf_writer::AccessorComponentCount::SCALAR,

                        VertexBufferViewType::Skin
                        | VertexBufferViewType::SkinWeight
                        | VertexBufferViewType::Unknown12
                        | VertexBufferViewType::Unknown14
                        | VertexBufferViewType::Unknown15
                        | VertexBufferViewType::Unknown16
                        | VertexBufferViewType::Unknown0x1b   
                        | VertexBufferViewType::Unknown0x1c   
                        | VertexBufferViewType::Unknown0x1d   
                        | VertexBufferViewType::Unknown0x1e   
                        | VertexBufferViewType::Unknown0x1f   
                        | VertexBufferViewType::Unknown0x20   
                        | VertexBufferViewType::Unknown0x21   
                        | VertexBufferViewType::Unknown0x22   
                        | VertexBufferViewType::KnknownFF => {
                            unreachable!()
                        }
                    };

                    let bvi = ctx.gltf.add_buffer_view(gltf_writer::BufferView::new(
                        buffer_i,
                        buffer_len,
                        resource_view.len(),
                        None,
                        // Some(resource_view.stride().into()),
                        None,
                    ));

                    // TODO: Force this to be little endina
                    ctx.gltf
                        .buffer_mut(buffer_i)
                        .ok_or("no buffer")?
                        .data
                        .extend_from_slice(resource_view.resource.as_bytes());

                    Some(ctx.gltf.add_accessor(gltf_writer::Accessor::new(
                        bvi,
                        0,
                        gltf_writer::AccessorDataType::F32,
                        num_vertices,
                        component_count,
                    )))
                }
                VertexBufferViewType::Skin => {
                    let mut resource = Vec::with_capacity(resource_view.len());

                    let mut cur = std::io::Cursor::new(&mut resource);

                    println!("warning: model assumed to have bones starting at VS register 24");

                    for [s1, s2] in resource_view.resource.as_chunks::<2>().0 {
                        cur.write_le(&((s1.round() as u16).saturating_sub(24)))?;
                        cur.write_le(&((s2.round() as u16).saturating_sub(24)))?;
                        cur.write_le(&[0u16; 2])?;
                    }

                    let bvi = ctx.gltf.add_buffer_view(gltf_writer::BufferView::new(
                        buffer_i,
                        buffer_len,
                        resource.len(),
                        None,
                        // Some(16),
                        None,
                    ));

                    ctx.gltf
                        .buffer_mut(buffer_i)
                        .ok_or("no buffer")?
                        .data
                        .extend_from_slice(&resource);

                    Some(ctx.gltf.add_accessor(gltf_writer::Accessor::new(
                        bvi,
                        0,
                        gltf_writer::AccessorDataType::U16,
                        num_vertices,
                        gltf_writer::AccessorComponentCount::VEC4,
                    )))
                }
                VertexBufferViewType::SkinWeight => {
                    let mut resource = Vec::with_capacity(resource_view.len());

                    let mut cur = std::io::Cursor::new(&mut resource);

                    for [f1, f2] in resource_view.resource.as_chunks::<2>().0 {
                        cur.write_le(f1)?;
                        cur.write_le(f2)?;
                        cur.write_le(&[0f32; 2])?;
                    }

                    let bvi = ctx.gltf.add_buffer_view(gltf_writer::BufferView::new(
                        buffer_i,
                        buffer_len,
                        resource.len(),
                        None,
                        // Some(16),
                        None,
                    ));

                    ctx.gltf
                        .buffer_mut(buffer_i)
                        .ok_or("no buffer")?
                        .data
                        .extend_from_slice(&resource);

                    Some(ctx.gltf.add_accessor(gltf_writer::Accessor::new(
                        bvi,
                        0,
                        gltf_writer::AccessorDataType::F32,
                        num_vertices,
                        gltf_writer::AccessorComponentCount::VEC4,
                    )))
                }
                VertexBufferViewType::Unknown12
                | VertexBufferViewType::Unknown14
                | VertexBufferViewType::Unknown15
                | VertexBufferViewType::Unknown16
                | VertexBufferViewType::Unknown0x1b 
                | VertexBufferViewType::Unknown0x1c
                | VertexBufferViewType::Unknown0x1d
                | VertexBufferViewType::Unknown0x1e
                | VertexBufferViewType::Unknown0x1f
                | VertexBufferViewType::Unknown0x20
                | VertexBufferViewType::Unknown0x21
                | VertexBufferViewType::Unknown0x22
                | VertexBufferViewType::KnknownFF => {
                    eprintln!(
                        "unimplemented res view {} for gltf",
                        resource_view.view_type
                    );

                    continue;
                }
            };

            if let Some(accessor_index) = accessor_index {
                ctx.set_accessor(resource_view.view_type, accessor_index)?;
            }
        }

        Ok(None)
    }
}
