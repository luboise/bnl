use bnl::xsb;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let wav_files = xsb::wav_files_from_path(args[1].clone().into()).expect(&format!(
        "Failed to get wave files from path {}",
        args[1].to_string()
    ));
    xsb::dump_wav_files(&wav_files, args[2].clone().into()).expect("Failed to dump bytes.");
}
