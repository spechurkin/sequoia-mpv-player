pub use crate::grouping::{Grouping, Level};
use crate::model::Media;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum SortKey {
    Title,
    #[default]
    Artist,
    AlbumArtist,
    Disc,
    Genre,
    Format,
    Album,
    Track,
    Year,
    Duration,
    Path,
}
impl SortKey {
    pub const ALL: &'static [(Self, &'static str)] = &[
        (Self::Artist, "text.artist"),
        (Self::AlbumArtist, "text.album_artist"),
        (Self::Disc, "text.disc_number"),
        (Self::Genre, "text.genre"),
        (Self::Format, "text.format"),
        (Self::Album, "text.album"),
        (Self::Title, "text.title"),
        (Self::Track, "text.track_number"),
        (Self::Year, "text.year"),
        (Self::Duration, "text.duration"),
        (Self::Path, "text.path"),
    ];
    pub fn label(self) -> &'static str {
        crate::i18n::t(crate::i18n::t(Self::ALL.iter().find(|(key, _)| *key == self).unwrap().1))
    }
}
pub fn sorted(mut items: Vec<Media>, key: SortKey, descending: bool) -> Vec<Media> {
    items.sort_by(|a, b| compare(a, b, key, descending));
    items
}
pub fn compare(a: &Media, b: &Media, key: SortKey, descending: bool) -> std::cmp::Ordering {
    let order = match key {
        SortKey::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        SortKey::Artist => a
            .primary_artist()
            .to_lowercase()
            .cmp(&b.primary_artist().to_lowercase())
            .then(a.album.cmp(&b.album))
            .then(a.disc.cmp(&b.disc))
            .then(a.track.cmp(&b.track)),
        SortKey::Album => a
            .album
            .to_lowercase()
            .cmp(&b.album.to_lowercase())
            .then(a.disc.cmp(&b.disc))
            .then(a.track.cmp(&b.track)),
        SortKey::AlbumArtist => Level::AlbumArtist
            .value(a)
            .to_lowercase()
            .cmp(&Level::AlbumArtist.value(b).to_lowercase())
            .then(a.album.cmp(&b.album))
            .then(a.disc.cmp(&b.disc))
            .then(a.track.cmp(&b.track)),
        SortKey::Disc => a.disc.cmp(&b.disc).then(a.track.cmp(&b.track)),
        SortKey::Genre => a.genre.to_lowercase().cmp(&b.genre.to_lowercase()),
        SortKey::Format => {
            crate::library::extension(&a.path).cmp(&crate::library::extension(&b.path))
        }
        SortKey::Track => a.disc.cmp(&b.disc).then(a.track.cmp(&b.track)),
        SortKey::Year => a.year.cmp(&b.year),
        SortKey::Duration => a.duration.total_cmp(&b.duration),
        SortKey::Path => a.path.cmp(&b.path),
    }
        .then(a.title.cmp(&b.title))
        .then(a.path.cmp(&b.path));
    if descending {
        order.reverse()
    } else {
        order
    }
}
pub fn row_order(len: usize, selected: &[usize], before: usize) -> (Vec<usize>, Vec<usize>) {
    let selected: std::collections::BTreeSet<_> =
        selected.iter().copied().filter(|&i| i < len).collect();
    let mut order: Vec<_> = (0..len).filter(|i| !selected.contains(i)).collect();
    let target = before.min(len) - selected.iter().filter(|&&i| i < before.min(len)).count();
    order.splice(target..target, selected.iter().copied());
    let selection = (target..target + selected.len()).collect();
    (order, selection)
}
pub fn move_rows(items: &mut Vec<Media>, selected: &[usize], before: usize) -> Vec<usize> {
    let (order, selection) = row_order(items.len(), selected, before);
    *items = order.into_iter().map(|i| items[i].clone()).collect();
    selection
}
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Queue,
    Playlist(String),
}
#[derive(Clone)]
pub struct Drag {
    pub items: Vec<Media>,
    pub source: Option<Target>,
    pub indices: Vec<usize>,
}
