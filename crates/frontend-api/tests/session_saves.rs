//! Session save/load round-trips and CLI-format compatibility: the native
//! UI must read every format the CLI writes (plain JSON, MessagePack,
//! gzip/zstd) and the CLI must read what `Session::save` writes.

use frontend_api::Session;
use frontend_api::session::list_saves;
use infrastructure::data_loader::load_embedded_game_data;
use infrastructure::persistence::{self, SaveCompression};
use std::path::PathBuf;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "imperialism-session-saves-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn new_session() -> Session {
    Session::from_game(frontend_api::setup::new_game(
        "session-save-test",
        2,
        0,
        40,
        30,
        3,
        4,
        "",
        "",
        None,
    ))
}

#[test]
fn session_save_round_trips_and_lists() {
    let dir = temp_dir("roundtrip");
    let session = new_session();
    let path = dir.join("native.json.gz");
    session.save(&path).unwrap();

    let loaded = Session::load(&path).unwrap();
    assert_eq!(loaded.turn_number(), session.turn_number());
    assert_eq!(loaded.map_key(), session.map_key());
    assert_eq!(loaded.observer_mode(), session.observer_mode());
    assert_eq!(loaded.human_nation(), session.human_nation());

    let saves = list_saves(&dir);
    assert_eq!(saves.len(), 1);
    assert_eq!(saves[0].file_name, "native.json.gz");
    assert!(!saves[0].turn_display.is_empty(), "metadata readable");

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn session_loads_every_cli_save_format() {
    let dir = temp_dir("cli-formats");
    let session = new_session();
    let game = session.game();

    let json = dir.join("cli.json");
    persistence::save_game(game, &json).unwrap();
    let bin = dir.join("cli.bin");
    persistence::save_game_binary(game, &bin).unwrap();
    let gz = dir.join("cli.json.gz");
    persistence::save_game_compressed(game, &gz, SaveCompression::Gzip).unwrap();
    let zst = dir.join("cli.bin.zst");
    persistence::save_game_binary_compressed(game, &zst, SaveCompression::Zstd).unwrap();

    for path in [&json, &bin, &gz, &zst] {
        let loaded = Session::load(path)
            .unwrap_or_else(|e| panic!("failed to load {}: {}", path.display(), e.message()));
        assert_eq!(loaded.turn_number(), session.turn_number());
        assert_eq!(loaded.map_key(), session.map_key());
    }

    // Every format is listed with readable metadata.
    let saves = list_saves(&dir);
    assert_eq!(saves.len(), 4);
    assert!(saves.iter().all(|s| !s.turn_display.is_empty()));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn cli_reads_native_session_save() {
    let dir = temp_dir("cli-reads-native");
    let session = new_session();
    let path = dir.join("native.json.gz");
    session.save(&path).unwrap();

    // The CLI's load path: persistence::load_game_with_data.
    let game = persistence::load_game_with_data(&path, load_embedded_game_data()).unwrap();
    assert_eq!(game.turn.0, session.turn_number());

    // And its save browser metadata reader.
    let meta = persistence::read_save_metadata(&path).unwrap();
    assert_eq!(meta.version, persistence::CURRENT_SAVE_VERSION);

    std::fs::remove_dir_all(&dir).unwrap();
}
