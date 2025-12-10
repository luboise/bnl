use std::{collections::HashMap, fs, io, path::Path};

use crate::{
    BNLFile, RawAsset,
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
pub struct AssetOverride {
    pub descriptor_bytes: Vec<u8>,
    pub resource_bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct Mod {
    spec: ModSpecification,
    overrides: HashMap<String, AssetOverride>,
}

#[derive(Debug)]
pub enum ModErrorType {
    SpecificationError,
    AssetOverrideError,
}

#[derive(Debug)]
pub struct ModError {
    error_type: ModErrorType,
    details: String,
}

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

impl Mod {
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

        let re = Regex::new(r"^aid_([a-z0-9]+)_([a-z0-9]+)_([a-z0-9]+)_([a-z0-9]+)$").unwrap();

        let mut overrides = HashMap::<String, AssetOverride>::new();

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

            let Some((_, [raw_asset_type, asset_category, asset_group, asset_entry])) =
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

            match asset_type {
                AssetType::ResAidList => {
                    let aid_list = AidList::parse(override_dir.join("override.txt"))?;
                    if let Some(bruh) = overrides.insert(
                        override_aid.to_string(),
                        AssetOverride {
                            descriptor_bytes: aid_list.get_descriptor().to_bytes()?,
                            resource_bytes: vec![],
                        },
                    ) {
                        return Err(ModError {
                            error_type: ModErrorType::AssetOverrideError,
                            details: format!("Asset {override_aid} has already been overwritten."),
                        });
                    }
                }
                _ => (), //
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
            }
        }

        let spec: ModSpecification =
            serde_json::from_slice(&fs::read(mod_root_file)?).expect("Failed to deserialize mod.");

        Ok(Self { spec, overrides })
    }

    pub fn spec(&self) -> &ModSpecification {
        &self.spec
    }

    pub fn overrides(&self) -> &HashMap<String, AssetOverride> {
        &self.overrides
    }

    /// Applies a Mod to an existing BNL file in memory. On success, returns the number of assets
    /// modified.
    pub fn apply(&self, bnl: &mut BNLFile) -> Result<usize, ModError> {
        let num_overrides = self.overrides().len();

        println!(
            "Applying {num_overrides} asset override{}.",
            if num_overrides != 1 { "s" } else { "" }
        );

        let mut overrides_applied = 0usize;

        for (override_aid, asset_override) in self.overrides() {
            if let Ok(mut raw_asset) = bnl.remove_asset(override_aid) {
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

        Ok(overrides_applied)
    }
}
