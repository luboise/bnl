use mod3d_gltf::{Gltf, GltfScene};

use crate::asset::{
    Asset, AssetDescription,
    model::{ModelDescriptor, nd::Nd},
};

#[derive(Debug)]
pub struct GLTFModel {
    description: AssetDescription,
    descriptor: ModelDescriptor,
    // subresource_descriptors: Vec<ModelSubresourceDescriptor>,
    gltf: Gltf,
}

impl Asset for GLTFModel {
    type Descriptor = ModelDescriptor;

    fn descriptor(&self) -> &Self::Descriptor {
        &self.descriptor
    }

    fn new(
        description: &AssetDescription,
        descriptor: &Self::Descriptor,
        virtual_res: &crate::VirtualResource,
    ) -> Result<Self, crate::asset::AssetParseError> {
        let mut gltf = Gltf::default();

        for (i, mesh_desc) in descriptor.mesh_descriptors.iter().enumerate() {
            let nodes = vec![];

            for nd in &mesh_desc.primitives {
                match nd {
                    Nd::VertexBuffer(buf) => for res_view in buf.resource_views() {},
                    Nd::PushBuffer(buf) => todo!(),
                    Nd::Other() => todo!(),
                };
            }

            gltf.add_scene(GltfScene {
                name: format!("{}_{}", description.name(), i + 1),
                nodes,
            });
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
}
