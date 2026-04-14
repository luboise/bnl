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
