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

    let affected_assets = modification.affected_assets();

    let mut ctx = bnl::modding::ModContext {
        bnl_basename: String::default(),
        all_bnl_paths: vec![],
        cached_assets: std::collections::HashMap::default(),
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

        // If none of the mods affect this file
        if aid_list
            .iter()
            .find(|v| affected_assets.contains(v))
            .is_none()
        {
            continue;
        }

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
