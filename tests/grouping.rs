use sequoia::{
    grouping::{Grouping, Level},
    model::{Data, Media},
};

#[test]
fn old_library_loads_and_grouping_survives_save() {
    let mut data: Data = serde_json::from_str(r#"{"library":[{"path":"song.mp3","title":"Song","artist":"A/B","album":"Album","duration":1,"year":2000,"track":1,"genre":"Rock","audio":true}]}"#).unwrap();
    assert_eq!(data.library[0].album_artist, "");
    assert_eq!(Level::AlbumArtist.value(&data.library[0]), "A");
    data.settings.music_grouping = Grouping {
        levels: [Level::Genre, Level::Artist, Level::YearAlbumDisc],
        separate_albums: true,
    };
    let reloaded: Data = serde_json::from_str(&serde_json::to_string(&data).unwrap()).unwrap();
    assert_eq!(
        reloaded.settings.music_grouping,
        data.settings.music_grouping
    );
}

#[test]
fn album_artist_compilations_and_disc_boundaries() {
    let a = Media {
        artist: "Singer; Guest".into(),
        album_artist: "Various Artists".into(),
        album: "Collection".into(),
        disc: 1,
        year: 2020,
        ..Default::default()
    };
    let mut b = a.clone();
    b.artist = "Other singer".into();
    b.disc = 2;
    assert_eq!(Level::AlbumArtist.value(&a), Level::AlbumArtist.value(&b));
    assert_ne!(Level::Artist.value(&a), Level::Artist.value(&b));
    assert_ne!(Level::AlbumDisc.value(&a), Level::AlbumDisc.value(&b));
    assert_ne!(
        Level::YearAlbumDisc.value(&a),
        Level::YearAlbumDisc.value(&b)
    );
    let mut grouping = Grouping::default();
    b.disc = 1;
    b.grouping = "Deluxe".into();
    assert_eq!(grouping.key(&a, 1), grouping.key(&b, 1));
    grouping.separate_albums = true;
    assert_ne!(grouping.key(&a, 1), grouping.key(&b, 1));
}

#[test]
fn every_preset_has_real_fields_and_unknown_values_are_visible() {
    assert_eq!(Grouping::presets().len(), 14);
    for grouping in Grouping::presets() {
        assert!(!grouping.label().is_empty());
        for (depth, level) in grouping.levels.iter().enumerate() {
            if *level != Level::None {
                assert!(!grouping.key(&Media::default(), depth).0.is_empty());
            }
        }
    }
}
