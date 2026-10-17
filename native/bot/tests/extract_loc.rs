//! `simtext::extract_localization` on a stand-in Balatro.exe: an exe stub with a love (zip) archive
//! appended, as a fused LÖVE game is.
use std::io::Write;

#[test]
fn extracts_text_hands_and_smods_overlay() {
    let dir = std::env::temp_dir().join(format!("extract_loc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("smods/localization")).unwrap();
    let game = "function Game:start() hands = {not = 'this'} end\n\
                function Game:init_game_object()\n    return {\n        hands = {\n            [\"Pair\"] = {chips = 10, example = {{'S_K', true}}},\n        },\n        other = {},\n    }\nend\n";
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    zip.start_file("localization/en-us.lua", opts).unwrap();
    zip.write_all(b"return {descriptions={}}").unwrap();
    zip.start_file("game.lua", opts).unwrap();
    zip.write_all(game.as_bytes()).unwrap();
    let archive = zip.finish().unwrap().into_inner();
    let mut exe = b"MZ\x90\x00 not really an exe ".repeat(64);
    exe.extend(archive);
    std::fs::write(dir.join("Balatro.exe"), exe).unwrap();
    std::fs::write(dir.join("smods/localization/en-us.lua"), "return {misc={}}").unwrap();

    let out = dir.join("loc");
    let wrote = jevbot::simtext::extract_localization(&dir.join("Balatro.exe"), Some(&dir.join("smods")), &out).unwrap();
    assert_eq!(wrote.len(), 3);
    assert_eq!(std::fs::read_to_string(out.join("en-us.lua")).unwrap(), "return {descriptions={}}");
    assert_eq!(
        std::fs::read_to_string(out.join("game-hands.lua")).unwrap(),
        "return {\n            [\"Pair\"] = {chips = 10, example = {{'S_K', true}}},\n        }\n"
    );
    assert_eq!(std::fs::read_to_string(out.join("smods-en-us.lua")).unwrap(), "return {misc={}}");
    // the hands table parses like the real one
    let v = jevbot::simtext::parse_lua_table(&std::fs::read_to_string(out.join("game-hands.lua")).unwrap());
    assert_eq!(v["Pair"]["chips"], serde_json::json!(10));

    // no Steamodded folder: the overlay is optional
    let wrote = jevbot::simtext::extract_localization(&dir.join("Balatro.exe"), Some(&dir.join("nope")), &out).unwrap();
    assert_eq!(wrote.len(), 2);
    assert!(jevbot::simtext::extract_localization(&dir.join("smods/localization/en-us.lua"), None, &out).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
