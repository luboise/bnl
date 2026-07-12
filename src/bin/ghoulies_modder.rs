fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let game_dir: std::path::PathBuf = args[0].clone().into();

    bnl::modding::apply_mod_to_game(game_dir, &args[1])?;
    Ok(())
}
