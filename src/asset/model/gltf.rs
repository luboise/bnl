use std::path::{self, Path};

use gltf_writer::gltf::{self, Gltf, GltfIndex, serialisation::GltfExportType};

use crate::{
    VirtualResource,
    asset::{
        Asset, AssetDescription, AssetParseError, Dump, DumpToDir,
        model::{ModelDescriptor, nd::NdNode},
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
    pub(crate) gltf: Gltf,
    pub(crate) positions_accessor: Option<GltfIndex>,
    pub(crate) uv_accessor: Option<GltfIndex>,

    pub(crate) current_material: Option<GltfIndex>,
    pub(crate) current_scene: GltfIndex,

    pub(crate) node_stack: Vec<GltfIndex>,
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

        let mut ctx = NdGltfContext {
            gltf,
            ..Default::default()
        };

        for (i, mesh_desc) in descriptor.mesh_descriptors.iter().enumerate() {
            let scene_name = format!("{}_{}", description.name(), i + 1);

            let mut scene = gltf::Scene::new(scene_name);

            ctx.current_scene = ctx.gltf.scenes().len() as u32;

            for nd in &mesh_desc.primitives {
                if let Some(new_index) = nd.insert_into_gltf_heirarchy(virtual_res, &mut ctx)? {
                    scene.add_node(new_index);
                }
            }
            ctx.gltf.add_scene(scene);
        }

        ctx.gltf
            .prepare_for_export()
            .map_err(|e| AssetParseError::InvalidDataViews(format!("{:?}", e)))?;

        Ok(Self {
            description: description.clone(),
            descriptor: descriptor.clone(),
            gltf: ctx.gltf,
        })
    }

    fn description(&self) -> &AssetDescription {
        &self.description
    }

    fn as_bnl_asset(&self) -> crate::BNLAsset {
        todo!()
    }
}

/*
fn insert_nd_into_gltf(
    nd_node: &Nd,
    virtual_res: &VirtualResource,
    ctx: &mut NdGltfContext,
) -> Result<Option<GltfIndex>, AssetParseError> {
    let mut node_index = None;

    match nd_node {
        Nd::VertexBuffer(buf) => {}
        Nd::PushBuffer(buf) => {}
        Nd::BGPushBuffer(bg_buf) => {}
        Nd::ShaderParam2(val) => {}
        Nd::Group(_val) => {}
        Nd::Unknown(_val) => (),
    };

    if let Some(index) = node_index {
        ctx.node_stack.push(index);
    }

    let header = nd_node.header();

    // If has child, get child
    if let Some(child) = header.first_child() {
        insert_nd_into_gltf(child, virtual_res, ctx)?;
    }

    /*
    if let Some(new_child) = new_node.take() {
        // Insert into self or parent
        if let Some(ni) = node_index {
            ctx.gltf
                .nodes_mut()
                .get_mut(ni as usize)
                .unwrap()
                .add_child(new_child);
        } else {
            match &ctx.node_stack.last() {
                Some(last) => ctx
                    .gltf
                    .nodes_mut()
                    .get_mut(**last as usize)
                    .unwrap()
                    .add_child(new_child),
                None => ctx
                    .gltf
                    .scenes_mut()
                    .get_mut(ctx.current_scene as usize)
                    .unwrap()
                    .add_node(new_child),
            };
        }
    }


    */

    if let Some(next_sibling) = header.next_sibling() {
        insert_nd_into_gltf(next_sibling, virtual_res, ctx)?;
    }

    // Pop self from ctx when done processing
    if node_index.is_some() {
        ctx.node_stack.pop();
    }

    Ok(node_index)
}
*/
