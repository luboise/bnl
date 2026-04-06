use std::{collections::HashMap, fs, io, path::Path};

use crate::{
    BNLFile,
    asset::{AssetDescriptor, AssetLike, AssetParseError, AssetType, Parse, aidlist::AidList},
};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ModSpecification {
    version: u32,
    name: String,
}

#[derive(Debug)]
pub struct RawAssetOverride {
    pub asset_type: AssetType,
    pub descriptor_bytes: Vec<u8>,
    pub resource_bytes: Vec<u8>,
}

#[derive(Debug)]
pub enum ModErrorType {
    SpecificationError,
    AssetOverrideError,
}

impl std::fmt::Display for ModErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::SpecificationError => "SpecificationError",
                Self::AssetOverrideError => "AssetOverrideError",
            }
        )
    }
}

#[derive(Debug)]
pub struct ModError {
    error_type: ModErrorType,
    details: String,
}

impl std::fmt::Display for ModError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.error_type, self.details)
    }
}

impl std::error::Error for ModError {}

impl From<io::Error> for ModError {
    fn from(value: io::Error) -> Self {
        Self {
            error_type: ModErrorType::SpecificationError,
            details: format!("IO Error: {value}"),
        }
    }
}

impl From<AssetParseError> for ModError {
    fn from(_: AssetParseError) -> Self {
        Self {
            error_type: ModErrorType::SpecificationError,
            details: "Unable to parse asset.".to_string(),
        }
    }
}

pub trait ModLike {
    type Descriptor: AssetDescriptor;
    fn apply(&self, descriptor: &mut Self::Descriptor) -> Result<(), Box<dyn std::error::Error>>;
}

#[derive(Debug)]
pub struct Mod {
    spec: ModSpecification,
    raw_overrides: HashMap<String, RawAssetOverride>,
    cutscene_mods: HashMap<String, crate::asset::cutscene::CutsceneMod>,
}

impl Mod {
    pub fn new<S: AsRef<str>>(name: S) -> Self {
        Self {
            spec: ModSpecification {
                version: 0,
                name: name.as_ref().to_string(),
            },
            raw_overrides: Default::default(),
            cutscene_mods: HashMap::new(),
        }
    }

    /// Reads a mod on disk from a path
    pub fn from_dir<P: AsRef<Path>>(mod_dir: P) -> Result<Mod, ModError> {
        // Locate dirs
        let root_dir = fs::read_dir(&mod_dir)?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, io::Error>>()?;

        let mod_root_file = root_dir
            .iter()
            .find(|file| file.is_file() && file.file_name().unwrap_or_default() == "mod.json")
            .ok_or(ModError {
                error_type: ModErrorType::SpecificationError,
                details: format!(
                    "Unable to find root mod.json file in {}",
                    mod_dir.as_ref().display()
                ),
            })?;

        let raw_override_dirs = fs::read_dir(
            root_dir
                .iter()
                .find(|dir| dir.is_dir() && dir.file_name().unwrap_or_default() == "raw_overrides")
                .ok_or(ModError {
                    error_type: ModErrorType::SpecificationError,
                    details: format!(
                        "Unable to find raw_overrides directory in {}",
                        mod_dir.as_ref().display()
                    ),
                })?,
        )?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()
        .ok();

        let override_dirs = fs::read_dir(
            root_dir
                .iter()
                .find(|dir| dir.is_dir() && dir.file_name().unwrap_or_default() == "overrides")
                .ok_or(ModError {
                    error_type: ModErrorType::SpecificationError,
                    details: format!(
                        "Unable to find overrides directory in {}",
                        mod_dir.as_ref().display()
                    ),
                })?,
        )?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

        let re = Regex::new(r"^aid_([a-z0-9]+)_([a-z0-9]+)_([a-z0-9_]+)$").unwrap();

        let mut raw_overrides = HashMap::<String, RawAssetOverride>::new();
        let mut cutscene_mods = HashMap::new();

        if let Some(raw_override_dirs) = raw_override_dirs {
            for raw_override_dir in raw_override_dirs {
                if !raw_override_dir.is_dir() {
                    continue;
                }

                let override_aid = raw_override_dir
                    .file_name()
                    .ok_or(ModError {
                        error_type: ModErrorType::SpecificationError,
                        details: "Failed to retrieve file name from dir.".to_string(),
                    })?
                    .to_str()
                    .ok_or(ModError {
                        error_type: ModErrorType::SpecificationError,
                        details: format!(
                            "Failed to convert path {} to str.",
                            raw_override_dir.display()
                        ),
                    })?;

                // eg. aid_aidlist_ghoulies_sceneorder_game

                let Some((_, [raw_asset_type, _asset_category, _asset_entry])) =
                    re.captures(override_aid).map(|caps| caps.extract())
                else {
                    return Err(ModError {
                        error_type: ModErrorType::SpecificationError,
                        details: format!(
                            "Asset name {override_aid} did not match AID regex (aid_[TYPE]_[CATEGORY]_[ENTRY]).",
                        ),
                    });
                };

                let asset_type = AssetType::try_from(raw_asset_type).map_err(|_| ModError {
                    error_type: ModErrorType::SpecificationError,
                    details: format!(
                        "Asset type {} does not match any known type.",
                        raw_asset_type
                    ),
                })?;

                let descriptor_bytes = std::fs::read(raw_override_dir.join("descriptor"))?;

                // TODO: Make this read multiple resource chunks
                let resource_bytes =
                    std::fs::read(raw_override_dir.join("resource0")).unwrap_or_default();

                if let Some(_existing) = raw_overrides.insert(
                    override_aid.to_owned(),
                    RawAssetOverride {
                        asset_type,
                        descriptor_bytes,
                        resource_bytes,
                    },
                ) {
                    return Err(ModError {
                        error_type: ModErrorType::AssetOverrideError,
                        details: format!("Asset {override_aid} has already been overwritten."),
                    });
                }
            }
        }

        for override_dir in override_dirs {
            if !override_dir.is_dir() {
                continue;
            }

            let override_aid = override_dir
                .file_name()
                .ok_or(ModError {
                    error_type: ModErrorType::SpecificationError,
                    details: "Failed to retrieve file name from dir.".to_string(),
                })?
                .to_str()
                .ok_or(ModError {
                    error_type: ModErrorType::SpecificationError,
                    details: format!("Failed to convert path {} to str.", override_dir.display()),
                })?;

            // eg. aid_aidlist_ghoulies_sceneorder_game

            let Some((_, [raw_asset_type, _asset_category, _asset_entry])) =
                re.captures(override_aid).map(|caps| caps.extract())
            else {
                return Err(ModError {
                    error_type: ModErrorType::SpecificationError,
                    details: format!(
                        "Asset name {override_aid} did not match AID regex (aid_[TYPE]_[CATEGORY]_[GROUP]_[ENTRY]).",
                    ),
                });
            };

            let asset_type = AssetType::try_from(raw_asset_type).map_err(|_| ModError {
                error_type: ModErrorType::SpecificationError,
                details: format!(
                    "Asset type {} does not match any known type.",
                    raw_asset_type
                ),
            })?;

            let asset_override: Option<(String, RawAssetOverride)> = match asset_type {
                AssetType::ResAidList => {
                    let aid_list = AidList::parse(override_dir.join("override.txt"))?;
                    Some((
                        override_aid.to_string(),
                        RawAssetOverride {
                            asset_type: AssetType::ResAidList,
                            descriptor_bytes: aid_list.get_descriptor().to_bytes()?,
                            resource_bytes: vec![],
                        },
                    ))
                }
                AssetType::ResCutscene => {
                    if let Some(cutscene_mod) =
                        std::fs::File::open(override_dir.join("override.json"))
                            .ok()
                            .and_then(|v| serde_json::from_reader(v).ok())
                    {
                        cutscene_mods.insert(override_aid.to_string(), cutscene_mod);
                    }

                    None
                }
                _ => None, //
                           /*
                           AssetType::ResTexture => todo!(),
                           AssetType::ResAnim => todo!(),
                           AssetType::ResUnknown3 => todo!(),
                           AssetType::ResModel => todo!(),
                           AssetType::ResAnimEvents => todo!(),
                           AssetType::ResCutscene => todo!(),
                           AssetType::ResCutsceneEvents => todo!(),
                           AssetType::ResMisc => todo!(),
                           AssetType::ResActorGoals => todo!(),
                           AssetType::ResMarker => todo!(),
                           AssetType::ResFxCallout => todo!(),
                           AssetType::ResLoctext => todo!(),
                           AssetType::ResXSoundbank => todo!(),
                           AssetType::ResXDSP => todo!(),
                           AssetType::ResXCueList => todo!(),
                           AssetType::ResFont => todo!(),
                           AssetType::ResGhoulybox => todo!(),
                           AssetType::ResGhoulyspawn => todo!(),
                           AssetType::ResScript => todo!(),
                           AssetType::ResActorAttribs => todo!(),
                           AssetType::ResEmitter => todo!(),
                           AssetType::ResParticle => todo!(),
                           AssetType::ResRumble => todo!(),
                           AssetType::ResShakeCam => todo!(),
                           AssetType::ResCount => todo!(),
                           */
            };

            if let Some((name, asset_override)) = asset_override
                && let Some(_existing) = raw_overrides.insert(name, asset_override)
            {
                return Err(ModError {
                    error_type: ModErrorType::AssetOverrideError,
                    details: format!("Asset {override_aid} has already been overwritten."),
                });
            }
        }

        let spec: ModSpecification =
            serde_json::from_slice(&fs::read(mod_root_file)?).expect("Failed to deserialize mod.");

        Ok(Self {
            spec,
            raw_overrides,
            cutscene_mods,
        })
    }

    pub fn spec(&self) -> &ModSpecification {
        &self.spec
    }

    pub fn overrides(&self) -> &HashMap<String, RawAssetOverride> {
        &self.raw_overrides
    }

    pub fn overrides_mut(&mut self) -> &mut HashMap<String, RawAssetOverride> {
        &mut self.raw_overrides
    }

    pub fn affected_assets(&self) -> Vec<String> {
        self.raw_overrides
            .keys()
            .chain(self.cutscene_mods.keys())
            .cloned()
            .collect()
    }

    /// Applies a Mod to an existing BNL file in memory. On success, returns the number of assets
    /// modified.
    pub fn apply(&self, bnl: &mut BNLFile) -> Result<usize, ModError> {
        let mut overrides_applied = 0usize;

        if !self.overrides().is_empty() {
            println!(
                "Applying {} asset override{}.",
                self.overrides().len(),
                if self.overrides().len() != 1 { "s" } else { "" }
            );

            for (override_aid, asset_override) in self.overrides() {
                if let Ok(mut raw_asset) = bnl.remove_asset(override_aid) {
                    // Get the original asset in the correct type (eg. Cutscene)

                    // Apply the patch to the descriptor
                    // Re-write the asset

                    let desc_mut = raw_asset.descriptor_bytes_mut();
                    desc_mut.resize(asset_override.descriptor_bytes.len(), 0u8);
                    desc_mut.copy_from_slice(&asset_override.descriptor_bytes);

                    // TODO: Resource chunks
                    /*
                    let res_mut = raw_asset.resource_chunks_mut() {
                    }
                    */

                    overrides_applied += 1;

                    bnl.append_raw_asset(raw_asset);
                } else {
                    continue;
                }
            }
        }

        if !self.cutscene_mods.is_empty() {
            for (mod_name, cutscene_mod) in &self.cutscene_mods {
                if let Err(e) = bnl.modify_asset(
                    mod_name,
                    |cutscene: &mut crate::asset::Asset<crate::asset::cutscene::Cutscene>| {
                        if let Some(length) = cutscene_mod.length {
                            cutscene.asset_mut().descriptor.length = length
                        }

                        overrides_applied += 1;
                        Ok(())
                    },
                ) {
                    match e {
                        crate::asset::AssetError::NotFound => (),
                        _ => eprintln!("Failed to apply cutscene mod: {e}"),
                    };
                }
            }
        }

        Ok(overrides_applied)
    }
}
