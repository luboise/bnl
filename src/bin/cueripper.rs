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

            let (wavebank, sound_index) = {
                let (wavebank_name, sound_index) = match entry {
                    SoundEntry::Trivial(trivial) => (
                        soundbank
                            .wavebank_array
                            .names
                            .get(trivial.wavebank_index as usize)
                            .cloned()
                            .ok_or_else(|| {
                                format!(
                                    "wavebank index {} not found in soundbank",
                                    trivial.wavebank_index,
                                )
                            })?,
                        trivial.wave_index,
                    ),
                    SoundEntry::Simple(simple) => {
                        println!("Skipping simple sound.");
                        continue;
                    }
                    SoundEntry::Complex(complex) => {
                        println!("Skipping complex sound.");
                        continue;
                    }
                };

                let Some(wavebank) = wavebanks
                    .iter()
                    .find(|wavebank| wavebank.name == wavebank_name)
                else {
                    eprintln!(
                        "wavebank by name {} not found. skipping this sound",
                        str::from_utf8(&wavebank_name).unwrap_or("parse_error")
                    );
                    continue;
                };

                (wavebank, sound_index)
            };

            let wav_file = xsb::WavFile::from_raw(
                wavebank
                    .wav_entries
                    .get(sound_index as usize)
                    .cloned()
                    .ok_or("bad sound index")?,
                &wavebank.wave_data,
            )?;

            // TODO: Assert group.name == soundbank.name

            wav_file.dump(out_path.join(format!("{}_{}.wav", group.name, cue_name)))?;
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
