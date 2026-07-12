use dioxus::prelude::*;

use std::env::current_dir;

fn main() {
    dioxus::desktop::launch::launch(app, Default::default(), Default::default())
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct AppState {
    game_dir: Option<std::path::PathBuf>,
    mod_dir: Option<std::path::PathBuf>,
    rebuild_iso_path: Option<std::path::PathBuf>,
}

impl AppState {
    pub fn config_path() -> std::path::PathBuf {
        current_dir().unwrap_or("./".into()).join("config.json")
    }

    pub fn save(&self) -> Result<(), bnl::Error> {
        serde_json::to_writer_pretty(
            std::io::BufWriter::new(std::fs::File::create(Self::config_path())?),
            self,
        )?;
        Ok(())
    }
    pub fn load() -> Result<Self, bnl::Error> {
        let v = serde_json::from_slice(&std::fs::read(Self::config_path())?)?;
        Ok(v)
    }
}

fn app() -> Element {
    let mut game_dir = use_signal(|| {
        AppState::load()
            .ok()
            .and_then(|state| state.game_dir)
            .or(std::env::home_dir())
            .or(std::env::current_dir().ok())
    });
    let mut mod_dir = use_signal(|| {
        AppState::load()
            .ok()
            .and_then(|state| state.mod_dir)
            .or(std::env::home_dir())
            .or(std::env::current_dir().ok())
    });

    let mut output_iso_path = use_signal(|| {
        AppState::load()
            .ok()
            .and_then(|state| state.rebuild_iso_path)
            .or(std::env::current_dir().ok().map(|v| v.join("modded.iso")))
    });
    let mut rebuild_game = use_memo(move || output_iso_path().is_some());

    let onclick = move |_| {
        let Some(game_dir) = game_dir() else {
            eprintln!("bad game dir");
            return;
        };

        let Some(mod_dir) = mod_dir() else {
            eprintln!("bad game dir");
            return;
        };

        if let Err(e) = bnl::modding::apply_mod_to_game(&game_dir, mod_dir) {
            eprintln!("failed to apply mods: {e}");
            return;
        }

        println!("mod successfully applied");

        if !rebuild_game() {
            return;
        }

        println!("rebuilding game");

        let Some(iso_path) = output_iso_path() else {
            eprintln!("no iso path selected.");
            return;
        };

        if let Err(e) = xbpatch_core::iso_handling::create_iso("extract-xiso", &iso_path, &game_dir)
        {
            eprintln!("failed to rebuild iso: {e}");
            return;
        }

        println!("\nsuccessfully rebuilt game");
    };

    use_effect(move || {
        println!("{rebuild_game}");
        let state = AppState {
            game_dir: game_dir(),
            mod_dir: mod_dir(),
            rebuild_iso_path: output_iso_path(),
        };

        if let Err(e) = state.save() {
            eprintln!("error saving app state: {e}");
        }
    });

    rsx! {
        div {
            display: "flex",
            flex_direction: "column",
            label { "Game Directory" }
            div {
                button {
                    onclick: move |_| {
                        game_dir.set(rfd::FileDialog::new()
                            .set_directory(game_dir().unwrap_or("./".into()))
                            .pick_folder()
                            .or(game_dir())
                            );
                    },
                    "Choose Dir"
                }
                "{game_dir().map(|v| v.display().to_string()).unwrap_or(\"no path selected\".into())}"
            }
            label { "Mod Directory" }
            div {
                button {
                    onclick: move |_| {
                        mod_dir.set(rfd::FileDialog::new()
                            .set_directory(mod_dir().unwrap_or("./".into()))
                            .pick_folder()
                            .or(mod_dir())
                            );
                    },
                    "Choose Dir"
                }
                "{mod_dir().map(|v| v.display().to_string()).unwrap_or(\"no path selected\".into())}"
            }
            label { "Rebuild Game" }
            input {
                type: "checkbox",
                checked: rebuild_game(),
                onchange: move |evt| rebuild_game.set(evt.checked())
            }

            div {
                if rebuild_game() {
                    label { "Output ISO Path" },
                    div {
                        button {
                            onclick: move |_| {
                                output_iso_path.set(rfd::FileDialog::new()
                                    .set_directory(output_iso_path().unwrap_or("./".into()))
                                    .pick_file()
                                    .or(output_iso_path())
                                    );
                            },
                            "Choose Path"
                        }
                        "{output_iso_path().map(|v| v.display().to_string()).unwrap_or(\"no path selected\".into())}"
                    }
                }
            }

            button {
                onclick, "mod"
            }
        }
    }
}
