use std::{
    collections::HashMap,
    fs,
    io::{self},
    path::Path,
};

use crate::{
    BNLFile,
    asset::{AssetType, Parse, aidlist::AidList},
};
use binrw::{BinRead, BinWrite};
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
    pub data: crate::RawAssetData,
}

pub trait ModLike: Sized {
    type AssetDataType: TryFrom<crate::RawAssetData, Error = crate::Error>
        + TryInto<crate::RawAssetData, Error = crate::Error>;

    fn apply_raw(&self, raw_asset: &mut crate::RawAssetData) -> Result<(), crate::Error> {
        let mut data = Self::AssetDataType::try_from(raw_asset.clone())?;

        self.apply(&mut data)?;
        *raw_asset = data.try_into()?;

        Ok(())
    }

    fn apply(&self, asset_data: &mut Self::AssetDataType)
    -> Result<(), Box<dyn std::error::Error>>;

    fn from_dir(dir: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>>;
}

#[derive(Debug)]
pub struct Mod {
    pub spec: ModSpecification,
    /// The assets which came with the mod
    pub raw_asset_overrides: HashMap<String, RawAssetOverride>,
    pub cutscene_mods: HashMap<String, CutsceneMod>,
    pub audio_replacements: Vec<(String, String, std::path::PathBuf)>,
    pub model_mods: HashMap<String, ModelMod>,
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
            audio_replacements: vec![],
            model_mods: HashMap::new(),
        }
    }

    /// Reads a mod on disk from a path
    pub fn from_dir(mod_dir: impl AsRef<Path>) -> Result<Mod, crate::Error> {
        // Locate dirs
        let root_dir = fs::read_dir(&mod_dir)?
            .map(|res| res.map(|e| e.path()))
            .collect::<Result<Vec<_>, io::Error>>()?;

        let mod_root_file = root_dir
            .iter()
            .find(|file| file.is_file() && file.file_name().unwrap_or_default() == "mod.json")
            .ok_or(format!(
                "Unable to find root mod.json file in {}",
                mod_dir.as_ref().display()
            ))?;

        let spec: ModSpecification = serde_json::from_slice(&fs::read(mod_root_file)?)?;

        let audio_replacements = {
            let mut v = vec![];

            if let Some(audio_dir) = root_dir
                .iter()
                .find(|dir| dir.is_dir() && dir.file_name().unwrap_or_default() == "audio")
            {
                let re = Regex::new(r"^(G[a-zA-Z]+)_(.*).wav$").unwrap();

                for file in std::fs::read_dir(audio_dir)? {
                    let file = file?;

                    let file_name = file
                        .file_name()
                        .into_string()
                        .map_err(|_| "failed to read file path")?;

                    let Some((_, [group_name, cue_name])) =
                        re.captures(&file_name).map(|caps| caps.extract())
                    else {
                        return Err(format!(
                            "failed to parse file name: {}",
                            file.file_name().display()
                        )
                        .into());
                    };

                    v.push((group_name.to_owned(), cue_name.to_owned(), file.path()));
                }
            }

            v
        };

        let model_mods = {
            let mut model_mods = HashMap::new();

            if let Some(models_dir) = root_dir
                .iter()
                .find(|dir| dir.is_dir() && dir.file_name().unwrap_or_default() == "models")
            {
                for model_mod_path in std::fs::read_dir(models_dir)?.filter_map(|v| {
                    let path = v.ok()?.path();
                    Some(path).filter(|p| p.is_dir())
                }) {
                    let aid = format!(
                        "aid_model_ghoulies_{}",
                        model_mod_path
                            .file_name()
                            .and_then(|v| v.to_str())
                            .ok_or("no file name on model mod")?
                    );

                    model_mods.insert(aid, ModelMod::from_dir(model_mod_path)?);
                }
            }

            model_mods
        };

        let raw_override_dirs = fs::read_dir(
            root_dir
                .iter()
                .find(|dir| dir.is_dir() && dir.file_name().unwrap_or_default() == "raw_overrides")
                .ok_or(format!(
                    "Unable to find raw_overrides directory in {}",
                    mod_dir.as_ref().display()
                ))?,
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
                .ok_or(format!(
                    "Unable to find global_overrides directory in {}",
                    mod_dir.as_ref().display()
                ))?,
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
                    .ok_or("Failed to retrieve file name from dir.")?
                    .to_str()
                    .ok_or(format!(
                        "Failed to convert path {} to str.",
                        raw_override_dir.display()
                    ))?;

                let Some((_, [raw_asset_type, _asset_category, _asset_entry])) =
                    re.captures(override_aid).map(|caps| caps.extract())
                else {
                    return Err(format!(
                        "Asset name {override_aid} did not match AID regex (aid_[TYPE]_[CATEGORY]_[ENTRY]).",
                    ).into());
                };

                let asset_type = AssetType::try_from(raw_asset_type).map_err(|_| {
                    format!(
                        "Asset type {} does not match any known type.",
                        raw_asset_type
                    )
                })?;

                let data = {
                    let descriptor_bytes = std::fs::read(raw_override_dir.join("descriptor"))?;

                    let resource_bytes = {
                        if let Ok(res) = std::fs::read(raw_override_dir.join("resource")) {
                            res
                        } else {
                            let mut bytes = vec![];

                            for i in 0..i32::MAX {
                                let Ok(file) =
                                    std::fs::read(raw_override_dir.join(format!("resource{i}")))
                                else {
                                    break;
                                };
                                bytes.extend(file);
                            }

                            bytes
                        }
                    };

                    crate::RawAssetData {
                        descriptor_bytes,
                        resource_chunks: vec![resource_bytes],
                    }
                };

                if let Some(_existing) = raw_asset_overrides.insert(
                    override_aid.to_owned(),
                    RawAssetOverride { asset_type, data },
                ) {
                    return Err(
                        format!("Asset {override_aid} has already been overwritten.").into(),
                    );
                }
            }
        }

        for override_dir in override_dirs {
            if !override_dir.is_dir() {
                continue;
            }

            let override_aid = override_dir
                .file_name()
                .ok_or("Failed to retrieve file name from dir.".to_owned())?
                .to_str()
                .ok_or(format!(
                    "Failed to convert path {} to str.",
                    override_dir.display()
                ))?;

            // eg. aid_aidlist_ghoulies_sceneorder_game
            let Some((_, [raw_asset_type, _asset_category, _asset_entry])) =
                re.captures(override_aid).map(|caps| caps.extract())
            else {
                return Err(
                     format!(
                        "Asset name {override_aid} did not match AID regex (aid_[TYPE]_[CATEGORY]_[GROUP]_[ENTRY]).",
                    ).into());
            };

            let asset_type = AssetType::try_from(raw_asset_type).map_err(|_| {
                format!(
                    "Asset type {} does not match any known type.",
                    raw_asset_type
                )
            })?;

            let asset_override: Option<(String, RawAssetOverride)> = match asset_type {
                AssetType::AidList => {
                    let aid_list = AidList::parse(override_dir.join("override.txt"))?;

                    Some((
                        override_aid.to_string(),
                        RawAssetOverride {
                            asset_type: AssetType::AidList,
                            data: aid_list.try_into()?,
                        },
                    ))
                }
                AssetType::Cutscene => {
                    cutscene_mods.insert(
                        override_aid.to_string(),
                        CutsceneMod::from_dir(&override_dir)?,
                    );

                    None
                }
                // AssetType::Model => {
                //     model_mods.insert(override_aid.to_string(), ModelMod::from_dir(&override_dir)?);
                //     None
                // }
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
                return Err(format!("Asset {override_aid} has already been overwritten.").into());
            }
        }

        Ok(Self {
            spec,
            raw_asset_overrides,
            cutscene_mods,
            audio_replacements,
            model_mods,
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
            .chain(self.model_mods.keys().cloned())
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

        Ok(overrides_applied)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CutsceneMod {
    pub length: Option<f32>,
}

impl crate::modding::ModLike for CutsceneMod {
    type AssetDataType = crate::asset::cutscene::Cutscene;

    fn apply(
        &self,
        asset_data: &mut Self::AssetDataType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(length) = self.length {
            asset_data.length = length;
        }

        Ok(())
    }

    fn from_dir(dir: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let f = std::fs::File::open(dir.as_ref().join("override.json"))?;
        Ok(serde_json::from_reader(f)?)
    }
}

#[derive(Debug, Clone)]
pub struct ModelMod {
    textures: HashMap<usize, crate::asset::texture::Texture>,
}

impl crate::modding::ModLike for ModelMod {
    type AssetDataType = crate::asset::model::Model;

    fn apply(
        &self,
        asset_data: &mut Self::AssetDataType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if asset_data.textures_subresource.is_none() {
            return Ok(());
        }

        let Some(textures_subres) = &mut asset_data.textures_subresource else {
            return Err("no textures subresource on model".into());
        };

        for (index, new_texture) in &self.textures {
            let existing_tex = textures_subres
                .textures
                .get_mut(*index)
                .ok_or_else(|| format!("no texture for index {index}"))?;

            existing_tex.override_from(new_texture, false)?;
        }

        Ok(())
    }

    fn apply_raw(&self, _: &mut crate::RawAssetData) -> Result<(), Box<dyn std::error::Error>> {
        todo!("apply_raw unimplemented, use apply instead");
    }

    fn from_dir(dir: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let texture_regex = regex::Regex::new(r"^texturefile([0-9]+).*$")?;

        const FILE_EXTENSIONS: [&str; 1] = ["png"];

        let dir = dir.as_ref();

        let mut textures = HashMap::new();

        for entry in walkdir::WalkDir::new(dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let Some(file_name_str) = entry.file_name().to_str() else {
                continue;
            };
            let Some(extension) = entry.path().extension().and_then(|v| v.to_str()) else {
                continue;
            };

            let Some((_, [index])) = texture_regex
                .captures(file_name_str)
                .map(|caps| caps.extract())
            else {
                continue;
            };

            if !file_name_str.starts_with("texturefile") || !FILE_EXTENSIONS.contains(&extension) {
                continue;
            }

            textures.insert(
                index.parse()?,
                crate::asset::texture::Texture::from_path(entry.path())?,
            );
        }

        Ok(Self { textures })
    }
}

pub fn apply_mod_to_game(
    game_dir: impl AsRef<std::path::Path>,
    mod_dir: impl AsRef<std::path::Path>,
) -> Result<(), crate::Error> {
    let game_dir = game_dir.as_ref();
    let modification = crate::modding::Mod::from_dir(mod_dir)?;

    let bnl_paths = walkdir::WalkDir::new(game_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "bnl")
                .then(|| entry.path().to_path_buf())
        })
        .collect::<Vec<_>>();

    {
        let wavebank_hashes = [
            "harddisk0",
            "harddisk1",
            "dvd0",
            "dvd1",
            "dvd2",
            "dvd3",
            "dvd4",
            "dvd5",
            "dvd6",
            "dvd7",
            "dvd8",
            "dvddemo",
        ]
        .map(|v| "aid_xwavebank_ghoulies_".to_string() + v)
        .map(crate::asset::hash_aid);

        let common = bnl_paths
            .iter()
            .find(|path| path.ends_with("common.bnl"))
            .cloned()
            .ok_or("no common.bnl")?;

        let common_bnl = crate::BNLFile::from_bytes(&std::fs::read(&common)?)?;
        let cue_list = common_bnl
            .get_asset::<crate::asset::cuelist::CueList>("aid_xcuelist_ghoulies_default")?;

        let mut wavebanks = wavebank_hashes
            .iter()
            .map(|wavebank_hash| {
                let path = game_dir.join(format!("xwavebank/{wavebank_hash:08x}"));
                crate::xsb::XWavebank::read_le(&mut std::fs::File::open(path)?)
            })
            .collect::<Result<Vec<_>, _>>()?;

        for (group_name, cue_name, wav_path) in &modification.audio_replacements {
            let group = cue_list
                .data
                .groups()
                .iter()
                .find(|v| v.name == *group_name)
                .unwrap();
            let cue_index = group.cues.iter().position(|cue| cue == cue_name).unwrap();

            let soundbank_aid = format!("aid_xsoundbank_ghoulies_{}", &group.name[1..]);
            let soundbank = common_bnl
                .get_asset::<crate::xsb::XSoundbank>(&soundbank_aid)?
                .data;

            let cue = soundbank.cue_entries.get(cue_index).unwrap();

            let sound = soundbank
                .sound_entries
                .get(cue.sound_index as usize)
                .ok_or(format!("failed to get sound index {}", cue.sound_index))?;

            let bank_indices = match &sound.sound {
                crate::xsb::soundbank::Sound::Trivial(bank_index) => Some(vec![bank_index.clone()]),
                crate::xsb::soundbank::Sound::Simple(complex_wave_variations) => Some(
                    complex_wave_variations
                        .variations
                        .iter()
                        .map(|v| v.bank_index.clone())
                        .collect(),
                ),
                crate::xsb::soundbank::Sound::Complex(complex_tracks) => Some(
                    complex_tracks
                        .iter()
                        .flat_map(|track| {
                            track
                                .events
                                .iter()
                                .filter_map(|ev| {
                                    match &ev.params {
                                    crate::xsb::soundbank::ComplexEventParams::Play(bank_index) => {
                                        Some(vec![bank_index.clone()])
                                    }
                                    crate::xsb::soundbank::ComplexEventParams::PlayVaried {
                                        wave_variations,
                                    }
                                    | crate::xsb::soundbank::ComplexEventParams::PlayComplexVaried {
                                        wave_variations,
                                        ..
                                    } => Some(
                                        wave_variations
                                            .variations
                                            .iter()
                                            .map(|variation| variation.bank_index.clone())
                                            .collect(),
                                    ),
                                    crate::xsb::soundbank::ComplexEventParams::PlayComplex { bank_index, .. } => {
                                        Some(vec![bank_index.clone()])
                                    }
                                    crate::xsb::soundbank::ComplexEventParams::EnvelopeAmplitude { .. }
                                    | crate::xsb::soundbank::ComplexEventParams::Disabled()
                                    | crate::xsb::soundbank::ComplexEventParams::MixBinSpan { .. } => None,
                                }
                                })
                                .flatten()
                        })
                        .collect(),
                ),
            }
            .ok_or("failed to get bank index")?;

            if bank_indices.is_empty() {
                return Err(format!("no bank indices for {group_name}_{cue_name}").into());
            }

            let (samples, sample_rate) = wavers::read(wav_path)
                .map_err(|e| format!("failed to read wav file {}: {e}", wav_path.display()))?;

            for bank_index in bank_indices {
                let wavebank_name = soundbank
                    .wavebank_array
                    .names
                    .get(bank_index.wavebank_index as usize)
                    .ok_or("failed to get wavebank name")?;

                {
                    let wavebank = wavebanks
                        .iter_mut()
                        .find(|wavebank| wavebank.name == *wavebank_name)
                        .ok_or("failed to get wavebank by name")?;

                    let crate::xsb::WavEntry { format, bytes, .. } = wavebank
                        .wav_entries
                        .get_mut(bank_index.wave_index as usize)
                        .ok_or(format!(
                            "failed to get bank {} wave {}",
                            bank_index.wave_index, bank_index.wavebank_index
                        ))?;

                    format.samples_per_sec = sample_rate as u32;
                    format.num_channels = 1;
                    format.uses_wide_format = true;

                    *bytes = samples
                        .iter()
                        .flat_map(|v: &i16| v.to_le_bytes())
                        .collect::<Vec<_>>();
                }
            }
        }

        for (wavebank, hash) in wavebanks.into_iter().zip(wavebank_hashes) {
            wavebank.write_le(&mut std::fs::File::create(
                game_dir.join(format!("xwavebank/{hash:08x}")),
            )?)?;
        }
    }

    let mut assets = HashMap::default();

    // Get overrides from mod
    for (aid, raw_override) in &modification.raw_asset_overrides {
        let raw_asset = bnl_paths
            .iter()
            .find_map(|path| {
                // TODO: Display errors here properly
                let bytes = std::fs::read(path).ok()?;

                if crate::get_aid_list(&bytes).ok()?.contains(aid) {
                    let mut raw_asset = crate::BNLFile::from_bytes(&bytes)
                        .ok()?
                        .get_raw_asset(aid)?
                        .to_owned();

                    raw_asset.data = raw_override.data.clone();

                    Some(raw_asset)
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("{aid} not found"))?;

        assets.insert(aid.clone(), raw_asset);
    }

    // Get rest of assets from game files
    for aid in modification.affected_assets() {
        if assets.contains_key(&aid) {
            continue;
        }

        let found_asset = bnl_paths
            .iter()
            .find_map(|path| {
                // TODO: Display errors here properly
                let bytes = std::fs::read(path).ok()?;

                if crate::get_aid_list(&bytes).ok()?.contains(&aid) {
                    Some(
                        crate::BNLFile::from_bytes(&bytes)
                            .ok()?
                            .get_raw_asset(&aid)?
                            .to_owned(),
                    )
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("Unable to find asset {aid}"))?;

        assets.insert(aid, found_asset);
    }

    for (_aid, _asset) in assets
        .iter_mut()
        .filter(|(aid, _)| modification.cutscene_mods.contains_key(*aid))
    {
        todo!("cutscene mod apply not implemented (BUG ME ABOUT THIS)");
        // Apply the cutscene mod
    }

    for (aid, raw_asset) in assets
        .iter_mut()
        .filter(|(aid, _)| modification.model_mods.contains_key(*aid))
    {
        // TODO: remove this clone somehow
        let mut model = raw_asset.data.clone().try_into()?;

        let model_mod = modification.model_mods.get(aid).unwrap();
        model_mod.apply(&mut model)?;

        raw_asset.data = crate::RawAssetData::try_from(model)?;
    }

    let mut ctx = crate::modding::ModContext {
        bnl_basename: String::default(),
        all_bnl_paths: vec![],
        assets,
    };

    ctx.all_bnl_paths = bnl_paths.clone();

    for bnl_path in bnl_paths {
        let bnl_bytes = std::fs::read(&bnl_path)?;
        let Ok(aid_list) = crate::get_aid_list(&bnl_bytes) else {
            continue;
        };

        ctx.bnl_basename = bnl_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_owned())
            .unwrap();

        if aid_list.iter().all(|aid| !ctx.assets.contains_key(aid))
            && !modification.spec.bnl_edits.contains_key(&ctx.bnl_basename)
        {
            continue;
        }

        let mut bnl = crate::BNLFile::from_bytes(&bnl_bytes).expect("Stupid bnl error");

        let num_applied = modification.apply(&mut ctx, &mut bnl)?;

        if num_applied > 0 {
            println!(
                "Applied {num_applied} modifications to {}",
                bnl_path.display(),
            );
            std::fs::write(bnl_path, bnl.to_bytes()?)?;
        }
    }

    Ok(())
}
