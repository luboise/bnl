use bnl::xsb;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    xsb::dump_xwavebank_bytes(args[1].clone().into(), args[2].clone().into())
        .expect("Failed to dump bytes");
}
