use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use bnl::BNLFile;

fn main() {
    let args: Vec<String> = env::args().collect();

    // TODO: Refactor this to use a CLI args crate if this gets worked on more
    if args.len() != 3 {
        print_usage();
        return;
    }

    if &args[1].to_lowercase() != "-x" {
        eprintln!("Expected -x as second argument.");
        error_exit(true);
    }

    let bnl_path = PathBuf::from(&args[2]);

    println!("Opening BNL file {}", bnl_path.display());

    let bytes: Vec<u8> = match std::fs::read(&bnl_path) {
        Ok(f) => f,
        Err(e) => {
            println!("Unable to open file {}. Error: {}", bnl_path.display(), e);
            return;
        }
    };

    let bnl = match BNLFile::from_bytes(&bytes) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Unable to process BNL file: {:?}", e);

            error_exit(false);
        }
    };

    let raw_assets = bnl.get_raw_assets();

    let out_filename = format!(
        "{}_bnl",
        bnl_path
            .file_stem()
            .unwrap_or(OsStr::new("unknown"))
            .display()
    );

    // ./out/common_bnl
    let bnl_out_path = Path::new("./out").join(out_filename);

    raw_assets.iter().for_each(|raw_asset| {
        // ./out/common_bnl/aid_texture_xyz
        let asset_path: PathBuf = bnl_out_path.join(&raw_asset.name);

        if asset_path.is_file() {
            eprintln!(
                "Unable to write to {} (A file already exists by that name)",
                asset_path.display()
            );
            return;
        } else if !asset_path.exists() {
            match fs::create_dir_all(&asset_path) {
                Ok(_) => (),
                Err(e) => {
                    eprintln!(
                        "Unable to create directory {}.\nError: {}",
                        asset_path.display(),
                        e
                    );
                    return;
                }
            }
        }

        std::fs::write(asset_path.join("descriptor"), &raw_asset.descriptor_bytes).unwrap_or_else(
            |e| {
                eprintln!(
                    "Unable to write descriptor for {}\nError: {}",
                    &raw_asset.name, e
                );
            },
        );

        raw_asset
            .data_slices
            .iter()
            .enumerate()
            .for_each(|(i, slice)| {
                std::fs::write(asset_path.join(format!("resource{}", i)), slice).unwrap_or_else(
                    |e| {
                        eprintln!(
                            "Unable to write descriptor for {}\nError: {}",
                            &raw_asset.name, e
                        );
                    },
                );
            });
    });
}

fn print_usage() {
    println!(
        r"Usage: bnltool -x [path to BNL file]
Examples:
    bnltool -x my_bnl.bnl
    bnltool -x /home/username/game/bundles/common.bnl"
    );
}

fn error_exit(show_usage: bool) -> ! {
    eprintln!("\nUnable to continue.");

    if show_usage {
        print_usage();
    }

    std::process::exit(1);
}
