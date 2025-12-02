use std::io::{Cursor, Read};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::asset::AssetParseError;

/*
pub struct LoctextKey {
 keyHash:     u16,
 offset:     u32,
    std::string::NullString16 string @ (parent.wCharsStart + offset * sizeof(u16)),
}

pub struct LoctextMapHeader {
 mapStart:     u32,
 numLoctexts:     u32,

 wCharsStart = $ + (6 * numLoctexts) + sizeof(u16) + sizeof(u32):     u32,
    LoctextKey keys[numLoctexts],
 endSentinel:     u16,
 numChars:     u32,
}

pub struct LoctextHashList{
 size:     u32,
 numHashes:     u32,
 hashes[numHashes]:     u16,
}

pub struct LoctextLocator {
 hash:     u16,
 hashIndex:     u16,
 charOffset:     u32,

    std::string::NullString key @ (parent.locatorsStart + charOffset),

}

pub struct LoctextLocatorList {
 size:     u32,
 numLocators:     u32,
}

*/

pub struct LoctextFile {
    lsbl: [u8; 4],
    values_ptr: u32,

    unknown_count_1: u16,
    unknown_count_2: u16,
    unknown_u16_1: u16,
    unknown_u32_1: u32,

    keys_ptr: u32,
    unknown_u32_2: u32,
    hash_list_ptr: u32,
}

pub struct DemandHeader {
    /// TODO: Replace with an enum later once the values are known
    pub demand_asset_type: u32,
    pub unknown_u32_1: u32,
    pub unknown_u32_2: u32,
    pub unknown_u32_3: u32,
    pub loctext_resource_header_ptr: u32,
    pub loctext_file_size: u32,
    pub unknown_u32_4: u32,
}

impl DemandHeader {
    pub fn from_cursor(cur: &mut Cursor<&[u8]>) -> Result<Self, AssetParseError> {
        Ok(Self {
            demand_asset_type: cur.read_u32::<LittleEndian>()?,
            unknown_u32_1: cur.read_u32::<LittleEndian>()?,
            unknown_u32_2: cur.read_u32::<LittleEndian>()?,
            unknown_u32_3: cur.read_u32::<LittleEndian>()?,
            loctext_resource_header_ptr: cur.read_u32::<LittleEndian>()?,
            loctext_file_size: cur.read_u32::<LittleEndian>()?,
            unknown_u32_4: cur.read_u32::<LittleEndian>()?,
        })
    }
}
