use std::io::{Seek as _, SeekFrom};

use binrw::BinReaderExt;
use byteorder::{LittleEndian, ReadBytesExt as _};

use super::AssetType;

#[derive(Debug, Clone)]
pub struct RawFontDescriptor {
    pub start_glyph: u32,
    pub end_glyph: u32,
    /// The number of variants for each glyph
    pub num_variants: u32,
    pub text_x: u32,
    /// Line height?
    pub text_y: u32,
    pub entries_start_ptr: u32,
}

#[derive(Debug, Clone)]
pub struct RawGlyph {
    // Not part of original struct
    pub glyph_index: u32,
    // Raw struct
    pub texture_descriptor: super::texture::TextureDescriptor,
    pub num_somethings: u32,
    pub unknown_u32_1: u32,
    pub unknown_u32_2: u32,
    pub unknown_u32_3: u32,
    pub unknown_u32_4: u32,
}

#[derive(Debug, Clone)]
pub struct FontDescriptor {
    pub num_variants: u32,
    pub text_x: u32,
    pub text_y: u32,
    pub glyphs: Vec<RawGlyph>,
}

impl FontDescriptor {
    pub fn first_glyph(&self) -> u32 {
        self.glyphs
            .iter()
            .min_by_key(|glyph| glyph.glyph_index)
            .map(|glyph| glyph.glyph_index)
            .unwrap_or(0)
    }

    pub fn last_glyph(&self) -> u32 {
        self.glyphs
            .iter()
            .max_by_key(|glyph| glyph.glyph_index)
            .map(|glyph| glyph.glyph_index)
            .unwrap_or(0)
    }
}

impl FontDescriptor {
    fn from_bytes(data: &[u8]) -> Result<Self, crate::Error> {
        let mut cur = std::io::Cursor::new(data);

        let raw_descriptor = RawFontDescriptor {
            start_glyph: cur.read_u32::<LittleEndian>()?,
            end_glyph: cur.read_u32::<LittleEndian>()?,
            num_variants: cur.read_u32::<LittleEndian>()?,
            text_x: cur.read_u32::<LittleEndian>()?,
            text_y: cur.read_u32::<LittleEndian>()?,
            entries_start_ptr: cur.read_u32::<LittleEndian>()?,
        };

        let num_glyphs = raw_descriptor.end_glyph - raw_descriptor.start_glyph + 1;

        cur.seek(SeekFrom::Start(raw_descriptor.entries_start_ptr.into()))?;

        let mut glyphs = vec![];

        for i in 0..num_glyphs {
            let tex_ptr = cur.read_u32::<LittleEndian>()?;
            let ptr_2 = cur.read_u32::<LittleEndian>()?;
            let unknown_u32_1 = cur.read_u32::<LittleEndian>()?;
            let unknown_u32_2 = cur.read_u32::<LittleEndian>()?;
            let unknown_u32_3 = cur.read_u32::<LittleEndian>()?;
            let unknown_u32_4 = cur.read_u32::<LittleEndian>()?;

            if tex_ptr == 0 || ptr_2 == 0xffffffff {
                continue;
            }

            let mut tex_cur = cur.clone();
            tex_cur.seek(SeekFrom::Start(tex_ptr.into()))?;

            let tex_descriptor = std::io::Cursor::new(&data[tex_ptr as usize..]).read_le()?;

            glyphs.push(RawGlyph {
                glyph_index: raw_descriptor.start_glyph + i,
                texture_descriptor: tex_descriptor,
                num_somethings: ptr_2,
                unknown_u32_1,
                unknown_u32_2,
                unknown_u32_3,
                unknown_u32_4,
            });
        }

        Ok(Self {
            num_variants: raw_descriptor.num_variants,
            text_x: raw_descriptor.text_x,
            text_y: raw_descriptor.text_y,
            glyphs,
        })
    }
}

#[derive(Debug)]
pub struct Glyph {
    pub glyph_index: u32,
    pub textures: Vec<super::texture::Texture>,
    pub num_somethings: u32,
    pub unknown_u32_1: u32,
    pub unknown_u32_2: u32,
    pub unknown_u32_3: u32,
    pub unknown_u32_4: u32,
}

#[derive(Debug)]
pub struct Font {
    pub descriptor: FontDescriptor,
    pub glyphs: Vec<Glyph>,
}

impl TryFrom<crate::RawAssetData> for Font {
    type Error = crate::Error;

    fn try_from(value: crate::RawAssetData) -> Result<Self, Self::Error> {
        let crate::RawAssetData {
            descriptor_bytes,
            resource_chunks,
        } = value;

        let res_bytes = resource_chunks.into_iter().flatten().collect::<Vec<_>>();

        let descriptor = FontDescriptor::from_bytes(&descriptor_bytes)?;

        let glyphs = descriptor
            .glyphs
            .iter()
            .map(|raw| {
                let tex_start = raw.texture_descriptor.texture_offset as usize;
                let tex_size = raw.texture_descriptor.texture_size as usize;

                let textures = (0..descriptor.num_variants as usize)
                    .map(|i| {
                        let start = tex_start + i * tex_size;

                        super::texture::Texture::new(
                            raw.texture_descriptor.clone(),
                            res_bytes[start..start + tex_size].to_vec(),
                        )
                    })
                    .collect();

                Glyph {
                    glyph_index: raw.glyph_index,
                    textures,
                    num_somethings: raw.num_somethings,
                    unknown_u32_1: raw.unknown_u32_1,
                    unknown_u32_2: raw.unknown_u32_2,
                    unknown_u32_3: raw.unknown_u32_3,
                    unknown_u32_4: raw.unknown_u32_4,
                }
            })
            .collect::<Vec<_>>();

        Ok(Self {
            descriptor: descriptor.clone(),
            glyphs,
        })
    }
}

impl TryFrom<Font> for crate::RawAssetData {
    type Error = crate::Error;

    fn try_from(_value: Font) -> Result<Self, Self::Error> {
        todo!()
    }
}

impl super::AssetData for Font {
    const ASSET_TYPE: AssetType = AssetType::Font;
}
