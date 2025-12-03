use std::{collections::HashMap, path::Path};

use bnl::asset::loctext::LoctextResource;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let json_path = Path::new(&args[1]);
    let out_path = Path::new(&args[2]);

    let json_bytes = std::fs::read(json_path).expect("Failed to read file.");

    let json: HashMap<String, String> =
        serde_json::from_slice(&json_bytes).expect("Failed to deserialise json.");

    let loctext = LoctextResource::from_hashmap(json).expect("Failed to read LoctextResource.");

    std::fs::write(
        out_path.to_str().unwrap(),
        loctext.dump().expect("Failed to dump loctext"),
    )
    .expect("Failed to write new loctext resource.");
}
