use base64::{Engine, prelude::BASE64_STANDARD};
use mod3d_base::BufferElementType;
use mod3d_gltf::{BufferIndex, Gltf, GltfBuffer, GltfScene};

use crate::asset::{
    Asset, AssetDescription, AssetParseError,
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
                let mut buf_index: Option<BufferIndex> = None;

                match nd {
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

                        let b64_bytes = BASE64_STANDARD
                            .decode(res_bytes)
                            .map_err(|e| AssetParseError::InvalidDataViews(e.to_string()))?;

                        let gb = GltfBuffer::of_base64(b64_bytes);
                        let index = gltf.add_buffer(gb);

                        if buf_index.is_none() {
                            buf_index = Some(index);
                        }

                        for res_view in buf.resource_views() {
                            let bv_index = gltf.add_view(
                                buf_index.expect("Index is None."),
                                res_view.start() as usize,
                                res_view.len(),
                                Some(res_view.stride() as usize),
                            );
                            res_view.add_to_gltf(&mut gltf, bv_index)?;
                        }
                    }
                    Nd::PushBuffer(_buf) => {
                        println!("Pushbuffer to gltf not implemented");
                    }
                    Nd::Unknown(_val) => (),
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

    pub fn to_gltf_bytes(&self) -> serde_json::Result<Vec<u8>> {
        serde_json::to_vec(&self.gltf)
    }
}
