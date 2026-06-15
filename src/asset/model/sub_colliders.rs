use std::io::SeekFrom;

use binrw::BinWriterExt;

#[binrw::binread]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct CollisionSubresource {
    #[br(temp)]
    num_bodies: u32,
    min_flags: u16,
    max_flags: u16,
    #[br(temp)]
    num_vertices: u32,
    #[br(temp)]
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

impl binrw::BinWrite for CollisionSubresource {
    type Args<'a> = ();

    fn write_options<W: std::io::Write + std::io::Seek>(
        &self,
        writer: &mut W,
        _: binrw::Endian,
        _: Self::Args<'_>,
    ) -> binrw::BinResult<()> {
        let base = writer.stream_position()?;

        let Self {
            min_flags,
            max_flags,
            bodies,
            vertices,
        } = self;

        let bodies_offset = 0x10 + base;
        let bodies_size = bodies.iter().map(|body| body.size()).sum::<usize>() as u64;
        let vertices_offset = bodies_offset + bodies_size;
        let vertices_size = vertices.len() as u64 * 12;
        let mut data_offset = vertices_offset + vertices_size;

        let mut data_bytes = vec![];
        let mut data_writer = std::io::Cursor::new(&mut data_bytes);

        writer.write_le(&(bodies.len() as u32))?;
        writer.write_le(&min_flags)?;
        writer.write_le(&max_flags)?;

        writer.write_le(&(vertices.len() as u32))?;
        writer.write_le(&(vertices_offset as u32))?;

        for body in bodies {
            let CollisionBody {
                body_type,
                idk1,
                idk2,
                idk3,
                collision_mask,
                idk4,
                data,
            } = body;
            writer.write_le(body_type)?;
            writer.write_le(idk1)?;
            writer.write_le(idk2)?;
            writer.write_le(idk3)?;
            writer.write_le(collision_mask)?;

            let body_size: u32 = match body_type {
                CollisionBodyType::Type0 => todo!(),
                CollisionBodyType::Mesh => todo!(),
                CollisionBodyType::Box => 0x1c,
                CollisionBodyType::Type3 => todo!(),
            };
            writer.write_le(&body_size)?;

            writer.write_le(idk4)?;

            match data.as_ref() {
                CollisionBodyData::Box { triangles } => {
                    writer.write_le(&(triangles.len() as u32))?;
                    writer.write_le(&(data_offset as u32))?;
                    data_offset += triangles.len() as u64 * 12;
                    writer.write_le(&0u32)?;
                    data_writer.write_le(triangles)?;
                }
                CollisionBodyData::Mesh { .. } => todo!(),
            }
        }

        writer.write_le(vertices)?;
        writer.write_le(&data_bytes)?;

        while (writer.stream_position()? - base) % 32 != 0 {
            writer.write_le(&0u8)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[binrw::binrw]
#[br(repr = u8)]
#[bw(repr = u8)]
pub enum CollisionBodyType {
    Type0 = 0,
    Mesh = 1,
    Box = 2,
    Type3 = 3,
}

#[binrw::binread]
#[derive(Debug, Clone)]
pub struct CollisionBody {
    body_type: CollisionBodyType,
    idk1: u8,
    idk2: u8,
    idk3: u8,
    collision_mask: u32,
    #[br(temp)]
    size: u32,
    idk4: u32,
    #[br(args(body_type))]
    data: Box<CollisionBodyData>,
}

impl CollisionBody {
    pub fn size(&self) -> usize {
        0x10 + self.data.size()
    }
}

#[binrw::binrw]
#[br(import(body_type: CollisionBodyType))]
#[derive(Debug, Clone)]
pub enum CollisionBodyData {
    #[br(assert(body_type == CollisionBodyType::Box))]
    Box {
        #[br(temp)]
        #[bw(ignore)]
        num_triangles: u32,
        #[br(temp)]
        #[bw(ignore)]
        triangles_ptr: u32,
        #[br(count = num_triangles, restore_position, seek_before = SeekFrom::Start(triangles_ptr.into()))]
        triangles: Vec<CollisionTriangle>,
    },
    #[br(assert(body_type == CollisionBodyType::Mesh))]
    Mesh {
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

impl CollisionBodyData {
    pub fn size(&self) -> usize {
        match self {
            CollisionBodyData::Box { .. } => 0xc,
            CollisionBodyData::Mesh { .. } => todo!("mesh size collision body not implemented yet"),
        }
    }
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

#[binrw::binrw]
#[derive(Debug, Clone)]
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

        todo!("collision in gltf not implemented yet");
        let indices: Vec<u32> = todo!();
        // self
        // .bodies
        // .iter()
        // .flat_map(|body| match body {
        //     CollisionBody::Mesh { primitives, .. } => {
        //         todo!();
        //         // primitives.iter().flat_map(|primitive| {
        //         //     primitive
        //         //         .triangles
        //         //         .iter()
        //         //         .flat_map(|triangle| [triangle.index1, triangle.index2, triangle.index3])
        //         // })
        //     }
        //     CollisionBody::Box { .. } => {
        //         todo!("export collision type 2 to gltf not implemented")
        //     }
        // })
        // .collect();

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
