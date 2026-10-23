use crate::model::Media;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Level {
    #[default]
    None,
    Artist,
    AlbumArtist,
    Album,
    AlbumDisc,
    Disc,
    Format,
    Genre,
    Year,
    YearAlbum,
    YearAlbumDisc,
}
impl Level {
    pub const ALL: [Self; 11] = [
        Self::None,
        Self::Artist,
        Self::AlbumArtist,
        Self::Album,
        Self::AlbumDisc,
        Self::Disc,
        Self::Format,
        Self::Genre,
        Self::Year,
        Self::YearAlbum,
        Self::YearAlbumDisc,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => crate::i18n::t("text.none"),
            Self::Artist => crate::i18n::t("text.artist"),
            Self::AlbumArtist => crate::i18n::t("text.album_artist"),
            Self::Album => crate::i18n::t("text.album"),
            Self::AlbumDisc => crate::i18n::t("text.album_disc"),
            Self::Disc => crate::i18n::t("text.disc"),
            Self::Format => crate::i18n::t("text.format"),
            Self::Genre => crate::i18n::t("text.genre"),
            Self::Year => crate::i18n::t("text.year"),
            Self::YearAlbum => crate::i18n::t("text.year_album"),
            Self::YearAlbumDisc => crate::i18n::t("text.year_album_disc"),
        }
    }
    pub fn value(self, m: &Media) -> String {
        let album = if m.album.trim().is_empty() {
            crate::i18n::t("text.unknown_album")
        } else {
            &m.album
        };
        let year = if m.year == 0 {
            crate::i18n::t("text.unknown_year").into()
        } else {
            format!("{:04}", m.year)
        };
        let disc = if m.disc == 0 {
            crate::i18n::t("text.unknown_disc_number").into()
        } else {
            crate::i18n::format("text.disc_02", &[("arg0", format!("{:02}", m.disc))])
        };
        match self {
            Self::None => String::new(),
            Self::Artist => if m.primary_artist().is_empty() {
                crate::i18n::t("text.unknown_artist")
            } else {
                m.primary_artist()
            }
                .into(),
            Self::AlbumArtist => {
                let artist = m
                    .album_artist
                    .split([';', '/'])
                    .next()
                    .unwrap_or_default()
                    .trim();
                if artist.is_empty() {
                    Self::Artist.value(m)
                } else {
                    artist.into()
                }
            }
            Self::Album => album.into(),
            Self::AlbumDisc => format!("{album} · {disc}"),
            Self::Disc => disc,
            Self::Format => crate::library::extension(&m.path).to_uppercase(),
            Self::Genre => if m.genre.trim().is_empty() {
                crate::i18n::t("text.unknown_genre")
            } else {
                &m.genre
            }
                .into(),
            Self::Year => year,
            Self::YearAlbum => format!("{year} · {album}"),
            Self::YearAlbumDisc => format!("{year} · {album} · {disc}"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Grouping {
    pub levels: [Level; 3],
    pub separate_albums: bool,
}
impl Default for Grouping {
    fn default() -> Self {
        Self {
            levels: [Level::AlbumArtist, Level::AlbumDisc, Level::None],
            separate_albums: false,
        }
    }
}
impl Grouping {
    pub fn presets() -> Vec<Self> {
        use Level::*;
        let mut result = Vec::new();
        for artist in [AlbumArtist, Artist] {
            for album in [Album, AlbumDisc, YearAlbum, YearAlbumDisc] {
                result.push(Self {
                    levels: [artist, album, None],
                    separate_albums: false,
                });
            }
            result.push(Self {
                levels: [Genre, artist, Album],
                separate_albums: false,
            });
        }
        for levels in [
            [AlbumArtist, None, None],
            [Artist, None, None],
            [Album, None, None],
            [Genre, Album, None],
        ] {
            result.push(Self {
                levels,
                separate_albums: false,
            });
        }
        result
    }
    pub fn label(&self) -> String {
        self.levels
            .iter()
            .take_while(|&&l| l != Level::None)
            .map(|l| l.label())
            .collect::<Vec<_>>()
            .join(" / ")
    }
    pub fn key(&self, m: &Media, depth: usize) -> (String, String) {
        let level = self.levels[depth];
        let value = level.value(m);
        let extra = if self.separate_albums
            && matches!(
                level,
                Level::Album | Level::AlbumDisc | Level::YearAlbum | Level::YearAlbumDisc
            ) {
            m.grouping.clone()
        } else {
            String::new()
        };
        (value, extra)
    }
}
