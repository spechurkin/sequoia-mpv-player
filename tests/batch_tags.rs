use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    prelude::ItemKey,
};
use sequoia::{
    library,
    model::Media,
    tags::{self, TagPatch},
};
use std::path::Path;

fn wav(path: &Path, title: &str, track: u32, root: &Path) -> Media {
    let mut data = Vec::new();
    data.extend(b"RIFF");
    data.extend(2036u32.to_le_bytes());
    data.extend(b"WAVEfmt ");
    data.extend(16u32.to_le_bytes());
    data.extend(1u16.to_le_bytes());
    data.extend(1u16.to_le_bytes());
    data.extend(8000u32.to_le_bytes());
    data.extend(16000u32.to_le_bytes());
    data.extend(2u16.to_le_bytes());
    data.extend(16u16.to_le_bytes());
    data.extend(b"data");
    data.extend(2000u32.to_le_bytes());
    data.extend(vec![0; 2000]);
    std::fs::write(path, data).unwrap();
    library::write_tags(
        &Media {
            path: path.into(),
            title: title.into(),
            artist: "Original artist".into(),
            album: "Original album".into(),
            genre: "Jazz".into(),
            year: 2001,
            track,
            ..Default::default()
        },
        None,
        root,
    )
        .unwrap()
}
#[test]
fn batch_preserves_individual_tags_and_extra_metadata_and_deduplicates() {
    let root = tempfile::tempdir().unwrap();
    let a = wav(&root.path().join("a.wav"), "First", 2, root.path());
    let b = wav(&root.path().join("b.wav"), "Second", 7, root.path());
    let mut file = lofty::read_from_path(&a.path).unwrap();
    file.primary_tag_mut()
        .unwrap()
        .insert_text(ItemKey::Composer, "Composer".into());
    file.save_to_path(&a.path, WriteOptions::default()).unwrap();
    let backups = std::fs::read_dir(root.path().join("tag-backups"))
        .unwrap()
        .count();
    let mut progress = Vec::new();
    let result = tags::write_batch(
        &[a.clone(), b.clone(), a.clone()],
        &TagPatch {
            album: Some("Together".into()),
            ..Default::default()
        },
        None,
        root.path(),
        |n| progress.push(n),
    );
    assert!(result.failed.is_empty(), "{:?}", result.failed);
    assert_eq!(result.saved.len(), 2);
    assert_eq!(progress, [1, 2]);
    for (old, saved) in [a.clone(), b].iter().zip(&result.saved) {
        assert_eq!(saved.title, old.title);
        assert_eq!(saved.track, old.track);
        assert_eq!(saved.artist, old.artist);
        assert_eq!(saved.genre, old.genre);
        assert_eq!(saved.year, old.year);
        assert_eq!(saved.album, "Together");
    }
    assert_eq!(
        lofty::read_from_path(&a.path)
            .unwrap()
            .primary_tag()
            .unwrap()
            .get_string(&ItemKey::Composer),
        Some("Composer")
    );
    assert_eq!(
        std::fs::read_dir(root.path().join("tag-backups"))
            .unwrap()
            .count(),
        backups + 2
    );
}
#[test]
fn batch_clear_number_and_partial_failure() {
    let root = tempfile::tempdir().unwrap();
    let a = wav(&root.path().join("a.wav"), "First", 2, root.path());
    let b = wav(&root.path().join("b.wav"), "Second", 7, root.path());
    let missing = Media {
        path: root.path().join("missing.wav"),
        ..Default::default()
    };
    let patch = TagPatch {
        artist: Some(String::new()),
        title: Some(String::new()),
        year: Some(0),
        genre: Some(String::new()),
        ..Default::default()
    };
    let result = tags::write_batch(&[a, missing, b], &patch, Some(5), root.path(), |_| {});
    assert_eq!(result.saved.len(), 2);
    assert_eq!(result.failed.len(), 1);
    assert_eq!(
        result.saved.iter().map(|m| m.track).collect::<Vec<_>>(),
        [5, 7]
    );
    for saved in result.saved {
        assert!(saved.artist.is_empty());
        assert!(saved.genre.is_empty());
        assert_eq!(saved.year, 0);
        assert!(lofty::read_from_path(&saved.path)
            .unwrap()
            .primary_tag()
            .unwrap()
            .get_string(&ItemKey::TrackTitle)
            .is_none());
    }
}
#[test]
fn no_op_and_invalid_numbering_do_not_write() {
    let root = tempfile::tempdir().unwrap();
    let a = wav(&root.path().join("a.wav"), "First", 2, root.path());
    let before = std::fs::read(&a.path).unwrap();
    let backups = std::fs::read_dir(root.path().join("tag-backups"))
        .unwrap()
        .count();
    assert!(tags::write_batch(
        &[a.clone()],
        &TagPatch::default(),
        None,
        root.path(),
        |_| panic!()
    )
        .saved
        .is_empty());
    assert_eq!(
        tags::write_batch(
            &[a.clone()],
            &TagPatch::default(),
            Some(0),
            root.path(),
            |_| panic!()
        )
            .failed
            .len(),
        1
    );
    assert_eq!(std::fs::read(&a.path).unwrap(), before);
    assert_eq!(
        std::fs::read_dir(root.path().join("tag-backups"))
            .unwrap()
            .count(),
        backups
    );
}

#[test]
fn invalid_artwork_preserves_original_and_backup() {
    let root = tempfile::tempdir().unwrap();
    let a = wav(&root.path().join("a.wav"), "First", 2, root.path());
    let before = std::fs::read(&a.path).unwrap();
    let result = tags::write_batch(
        &[a.clone()],
        &TagPatch {
            album: Some("Changed".into()),
            artwork: Some(vec![0, 1, 2]),
            ..Default::default()
        },
        None,
        root.path(),
        |_| {},
    );
    assert_eq!(result.failed.len(), 1);
    assert!(result.saved.is_empty());
    assert_eq!(std::fs::read(&a.path).unwrap(), before);
    assert!(std::fs::read_dir(root.path().join("tag-backups"))
        .unwrap()
        .any(|e| std::fs::read(e.unwrap().path()).unwrap() == before));
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".sequoia-")));
}

#[test]
fn grouping_tags_roundtrip_batch_preservation_and_explicit_clear() {
    let root = tempfile::tempdir().unwrap();
    let a = wav(&root.path().join("grouping.wav"), "Song", 1, root.path());
    let patch = TagPatch {
        album_artist: Some("Album artist".into()),
        disc: Some(2),
        grouping: Some("Deluxe".into()),
        ..Default::default()
    };
    let saved = library::patch_tags(&a.path, &patch, root.path()).unwrap();
    assert_eq!(saved.album_artist, "Album artist");
    assert_eq!(saved.disc, 2);
    assert_eq!(saved.grouping, "Deluxe");
    let saved = library::patch_tags(
        &a.path,
        &TagPatch {
            title: Some("Renamed".into()),
            ..Default::default()
        },
        root.path(),
    )
        .unwrap();
    assert_eq!(saved.album_artist, "Album artist");
    assert_eq!(saved.disc, 2);
    assert_eq!(saved.grouping, "Deluxe");
    let saved = library::patch_tags(
        &a.path,
        &TagPatch {
            album_artist: Some(String::new()),
            disc: Some(0),
            grouping: Some(String::new()),
            ..Default::default()
        },
        root.path(),
    )
        .unwrap();
    assert!(saved.album_artist.is_empty());
    assert_eq!(saved.disc, 0);
    assert!(saved.grouping.is_empty());
}
