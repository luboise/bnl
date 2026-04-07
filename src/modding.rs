use std::{
    collections::HashMap,
    fs,
    io::{self},
    path::Path,
};

use crate::{
    BNLFile,
    asset::{AssetDescriptor, AssetLike, AssetParseError, AssetType, Parse, aidlist::AidList},
};
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct BNLMod {
    /// Assets to find and add to this scene
    #[serde(default)]
    add: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModSpecification {
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub asset_groups: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub bnl_edits: HashMap<String, BNLMod>,
}

#[derive(Debug)]
pub struct ModContext {
    pub bnl_basename: String,
    pub all_bnl_paths: Vec<std::path::PathBuf>,
    pub assets: HashMap<String, crate::RawAsset>,
}

#[derive(Debug, Clone)]
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
    pub spec: ModSpecification,
    /// The assets which came with the mod
    pub raw_asset_overrides: HashMap<String, RawAssetOverride>,
    pub cutscene_mods: HashMap<String, crate::asset::cutscene::CutsceneMod>,
}

impl Mod {
    pub fn new(name: impl AsRef<str>) -> Self {
        Self {
            spec: ModSpecification {
                version: 0,
                name: name.as_ref().to_string(),
                asset_groups: HashMap::default(),
                bnl_edits: HashMap::default(),
            },
            raw_asset_overrides: HashMap::default(),
            cutscene_mods: HashMap::new(),
        }
    }

    /// Reads a mod on disk from a path
    pub fn from_dir(mod_dir: impl AsRef<Path>) -> Result<Mod, ModError> {
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

        // TODO: Clean up this ugly ModError
        let spec: ModSpecification =
            serde_json::from_slice(&fs::read(mod_root_file)?).map_err(|e| ModError {
                error_type: ModErrorType::SpecificationError,
                details: e.to_string(),
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
                .find(|dir| {
                    dir.is_dir() && dir.file_name().unwrap_or_default() == "global_overrides"
                })
                .ok_or(ModError {
                    error_type: ModErrorType::SpecificationError,
                    details: format!(
                        "Unable to find global_overrides directory in {}",
                        mod_dir.as_ref().display()
                    ),
                })?,
        )?
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>()?;

        let re = Regex::new(r"^aid_([a-z0-9]+)_([a-z0-9]+)_([a-z0-9_]+)$").unwrap();

        let mut raw_asset_overrides = HashMap::<String, RawAssetOverride>::new();
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

                if let Some(_existing) = raw_asset_overrides.insert(
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
                && let Some(_existing) = raw_asset_overrides.insert(name, asset_override)
            {
                return Err(ModError {
                    error_type: ModErrorType::AssetOverrideError,
                    details: format!("Asset {override_aid} has already been overwritten."),
                });
            }
        }

        Ok(Self {
            spec,
            raw_asset_overrides,
            cutscene_mods,
        })
    }

    pub fn spec(&self) -> &ModSpecification {
        &self.spec
    }

    /// List of aids which will be affected by this mod
    pub fn affected_assets(&self) -> Vec<String> {
        // For each bnl edit
        self.spec
            .bnl_edits
            .values()
            .flat_map(|bnl_mod| {
                // Replace the inline name with the list of aids instead
                bnl_mod.add.iter().flat_map(|v| {
                    self.spec
                        .asset_groups
                        .get(v)
                        .cloned()
                        .unwrap_or_else(|| vec![v.clone()])
                })
            })
            .chain(self.raw_asset_overrides.keys().cloned())
            .chain(self.cutscene_mods.keys().cloned())
            .collect()
    }

    /// Applies a Mod to an existing BNL file in memory. On success, returns the number of assets
    /// modified.
    pub fn apply(
        &self,
        ctx: &mut ModContext,
        bnl: &mut BNLFile,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut overrides_applied = 0usize;

        // Upsert all new assets into the bnl (might not have existed previously)
        if let Some(bnl_mod) = self.spec.bnl_edits.get(&ctx.bnl_basename) {
            // For each add in bnl_mod.add, convert it into a list of aids
            let aids_to_insert = bnl_mod.add.iter().flat_map(|key| {
                self.spec
                    .asset_groups
                    .get(key)
                    .cloned()
                    .unwrap_or(vec![key.clone()])
            });

            for aid in aids_to_insert {
                bnl.upsert_raw_asset(
                    ctx.assets
                        .get(&aid)
                        .cloned()
                        .ok_or_else(|| format!("unable to get mod asset {aid}"))?,
                );
            }
        }

        // Then, apply all available overrides
        if !ctx.assets.is_empty() {
            for (override_aid, raw_asset) in &ctx.assets {
                let Ok(_) = bnl.remove_asset(override_aid) else {
                    continue;
                };

                bnl.append_raw_asset(raw_asset.clone());
                overrides_applied += 1;
            }
        }

        /*
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
        */

        Ok(overrides_applied)
    }
}
