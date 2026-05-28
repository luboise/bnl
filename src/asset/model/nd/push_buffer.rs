use super::prelude::*;
use crate::d3d::D3DPrimitiveType;

#[derive(Debug, Clone, Serialize)]
pub struct DrawCall {
    pub data_ptr: u32,
    pub prim_type: D3DPrimitiveType,
    pub num_vertices: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NdPushBufferData {
    pub(crate) num_draws: u32,
    pub(crate) unknown_u32_1: u32,
    pub(crate) unknown_u32_2: u32,
    pub(crate) unknown_u32_3: u32,

    // File offsets
    pub(crate) data_pointers_start: u32,
    pub(crate) primitive_types_list_ptr: u32,
    pub(crate) vertex_counts_list_ptr: u32,

    pub(crate) prevent_culling_flag: u8,
    pub(crate) padding: [u8; 3],

    #[serde(skip_serializing)]
    pub(crate) buffer_bytes: Vec<u8>,

    pub push_buffer_base: u32,
    pub(crate) push_buffer_size: u32,

    pub draw_calls: Vec<DrawCall>,
}

impl NdPushBufferData {
    pub fn indices(&self) -> Vec<u16> {
        self.buffer_bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes(c[0..2].try_into().unwrap()))
            .collect()
    }
}
