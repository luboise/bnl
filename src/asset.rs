use std::{
    cmp,
    fmt::{self, Display},
    io::{self, Cursor, Read, Write},
    path::Path,
};

use crate::{AssetMetadata, DataView, RawAssetData, VirtualResourceError};

use binrw::BinReaderExt;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use num_enum::{IntoPrimitive, TryFromPrimitive};

pub mod param;

// pub mod marker;
pub mod aidlist;
pub mod anim;
pub mod cuelist;
pub mod cutscene;
pub mod font;
pub mod loctext;
pub mod model;
pub mod script;
pub mod texture;

#[derive(Debug, Clone)]
pub struct Asset<AD> {
    pub metadata: AssetMetadata,
    pub data: AD,
}

impl<AD> Asset<AD> {
    pub fn metadata(&self) -> &AssetMetadata {
        &self.metadata
    }

    #[deprecated(note = "Use Asset.metadata.name() instead")]
    pub fn name(&self) -> &str {
        self.metadata.name()
    }

    pub fn asset(&self) -> &AD {
        &self.data
    }
    pub fn asset_mut(&mut self) -> &mut AD {
        &mut self.data
    }

    pub fn to_raw_asset(self) -> Result<crate::RawAsset, crate::Error>
    where
        AD: TryInto<RawAssetData, Error = crate::Error>,
    {
        Ok(crate::RawAsset {
            metadata: self.metadata,
            data: self.data.try_into()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DataViewList {
    pub(crate) size: u32,
    pub(crate) num_views: u32,
    pub(crate) views: Vec<DataView>,
}

impl DataViewList {
    pub fn from_bytes(view_bytes: &[u8]) -> Result<DataViewList, crate::Error> {
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
    FileNotFound(String),
}

impl std::error::Error for AssetParseError {}

impl From<std::io::Error> for AssetParseError {
    fn from(e: std::io::Error) -> Self {
        AssetParseError::InvalidDataViews(
            format!("IO error occurred when parsing Asset.\nError: {}", e).to_string(),
        )
    }
}

impl fmt::Display for AssetParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::ParserNotImplemented => "Parser not implemented".to_string(),
                Self::ErrorParsingDescriptor => "Error parsing descriptor".to_string(),
                Self::InputTooSmall => "Input too small".to_string(),
                Self::InvalidDataViews(e) => format!("Invalid data views: {e}"),
                Self::FileNotFound(e) => format!("File not found: {e}"),
            }
        )
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
        match self {
            AssetError::ParseError(asset_parse_error) => write!(f, "{asset_parse_error}"),
            AssetError::TypeMismatch => write!(f, "Type mismatch"),
            AssetError::NotFound => write!(f, "Not found"),
        }
    }
}

pub trait DumpToDir: Dump {
    fn dump_to_dir<P: AsRef<Path>>(&self, dump_dir: P) -> Result<(), std::io::Error>;
}

pub trait Dump {
    fn dump<P: AsRef<Path>>(&self, dump_path: P) -> Result<(), Box<dyn std::error::Error>>;
}

/// Parses a naturally serialised version of an asset. This is NOT used with descriptors, but
/// instead with any human readable output formats. For example, an AidList resource as a text file
/// with each AID being on a separate line.
pub trait Parse: Sized {
    fn parse<P: AsRef<Path>>(parse_path: P) -> Result<Self, AssetParseError>;
}

pub trait AssetData: Sized + TryFrom<RawAssetData> + TryInto<RawAssetData> {
    const ASSET_TYPE: AssetType;

    fn asset_type() -> AssetType {
        Self::ASSET_TYPE
    }
}

pub type AssetName = [u8; 128];
pub const MAX_ASSET_NAME_LENGTH: usize = size_of::<AssetName>() - 1;

pub const ASSET_DESCRIPTION_SIZE: usize = 0xa0;

#[derive(Clone)]
pub struct AssetDescription {
    pub(crate) metadata: AssetMetadata,

    pub(crate) chunk_count: u32,

    pub(crate) descriptor_ptr: u32,
    pub(crate) descriptor_size: u32,
    pub(crate) dataview_list_ptr: u32,
    pub(crate) resource_size: u32, // The total size needed for this asset, including its descriptor list
}

// Taken from project_grabbed
// https://github.com/x1nixmzeng/project-grabbed
#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum AssetType {
    Texture = 1,
    Anim = 2,
    Unknown3 = 3,
    Model = 4,
    AnimEvents = 5,

    Cutscene = 7,
    CutsceneEvents = 8,

    Misc = 10,
    ActorGoals = 11,
    Marker = 12,
    FxCallout = 13,
    AidList = 14,

    Loctext = 16,

    XSoundbank = 18,
    XDSP = 19,
    XCueList = 20,
    Font = 21,
    Ghoulybox = 22,
    Ghoulyspawn = 23,
    Script = 24,
    ActorAttribs = 25,
    Emitter = 26,
    Particle = 27,
    Rumble = 28,
    ShakeCam = 29,
    Count = 30,

    // Helper to let RawAssetData satisfy AssetLike
    Raw = 31,
}

impl Ord for AssetType {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        let x: u32 = (*self).into();
        let y: u32 = (*other).into();
        x.cmp(&y)
    }
}

impl PartialOrd for AssetType {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for AssetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                AssetType::Texture => "Texture",
                AssetType::Anim => "Anim",
                AssetType::Unknown3 => "Unknown3",
                AssetType::Model => "Model",
                AssetType::AnimEvents => "AnimEvents",
                AssetType::Cutscene => "Cutscene",
                AssetType::CutsceneEvents => "CutsceneEvents",
                AssetType::Misc => "Misc",
                AssetType::ActorGoals => "ActorGoals",
                AssetType::Marker => "Marker",
                AssetType::FxCallout => "FxCallout",
                AssetType::AidList => "AidList",
                AssetType::Loctext => "Loctext",
                AssetType::XSoundbank => "XSoundbank",
                AssetType::XDSP => "XDSP",
                AssetType::XCueList => "XCueList",
                AssetType::Font => "Font",
                AssetType::Ghoulybox => "Ghoulybox",
                AssetType::Ghoulyspawn => "Ghoulyspawn",
                AssetType::Script => "Script",
                AssetType::ActorAttribs => "ActorAttribs",
                AssetType::Emitter => "Emitter",
                AssetType::Particle => "Particle",
                AssetType::Rumble => "Rumble",
                AssetType::ShakeCam => "ShakeCam",
                AssetType::Count => "Count",
                AssetType::Raw => "Raw",
            }
        )
    }
}

impl TryFrom<&str> for AssetType {
    type Error = AssetError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "texture" => Ok(AssetType::Texture),
            "anim" => Ok(AssetType::Anim),
            "animevents" => Ok(AssetType::AnimEvents),
            "actorgoals" => Ok(AssetType::ActorGoals),
            "unknown3" => Ok(AssetType::Unknown3),
            "model" => Ok(AssetType::Model),
            "animevent" => Ok(AssetType::AnimEvents),
            "cutscene" => Ok(AssetType::Cutscene),
            "cutsceneevents" => Ok(AssetType::CutsceneEvents),
            "misc" => Ok(AssetType::Misc),
            "actorgoal" => Ok(AssetType::ActorGoals),
            "marker" => Ok(AssetType::Marker),
            "callout" => Ok(AssetType::FxCallout),
            "aidlist" => Ok(AssetType::AidList),
            "loctext" => Ok(AssetType::Loctext),
            "soundbank" => Ok(AssetType::XSoundbank),
            "dsp" => Ok(AssetType::XDSP),
            "cue" => Ok(AssetType::XCueList),
            "font" => Ok(AssetType::Font),
            "ghoulybox" => Ok(AssetType::Ghoulybox),
            "ghoulyspawn" => Ok(AssetType::Ghoulyspawn),
            "script" => Ok(AssetType::Script),
            "actorattribs" => Ok(AssetType::ActorAttribs),
            "fxemitter" => Ok(AssetType::Emitter),
            "fxparticle" => Ok(AssetType::Particle),
            "fxrumble" => Ok(AssetType::Rumble),
            "shakecam" => Ok(AssetType::ShakeCam),
            "xsoundbank" => Ok(AssetType::XSoundbank),
            _ => Err(AssetError::TypeMismatch),
        }
    }
}

impl AssetDescription {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, std::io::Error> {
        let mut cur = Cursor::new(&bytes);

        let mut name: AssetName = [0u8; 0x80];
        cur.read_exact(&mut name)?;

        let asset_type = AssetType::try_from(cur.read_u32::<LittleEndian>()?)
            .map_err(|_| std::io::Error::other("Unable to parse asset type from BNL."))?;

        let unk_1 = cur.read_u32::<LittleEndian>()?;
        let unk_2 = cur.read_u32::<LittleEndian>()?;

        let metadata = AssetMetadata {
            name,
            asset_type,
            unk_1,
            unk_2,
        };

        let asset_description = AssetDescription {
            metadata,
            chunk_count: cur.read_u32::<LittleEndian>()?,
            descriptor_ptr: cur.read_u32::<LittleEndian>()?,
            descriptor_size: cur.read_u32::<LittleEndian>()?,
            dataview_list_ptr: cur.read_u32::<LittleEndian>()?,
            resource_size: cur.read_u32::<LittleEndian>()?,
        };

        Ok(asset_description)
    }

    pub fn to_bytes(&self) -> [u8; ASSET_DESCRIPTION_SIZE] {
        let mut bytes = [0x00; ASSET_DESCRIPTION_SIZE];

        let mut cur = Cursor::new(&mut bytes[..]);

        // Ensure the size of the name is 128 so that we can safely unwrap
        assert_eq!(size_of_val(&self.metadata.name), 0x80);
        cur.write_all(&self.metadata.name).unwrap();

        cur.write_u32::<LittleEndian>(self.metadata.asset_type.into())
            .unwrap();
        cur.write_u32::<LittleEndian>(self.metadata.unk_1).unwrap();
        cur.write_u32::<LittleEndian>(self.metadata.unk_2).unwrap();
        cur.write_u32::<LittleEndian>(self.chunk_count).unwrap();
        cur.write_u32::<LittleEndian>(self.descriptor_ptr).unwrap();
        cur.write_u32::<LittleEndian>(self.descriptor_size).unwrap();
        cur.write_u32::<LittleEndian>(self.dataview_list_ptr)
            .unwrap();
        cur.write_u32::<LittleEndian>(self.resource_size).unwrap();

        bytes
    }

    // Getters
    pub fn name(&self) -> &str {
        self.metadata.name()
    }
    pub fn has_raw_data(&self) -> bool {
        self.resource_size > 0
    }
    pub fn asset_type(&self) -> AssetType {
        self.metadata.asset_type
    }
    pub fn unk_1(&self) -> u32 {
        self.metadata.unk_1
    }
    pub fn unk_2(&self) -> u32 {
        self.metadata.unk_2
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
        f.debug_struct("HeaderEntry")
            .field("name", &self.name())
            .field("res_type", &self.metadata.asset_type)
            .field("unk_1", &self.metadata.unk_1)
            .field("unk_2", &self.metadata.unk_2)
            .field("chunk_count", &self.chunk_count)
            .field("descriptor_ptr", &self.descriptor_ptr)
            .field("descriptor_size", &self.descriptor_size)
            .field("bufferview_list_ptr", &self.dataview_list_ptr)
            .field("resource_size", &self.resource_size)
            .finish()
    }
}

/// hashes and AID using Grabbed By The Ghoulies' hashing method, which ignores aid_ and hashes the
/// rest.
pub fn hash_aid(aid: impl AsRef<[u8]>) -> u32 {
    let aid = aid.as_ref();

    let mut hashed = 0u32;

    for c in aid.iter().copied().skip(4) {
        // AID is null terminated
        if c == 0 {
            break;
        }

        (hashed, _) = u32::from(c & 0xdf).overflowing_add(hashed * 0x10);

        let hash_flag = hashed & 0xf0000000;
        if hash_flag > 0 {
            hashed ^= hash_flag.overflowing_shr(24).0 | hash_flag
        }
    }

    hashed
}

impl AssetData for crate::xsb::XSoundbank {
    const ASSET_TYPE: AssetType = AssetType::XSoundbank;
}

#[binrw::binrw]
#[derive(Debug, Clone)]
pub struct DemandHeader {
    pub demand_asset_type: crate::asset::AssetType,
    pub unknown_u32_1: u32,
    pub unknown_u32_2: u32,
    pub num_chunks: u32,
    pub descriptor_ptr: u32,
    pub descriptor_size: u32,
    pub resource_view_ptr: u32,
    pub total_resource_size: u32,
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

    #[test]
    fn asset_name_hashing() {
        assert_eq!(
            hash_aid("aid_xwavebank_ghoulies_dvd0"),
            0x099edd10,
            "AID hash does not match."
        );
    }
}
