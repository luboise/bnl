use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let game_dir: std::path::PathBuf = args[0].clone().into();
    let modification = bnl::modding::Mod::from_dir(&args[1])?;

    let bnl_paths = walkdir::WalkDir::new(game_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "bnl")
                .then(|| entry.path().to_path_buf())
        })
        .collect::<Vec<_>>();

    let mut assets = HashMap::default();

    // Get overrides from mod
    for (aid, raw_override) in &modification.raw_asset_overrides {
        let raw_asset = bnl_paths
            .iter()
            .find_map(|path| {
                // TODO: Display errors here properly
                let bytes = std::fs::read(path).ok()?;

                if bnl::get_aid_list(&bytes).ok()?.contains(aid) {
                    let mut raw_asset = bnl::BNLFile::from_bytes(&bytes)
                        .ok()?
                        .get_raw_asset(aid)?
                        .to_owned();

                    *raw_asset.descriptor_bytes_mut() = raw_override.descriptor_bytes.clone();
                    // TODO: Resource chunks

                    Some(raw_asset)
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("{aid} not found"))?;

        assets.insert(aid.clone(), raw_asset);
    }

    // Get rest of assets from game files
    for aid in modification.affected_assets() {
        if assets.contains_key(&aid) {
            continue;
        }

        let found_asset = bnl_paths
            .iter()
            .find_map(|path| {
                // TODO: Display errors here properly
                let bytes = std::fs::read(path).ok()?;

                if bnl::get_aid_list(&bytes).ok()?.contains(&aid) {
                    Some(
                        bnl::BNLFile::from_bytes(&bytes)
                            .ok()?
                            .get_raw_asset(&aid)?
                            .to_owned(),
                    )
                } else {
                    None
                }
            })
            .ok_or_else(|| format!("Unable to find asset {aid}"))?;

        assets.insert(aid, found_asset);
    }

    let mut ctx = bnl::modding::ModContext {
        bnl_basename: String::default(),
        all_bnl_paths: vec![],
        assets,
    };

    ctx.all_bnl_paths = bnl_paths.clone();

    for bnl_path in bnl_paths {
        let bnl_bytes = std::fs::read(&bnl_path)?;

        ctx.bnl_basename = bnl_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_owned())
            .unwrap();

        let aid_list = bnl::get_aid_list(&bnl_bytes).expect("BAD");

        let mut bnl = bnl::BNLFile::from_bytes(&bnl_bytes).expect("Stupid bnl error");

        let num_applied = modification.apply(&mut ctx, &mut bnl)?;
        if num_applied > 0 {
            println!(
                "Applied {num_applied} modifications to {}",
                bnl_path.display()
            );
            std::fs::write(bnl_path, bnl.to_bytes())?;
        }
    }

    Ok(())
}
/*
 let new_cached_asset = ctx
                            .all_bnl_paths
                            .iter()
                            .find_map(|path| {
                                // TODO: Display errors here properly
                                let bytes = std::fs::read(path).ok()?;

                                if get_aid_list(&bytes).ok()?.contains(&aid_to_add) {
                                    Some(
                                        BNLFile::from_bytes(&bytes)
                                            .ok()?
                                            .get_raw_asset(&aid_to_add)?
                                            .to_owned(),
                                    )
                                } else {
                                    None
                                }
                            })
                            .ok_or_else(|| "Unable to get asset".to_string())?;

                        ctx.assets.insert(aid_to_add.clone(), new_cached_asset);
                        ctx.assets.get(&aid_to_add).unwrap()
*/
