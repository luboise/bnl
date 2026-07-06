use binrw::BinRead;
use bnl::xsb;

fn main() -> Result<(), crate::Error> {
    let mut args = std::env::args().skip(1);

    let bnl_path = args.next().unwrap();
    let wavebank_path = args.next().unwrap();

    let bnl = bnl::BNLFile::from_bytes(&std::fs::read(bnl_path)?)?;

    let cuelist = bnl.get_asset::<bnl::asset::cuelist::CueList>("aid_xcuelist_ghoulies_default")?;
    let wavebank = xsb::XWavebank::read_le(&mut std::fs::File::open(wavebank_path)?)?;

    let bnl = bnl::BNLFile::from_bytes(&std::fs::read(bnl_path)?)?
        .get_asset::<bnl::asset::cuelist::CueList>("aid_xcuelist_ghoulies_default");

    let wav_files = xsb::wav_files_from_path(args[1].clone().into()).expect(&format!(
        "Failed to get wave files from path {}",
        args[1].to_string()
    ));
    xsb::dump_wav_files(&wav_files, args[2].clone().into()).expect("Failed to dump bytes.");

    Ok(())
}
