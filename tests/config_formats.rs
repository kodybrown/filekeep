use filekeep::config::Settings;
use power_code::config::load;
use std::path::Path;

#[test]
fn all_documented_formats_have_the_same_schema_and_values() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let yaml: Settings = load(&root.join("filekeep.yaml")).unwrap();
    for name in ["filekeep.json", "filekeep.toml", "filekeep.xml"] {
        let other: Settings = load(&root.join("examples").join(name)).unwrap();
        assert_eq!(yaml, other, "{name}");
    }
}

#[test]
fn yaml_yml_bom_and_unknown_fields() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("settings.yml");
    std::fs::write(&file, "\u{feff}entries:\n  - id: one\n    source: { type: file, path: input }\n    destination: output\n    filename: backup\n").unwrap();
    let settings: Settings = load(&file).unwrap();
    assert!(settings.entries[0].skip_if_unchanged);
    std::fs::write(&file, "entries: []\nskpi_if_unchanged: true\n").unwrap();
    assert!(load::<Settings>(&file).is_err());
}
