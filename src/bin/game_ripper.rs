use binrw::BinRead;
use bnl::{
    asset::Dump,
    xsb::{self},
};

fn main() -> Result<(), bnl::Error> {
    let mut args = std::env::args().skip(1);

    let game_dir: std::path::PathBuf = args.next().unwrap().into();
    let out_path: std::path::PathBuf = args.next().unwrap_or_else(|| "out".to_owned()).into();

    println!("ripping audio");
    rip_cues(&game_dir, &out_path)?;

    println!("ripping models");
    rip_textures(&game_dir, &out_path)?;

    Ok(())
}

fn rip_textures(
    game_dir: impl AsRef<std::path::Path>,
    out_dir: impl AsRef<std::path::Path>,
) -> Result<(), bnl::Error> {
    let game_dir = game_dir.as_ref();
    let out_dir = out_dir.as_ref().join("models");

    let mut found_models = std::collections::HashSet::new();

    for entry in walkdir::WalkDir::new(game_dir) {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|v| v == "bnl") {
            let bnl = bnl::BNLFile::from_path(path)?;
            let models = bnl.get_assets::<bnl::asset::model::Model>();

            for model in models {
                let bnl::asset::Asset { metadata, data } = model;

                let model_name = metadata
                    .name()
                    .chars()
                    .skip("aid_model_ghoulies_".len())
                    .collect::<String>();

                if model_name.is_empty() {
                    eprintln!("invalid model name: {}", metadata.name());
                    continue;
                }
                if !found_models.insert(model_name.clone()) {
                    continue;
                }
                let Some(textures) = &data.textures_subresource else {
                    continue;
                };
                if textures.textures.is_empty() {
                    continue;
                };

                // do the dumping
                let model_dir = model_name
                    .split('_')
                    .fold(out_dir.clone(), |out_dir, b| out_dir.join(b));

                std::fs::create_dir_all(&model_dir)?;

                for (i, texture) in textures.textures.iter().enumerate() {
                    texture.dump(model_dir.join(format!("texturefile{i:04}.png")))?;
                }
            }
        }
    }

    Ok(())
}

fn rip_cues(
    game_dir: impl AsRef<std::path::Path>,
    out_dir: impl AsRef<std::path::Path>,
) -> Result<(), bnl::Error> {
    let game_dir = game_dir.as_ref();
    let out_dir = out_dir.as_ref().join("audio");

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

    let wavebanks = wavebank_hashes
        .into_iter()
        .map(|wavebank_hash| {
            let path = game_dir.join(format!("xwavebank/{wavebank_hash:08x}"));
            xsb::XWavebank::read_le(&mut std::fs::File::open(path)?)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let bnl = bnl::BNLFile::from_bytes(&std::fs::read(game_dir.join("bundles/common.bnl"))?)?;

    let cue_list =
        bnl.get_asset::<bnl::asset::cuelist::CueList>("aid_xcuelist_ghoulies_default")?;

    for group in cue_list.data.groups() {
        println!("Ripping group {}", group.name);

        let soundbank_aid = format!("aid_xsoundbank_ghoulies_{}", &group.name[1..]);
        let soundbank = bnl.get_asset::<xsb::XSoundbank>(&soundbank_aid)?.data;

        for (i, entry) in soundbank.sound_entries.iter().enumerate() {
            let cue_name = group.cues.get(i).ok_or("no cue at index {i}")?;

            use xsb::soundbank::Sound;

            let bank_indices = match &entry.sound {
                Sound::Trivial(bank_index) => vec![bank_index.clone()],
                Sound::Simple(variations) => variations
                    .variations
                    .iter()
                    .map(|v| v.bank_index.clone())
                    .collect(),
                Sound::Complex(tracks) => {
                    use xsb::soundbank::ComplexEventParams;

                    let mut bank_indices = vec![];

                    for track in tracks {
                        for event in &track.events {
                            match &event.params {
                                ComplexEventParams::Play(bank_index)
                                | ComplexEventParams::PlayComplex { bank_index, .. } => {
                                    bank_indices.push(bank_index.clone())
                                }
                                ComplexEventParams::PlayVaried { wave_variations }
                                | ComplexEventParams::PlayComplexVaried {
                                    wave_variations, ..
                                } => {
                                    for complex_variation in &wave_variations.variations {
                                        bank_indices.push(complex_variation.bank_index.clone());
                                    }
                                }
                                ComplexEventParams::EnvelopeAmplitude { .. }
                                | ComplexEventParams::Disabled()
                                | ComplexEventParams::MixBinSpan { .. } => (),
                            }
                        }
                    }

                    bank_indices
                }
            };

            if bank_indices.is_empty() {
                eprintln!("No sounds found for {}_{cue_name}", group.name);
                continue;
            }

            for (
                i,
                xsb::soundbank::BankIndex {
                    wave_index,
                    wavebank_index,
                },
            ) in bank_indices.iter().enumerate()
            {
                let Some(wavebank_name) =
                    soundbank.wavebank_array.names.get(*wavebank_index as usize)
                else {
                    return Err(format!(
                        "wavebank index {wavebank_index} not found in soundbank (sound_index: {wave_index})"
                    ).into());
                };

                let Some(wavebank) = wavebanks
                    .iter()
                    .find(|wavebank| wavebank.name == *wavebank_name)
                else {
                    eprintln!(
                        "wavebank by name {} not found. skipping this sound",
                        str::from_utf8(wavebank_name).unwrap_or("parse_error")
                    );
                    continue;
                };

                let wav_file = wavebank
                    .wav_entries
                    .get(*wave_index as usize)
                    .ok_or("bad sound index")?;

                let filename = if bank_indices.len() > 1 {
                    format!("{}_{}_{i}.wav", group.name, cue_name)
                } else {
                    format!("{}_{}.wav", group.name, cue_name)
                };

                wav_file.dump(out_dir.join(filename))?;
            }
        }
    }

    /*
    let names = cuelist.data.groups().iter().flat_map(|group| {
        group
            .cues
            .iter()
            .map(|cue| format!("{}_{cue}.wav", group.name))
    });

    let wav_files = wavebank
        .wav_entries
        .iter()
        .map(|raw| xsb::WavFile::from_raw(raw.clone(), &wavebank.wave_data))
        .collect::<Result<Vec<_>, _>>()?;

    for (name, wav) in names.zip(wav_files).fuse() {
        wav.dump(out_path.join(name))?;
    }
    */

    Ok(())
}
