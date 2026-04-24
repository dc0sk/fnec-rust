use std::fs;
use std::path::PathBuf;

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}

#[test]
fn all_corpus_nec_decks_include_ge_card() {
    let dir = corpus_dir();
    let entries = fs::read_dir(&dir).expect("failed to read corpus directory");

    let mut checked = 0usize;
    for entry in entries {
        let entry = entry.expect("failed to read corpus directory entry");
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) != Some("nec") {
            continue;
        }

        checked += 1;
        let text = fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("failed to read deck file: {}", path.display());
        });

        let has_ge = text
            .lines()
            .map(str::trim)
            .any(|line| line == "GE" || line.starts_with("GE "));

        assert!(
            has_ge,
            "missing GE card in corpus deck {}",
            path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("<unknown>")
        );
    }

    assert!(checked > 0, "no .nec files found in corpus directory");
}
