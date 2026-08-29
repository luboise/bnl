use std::io::{Seek, SeekFrom};

use binrw::{BinReaderExt, BinWrite};
use byteorder::{LittleEndian, ReadBytesExt};

use crate::asset::texture;

#[binrw::binrw]
#[repr(u32)]
#[bw(repr = u32)]
#[br(repr = u32)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    num_enum::TryFromPrimitive,
    num_enum::IntoPrimitive,
    strum::EnumString,
    strum::EnumIter,
    strum::Display,
)]
pub enum ModelSubresType {
    Mesh = 0x00,
    Flags = 0x01,
    Unknown0x02 = 0x02,
    Unknown0x03 = 0x03,
    Unknown0x04 = 0x04,
    Matrices = 0x05,
    Collision = 0x06,
    Texture = 0x07,
    Unknown0x08 = 0x08,
    BoneIndices = 0x09,
    Transforms = 0x0a,
    Unknown0x0b = 0x0b,
    Unknown0x0c = 0x0c,
    Unknown0x0d = 0x0d,
    Unknown0x0e = 0x0e,
    Unknown0x0f = 0x0f,
    Unknown0x10 = 0x10,
    Unknown0x11 = 0x11,
    Tiles = 0x12,
    Unknown0x13 = 0x13,
    Unknown0x14 = 0x14,
    Unknown0x15 = 0x15,
}

// TODO: Properly figure these out
// These can realistically stay as Vec<u8> for now since none of them have any resource attached (?)
// Assuming nothing changes size, these should be able to be kept in-place
pub type Subresource0x2 = Vec<u8>;
pub type Subresource0x3 = Vec<u8>;
pub type Subresource0x4 = Vec<u8>;
pub type Subresource0x5 = Vec<u8>;

#[derive(Clone, Debug, Default)]
pub struct TexturesSubresource {
    pub textures: Vec<crate::asset::texture::Texture>,
}
impl binrw::BinRead for TexturesSubresource {
    type Args<'a> = super::ModelReadContext<'a>;

    fn read_options<R: std::io::prelude::Read + Seek>(
        reader: &mut R,
        _: binrw::Endian,
        mrc: Self::Args<'_>,
    ) -> binrw::prelude::BinResult<Self> {
        let num_textures = reader.read_u32::<LittleEndian>()?;
        let tex_ptr_ptr = reader.read_u32::<LittleEndian>()?;

        reader.seek(SeekFrom::Start(tex_ptr_ptr.into()))?;

        let tex_offset_ptrs = (0..num_textures)
            .map(|_| reader.read_u32::<LittleEndian>())
            .collect::<Result<Vec<_>, _>>()?;

        let textures = tex_offset_ptrs
            .into_iter()
            .map(|tex_offset_ptr| {
                reader.seek(SeekFrom::Start(tex_offset_ptr.into()))?;
                let tex_desc = reader.read_le::<texture::TextureDescriptor>()?;

                let res_start = usize::try_from(tex_desc.texture_offset)?;

                let res_end = res_start
                    .checked_add(tex_desc.texture_size.try_into()?)
                    .ok_or("tex size add out of range")?;

                let tex_res = mrc
                    .resource
                    .get(res_start..res_end)
                    .ok_or("failed to get texture res")?;

                Ok(texture::Texture::new(tex_desc, tex_res.to_vec()))
            })
            .collect::<Result<Vec<_>, crate::Error>>()
            .map_err(|e| binrw::Error::Custom {
                pos: reader.stream_position().unwrap_or(0),
                err: Box::new(format!("failed to get data for Nd: {e}")),
            })?;

        Ok(Self { textures })
    }
}

impl TexturesSubresource {
    #[deprecated(note = "use BinReaderExt::read_le_args(ModelReadContext)")]
    pub fn new(
        data: &[u8],
        descriptor_base: u32,
        resource: &[u8],
        _resource_base: u32,
    ) -> Result<Self, crate::Error> {
        let mut cur = std::io::Cursor::new(data);
        cur.seek(SeekFrom::Start(descriptor_base.into()))?;
        Ok(cur.read_le_args(super::ModelReadContext {
            properties: &Default::default(),
            resource,
        })?)
    }

    pub fn serialize(
        &self,
        descriptor_base: u32,
        resource_base: u32,
        alignment: usize,
    ) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error>> {
        let mut descriptor_bytes = vec![0u8; alignment];
        let mut resource_bytes = vec![];

        let ptrs_list_size = {
            let mut size = self.textures.len() * 4;

            // If not aligned, align it
            if !size.is_multiple_of(alignment) {
                size += alignment - (size % alignment);
            }

            size
        };

        let mut ptrs_list = vec![0u8; ptrs_list_size];

        descriptor_bytes[0..4].copy_from_slice(&u32::try_from(self.textures.len())?.to_le_bytes());
        descriptor_bytes[4..8]
            .copy_from_slice(&(descriptor_base + u32::try_from(alignment)?).to_le_bytes());

        for i in 0..self.textures.len() {
            ptrs_list[i * 4..i * 4 + 4].copy_from_slice(
                &(u32::try_from(
                    i * 0x40
                        + usize::try_from(descriptor_base)?
                        + descriptor_bytes.len()
                        + ptrs_list_size,
                )?)
                .to_le_bytes(),
            );
        }

        descriptor_bytes.extend_from_slice(&ptrs_list);

        for texture in &self.textures {
            let mut descriptor = texture.descriptor().clone();
            descriptor.texture_offset = resource_base + u32::try_from(resource_bytes.len())?;
            descriptor.texture_size = texture.bytes().len().try_into()?;

            resource_bytes.extend_from_slice(texture.bytes());

            let bytes = {
                let mut bytes = vec![];
                descriptor.write_le(&mut std::io::Cursor::new(&mut bytes))?;

                if bytes.len() < 0x40 {
                    bytes.resize(0x40, 0u8);
                }

                bytes
            };

            descriptor_bytes.extend_from_slice(&bytes);
        }

        Ok((descriptor_bytes, resource_bytes))
    }
}

pub type Subresource0x8 = Vec<u8>;
pub type Subresource0x9 = Vec<u8>;

#[binrw::binrw]
#[derive(Clone, Debug)]
pub struct FloatSet0xc {
    float1: f32,
    float2: f32,
    float3: f32,
    flags: u32,
    float4: f32,
    float5: f32,
}

#[binrw::binrw]
#[derive(Clone, Debug)]
pub struct Subresource0xc {
    #[br(temp)]
    #[bw(try_calc = float_sets.len().try_into())]
    count1: u32,
    #[br(temp)]
    #[bw(ignore)]
    float_sets_ptr: u32,
    #[br(count = count1, seek_before = SeekFrom::Start(float_sets_ptr as u64))]
    float_sets: Vec<FloatSet0xc>,
    #[br(temp)]
    // TODO: UNIGNORE THESE!
    #[bw(ignore)]
    num_textures: u32,
    #[br(temp)]
    #[bw(ignore)]
    #[br(count = num_textures)]
    texture_ptrs: Vec<binrw::FilePtr<u32, texture::TextureDescriptor>>,
    // textures_ptr_ptr: std::ptr::NullablePtr<TEXTURE_HEADER, u32>* texture_ptrs[numTextures]: u32;
}

pub type Subresource0xd = Vec<u8>;
pub type Subresource0xe = Vec<u8>;
pub type Subresource0xf = Vec<u8>;
pub type Subresource0x10 = Vec<u8>;
pub type Subresource0x11 = Vec<u8>;

#[wezat::wz]
#[derive(Debug, Clone)]
pub struct Tile {
    pub idk1: u32,
    pub idk2: u32,
    pub res_size: u32,
    pub res_ptr: u32,

    pub d3d_vertex_buffer_header: u32,
    pub idk3: u32,
    pub idk4: u32,
}

#[wezat::wz]
#[derive(Debug, Clone)]
pub struct TilesSubresource {
    pub idka1: u32,
    pub idka2: u32,
    pub idk3: u32,
    pub idk4: u32,

    pub num_x: u32,
    pub num_y: u32,
    pub num_z: u32,

    num_tiles: u32,

    pub some_vec3: [f32; 3],
    pub idk0x2c: u32,

    pub idk0x30: u32,
    pub idk0x34: u32,
    pub scale: f32,

    tiles_ptr: &tiles,
    tiles: [Tile; num_tiles],
}

/*
#[derive(Debug, Clone)]
#[binrw::binrw]
pub struct TilesSubresource {
    pub idka1: u32,
    pub idka2: u32,
    pub idk3: u32,
    pub idk4: u32,

    pub num_x: u32,
    pub num_y: u32,
    pub num_z: u32,

    num_tiles: u32,

    pub some_vec3: [f32; 3],
    pub idk0x2c: u32,

    pub idk0x30: u32,
    pub idk0x34: u32,
    pub scale: f32,

    tiles_ptr: u32,

    #[br(count = num_tiles, restore_position, seek_before = SeekFrom::Start(tiles_ptr.into()))]
    tiles: Vec<Tile>,
}
*/

pub type Subresource0x13 = Vec<u8>;
pub type Subresource0x14 = Vec<u8>;
pub type Subresource0x15 = Vec<u8>;

#[binrw::binrw]
#[derive(Clone, Debug)]
pub struct Subresource0xa {
    #[bw(calc = transforms.len() as u32)]
    #[br(temp)]
    num_transforms: u32,
    #[br(count = num_transforms)]
    transforms: Vec<[f32; 10]>,
}

pub type Subresource0xb = Vec<u8>;
