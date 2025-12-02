use std::path::Path;

use bnl::asset::loctext::LoctextResource;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let loctext_path = Path::new(&args[1]);

    let bytes = std::fs::read(loctext_path).expect("Failed to read file.");

    let loctext = LoctextResource::from_bytes(&bytes).expect("Failed to read LoctextResource.");

    std::fs::write(
        format!(
            "./out/loctext_{}.json",
            loctext_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .expect("Failed to get file stem.")
        ),
        serde_json::to_vec_pretty(&loctext).expect("Failed to serialise"),
    )
    .expect("Failed to write serialised loctext resource.");
}
