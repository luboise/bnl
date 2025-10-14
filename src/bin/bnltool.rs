use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use bnl::BNLFile;
use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    version,
    propagate_version = true,
    subcommand_required = true,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(name = "extract", short_flag = 'x')]
    /// Extract an existing BNL file
    Extract {
        /// The .bnl file to extract
        #[arg(value_name = "BNL")]
        bnl_file: PathBuf,

        /// The output directory for the extracted files
        #[arg(short = 'd', default_value = "./out")]
        output_dir: PathBuf,
    },

    #[command(short_flag = 'c')]
    /// Create a new BNL file from one or more directories which contain loose assets.
    Create {
        /// The directories containing the assets
        asset_dirs: Vec<PathBuf>,

        #[arg(short = 'o', value_name = "FILE")]
        /// The path which the new .bnl file will be written to
        output_file: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Extract {
            bnl_file,
            output_dir,
        } => {
            println!("Opening BNL file {}", bnl_file.display());

            let bytes: Vec<u8> = match std::fs::read(&bnl_file) {
                Ok(f) => f,
                Err(e) => {
                    println!("Unable to open file {}. Error: {}", bnl_file.display(), e);
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
                bnl_file
                    .file_stem()
                    .unwrap_or(OsStr::new("unknown"))
                    .display()
            );

            // ./out/common_bnl
            let bnl_out_path = Path::new(&output_dir).join(out_filename);

            raw_assets.iter().for_each(|raw_asset| {
                // ./out/common_bnl/aid_texture_xyz
                let asset_path: PathBuf = bnl_out_path.join(raw_asset.name());

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

                std::fs::write(asset_path.join("descriptor"), raw_asset.descriptor_bytes())
                    .unwrap_or_else(|e| {
                        eprintln!(
                            "Unable to write descriptor for {}\nError: {}",
                            &raw_asset.name(),
                            e
                        );
                    });

                if let Some(data_slices) = raw_asset.resource_chunks() {
                    data_slices.iter().enumerate().for_each(|(i, slice)| {
                        std::fs::write(asset_path.join(format!("resource{}", i)), slice)
                            .unwrap_or_else(|e| {
                                eprintln!(
                                    "Unable to write descriptor for {}\nError: {}",
                                    raw_asset.name(),
                                    e
                                );
                            });
                    });
                }
            });
        }
        Commands::Create {
            asset_dirs,
            output_file,
        } => println!("Not implemented yet!"),
    }
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
