use binrw::BinReaderExt;
use bnl::xsb;

fn main() -> Result<(), bnl::Error> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let path: String = args[0].clone();

    let bytes = std::fs::read(path)?;

    let mut cur = std::io::Cursor::new(&bytes);

    let wavebank: xsb::XWavebank = cur.read_le()?;

    xsb::dump_wav_entries(&wavebank.wav_entries, args[1].clone().into())?;

    Ok(())
}
