use crate::asset::model::nd::res_view::VertexBufferResourceView;

use super::prelude::*;

pub mod res_view;

#[derive(Debug, Clone, Serialize)]
pub struct NdVertexBuffer {}

pub fn get_resource_view<V: res_view::VertexBufferView>(
    resource: &[u8],
    views: &[VertexBufferResourceView],
) -> Option<Vec<f32>> {
    let view = views
        .iter()
        .find(|view| view.view_type() == V::VIEW_TYPE)
        .filter(|view| {
            view.len() <= resource.len() && (view.start() as usize + view.len()) <= resource.len()
        })?;

    Some(
        resource[view.start() as usize..view.start() as usize + view.len()]
            .to_owned()
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect(),
    )
}

pub fn get_vertex_positions(
    resource: &[u8],
    views: &[VertexBufferResourceView],
) -> Option<Vec<[f32; 3]>> {
    views.iter().find_map(|view| {
        (view.view_type() == res_view::VertexBufferViewType::Vertex).then(|| {
            resource[view.start() as usize..view.end() as usize]
                .chunks_exact(12)
                .map(|chunk| {
                    [
                        f32::from_le_bytes(chunk[0..4].try_into().unwrap()),
                        f32::from_le_bytes(chunk[4..8].try_into().unwrap()),
                        f32::from_le_bytes(chunk[8..12].try_into().unwrap()),
                    ]
                })
                .collect()
        })
    })
}
