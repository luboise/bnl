use super::prelude::*;

#[derive(Debug, Clone, Serialize)]
pub struct Bone {
    pub name: Option<String>,
    pub parent_id: u16,
    pub id: u16,
    pub local_transform: [f32; 3],
    pub global_transform: [f32; 3],
    pub sentinel: [u8; 4],
}

#[derive(Debug, Clone, Serialize)]
pub struct NdSkeleton {
    pub(crate) header: NdHeader,

    // These are from the original binary
    // num_bones: u32
    // bones_ptr: u32
    pub(crate) bones: Vec<Bone>,
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
        if ctx.current_skin.is_some() {
            return Err(AssetParseError::ErrorParsingDescriptor);
        }

        let skeleton_index = ctx
            .gltf
            .add_node(gltf::Node::new(Some("ndSkeleton".to_string())));

        let root_index = ctx.gltf.add_node(gltf::Node::new(Some("BASE".to_string())));

        let mut new_skin = gltf::Skin::default();
        new_skin.joints.push(root_index);

        for (i, bone) in self.bones.iter().enumerate().skip(1) {
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
}
