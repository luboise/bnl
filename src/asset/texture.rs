use std::{
    fs::File,
    io::{BufWriter, Cursor, Write},
    path::Path,
};

use binrw::{BinReaderExt, BinWrite};
use image::EncodableLayout;

use crate::{
    asset::{AssetData, AssetType, Dump},
    d3d::{D3DFormat, PixelBits},
    transcode_image,
};

// 28 bytes
#[derive(Debug, Clone)]
#[binrw::binrw]
pub struct TextureDescriptor {
    pub format: D3DFormat,
    pub header_size: u32, // 0x1c
    pub width: u16,
    pub height: u16,
    pub flags: u32, // 0x00000001
    pub unknown_3a: u32,
    pub texture_offset: u32,
    #[brw(align_after = 0x20)]
    pub texture_size: u32,
}

impl TextureDescriptor {
    pub fn format(&self) -> D3DFormat {
        self.format
    }

    pub fn required_image_size(&self) -> usize {
        (self.width as usize * self.height as usize * self.format.bits_per_pixel()).div_ceil(8)
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn header_size(&self) -> u32 {
        self.header_size
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }

    pub fn unknown_3a(&self) -> u32 {
        self.unknown_3a
    }

    pub fn texture_offset(&self) -> u32 {
        self.texture_offset
    }

    pub fn texture_size(&self) -> u32 {
        self.texture_size
    }
}

#[derive(Debug, Clone)]
pub enum TextureError {
    SizeMismatch,
    InvalidInput,
    UnsupportedOutputType,
}

#[derive(Clone)]
#[binrw::binread]
#[br(import(mrc: super::model::nd::ModelReadContext<'_>))]
pub struct Texture {
    pub descriptor: TextureDescriptor,
    #[br(count = descriptor.texture_size, map_stream = |_| std::io::Cursor::new(mrc.resource))]
    pub bytes: Vec<u8>,
}

impl std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Texture")
            .field("descriptor", &self.descriptor)
            .field("bytes", &format!("{} bytes", self.bytes.len()))
            .finish()
    }
}

impl Texture {
    pub fn new(descriptor: TextureDescriptor, image_bytes: Vec<u8>) -> Self {
        Texture {
            descriptor,
            bytes: image_bytes,
        }
    }

    /// Load a texture from a path with no mipmap
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let image = image::open(path)?.to_rgba8();
        Ok(Self {
            descriptor: TextureDescriptor {
                format: D3DFormat::RGBA8,
                header_size: 28,
                width: image.width().try_into()?,
                height: image.height().try_into()?,
                // TODO: 1 mip, Figure out other flags
                flags: 0x01000000,
                unknown_3a: 0,
                texture_offset: 0,
                texture_size: 0,
            },
            bytes: image.as_bytes().to_owned(),
        })
    }

    /// Override this texture with data from another texture
    // TODO: Make this retain this textures format
    pub fn override_from(
        &mut self,
        other: &Self,
        resize: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if resize
            && (self.descriptor.width, self.descriptor.height)
                != (other.descriptor.width, other.descriptor.height)
        {
            todo!("resize not implemented yet on textures");
        }

        // Convert other texture into our own format
        let transcoded = transcode_image(
            other.descriptor.width.into(),
            other.descriptor.height.into(),
            other.descriptor.format,
            self.descriptor.format,
            &other.bytes,
        )?;

        self.descriptor.width = other.descriptor.width;
        self.descriptor.height = other.descriptor.height;

        // TODO: Properly set mip levels rather than copy whole u32
        self.descriptor.flags &= 0x00FFFFFF | other.descriptor.flags;

        self.bytes = transcoded;

        Ok(())
    }

    pub fn to_rgba_image(&self) -> Result<RGBAImage, Box<dyn std::error::Error>> {
        Ok(RGBAImage {
            width: self.descriptor.width as usize,
            height: self.descriptor.height as usize,
            bytes: crate::transcode_image(
                self.descriptor.width.into(),
                self.descriptor.height.into(),
                self.descriptor.format,
                D3DFormat::RGBA8,
                &self.bytes,
            )?,
        })
    }

    pub fn descriptor(&self) -> &TextureDescriptor {
        &self.descriptor
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl Dump for Texture {
    // fn dump<P: AsRef<Path>>(&self, dump_path: P) -> Result<(), std::io::Error> {
    fn dump<P: AsRef<Path>>(&self, dump_path: P) -> Result<(), Box<dyn std::error::Error>> {
        let path = dump_path.as_ref();

        let file = File::create(path)?;
        let w = &mut BufWriter::new(file);

        self.to_rgba_image()?
            .dump_png_bytes(w)
            .map_err(|e| std::io::Error::other(format!("{e:?}")))?;

        Ok(())
    }
}

impl TryFrom<crate::RawAssetData> for Texture {
    type Error = crate::Error;

    fn try_from(value: crate::RawAssetData) -> Result<Self, Self::Error> {
        let crate::RawAssetData {
            descriptor_bytes,
            resource_chunks,
        } = value;

        if resource_chunks.is_empty() {
            return Err("texture resource buf is empty".into());
        }

        let descriptor: TextureDescriptor = Cursor::new(&descriptor_bytes).read_le()?;
        let resource_bytes = resource_chunks.into_iter().flatten().collect::<Vec<_>>();

        let offset = descriptor.texture_offset as usize;
        let size = descriptor.texture_size as usize;

        let bytes = resource_bytes
            .get(offset..offset + size)
            .ok_or_else(|| format!("bad slice {offset}..{}", offset + size))?
            .to_owned();

        Ok(Texture {
            descriptor: descriptor.clone(),
            bytes,
        })
    }
}

impl TryFrom<Texture> for crate::RawAssetData {
    type Error = crate::Error;

    fn try_from(value: Texture) -> Result<Self, Self::Error> {
        let mut descriptor_bytes = vec![];
        value
            .descriptor
            .write_le(&mut Cursor::new(&mut descriptor_bytes));

        Ok(crate::RawAssetData {
            descriptor_bytes,
            resource_chunks: vec![value.bytes],
        })
    }
}

impl AssetData for Texture {
    const ASSET_TYPE: AssetType = AssetType::Texture;
}

#[derive(Clone)]
pub struct RGBAImage {
    width: usize,
    height: usize,
    bytes: Vec<u8>,
}

impl RGBAImage {
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn dump_png_bytes<W: Write>(&self, w: &mut W) -> Result<(), TextureError> {
        let mut encoder = png::Encoder::new(w, self.width as u32, self.height as u32);

        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);

        // encoder.set_source_gamma(png::ScaledFloat::new(1.0 / 2.2));
        /*
        let chroma = png::SourceChromaticities::new(
            (0.3127, 0.3290), // red
            (0.6400, 0.3300), // green
            (0.3000, 0.6000), // blue
            (0.1500, 0.0600), // white
        );
        encoder.set_source_chromaticities(chroma);
        */

        let mut writer = encoder.write_header().unwrap();

        writer
            .write_image_data(&self.bytes)
            .map_err(|_| TextureError::InvalidInput)?;
        writer.finish().expect("Unable to close writer");

        Ok(())
    }
}

impl Texture {
    pub fn set_from_rgba(
        &mut self,
        width: usize,
        height: usize,
        data: &[u8],
    ) -> Result<(), TextureError> {
        if (data.len() < width * height * 4)
            || width != self.descriptor().width as usize
            || height != self.descriptor().height as usize
        {
            return Err(TextureError::SizeMismatch);
        }

        let transcoded = crate::images::transcode(
            self.descriptor().width as usize,
            self.descriptor().height as usize,
            D3DFormat::RGBA8,
            self.descriptor().format,
            data,
        )
        .map_err(|_| {
            eprintln!(
                "Unable to convert from RGBA to format {:?}",
                self.descriptor().format
            );
            TextureError::UnsupportedOutputType
        })?;

        self.bytes = transcoded;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /*
    #[test]
    fn texture_descriptor_size() {
        assert_eq!(size_of::<TextureDescriptor>(), 28);
    }
    */

    #[test]
    fn from_bytes_non_zero_offset() {
        let data: [u8; 0x1C] = [
            0x0C, 0x00, 0x00, 0x00, // DXT1
            0x1C, 0x00, 0x00, 0x00, // Header size
            0x80, 0x00, // 0x80 wide
            0x80, 0x00, // 0x80 high
            0x00, 0x00, 0x00, 0x08, // Flags
            0x00, 0x01, 0x00, 0x00, // Unknown
            0x00, 0x52, 0x01, 0x00, // Offset
            0x00, 0x2B, 0x00, 0x00, // Size
        ];

        let tex_desc = std::io::Cursor::new(data)
            .read_le::<TextureDescriptor>()
            .unwrap();
        assert_eq!(tex_desc.format, D3DFormat::DXT1);
        assert_eq!(tex_desc.header_size, 0x1c);
        assert_eq!(tex_desc.width, 0x80);
        assert_eq!(tex_desc.height, 0x80);
        assert_eq!(tex_desc.texture_offset, 0x15200);
        assert_eq!(tex_desc.texture_size, 0x2b00);
    }

    #[test]
    fn from_bytes_zero_offset() {
        let data: [u8; 0x1C] = [
            0x0C, 0x00, 0x00, 0x00, // DXT1
            0x1C, 0x00, 0x00, 0x00, // Header size
            0x80, 0x00, // 0x80 wide
            0x80, 0x00, // 0x80 high
            0x00, 0x00, 0x00, 0x08, // Flags
            0x00, 0x01, 0x00, 0x00, // Unknown
            0x00, 0x00, 0x00, 0x00, // Offset
            0x00, 0x2B, 0x00, 0x00, // Size
        ];

        let tex_desc = std::io::Cursor::new(data)
            .read_le::<TextureDescriptor>()
            .unwrap();
        assert_eq!(tex_desc.format, D3DFormat::DXT1);
        assert_eq!(tex_desc.header_size, 0x1c);
        assert_eq!(tex_desc.width, 0x80);
        assert_eq!(tex_desc.height, 0x80);
        assert_eq!(tex_desc.texture_offset, 0);
        assert_eq!(tex_desc.texture_size, 0x2b00);
    }

    #[test]
    fn from_test_file() -> Result<(), String> {
        let descriptor_bytes = include_bytes!("test_data/texture0_descriptor");
        let resource_bytes = include_bytes!("test_data/texture0_resource0");

        let tex_desc = std::io::Cursor::new(descriptor_bytes)
            .read_le::<TextureDescriptor>()
            .unwrap();

        let _tex = Texture::new(tex_desc, resource_bytes.to_vec());

        Ok(())
    }
}
