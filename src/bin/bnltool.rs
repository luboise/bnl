use std::{
    collections::HashSet,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use binrw::BinWrite;
use bnl::{BNLFile, asset::AssetType};
use clap::{Parser, Subcommand};
use walkdir::WalkDir;

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
        #[arg(value_name = "BNL FILES", required = true)]
        bnl_files: Vec<PathBuf>,

        /// The output directory for the extracted files
        #[arg(short = 'd', default_value = "./out")]
        output_dir: PathBuf,
    },

    #[command(short_flag = 'c')]
    /// Create a new BNL file from one or more directories which contain loose assets.
    Create {
        /// The directories containing the assets
        #[arg(required = true)]
        asset_dirs: Vec<PathBuf>,

        #[arg(short = 'o', value_name = "FILE")]
        /// The path which the new .bnl file will be written to
        output_file: PathBuf,
    },

    #[command(short_flag = 'l')]
    /// List the contents of a BNL file
    List {
        /// The BNL file whose contents to list
        #[arg(value_name = "BNL_FILE", required = true)]
        bnl_path: PathBuf,

        #[arg(short = 't')]
        /// The type of assets to list
        asset_type_filter: Option<String>,

        #[arg(short = 'a')]
        /// Print the assets in alphabetical order
        alphabetical_order: bool,

        /// Print a summary of the contents
        #[arg(short = 's')]
        print_summary: bool,
    },

    Diff {
        /// The first bnl file to compare
        file_1: PathBuf,
        /// The Second bnl file to compare
        file_2: PathBuf,

        /// Check asset names only, not their contents
        #[arg(short = 'n')]
        names_only: bool,

        /// Do not verify that the assets are in the same order in the files
        #[arg(short = 'a')]
        ignore_order: bool,
    },
}

fn main() -> Result<(), bnl::Error> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Extract {
            bnl_files,
            output_dir,
        } => {
            if bnl_files.is_empty() {
                return Err("Unable to extract: no bnl files provided.".into());
            }

            for bnl_file in bnl_files {
                println!("Extracting BNL file {}", bnl_file.display());

                let bytes = std::fs::read(&bnl_file)
                    .map_err(|e| format!("unable to open {}: {e}", bnl_file.display()))?
                    .to_vec();

                let bnl = BNLFile::from_bytes(&bytes)
                    .map_err(|e| format!("failed to process BNL file: {e}"))?;

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

                for raw_asset in raw_assets {
                    // ./out/common_bnl/aid_texture_xyz
                    let asset_path = bnl_out_path.join(raw_asset.metadata.name());

                    if asset_path.is_file() {
                        return Err(format!(
                            "unable to write to directory {} (file already exists)",
                            asset_path.display()
                        )
                        .into());
                    } else if !asset_path.exists() {
                        fs::create_dir_all(&asset_path).map_err(|e| {
                            format!(
                                "Unable to create directory {}.\nError: {}",
                                asset_path.display(),
                                e
                            )
                        })?
                    }

                    if let Err(e) = raw_asset
                        .metadata()
                        .write_le(&mut std::fs::File::create(asset_path.join("metadata"))?)
                    {
                        eprintln!(
                            "Unable to write metadata for {}\nError: {}",
                            &raw_asset.metadata.name(),
                            e
                        );
                    }

                    std::fs::write(
                        asset_path.join("descriptor"),
                        &raw_asset.data.descriptor_bytes,
                    )
                    .unwrap_or_else(|e| {
                        eprintln!(
                            "Unable to write descriptor for {}\nError: {}",
                            &raw_asset.metadata.name(),
                            e
                        );
                    });

                    if let Some(data_slices) = raw_asset.data.resource_chunks() {
                        data_slices.iter().enumerate().for_each(|(i, slice)| {
                            std::fs::write(asset_path.join(format!("resource{}", i)), slice)
                                .unwrap_or_else(|e| {
                                    eprintln!(
                                        "Unable to write descriptor for {}\nError: {}",
                                        raw_asset.metadata.name(),
                                        e
                                    );
                                });
                        });
                    }
                }
            }
        }

        Commands::Create {
            asset_dirs,
            output_file,
        } => {
            let mut bnl = BNLFile::default();

            let mut asset_paths = vec![];

            for dir in &asset_dirs {
                let walker = WalkDir::new(dir).into_iter();
                for asset_dir in walker
                    .filter_map(|val| val.ok())
                    .filter(|entry| {
                        if let Ok(entries) = fs::read_dir(entry.path()) {
                            entries
                                .filter_map(|e| e.ok())
                                .map(|e| e.path())
                                .all(|path| !path.is_dir())
                        } else {
                            false
                        }
                    })
                    .map(|dir_entry| dir_entry.path().to_owned())
                {
                    asset_paths.push(asset_dir.clone());
                }
                /*
                    if !dir.exists() {
                        eprintln!(
                            "ERROR: Provided asset dir {} does not exist.",
                            dir.display()
                        );
                        error_exit();
                    }

                        subwalker.filter_map(|e|e.ok()).filter_entry(|entry|entry.into_path.is_ok).all(|entry|)
                    }) {
                        println!("{}", entry?.path().display());
                    }
                }
                */
            }

            let raw_assets = asset_paths
                .iter()
                .map(|asset_path| {
                    println!("Reading raw asset from {}", asset_path.display());
                    bnl::RawAsset::from_dir(asset_path).unwrap()
                })
                .collect::<Vec<_>>();

            for raw_asset in raw_assets {
                println!(
                    "Adding {} to {}",
                    raw_asset.metadata.name(),
                    output_file.display()
                );

                bnl.append_raw_asset(raw_asset);
            }

            println!(
                "\nSuccessfully wrote all assets. Outputting to {}",
                output_file.display()
            );

            let bnl_bytes = bnl.to_bytes()?;

            std::fs::write(output_file, bnl_bytes)
                .map_err(|e| format!("failed to write output bnl file: {e}"))?;

            println!("\nSuccessfully wrote bnl file.");
        }

        Commands::List {
            bnl_path,
            alphabetical_order,
            asset_type_filter,
            print_summary,
        } => {
            let bytes = std::fs::read(&bnl_path)
                .map_err(|e| format!("unable to open {}: {e}", bnl_path.display()))?;

            let bnl = BNLFile::from_bytes(&bytes)
                .map_err(|e| format!("unable to process bnl file: {e}"))?;

            let mut raw_assets = bnl
                .get_raw_assets()
                .iter()
                .filter(|raw_asset| {
                    if let Some(type_filter) = &asset_type_filter {
                        raw_asset.metadata().asset_type.to_string() == type_filter.as_str()
                    } else {
                        true
                    }
                })
                .collect::<Vec<&bnl::RawAsset>>();

            // Sort by asset type
            raw_assets.sort_by_key(|raw| raw.metadata().asset_type);

            if alphabetical_order {
                // Since sort by key is stable, we can alphabetical sort after
                raw_assets.sort_by_key(|raw| raw.metadata().asset_type.to_string());
            }

            raw_assets.iter().for_each(|raw_asset| {
                println!("{}", raw_asset.metadata.name());
            });

            if print_summary {
                println!("{} assets found.", raw_assets.len());

                // Print the list of types found if theres no filter
                if asset_type_filter.is_none() {
                    let types_found =
                        raw_assets
                            .iter()
                            .fold(HashSet::<AssetType>::new(), |mut acc, val| {
                                acc.insert(val.metadata().asset_type);
                                acc
                            });

                    let mut types_str = types_found
                        .iter()
                        .map(|t| t.to_string())
                        .collect::<Vec<String>>();

                    types_str.sort();

                    println!(
                        "{num_types} Asset types: {}",
                        types_str.join(" "),
                        num_types = types_str.len(),
                    );
                }
            }
        }

        Commands::Diff { .. } => {
            println!("Diff feature coming soon.");
        }
    }

    Ok(())
}
