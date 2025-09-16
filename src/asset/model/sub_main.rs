use byteorder::{LittleEndian, ReadBytesExt};
use std::io::{self, Cursor, Read, Seek, SeekFrom};

use crate::asset::model::nd::Nd;

#[derive(Debug)]
pub enum SubresourceError {
    CreationError,
}

const MESH_HEADER_SIZE: usize = 40;

#[derive(Debug)]
pub struct Mesh {
    header: MeshDescriptor,
    primitives: Vec<Nd>,
}

impl Mesh {
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Mesh, SubresourceError> {
        let mut cur = Cursor::new(bytes);

        // TODO: Add bounds checks

        // let end = bytes.len();

        let mut mesh_header_bytes = [0x00; MESH_HEADER_SIZE];

        cur.read_exact(&mut mesh_header_bytes)?;

        let header = MeshDescriptor::from_bytes(&mesh_header_bytes)?;

        let mut primitive_ptrs = vec![0u32; header.primitive_count as usize];

        let mut primitive_cur = cur.clone();

        primitive_cur.seek(SeekFrom::Start(header.primitive_ptrs_start as u64));

        for i in 0..header.primitive_count as usize {
            primitive_ptrs[i] = primitive_cur.read_u32::<LittleEndian>()?;
        }

        let mut primitives = Vec::with_capacity(primitive_ptrs.len());

        for primitive_ptr in primitive_ptrs {
            if let Ok(nd) = Nd::new(bytes, primitive_ptr as usize) {
                primitives.push(nd);
            };
        }

        Ok(Mesh { header, primitives })
    }

    pub fn primitives(&self) -> &[Nd] {
        &self.primitives
    }
}

#[repr(C)]
#[derive(Debug, Clone)]
pub struct MeshDescriptor {
    pub(crate) unknown1: u32,
    pub(crate) unknown2: u32,
    pub(crate) primitive_ptrs_start: u32,
    pub(crate) primitive_count: u32,
    pub(crate) key_values_ptr: u32,
    pub(crate) unknown3: u32,
    pub(crate) floats: [f32; 4],

    // DO NOT SERIALISE
    pub(crate) primitives: Vec<Nd>,
}

impl From<io::Error> for SubresourceError {
    fn from(_: io::Error) -> Self {
        Self::CreationError
    }
}

impl MeshDescriptor {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SubresourceError> {
        let mut cur = Cursor::new(bytes);

        let unknown1 = cur.read_u32::<LittleEndian>()?;
        let unknown2 = cur.read_u32::<LittleEndian>()?;
        let primitive_ptrs_start = cur.read_u32::<LittleEndian>()?;
        let primitive_count = cur.read_u32::<LittleEndian>()?;
        let key_values_ptr = cur.read_u32::<LittleEndian>()?;
        let unknown3 = cur.read_u32::<LittleEndian>()?;

        let mut floats = [0f32; 4];

        for float in floats.iter_mut() {
            *float = cur.read_f32::<LittleEndian>()?;
        }

        let mut primitive_ptrs = vec![0u32; primitive_count as usize];

        let mut primitive_cur = cur.clone();

        primitive_cur.seek(SeekFrom::Start(primitive_ptrs_start as u64));

        for i in 0..primitive_count as usize {
            primitive_ptrs[i] = primitive_cur.read_u32::<LittleEndian>()?;
        }

        let mut primitives = Vec::with_capacity(primitive_ptrs.len());

        for primitive_ptr in primitive_ptrs {
            match Nd::new(bytes, primitive_ptr as usize) {
                Ok(nd) => primitives.push(nd),
                Err(_) => {
                    return Err(SubresourceError::CreationError);
                }
            }
            {};
        }

        Ok(MeshDescriptor {
            unknown1,
            unknown2,
            primitive_ptrs_start,
            primitive_count,
            key_values_ptr,
            unknown3,
            floats,
            primitives,
        })
    }
}

#[derive(Debug)]
struct MeshPrimitive {
    root: Nd,
}

impl MeshPrimitive {
    fn new(root: Nd) -> Self {
        Self { root }
    }

    fn root(&self) -> &Nd {
        &self.root
    }
}
