use std::{
    cmp,
    fmt::{self, Display},
    io::{self, Cursor, Read, Write},
};

use crate::{
    BNLAsset, DataView, VirtualResource, VirtualResourceError,
    asset::model::sub_main::SubresourceError, game::AssetType,
};

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

pub mod param;

// pub mod marker;
pub mod model;
pub mod script;
pub mod texture;

#[derive(Debug, Clone)]
pub struct RawAsset {
    pub name: String,
    pub asset_type: AssetType,
    pub descriptor_bytes: Vec<u8>,
    pub data_slices: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct DataViewList {
    pub(crate) size: u32,
    pub(crate) num_views: u32,
    pub(crate) views: Vec<DataView>,
}

impl DataViewList {
    pub fn from_bytes(view_bytes: &[u8]) -> Result<DataViewList, Box<io::Error>> {
        if view_bytes.len() < 8 {
            return Err(Box::new(io::Error::new(
                io::ErrorKind::InvalidData,
                "Data not long enough",
            )));
        };

        let b = view_bytes[0..4]
            .try_into()
            .expect("slice with incorrect length");
        let size = u32::from_le_bytes(b);

        let b = view_bytes[4..8]
            .try_into()
            .expect("slice with incorrect length");
        let num_views = u32::from_le_bytes(b);

        if num_views == 0 || size != num_views * size_of::<DataView>() as u32 + 8 {
            return Err(Box::new(io::Error::other("Invalid size.")));
        }

        if view_bytes.len() < num_views as usize * size_of::<DataView>() {
            return Err(
                io::Error::new(io::ErrorKind::InvalidData, "Input is not large enough.").into(),
            );
        }

        let mut views = Vec::with_capacity(num_views as usize);

        let mut chunks = view_bytes[8..].chunks(size_of::<DataView>());

        for _ in 0..num_views {
            let chunk = chunks.next().unwrap();

            let view_offset = u32::from_le_bytes(chunk[0..4].try_into().unwrap());
            let view_size = u32::from_le_bytes(chunk[4..8].try_into().unwrap());

            views.push(DataView {
                offset: view_offset,
                size: view_size,
            });
        }

        Ok(DataViewList {
            size,
            num_views,
            views,
        })
    }

    pub fn slices<'a>(&self, data: &'a [u8]) -> Result<Vec<&'a [u8]>, io::Error> {
        if self.num_views as usize != self.views.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid BufferViewList: num_views {} doesn't match the actual size of the views Vec {}",
                    self.num_views,
                    self.views.len()
                ),
            ));
        }

        Ok(self
            .views
            .iter()
            .map(|view| -> &[u8] {
                let start = view.offset as usize;
                let end = start + view.size as usize;
                &data[start..end]
            })
            .collect())
    }

    pub fn write_bytes(
        &self,
        bytes: &[u8],
        resource: &mut [u8],
    ) -> Result<(), VirtualResourceError> {
        let dvl_size = self.bytes_required();
        let write_size = bytes.len();

        if dvl_size != write_size {
            eprintln!("Write size does not match dvl.");
            return Err(VirtualResourceError::SizeOutOfBounds);
        }

        /*
        if end < write_offset {
            return Err(VirtualResourceError::OffsetOutOfBounds);
        } else if end - write_offset < write_size {
            return Err(VirtualResourceError::SizeOutOfBounds);
        }
        */

        let mut total_written = 0usize;

        for view in self.views() {
            let view_size = view.size as usize;

            // If this slice is part of the copy in any way
            let res_slice =
                &mut resource[view.offset as usize..view.offset as usize + view.size as usize];

            let desired_cp_size = write_size - total_written;
            let cp_size = cmp::min(desired_cp_size, view_size);

            res_slice[..cp_size].copy_from_slice(&bytes[total_written..total_written + cp_size]);

            total_written += cp_size;

            if total_written > write_size {
                return Err(VirtualResourceError::SizeOutOfBounds);
            } else if total_written == write_size {
                break;
            }
        }

        if total_written != write_size {
            return Err(VirtualResourceError::SizeOutOfBounds);
        }

        Ok(())
    }

    pub fn bytes_required(&self) -> usize {
        self.views().iter().map(|view| view.size as usize).sum()
    }

    pub fn views(&self) -> &[DataView] {
        &self.views
    }

    pub fn num_views(&self) -> u32 {
        self.num_views
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn overlaps(&self, other: &DataViewList) -> bool {
        self.views
            .iter()
            .zip(&other.views)
            .any(|(v1, v2)| v1.overlaps(v2))
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let num_views = self.views.len();

        let size = 8 + 8 * num_views;

        let mut v = vec![0x00; size];

        let mut cur = Cursor::new(&mut v[..]);

        cur.write_u32::<LittleEndian>(size as u32).unwrap();
        cur.write_u32::<LittleEndian>(num_views as u32).unwrap();

        self.views.iter().for_each(|view| {
            cur.write_u32::<LittleEndian>(view.offset)
                .expect("View offset should've been accounted for.");
            cur.write_u32::<LittleEndian>(view.size)
                .expect("View size should've been accounted for.");
        });

        v
    }
}

#[derive(Debug)]
pub enum AssetParseError {
    /// The parser of a given type was not implemented, and the asset was not about to be parsed.
    // TODO: Remove this and just make it required by the trait
    ParserNotImplemented,
    /// An error occurred when parsing the [`Asset::Descriptor`] of the asset.
    ErrorParsingDescriptor,
    InputTooSmall,
    InvalidDataViews(String),
}

impl Display for AssetParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<std::io::Error> for AssetParseError {
    fn from(e: std::io::Error) -> Self {
        AssetParseError::InvalidDataViews(
            format!("IO error occurred when parsing Asset.\nError: {}", e).to_string(),
        )
    }
}

impl From<SubresourceError> for AssetParseError {
    fn from(_: SubresourceError) -> Self {
        Self::ErrorParsingDescriptor
    }
}

#[derive(Debug)]
pub enum AssetError {
    /// The asset was found, but could not be parsed from the bytes of the [`crate::BNLFile`].
    ParseError(AssetParseError),
    /// The asset was found, but didn't match the expected [`AssetType`]
    TypeMismatch,
    /// The asset could not be found by name
    NotFound,
}

impl fmt::Display for AssetError {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Write strictly the first element into the supplied output
        // stream: `f`. Returns `fmt::Result` which indicates whether the
        // operation succeeded or failed. Note that `write!` uses syntax which
        // is very similar to `println!`.
        write!(f, "Asset error")?;
        Ok(())
    }
}

impl From<AssetParseError> for AssetError {
    fn from(err: AssetParseError) -> Self {
        AssetError::ParseError(err)
    }
}

/// Describes how a given asset is structured. Typically, an AssetDescriptor has information about how
/// to read an asset from its associated resources, as well as attributes of that asset. For
/// example, a [`texture::TextureDescriptor`] knows the width and height of its associated texture
/// resource.
pub trait AssetDescriptor: Sized + Clone {
    /// Creates a new AssetDescriptor from bytes.
    /// TODO: Finish the docs here
    fn from_bytes(data: &[u8]) -> Result<Self, AssetParseError>;

    fn to_bytes(&self) -> Result<Vec<u8>, AssetParseError>;

    /// The serialised size of the descriptor in bytes
    fn size(&self) -> usize;

    fn asset_type() -> AssetType;
}

pub trait Asset: Sized {
    type Descriptor: AssetDescriptor;

    fn new(
        description: &AssetDescription,
        descriptor: &Self::Descriptor,
        virtual_res: &VirtualResource,
    ) -> Result<Self, AssetParseError>;

    fn description(&self) -> &AssetDescription;
    fn descriptor(&self) -> &Self::Descriptor;

    fn asset_type() -> AssetType {
        Self::Descriptor::asset_type()
    }
    fn name(&self) -> &str {
        self.description().name()
    }

    fn as_bnl_asset(&self) -> BNLAsset;
}

pub type AssetName = [u8; 128];

pub const ASSET_DESCRIPTION_SIZE: usize = 0xa0;

#[derive(Clone)]
pub struct AssetDescription {
    pub(crate) name: AssetName,
    pub(crate) asset_type: AssetType,

    pub(crate) unk_1: u32,
    pub(crate) unk_2: u32,
    pub(crate) chunk_count: u32,

    pub(crate) descriptor_ptr: u32,
    pub(crate) descriptor_size: u32,
    pub(crate) dataview_list_ptr: u32,
    pub(crate) resource_size: u32, // The total size needed for this asset, including its descriptor list

    // DO NOT SERIALISE
    pub(crate) asset_desc_index: usize,
}

impl AssetDescription {
    pub fn from_bytes(bytes: &[u8]) -> Result<AssetDescription, std::io::Error> {
        let mut cur = Cursor::new(&bytes);

        let mut asset_name: AssetName = [0u8; 0x80];
        cur.read_exact(&mut asset_name)?;

        let asset_type = AssetType::try_from(cur.read_u32::<LittleEndian>()?)
            .map_err(|_| std::io::Error::other("Unable to parse asset type from BNL."))?;

        Ok(AssetDescription {
            name: asset_name,
            asset_type,
            unk_1: cur.read_u32::<LittleEndian>()?,
            unk_2: cur.read_u32::<LittleEndian>()?,
            chunk_count: cur.read_u32::<LittleEndian>()?,
            descriptor_ptr: cur.read_u32::<LittleEndian>()?,
            descriptor_size: cur.read_u32::<LittleEndian>()?,
            dataview_list_ptr: cur.read_u32::<LittleEndian>()?,
            resource_size: cur.read_u32::<LittleEndian>()?,

            // Default of max
            asset_desc_index: usize::MAX,
        })
    }

    pub fn to_bytes(&self) -> [u8; ASSET_DESCRIPTION_SIZE] {
        let mut bytes = [0x00; ASSET_DESCRIPTION_SIZE];

        let mut cur = Cursor::new(&mut bytes[..]);

        // Ensure the size of the name is 128 so that we can safely unwrap
        assert_eq!(size_of_val(&self.name), 0x80);
        cur.write_all(&self.name).unwrap();

        cur.write_u32::<LittleEndian>(self.asset_type.into())
            .unwrap();
        cur.write_u32::<LittleEndian>(self.unk_1).unwrap();
        cur.write_u32::<LittleEndian>(self.unk_2).unwrap();
        cur.write_u32::<LittleEndian>(self.chunk_count).unwrap();
        cur.write_u32::<LittleEndian>(self.descriptor_ptr).unwrap();
        cur.write_u32::<LittleEndian>(self.descriptor_size).unwrap();
        cur.write_u32::<LittleEndian>(self.dataview_list_ptr)
            .unwrap();
        cur.write_u32::<LittleEndian>(self.resource_size).unwrap();

        bytes
    }

    pub fn name(&self) -> &str {
        std::str::from_utf8(&self.name)
            .unwrap_or("")
            .split('\0')
            .next()
            .unwrap_or("")
    }

    // Getters
    pub fn has_raw_data(&self) -> bool {
        self.resource_size > 0
    }
    pub fn asset_type(&self) -> AssetType {
        self.asset_type
    }
    pub fn unk_1(&self) -> u32 {
        self.unk_1
    }
    pub fn bufferview_list_ptr(&self) -> u32 {
        self.dataview_list_ptr
    }
    pub fn resource_size(&self) -> u32 {
        self.resource_size
    }
    pub fn descriptor_ptr(&self) -> u32 {
        self.descriptor_ptr
    }
    pub fn descriptor_size(&self) -> u32 {
        self.descriptor_size
    }
}

impl std::fmt::Debug for AssetDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match str::from_utf8(&self.name) {
            Ok(s) => s.trim_matches('\x00'),
            Err(_) => return Err(std::fmt::Error {}),
        };
        f.debug_struct("HeaderEntry")
            .field("name", &name)
            .field("res_type", &self.asset_type)
            .field("unk_1", &self.unk_1)
            .field("unk_2", &self.unk_2)
            .field("chunk_count", &self.chunk_count)
            .field("descriptor_ptr", &self.descriptor_ptr)
            .field("descriptor_size", &self.descriptor_size)
            .field("bufferview_list_ptr", &self.dataview_list_ptr)
            .field("resource_size", &self.resource_size)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_view_overlap() {
        let dv1 = DataView {
            offset: 1000,
            size: 1000,
        };

        let dv2 = DataView {
            offset: 1800,
            size: 100,
        };

        assert!(dv1.overlaps(&dv2), "[1000,2000) should overlap [1800,1900)");

        let dv3 = DataView {
            offset: 2000,
            size: 500,
        };

        assert!(
            !dv2.overlaps(&dv3),
            "[1800,1900) should not overlap [2000,2500)"
        );
    }

    #[test]
    fn dvl_overlap_tests() {
        let dvl1 = DataViewList {
            size: 24,
            num_views: 2,
            views: vec![
                DataView {
                    offset: 1000,
                    size: 1000,
                },
                DataView {
                    offset: 2000,
                    size: 1000,
                },
            ],
        };

        let dvl2 = DataViewList {
            size: 16,
            num_views: 1,
            views: vec![DataView {
                offset: 1800,
                size: 100,
            }],
        };

        let dvl3 = DataViewList {
            size: 16,
            num_views: 1,
            views: vec![DataView {
                offset: 2000,
                size: 500,
            }],
        };

        let dvl4 = DataViewList {
            size: 16,
            num_views: 1,
            views: vec![DataView {
                offset: 1999,
                size: 500,
            }],
        };

        assert!(dvl1.overlaps(&dvl2), "(1) These should overlap.");
        assert!(!dvl2.overlaps(&dvl3), "(2) These should not overlap.");
        assert!(dvl1.overlaps(&dvl4), "(3) These should overlap.");
    }
}
