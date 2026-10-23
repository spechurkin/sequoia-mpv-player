#[test]
#[ignore = "live external MusicBrainz request"]
fn musicbrainz_returns_candidates() -> anyhow::Result<()> {
    let results = sequoia::online::search("Clair de lune", "Claude Debussy")?;
    anyhow::ensure!(
        !results.is_empty(),
        "MusicBrainz search returned no results"
    );
    anyhow::ensure!(
        results
            .iter()
            .any(|r| r.title.to_lowercase().contains("clair")),
        "Unexpected results"
    );
    Ok(())
}
