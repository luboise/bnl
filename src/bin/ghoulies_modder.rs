use std::collections::HashMap;

use binrw::{BinRead, BinWrite};
use bnl::xsb::soundbank::{ComplexEventParams, Sound};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let game_dir: std::path::PathBuf = args[0].clone().into();
    let modification = bnl::modding::Mod::from_dir(&args[1])?;

    let bnl_paths = walkdir::WalkDir::new(game_dir.clone())
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
        .map(bnl::asset::hash_aid);

        let common = bnl_paths
            .iter()
            .find(|path| path.ends_with("common.bnl"))
            .cloned()
            .ok_or("no common.bnl")?;

        let common_bnl = bnl::BNLFile::from_bytes(&std::fs::read(&common)?)?;
        let cue_list = common_bnl
            .get_asset::<bnl::asset::cuelist::CueList>("aid_xcuelist_ghoulies_default")?;

        let mut wavebanks = wavebank_hashes
            .iter()
            .map(|wavebank_hash| {
                let path = game_dir.join(format!("xwavebank/{wavebank_hash:08x}"));
                bnl::xsb::XWavebank::read_le(&mut std::fs::File::open(path)?)
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
                .get_asset::<bnl::xsb::XSoundbank>(&soundbank_aid)?
                .data;

            let cue = soundbank.cue_entries.get(cue_index).unwrap();

            let sound = soundbank
                .sound_entries
                .get(cue.sound_index as usize)
                .ok_or(format!("failed to get sound index {}", cue.sound_index))?;

            let bank_indices = match &sound.sound {
                Sound::Trivial(bank_index) => Some(vec![bank_index.clone()]),
                Sound::Simple(complex_wave_variations) => Some(
                    complex_wave_variations
                        .variations
                        .iter()
                        .map(|v| v.bank_index.clone())
                        .collect(),
                ),
                Sound::Complex(complex_tracks) => Some(
                    complex_tracks
                        .iter()
                        .flat_map(|track| {
                            track
                                .events
                                .iter()
                                .filter_map(|ev| match &ev.params {
                                    ComplexEventParams::Play(bank_index) => {
                                        Some(vec![bank_index.clone()])
                                    }
                                    ComplexEventParams::PlayVaried { wave_variations }
                                    | ComplexEventParams::PlayComplexVaried {
                                        wave_variations,
                                        ..
                                    } => Some(
                                        wave_variations
                                            .variations
                                            .iter()
                                            .map(|variation| variation.bank_index.clone())
                                            .collect(),
                                    ),
                                    ComplexEventParams::PlayComplex { bank_index, .. } => {
                                        Some(vec![bank_index.clone()])
                                    }
                                    ComplexEventParams::EnvelopeAmplitude { .. }
                                    | ComplexEventParams::Disabled()
                                    | ComplexEventParams::MixBinSpan { .. } => None,
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

                    let bnl::xsb::WavEntry {
                        unknown_1,
                        format,
                        unknown_2,
                        unknown_3,
                        bytes,
                        ..
                    } = wavebank
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

                if bnl::get_aid_list(&bytes).ok()?.contains(aid) {
                    let mut raw_asset = bnl::BNLFile::from_bytes(&bytes)
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

                if bnl::get_aid_list(&bytes).ok()?.contains(&aid) {
                    Some(
                        bnl::BNLFile::from_bytes(&bytes)
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

    // for (aid, raw_asset) in assets
    //     .iter_mut()
    //     .filter(|(aid, _)| modification.model_mods.contains_key(*aid))
    // {
    //     let model_mod = modification.model_mods.get(aid).unwrap();
    //     model_mod.apply_raw(&mut raw_asset.data)?;
    // }

    let mut ctx = bnl::modding::ModContext {
        bnl_basename: String::default(),
        all_bnl_paths: vec![],
        assets,
    };

    ctx.all_bnl_paths = bnl_paths.clone();

    for bnl_path in bnl_paths {
        let bnl_bytes = std::fs::read(&bnl_path)?;
        let Ok(aid_list) = bnl::get_aid_list(&bnl_bytes) else {
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

        let mut bnl = bnl::BNLFile::from_bytes(&bnl_bytes).expect("Stupid bnl error");

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
