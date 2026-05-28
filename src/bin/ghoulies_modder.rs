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

                    raw_asset.data = raw_override.data.clone();

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

    for (_aid, _asset) in assets
        .iter_mut()
        .filter(|(aid, _)| modification.cutscene_mods.contains_key(*aid))
    {
        todo!("cutscene mod apply not implemented (BUG ME ABOUT THIS)");
        // Apply the cutscene mod
    }

    // for (aid, raw_asset) in assets
    //     .iter_mut()
    //     .filter(|(aid, _)| modification.model_mods.contains_key(*aid))
    // {
    //     let model_mod = modification.model_mods.get(aid).unwrap();
    //     model_mod.apply_raw(&mut raw_asset.data)?;
    // }

    let mut ctx = bnl::modding::ModContext {
        bnl_basename: String::default(),
        all_bnl_paths: vec![],
        assets,
    };

    ctx.all_bnl_paths = bnl_paths.clone();

    for bnl_path in bnl_paths {
        let bnl_bytes = std::fs::read(&bnl_path)?;
        let Ok(aid_list) = bnl::get_aid_list(&bnl_bytes) else {
            continue;
        };

        ctx.bnl_basename = bnl_path
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| s.to_owned())
            .unwrap();

        if aid_list.iter().all(|aid| !ctx.assets.contains_key(aid))
            && !modification.spec.bnl_edits.contains_key(&ctx.bnl_basename)
        {
            continue;
        }

        let mut bnl = bnl::BNLFile::from_bytes(&bnl_bytes).expect("Stupid bnl error");

        let num_applied = modification.apply(&mut ctx, &mut bnl)?;

        if num_applied > 0 {
            println!(
                "Applied {num_applied} modifications to {}",
                bnl_path.display(),
            );
            std::fs::write(bnl_path, bnl.to_bytes()?)?;
        }
    }

    Ok(())
}
