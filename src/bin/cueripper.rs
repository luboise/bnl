use binrw::BinRead;
use bnl::xsb::{self};

fn main() -> Result<(), bnl::Error> {
    let mut args = std::env::args().skip(1);

    let game_dir: std::path::PathBuf = args.next().unwrap().into();
    let out_path: std::path::PathBuf = args.next().unwrap().into();

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

            use xsb::soundbank::SoundEntry;

            let bank_indices = match entry {
                SoundEntry::Trivial(trivial) => vec![trivial.bank_index.clone()],
                SoundEntry::Simple(simple) => simple.variations.variations.clone(),
                SoundEntry::Complex(complex) => {
                    use xsb::soundbank::ComplexEventParams;

                    let Some(bank_indices) =
                        complex
                            .sound
                            .events
                            .iter()
                            .find_map(|event| match &event.params {
                                ComplexEventParams::Play(bank_index)
                                | ComplexEventParams::PlayComplex(bank_index) => {
                                    Some(vec![bank_index.clone()])
                                }
                                ComplexEventParams::PlayComplexWaveVariations {
                                    wave_variations,
                                } => Some(
                                    wave_variations
                                        .variations
                                        .iter()
                                        .map(|v| v.bank_index.clone())
                                        .collect(),
                                ),
                                ComplexEventParams::Unknown(..) => None,
                            })
                    else {
                        eprintln!("No play event for complex sound. Skipping.");
                        dbg!(i, &group.name, &cue_name);
                        continue;
                    };

                    bank_indices
                }
            };

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
                    eprintln!("wavebank index {} not found in soundbank", wavebank_index);
                    continue;
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

                // TODO: Assert group.name == soundbank.name
                let wav_file = xsb::WavFile::from_raw(
                    wavebank
                        .wav_entries
                        .get(*wave_index as usize)
                        .cloned()
                        .ok_or("bad sound index")?,
                    &wavebank.wave_data,
                )?;

                let filename = if bank_indices.len() > 1 {
                    format!("{}_{}_{i}.wav", group.name, cue_name)
                } else {
                    format!("{}_{}.wav", group.name, cue_name)
                };

                wav_file.dump(out_path.join(filename))?;
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
