use std::io::{Seek, SeekFrom, Write};

use binrw::{BinReaderExt, BinWriterExt};

use crate::asset::model::nd::res_view::VertexBufferResourceView;

pub mod res_view;

#[derive(Debug, Clone, serde::Serialize)]
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

/*
let resource_views_ptr = reader.read_u32::<LittleEndian>()?;
let num_resource_views = reader.read_u32::<LittleEndian>()?;

let mut resource_views = Vec::with_capacity(num_resource_views as usize);

for _ in 0..num_resource_views {
    resource_views.push(reader.read_le()?);
}

Ok(NdData::VertexBuffer {
    resource_views_ptr,
    num_resource_views,
    resource_views,
})
*/

#[binrw::binrw]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[bw(stream=w)]
pub struct NdVertexBufferData {
    #[br(temp)]
    #[bw(calc = w.stream_position()? as u32)]
    resource_views_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = resource_views.len().try_into())]
    num_resource_views: u32,

    #[serde(skip)]
    #[br(count = num_resource_views,
            seek_before = SeekFrom::Start(resource_views_ptr.into()),
            restore_position
        )]
    resource_views: Vec<super::res_view::VertexBufferResourceView>,
}
