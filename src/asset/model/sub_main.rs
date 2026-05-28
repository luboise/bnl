use binrw::BinReaderExt;
use byteorder::{LittleEndian, ReadBytesExt};
use std::{
    collections::HashMap,
    io::{BufRead, Read, Seek, SeekFrom},
};

use crate::asset::model::nd::{ModelReadContext, ModelSlice, Nd};

#[derive(Debug, strum::Display)]
pub enum SubresourceError {
    CreationError,
}

impl std::error::Error for SubresourceError {}

impl From<std::io::Error> for SubresourceError {
    fn from(_: std::io::Error) -> Self {
        Self::CreationError
    }
}

const MESH_HEADER_SIZE: usize = 40;

#[derive(Debug)]
pub struct Mesh {
    header: ModelSubresource,
    primitives: Vec<Nd>,
}

/*
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


        let mut mrc = ModelReadContext::new(&);

        for primitive_ptr in primitive_ptrs {
            if let Ok(nd) = Nd::new(
                &mut ModelReadContext::default(),
                ModelSlice {
                    slice: bytes,
                    read_start: primitive_ptr as usize,
                },
            ) {
                primitives.push(nd);
            };
        }

        Ok(Mesh { header, primitives })
    }

    pub fn primitives(&self) -> &[Nd] {
        &self.primitives
    }
}
*/

#[repr(C)]
#[derive(Debug, Clone)]
pub struct ModelSubresource {
    pub(crate) unknown1: u32,
    pub(crate) unknown2: u32,
    // Temp values used in se/dese
    // primitive_ptrs_start: u32,
    // primitive_count: u32,
    // key_values_ptr: u32,
    pub(crate) unknown3: u32,
    pub(crate) floats: [f32; 4],

    // DO NOT SERIALISE
    pub(crate) primitives: Vec<Nd>,
    pub(crate) key_value_map: HashMap<String, Vec<u8>>,
}

impl ModelSubresource {
    pub fn from_bytes(
        bytes: &[u8],
        resource: &[u8],
        resource_base: u32,
    ) -> Result<Self, crate::Error> {
        Ok(std::io::Cursor::new(bytes).read_le_args((resource, resource_base))?)
    }

    pub fn primitives(&self) -> &[Nd] {
        &self.primitives
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

impl binrw::BinRead for ModelSubresource {
    type Args<'a> = (&'a [u8], u32);

    fn read_options<R: Read + Seek>(
        reader: &mut R,
        endian: binrw::Endian,
        args: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        let (resource_bytes, resource_base) = args;

        // Model subres starts with 0x40, which points to 0x20 inside of the resource
        let subresource_base = reader.read_u32::<LittleEndian>()?;

        if subresource_base != 0x40 {
            return Err(binrw::Error::BadMagic {
                pos: reader.stream_position().unwrap_or_default(),
                found: Box::new(subresource_base),
            });
        }

        let mut reader =
            std::io::Cursor::new(reader.bytes().skip(20).collect::<Result<Vec<_>, _>>()?);

        let unknown1 = reader.read_u32::<LittleEndian>()?;
        let unknown2 = reader.read_u32::<LittleEndian>()?;
        let primitive_ptrs_start = reader.read_u32::<LittleEndian>()?;
        let primitive_count = reader.read_u32::<LittleEndian>()?;
        let key_values_ptr = reader.read_u32::<LittleEndian>()?;
        let unknown3 = reader.read_u32::<LittleEndian>()?;

        let floats: [f32; 4] = [
            reader.read_le()?,
            reader.read_le()?,
            reader.read_le()?,
            reader.read_le()?,
        ];

        let mut key_value_map = HashMap::new();

        let key_value_map = {
            if key_values_ptr == 0 {
                Default::default()
            } else {
                let mut reader = reader.clone();
                reader.seek(SeekFrom::Start(key_values_ptr.into()));
                reader
                    .read_le::<ModelKeyValues>()?
                    .try_into()
                    .map_err(|e| binrw::Error::Custom {
                        pos: reader.stream_position().unwrap_or_default(),
                        err: Box::new("unable to get key value map"),
                    })?
            }
        };

        let primitive_ptrs: Vec<u32> = {
            let mut primitive_cur = reader.clone();
            primitive_cur.seek(SeekFrom::Start(primitive_ptrs_start as u64))?;

            (0..primitive_count as usize)
                .map(|_| primitive_cur.read_u32::<LittleEndian>())
                .collect::<Result<_, _>>()?
        };

        let mut mrc = ModelReadContext::new(&key_value_map);

        let mut primitives = vec![];

        for primitive_ptr in primitive_ptrs {
            match Nd::new(
                &mut mrc,
                ModelSlice {
                    slice: bytes,
                    read_start: primitive_ptr as usize,
                },
            ) {
                Ok(nd) => primitives.push(nd),
                Err(_) => {
                    return Err(SubresourceError::CreationError.into());
                }
            }
        }

        let mut primitives = Vec::with_capacity(primitive_ptrs.len());

        Ok(Self {
            unknown1,
            unknown2,
            unknown3,
            floats,
            primitives,
            key_value_map,
        })
    }
}

#[binrw::binread]
#[br(little)]
struct ModelKeyValues {
    #[br(temp)]
    num_key_values: u32,
    #[br(temp)]
    key_values_ptr: u32,
    #[br(count = num_key_values, seek_before = SeekFrom::Start(key_values_ptr as u64))]
    key_values: Vec<ModelKeyValue>,
}

impl TryFrom<ModelKeyValues> for HashMap<String, Vec<u8>> {
    type Error = crate::Error;

    fn try_from(value: ModelKeyValues) -> Result<Self, Self::Error> {
        let mut hm = HashMap::new();

        for ModelKeyValue { key, value } in value.key_values {
            hm.insert(String::from_utf8(key.value.0)?, value);
        }

        Ok(hm)
    }
}

#[binrw::binread]
#[br(little)]
struct ModelKeyValue {
    key: binrw::FilePtr32<binrw::NullString>,
    #[br(temp)]
    value_ptr: u32,
    #[br(temp)]
    value_size: u32,
    #[br(count = value_size, seek_before = SeekFrom::Start(value_ptr as u64))]
    value: Vec<u8>,
}
