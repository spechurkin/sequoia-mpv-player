use crate::{library, model::Media};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default)]
pub struct TagPatch {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub disc: Option<u32>,
    pub grouping: Option<String>,
    pub genre: Option<String>,
    pub year: Option<u32>,
    pub track: Option<u32>,
    pub artwork: Option<Vec<u8>>,
}
impl TagPatch {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.album_artist.is_none()
            && self.disc.is_none()
            && self.grouping.is_none()
            && self.artist.is_none()
            && self.album.is_none()
            && self.genre.is_none()
            && self.year.is_none()
            && self.track.is_none()
            && self.artwork.is_none()
    }
}

pub fn unique_tracks(items: Vec<Media>) -> Vec<Media> {
    let mut seen = HashSet::new();
    items
        .into_iter()
        .filter(|m| seen.insert(m.path.clone()))
        .collect()
}

#[derive(Default)]
pub struct BatchResult {
    pub saved: Vec<Media>,
    pub failed: Vec<(PathBuf, String)>,
}

pub fn write_batch(
    items: &[Media],
    patch: &TagPatch,
    number_from: Option<u32>,
    root: &Path,
    mut progress: impl FnMut(usize),
) -> BatchResult {
    let items = unique_tracks(items.to_vec());
    let mut result = BatchResult::default();
    if patch.is_empty() && number_from.is_none() {
        return result;
    }
    if let Some(start) = number_from {
        if start == 0
            || start
            .checked_add(items.len().saturating_sub(1) as u32)
            .is_none()
        {
            result.failed = items
                .iter()
                .map(|m| {
                    (
                        m.path.clone(),
                        crate::i18n::t("text.invalid_track_number_range").into(),
                    )
                })
                .collect();
            return result;
        }
    }
    for (i, item) in items.iter().enumerate() {
        let mut change = patch.clone();
        if let Some(start) = number_from {
            change.track = Some(start + i as u32);
        }
        match library::patch_tags(&item.path, &change, root) {
            Ok(media) => result.saved.push(media),
            Err(error) => result
                .failed
                .push((item.path.clone(), format!("{error:#}"))),
        }
        progress(i + 1);
    }
    result
}
