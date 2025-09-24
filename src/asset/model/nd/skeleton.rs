use super::prelude::*;

#[derive(Debug, Clone, Serialize)]
pub struct NdSkeleton {
    pub(crate) header: NdHeader,
}

impl NdNode for NdSkeleton {
    fn header(&self) -> &NdHeader {
        &self.header
    }

    fn add_gltf_node(
        &self,
        _virtual_res: &VirtualResource,
        ctx: &mut NdGltfContext,
    ) -> Result<Option<GltfIndex>, AssetParseError> {
        let node_index = ctx
            .gltf
            .add_node(gltf::Node::new(Some("ndSkeleton".to_string())));

        Ok(Some(node_index))
    }
}
