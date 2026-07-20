fn main() -> Result<(), bnl::Error> {
    let mut args = std::env::args().skip(1);

    let dir = std::path::PathBuf::from(args.next().ok_or("no descriptor provided")?);
    let model_aid = args.next().ok_or("no model name provided")?;

    let model: bnl::asset::Asset<bnl::asset::model::Model> = bnl::find_asset(dir, model_aid)?;

    let out_dir = std::path::PathBuf::from("./model_validation");

    std::fs::create_dir_all(&out_dir).map_err(|e| format!("failed to create output dir: {e}"))?;

    /*
    gltf.prepare_for_export().unwrap();
    std::fs::create_dir_all(&out_dir)?;
    let _ = std::fs::remove_file(out_dir.join("boy.gltf"));
    gltf.export(
        &out_dir.join("boy.gltf"),
        gltf_writer::serialisation::SerialGltfType::JSON,
    )
    .unwrap();
    */

    // get the asset and dump it
    let asset = model.clone().to_raw_asset()?;
    let pre_raw = asset.data;
    std::fs::write(out_dir.join("pre_descriptor"), &pre_raw.descriptor_bytes)?;
    std::fs::write(
        out_dir.join("pre_model_subres"),
        &pre_raw.descriptor_bytes[0x40..],
    )?;
    std::fs::write(
        out_dir.join("pre_resource"),
        pre_raw.resource_chunks.first().unwrap(),
    )?;

    // repack the asset, dump it then compare
    let post_model: bnl::asset::model::Model = pre_raw.clone().try_into()?;

    let post_raw = bnl::RawAssetData::try_from(post_model)?;
    std::fs::write(out_dir.join("post_descriptor"), &post_raw.descriptor_bytes)?;
    std::fs::write(
        out_dir.join("post_model_subres"),
        &post_raw.descriptor_bytes[0x40..],
    )?;
    std::fs::write(
        out_dir.join("post_resource"),
        post_raw.resource_chunks.first().unwrap(),
    )?;

    bnl::utils::compare_streams(&pre_raw.descriptor_bytes, &post_raw.descriptor_bytes)?;
    bnl::utils::compare_streams(
        pre_raw.resource_chunks.first().unwrap(),
        post_raw.resource_chunks.first().unwrap(),
    )?;

    Ok(())
}
