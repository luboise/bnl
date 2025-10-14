use std::{
    fs::File,
    io::{BufWriter, Cursor, Write},
    path::Path,
};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

use crate::{
    VirtualResource, VirtualResourceError,
    asset::{AssetDescriptor, AssetLike, AssetParseError, AssetType, Dump},
    d3d::{D3DFormat, LinearColour, PixelBits, StandardFormat, Swizzled},
};

const TEXTURE_DESCRIPTOR_SIZE: usize = 28;

#[derive(Debug, Clone)]
pub struct TextureDescriptor {
    format: D3DFormat,
    header_size: u32, // 0x1c
    width: u16,
    height: u16,
    flags: u32, // 0x00000001
    unknown_3a: u32,
    texture_offset: u32,
    texture_size: u32,
}

impl TextureDescriptor {
    pub fn new(
        format: D3DFormat,
        header_size: u32,
        width: u16,
        height: u16,
        flags: u32,
        unknown_3a: u32,
        texture_offset: u32,
        texture_size: u32,
    ) -> Self {
        Self {
            format,
            header_size,
            width,
            height,
            flags,
            unknown_3a,
            texture_offset,
            texture_size,
        }
    }

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

#[derive(Debug, Clone)]
pub struct Texture {
    descriptor: TextureDescriptor,
    bytes: Vec<u8>,
}

impl Texture {
    pub fn new(descriptor: TextureDescriptor, image_bytes: Vec<u8>) -> Self {
        Texture {
            descriptor,
            bytes: image_bytes,
        }
    }

    pub fn to_rgba_image(&self) -> Result<RGBAImage, std::io::Error> {
        let mut bytes: Vec<u8> = self.bytes.clone();

        let desired_format: D3DFormat = match self.descriptor.format {
            D3DFormat::Linear(LinearColour::R8G8B8A8)
            | D3DFormat::Swizzled(Swizzled::A8B8G8R8)
            | D3DFormat::Swizzled(Swizzled::A8R8G8B8) => D3DFormat::Linear(LinearColour::R8G8B8A8),
            _ => D3DFormat::Linear(LinearColour::R8G8B8A8),
        };

        if desired_format != self.descriptor.format {
            println!("Attempting transcode.");

            bytes = crate::images::transcode(
                self.descriptor.width.into(),
                self.descriptor.height.into(),
                self.descriptor.format,
                desired_format,
                bytes.as_ref(),
            )?;

            println!("Transcode succeeded.");
        }

        Ok(RGBAImage {
            width: self.descriptor.width as usize,
            height: self.descriptor.height as usize,
            bytes,
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
    fn dump<P: AsRef<Path>>(&self, dump_path: P) -> Result<(), std::io::Error> {
        let path = dump_path.as_ref();

        let file = File::create(path)?;
        let w = &mut BufWriter::new(file);

        self.to_rgba_image()?.dump_png_bytes(w);

        Ok(())
    }
}

impl AssetDescriptor for TextureDescriptor {
    fn from_bytes(data: &[u8]) -> Result<Self, AssetParseError> {
        if data.len() < TEXTURE_DESCRIPTOR_SIZE {
            return Err(AssetParseError::InputTooSmall);
        }

        let mut cur = Cursor::new(data);

        let format = match cur.read_u32::<LittleEndian>()? {
            0x00000012 => D3DFormat::Swizzled(Swizzled::B8G8R8A8),
            0x0000003f => D3DFormat::Swizzled(Swizzled::A8B8G8R8),
            0x00000040 => D3DFormat::Linear(LinearColour::A8R8G8B8),
            0x0000000c => D3DFormat::Standard(StandardFormat::DXT1),
            0x0000000e => D3DFormat::Standard(StandardFormat::DXT2Or3),
            0x0000000f => D3DFormat::Standard(StandardFormat::DXT4Or5),
            unknown_format => {
                println!(
                    "Unimplemented format found {}. Assuming A8B8G8R8.",
                    unknown_format
                );
                D3DFormat::Linear(LinearColour::A8R8G8B8)
            }
        };

        let header_size = cur.read_u32::<LittleEndian>()?;
        let width = cur.read_u16::<LittleEndian>()?;
        let height = cur.read_u16::<LittleEndian>()?;
        let flags = cur.read_u32::<LittleEndian>()?;
        let unknown_3a = cur.read_u32::<LittleEndian>()?;
        let texture_offset = cur.read_u32::<LittleEndian>()?;
        let texture_size = cur.read_u32::<LittleEndian>()?;

        Ok(TextureDescriptor {
            format,
            header_size,
            width,
            height,
            flags,
            unknown_3a,
            texture_offset,
            texture_size,
        })
    }

    fn size(&self) -> usize {
        TEXTURE_DESCRIPTOR_SIZE
    }

    fn asset_type() -> AssetType {
        AssetType::ResTexture
    }

    fn to_bytes(&self) -> Result<Vec<u8>, AssetParseError> {
        let mut bytes = vec![0x00; TEXTURE_DESCRIPTOR_SIZE];

        let mut cur = Cursor::new(&mut bytes[..]);

        cur.write_u32::<LittleEndian>(self.format().into())?;

        cur.write_u32::<LittleEndian>(self.header_size)?;
        cur.write_u16::<LittleEndian>(self.width)?;
        cur.write_u16::<LittleEndian>(self.height)?;
        cur.write_u32::<LittleEndian>(self.flags)?;
        cur.write_u32::<LittleEndian>(self.unknown_3a)?;
        cur.write_u32::<LittleEndian>(self.texture_offset)?;
        cur.write_u32::<LittleEndian>(self.texture_size)?;

        Ok(bytes)
    }
}

impl AssetLike for Texture {
    type Descriptor = TextureDescriptor;

    fn new(
        descriptor: &Self::Descriptor,
        virtual_res: &VirtualResource,
    ) -> Result<Self, AssetParseError> {
        if virtual_res.is_empty() {
            return Err(AssetParseError::InvalidDataViews(
                "Unable to create a Texture using 0 data views".to_string(),
            ));
        }

        let offset = descriptor.texture_offset as usize;
        let size = descriptor.texture_size as usize;

        let bytes = match virtual_res.get_bytes(offset, size) {
            Ok(b) => b,
            Err(e) => {
                match e {
                    VirtualResourceError::OffsetOutOfBounds => {
                        return Err(AssetParseError::InvalidDataViews(format!(
                            "Offset {} is out of bounds for virtual resource of size {}",
                            offset,
                            virtual_res.len()
                        )));
                    }

                    VirtualResourceError::SizeOutOfBounds => {
                        return Err(AssetParseError::InvalidDataViews(format!(
                            "Size would reach offset {}, which is out of bounds for virtual resource of size {}",
                            offset + size,
                            virtual_res.len()
                        )));
                    }
                };
            }
        };

        Ok(Texture {
            descriptor: descriptor.clone(),
            bytes,
        })
    }

    fn get_descriptor(&self) -> Self::Descriptor {
        self.descriptor.clone()
    }

    fn get_resource_chunks(&self) -> Option<Vec<Vec<u8>>> {
        Some(vec![self.bytes.clone()]) // Single view of the texture bytes
    }
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
        if data.len() < width * height * 4 {
            return Err(TextureError::SizeMismatch);
        } else if width != self.descriptor().width as usize
            || height != self.descriptor().height as usize
        {
            return Err(TextureError::SizeMismatch);
        }

        let transcoded = crate::images::transcode(
            self.descriptor().width as usize,
            self.descriptor().height as usize,
            D3DFormat::Swizzled(Swizzled::R8G8B8A8),
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

        let tex_desc = TextureDescriptor::from_bytes(&data).unwrap();
        assert_eq!(tex_desc.format, D3DFormat::Standard(StandardFormat::DXT1));
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

        let tex_desc = TextureDescriptor::from_bytes(&data).unwrap();
        assert_eq!(tex_desc.format, D3DFormat::Standard(StandardFormat::DXT1));
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

        let desc = TextureDescriptor::from_bytes(descriptor_bytes).map_err(|e| {
            format!(
                "Failed to create texture descriptor from test bytes. Error: {}",
                e
            )
        })?;

        let _tex = Texture::new(desc, resource_bytes.to_vec());

        Ok(())
    }
}
