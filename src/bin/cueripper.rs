use binrw::BinRead;
use bnl::xsb;

fn main() -> Result<(), bnl::Error> {
    let mut args = std::env::args().skip(1);

    let bnl_path = args.next().unwrap();
    let wavebank_path = args.next().unwrap();
    let out_path: std::path::PathBuf = args.next().unwrap().into();

    let bnl = bnl::BNLFile::from_bytes(&std::fs::read(bnl_path)?)?;

    let cuelist = bnl.get_asset::<bnl::asset::cuelist::CueList>("aid_xcuelist_ghoulies_default")?;
    let wavebank = xsb::XWavebank::read_le(&mut std::fs::File::open(wavebank_path)?)?;

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

    Ok(())
}
