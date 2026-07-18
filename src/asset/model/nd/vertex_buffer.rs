use crate::asset::model::nd::res_view::VertexBufferResourceView;
use std::io::SeekFrom;

pub mod res_view;

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
        (view.view_type() == res_view::VertexBufferViewType::Position).then(|| {
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

#[expect(clippy::manual_non_exhaustive)]
#[binrw::binrw]
#[br(import(mrc: crate::asset::model::ModelReadContext<'_>  ))]
#[bw(import(mwc: crate::asset::model::ModelWriteContext ))]
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[bw(stream=w)]
pub struct NdVertexBufferData {
    #[br(temp)]
    #[bw(calc = 8 + w.stream_position()? as u32)]
    resource_views_ptr: u32,
    #[br(temp)]
    #[bw(try_calc = resource_views.len().try_into())]
    num_resource_views: u32,
    #[br(args {inner: mrc}, count = num_resource_views, seek_before = SeekFrom::Start(resource_views_ptr.into()))]
    #[bw(args_raw(mwc))]
    pub resource_views: Vec<super::res_view::VertexBufferResourceView>,
    #[brw(magic = b"ndVertexBuffer\x00\x00")]
    _name: (),
}
