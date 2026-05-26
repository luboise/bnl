use binrw::{BinReaderExt, BinWrite};

use crate::asset::{AssetData, AssetType};

impl Script {
    pub fn operations(&self) -> &[ScriptOperation] {
        &self.operations
    }

    pub fn operations_mut(&mut self) -> &mut Vec<ScriptOperation> {
        &mut self.operations
    }
}

#[derive(Debug, Clone)]
pub enum ScriptError {
    SizeMismatch,
    InvalidInput,
    UnsupportedOutputType,
}

#[derive(Debug)]
#[binrw::binrw]
#[br(little)]
#[bw(little)]
pub struct Script {
    #[br(parse_with = binrw::helpers::until_eof)]
    operations: Vec<ScriptOperation>,
}

#[derive(Debug, Clone)]
#[binrw::binrw]
#[br(little)]
#[bw(little)]
pub enum ScriptOperation {
    #[brw(magic = b"\x08\x00\x00\x00\x00\x00\x00\x00")]
    EndScript,

    #[brw(magic = b"\x88\x00\x00\x00\x01\x00\x00\x00")]
    SetBackground { background_aid: super::AssetName },

    #[brw(magic = b"\x0c\x00\x00\x00\x07\x00\x00\x00")]
    SetPlayerHealth { health: u32 },

    #[brw(magic = b"\x54\x00\x00\x00\x0a\x00\x00\x00")]
    SetSceneName {
        name: [u8; 0x40],
        unknown_u32_1: u32,
        unknown_u32_2: u32,
        unknown_u32_3: u32,
    },

    #[brw(magic = b"\x08\x00\x00\x00\x0f\x00\x00\x00")]
    WaitToMoveOn,

    #[brw(magic = b"\x08\x00\x00\x00\x11\x00\x00\x00")]
    Signal0x11,

    #[brw(magic = b"\x08\x00\x00\x00\x18\x00\x00\x00")]
    Signal0x18,

    #[brw(magic = b"\x0c\x00\x00\x00\x1a\x00\x00\x00")]
    CreateTimeLimitChallenge { time_limit: f32 },

    #[brw(magic = b"\x4c\x00\x00\x00\x1c\x00\x00\x00")]
    CreateKillAllByTagChallenge { actor_tag: [u8; 40], some_u32: u32 },

    #[brw(magic = b"\x08\x00\x00\x00\x1f\x00\x00\x00")]
    CreateFindTheGhoulieKeyChallenge,

    #[brw(magic = b"\x0c\x01\x00\x00\x2a\x00\x00\x00")]
    SpawnGhoulieWithBox {
        ghoulybox_aid: super::AssetName,
        spawn_count: u32,
        actor_attribs_aid: super::AssetName,
    },

    #[brw(magic = b"\x08\x00\x00\x00\x23\x00\x00\x00")]
    CreateWeaponsOnlyChallenge,

    #[brw(magic = b"\x08\x00\x00\x00\x27\x00\x00\x00")]
    CreateFindTheKeyChallenge,

    #[brw(magic = b"\x08\x00\x00\x00\x28\x00\x00\x00")]
    CreateNoBreakHouseChallenge,

    #[brw(magic = b"\x18\x00\x00\x00\x29\x00\x00\x00")]
    UpdateDoor {
        door_id: u32,
        /// 1 = open, 0 = closed
        open_status: u32,
        some_u32_1: u32,
        some_u32_2: u32,
    },

    #[brw(magic = b"\x88\x00\x00\x00\x53\x00\x00\x00")]
    PlayWalkinCutscene {
        walkin_cutscene_aid: super::AssetName,
    },

    #[brw(magic = b"\x88\x00\x00\x00\x8d\x00\x00\x00")]
    PlaySound { audio_id: super::AssetName },

    Raw {
        // TODO: Calc this based on data size
        size: u32,
        opcode: u32,
        #[br(count = size - 8)]
        data: Vec<u8>,
    },
}

impl ScriptOperation {
    pub fn opcode(&self) -> Result<Opcode, u32> {
        match self {
            ScriptOperation::EndScript => Ok(Opcode::EndScript),
            ScriptOperation::SetBackground { .. } => Ok(Opcode::SetBackground),
            ScriptOperation::SetPlayerHealth { .. } => Ok(Opcode::SetPlayerHealth),
            ScriptOperation::WaitToMoveOn => Ok(Opcode::WaitToMoveOn),
            ScriptOperation::Signal0x11 => Err(0x11),
            ScriptOperation::Signal0x18 => Err(0x18),
            ScriptOperation::CreateTimeLimitChallenge { .. } => {
                Ok(Opcode::CreateTimeLimitChallenge)
            }
            ScriptOperation::SetSceneName { .. } => Ok(Opcode::SetSceneName),
            ScriptOperation::CreateKillAllByTagChallenge { .. } => {
                Ok(Opcode::CreateKillAllByTagChallenge)
            }
            ScriptOperation::CreateFindTheGhoulieKeyChallenge => {
                Ok(Opcode::CreateFindTheGhoulieKeyChallenge)
            }
            ScriptOperation::SpawnGhoulieWithBox { .. } => Ok(Opcode::SpawnGhoulieWithBox),
            ScriptOperation::CreateWeaponsOnlyChallenge => Ok(Opcode::CreateWeaponsOnlyChallenge),
            ScriptOperation::CreateFindTheKeyChallenge => Ok(Opcode::CreateFindTheKeyChallenge),
            ScriptOperation::CreateNoBreakHouseChallenge => Ok(Opcode::CreateNoBreakHouseChallenge),
            ScriptOperation::UpdateDoor { .. } => Ok(Opcode::UpdateDoor),
            ScriptOperation::PlayWalkinCutscene { .. } => Ok(Opcode::PlayWalkinCutscene),
            ScriptOperation::PlaySound { .. } => Ok(Opcode::PlaySound),
            ScriptOperation::Raw {
                size: _,
                opcode,
                data: _,
            } => Err(*opcode),
        }
    }
}

impl AssetData for Script {
    const ASSET_TYPE: AssetType = AssetType::Script;
}

impl TryFrom<crate::RawAssetData> for Script {
    type Error = crate::Error;

    fn try_from(value: crate::RawAssetData) -> Result<Self, Self::Error> {
        let crate::RawAssetData {
            descriptor_bytes,
            resource_chunks: _,
        } = value;

        Ok(std::io::Cursor::new(descriptor_bytes).read_le()?)
    }
}

impl TryFrom<Script> for crate::RawAssetData {
    type Error = crate::Error;

    fn try_from(value: Script) -> Result<Self, Self::Error> {
        let mut descriptor_bytes = vec![];
        value.write_le(&mut std::io::Cursor::new(&mut descriptor_bytes))?;

        Ok(crate::RawAssetData {
            descriptor_bytes,
            resource_chunks: vec![],
        })
    }
}

#[derive(Debug, Clone, Copy, num_enum::TryFromPrimitive, num_enum::IntoPrimitive, PartialEq)]
#[repr(u32)]
pub enum Opcode {
    EndScript = 0x0,
    SetBackground = 0x1,

    SetPlayerHealth = 0x7,

    SetSceneName = 0xa,

    // SetPlayState = 0xe, // eg. Free Play
    WaitToMoveOn = 0x0f,
    // Signal11 = 0x11,

    // Signal18 = 0x18,
    CreateTimeLimitChallenge = 0x1a,

    CreateKillAllByTagChallenge = 0x1c,

    CreateFindTheGhoulieKeyChallenge = 0x1f,
    // CreateXChallenge = 0x1b,
    SpawnGhoulieWithBox = 0x2a, // Box then Attribs

    CreateWeaponsOnlyChallenge = 0x23,
    CreateFindTheKeyChallenge = 0x27,
    CreateNoBreakHouseChallenge = 0x28,
    UpdateDoor = 0x29,

    // Signal2f = 0x2f,
    // Signal30 = 0x30,

    // g10x32 = 0x32,
    // g10x33 = 0x33,
    // g10x34 = 0x34,
    // g10x35 = 0x35,
    // g10x36 = 0x36,
    // g10x37 = 0x37,
    // g10x38 = 0x38,

    // Unknown39 = 0x39,
    // Signal3b = 0x3b,
    // Signal3c = 0x3c,

    // Signal45 = 0x45,
    PlayWalkinCutscene = 0x53, // ?

    // SetChallengeId = 0x7a,
    PlaySound = 0x8d,
}
