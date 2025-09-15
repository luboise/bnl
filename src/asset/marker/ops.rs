use indexmap::IndexMap;
use num_enum::{IntoPrimitive, TryFromPrimitive};

use crate::asset::param::{KnownUnknown, ParamDescriptor, ParamType, ParamsShape};

pub type MarkerType = KnownUnknown<KnownMarkerType, u32>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarkerType {
    Known(KnownMarkerType),
    Unknown(u32),
}

impl From<MarkerType> for u32 {
    fn from(val: MarkerType) -> Self {
        match val {
            MarkerType::Known(known_opcode) => known_opcode.into(),
            MarkerType::Unknown(val) => val,
        }
    }
}

impl From<u32> for MarkerType {
    fn from(value: u32) -> Self {
        match value.try_into() {
            Ok(known) => MarkerType::Known(known),
            Err(_) => MarkerType::Unknown(value),
        }
    }
}

#[derive(Debug, Clone, Copy, TryFromPrimitive, IntoPrimitive, PartialEq)]
#[repr(u32)]
pub enum KnownMarkerType {
    Powerup,
}

impl KnownMarkerType {
    pub fn get_shape(&self) -> ParamsShape {
        let mut map: ParamsShape = IndexMap::new();

        match self {
            KnownMarkerType::Powerup => {
                map.insert("background_aid".to_string(), ParamDescriptor {
                                            param_type: ParamType::String(0x80),
                                            description:
                                                "The asset ID of the background to be loaded at the beginning of the scene."
                                                    .to_string(),
                                        });
            }
            KnownMarkerType::SetSceneName => {
                map.insert(
                    "scene_name".to_string(),
                    ParamDescriptor {
                        param_type: ParamType::String(0x40),
                        description:
                            "The name of the current scene as a string (eg. Scummy Scullery)"
                                .to_string(),
                    },
                );
                map.insert(
                    "unknown1".to_string(),
                    ParamDescriptor {
                        param_type: ParamType::Bytes(4),
                        description: "Unknown value of size 4 bytes. Suspected to be a u32."
                            .to_string(),
                    },
                );
                map.insert(
                    "unknown2".to_string(),
                    ParamDescriptor {
                        param_type: ParamType::Bytes(4),
                        description: "Unknown value of size 4 bytes. Suspected to be a f32."
                            .to_string(),
                    },
                );
                map.insert(
                    "unknown3".to_string(),
                    ParamDescriptor {
                        param_type: ParamType::Bytes(4),
                        description: "Unknown value of size 4 bytes. Suspected to be a f32."
                            .to_string(),
                    },
                );
            }
            KnownMarkerType::CreateTimeLimitChallenge => {
                map.insert(
                    "duration".to_string(),
                    ParamDescriptor {
                        param_type: ParamType::F32,
                        description: "The duration of the timer in the challenge.".to_string(),
                    },
                );
            }
            KnownMarkerType::SpawnGhoulieWithBox => {
                map.insert(
                    "ghoulybox_aid".to_string(),
                    ParamDescriptor {
                        param_type: ParamType::String(0x80),
                        description: "The asset ID of the ghoulybox that will be spawned."
                            .to_string(),
                    },
                );

                map.insert(
                    "spawn_count".to_string(),
                    ParamDescriptor {
                        param_type: ParamType::U32,
                        description: "The number of entities spawned? (Not 100% sure on this)"
                            .to_string(),
                    },
                );

                map.insert(
                    "actor_attribs_aid".to_string(),
                    ParamDescriptor {
                        param_type: ParamType::String(0x80),
                        description: "The asset ID of the actor attribs asset that will be used."
                            .to_string(),
                    },
                );
            }
            KnownMarkerType::PlayWalkinCutscene => {
                map.insert(
                                            "cutscene_aid".to_string(),
                                            ParamDescriptor {
                                                param_type: ParamType::String(0x80),
                                                description: "The asset ID of the cutscene to be played on room walk in (eg. aid_cutscene_ghoulies_roomwalkins_walkina)".to_string(),
                                            },
                                        );
            }
            KnownMarkerType::PlaySound => {
                map.insert(
                                            "soundbank_id".to_string(),
                                            ParamDescriptor {
                                                param_type: ParamType::String(0x80),
                                                description: "The soundbank ID of the audio to be played. (eg. XACT_SOUNDBANK_GZOMBIE_DISAPPOINTED)"
                                                    .to_string(),
                                            },
                                        );
            }
            KnownMarkerType::CreateKillAllByTagChallenge => {
                map.insert(
                                            "actor_tag".to_string(),
                                            ParamDescriptor {
                                                param_type: ParamType::String(0x40),
                                                description: "The tag of the actor which must be killed in the challenge. (eg. objTag_Actor_Zombie)"
                                                    .to_string(),
                                            },
                                        );

                map.insert(
                                            "unknownU32".to_string(),
                                            ParamDescriptor {
                                                param_type: ParamType::U32,
                                                description: "Unknown U32 value. Has a value of 1 typically even for kill all challenges."
                                                    .to_string(),
                                            },
                                        );
            }
            KnownMarkerType::SetPlayerHealth => {
                map.insert(
                    "health".to_string(),
                    ParamDescriptor {
                        param_type: ParamType::U32,
                        description:
                            "The amount of health that the player will start this room with."
                                .to_string(),
                    },
                );
            }
            KnownMarkerType::CreateFindTheGhoulieKeyChallenge
            | KnownMarkerType::CreateWeaponsOnlyChallenge
            | KnownMarkerType::CreateFindTheKeyChallenge
            | KnownMarkerType::CreateNoBreakHouseChallenge => (),
        }

        map
    }

    pub fn operands_size(&self) -> usize {
        match self {
            KnownMarkerType::EndMarker => 0x00,
            KnownMarkerType::CreateTimeLimitChallenge => 0x4,
            KnownMarkerType::CreateKillAllByTagChallenge => 0x40 + 0x4,
            KnownMarkerType::CreateFindTheGhoulieKeyChallenge
            | KnownMarkerType::CreateFindTheKeyChallenge
            | KnownMarkerType::CreateNoBreakHouseChallenge
            | KnownMarkerType::CreateWeaponsOnlyChallenge => 0x00,
            KnownMarkerType::SetBackground => 0x80,
            KnownMarkerType::SetSceneName => 0x48,
            KnownMarkerType::SpawnGhoulieWithBox => 0x108,
            KnownMarkerType::PlayWalkinCutscene => 0x80,
            KnownMarkerType::PlaySound => 0x80,
            KnownMarkerType::SetPlayerHealth => 0x4,
        }
    }
}
