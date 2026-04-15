use std::io::SeekFrom;

#[binrw::binrw]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct CollisionSubresource {
    num_bodies: u32,
    min_flags: u16,
    max_flags: u16,
    num_vertices: u32,
    vertices_ptr: u32,
    #[br(count = num_bodies)]
    bodies: Vec<CollisionBody>,
    #[br(
    count = num_vertices,
    seek_before(SeekFrom::Start(vertices_ptr.into())),
    restore_position,
   )]
    vertices: Vec<[f32; 3]>,
}

impl CollisionSubresource {
    pub fn add_to_gltf(
        &self,
        gltf: &mut gltf_writer::gltf::Gltf,
    ) -> Result<(), gltf_writer::gltf::GltfError> {
        let vertices_accessor = {
            let vertex_bytes = self
                .vertices
                .iter()
                .flatten()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<_>>();
            let vertex_buffer_len = vertex_bytes.len();
            let vertex_buffer_index =
                gltf.add_buffer(gltf_writer::gltf::Buffer::new(&vertex_bytes));
            let vertex_buffer_view = gltf.add_buffer_view(gltf_writer::gltf::BufferView::new(
                vertex_buffer_index,
                0,
                vertex_buffer_len,
                None,
                None,
            ));

            gltf.add_accessor(gltf_writer::gltf::Accessor::new(
                vertex_buffer_view,
                0,
                gltf_writer::gltf::AccessorDataType::F32,
                vertex_buffer_len / 12,
                gltf_writer::gltf::AccessorComponentCount::VEC3,
            ))
        };

        let indices: Vec<u32> = self
            .bodies
            .iter()
            .flat_map(|body| match body {
                CollisionBody::Mesh { primitives, .. } => primitives.iter().flat_map(|primitive| {
                    primitive
                        .triangles
                        .iter()
                        .flat_map(|triangle| [triangle.index1, triangle.index2, triangle.index3])
                }),
            })
            .collect();

        let indices_accessor = {
            let bi = gltf.add_buffer(gltf_writer::gltf::Buffer::new(
                indices
                    .clone()
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect::<Vec<_>>(),
            ));

            let bvi = gltf.add_buffer_view(gltf_writer::gltf::BufferView::new(
                bi,
                0,
                indices.len() * size_of::<u32>(),
                None,
                None,
            ));

            gltf.add_accessor(gltf_writer::gltf::Accessor::new(
                bvi,
                0,
                gltf_writer::gltf::AccessorDataType::U32,
                indices.len(),
                gltf_writer::gltf::AccessorComponentCount::SCALAR,
            ))
        };

        let mesh_index = {
            let mut new_mesh = gltf_writer::gltf::Mesh::new("Mesh".to_owned());

            let mut attributes = std::collections::HashMap::default();

            attributes.insert(
                gltf_writer::gltf::VertexAttribute::Position,
                vertices_accessor,
            );

            new_mesh.add_primitive(gltf_writer::gltf::Primitive {
                indices_accessor: Some(indices_accessor),
                topology_type: None,
                attributes,
                material: None,
            });

            gltf.add_mesh(new_mesh)
        };

        let mut new_node = gltf_writer::gltf::Node::new(Some("CollisionShape".to_owned()));
        new_node.set_mesh_index(Some(mesh_index));

        gltf.add_node(new_node);

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
#[binrw::binrw]
#[br(repr = u8)]
#[bw(repr = u8)]
pub enum CollisionBodyType {
    Type0 = 0,
    Mesh = 1,
    Type2 = 2,
    Type3 = 3,
}

#[binrw::binrw]
#[derive(Debug, Clone)]
pub enum CollisionBody {
    Mesh {
        #[br(assert(body_type == CollisionBodyType::Mesh))]
        body_type: CollisionBodyType,
        idk1: u8,
        idk2: u8,
        idk3: u8,

        collision_mask: u32,
        size: u32,

        idk4: u32,

        max_x: i16,
        max_y: i16,
        max_z: i16,

        count1: u16,
        count2: u16,
        count3: u16,
        count4: u16,
        count5: u16,
        num_primitives: u16,
        grid_divisions: u16,
        min_x: i16,
        min_y: i16,
        min_z: i16,
        idk5: u16,

        #[br(count = num_primitives)]
        primitives: Vec<CollisionPrimitive>,
    },
}

#[derive(Debug, Clone)]
#[binrw::binrw]
pub struct CollisionPrimitive {
    num_triangles: u32,
    triangles_ptr: u32,
    #[br(
        count = num_triangles,
        seek_before(SeekFrom::Start(triangles_ptr.into())),
        restore_position
        )]
    triangles: Vec<CollisionTriangle>,
}

#[derive(Debug, Clone)]
#[binrw::binrw]
pub struct CollisionTriangle {
    /// Vertex indices
    index1: u32,
    index2: u32,
    index3: u32,
    collision_mask: u32,
    some_u16: u16,
    #[br(assert(pad == 0xcccc))]
    pad: u16,
}
