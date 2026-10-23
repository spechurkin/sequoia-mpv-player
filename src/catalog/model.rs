use rand::seq::IndexedRandom;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Media {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub disc: u32,
    pub grouping: String,
    pub duration: f64,
    pub year: u32,
    pub track: u32,
    pub genre: String,
    pub audio: bool,
}
impl Media {
    pub fn primary_artist(&self) -> &str {
        self.artist
            .split([';', '/'])
            .next()
            .unwrap_or_default()
            .trim()
    }

    pub fn subtitle(&self) -> String {
        [self.artist.as_str(), self.album.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" В· ")
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(default = "crate::i18n::default_language")]
    pub language: String,
    pub mpv_path: PathBuf,
    pub music_root: PathBuf,
    pub screenshot_folder: PathBuf,
    pub seek_seconds: f64,
    pub volume: f64,
    pub recursive: bool,
    pub auto_lookup: bool,
    pub close_to_tray: bool,
    pub music_sort: crate::music::SortKey,
    pub music_grouping: crate::music::Grouping,
    pub music_descending: bool,
    pub music_browser_width: f32,
    pub mpv_config: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            language: crate::i18n::default_language(),
            mpv_path: PathBuf::new(),
            music_root: crate::library::music_folder(),
            screenshot_folder: crate::library::pictures_folder().join("Sequoia"),
            seek_seconds: 5.,
            volume: 70.,
            recursive: true,
            auto_lookup: false,
            close_to_tray: true,
            music_sort: Default::default(),
            music_grouping: Default::default(),
            music_descending: false,
            music_browser_width: 300.,
            mpv_config: String::new(),
        }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub settings: Settings,
    pub library: Vec<Media>,
    pub playlists: std::collections::BTreeMap<String, Vec<Media>>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Repeat {
    #[default]
    None,
    One,
    All,
}
#[derive(Default)]
pub struct Queue {
    pub items: Vec<Media>,
    pub index: Option<usize>,
    pub repeat: Repeat,
    pub shuffle: bool,
    history: Vec<usize>,
    visited: std::collections::HashSet<usize>,
}
impl Queue {
    pub fn insert(&mut self, before: usize, items: Vec<Media>) {
        let at = before.min(self.items.len());
        if let Some(current) = self.index {
            if at <= current {
                self.index = Some(current + items.len());
            }
        }
        let count = items.len();
        self.items.splice(at..at, items);
        for old in &mut self.history {
            if *old >= at {
                *old += count;
            }
        }
        self.visited = self
            .visited
            .iter()
            .map(|&i| if i >= at { i + count } else { i })
            .collect();
    }
    pub fn move_rows(&mut self, selected: &[usize], before: usize) -> Vec<usize> {
        let (order, selection) = crate::music::row_order(self.items.len(), selected, before);
        self.reorder_to(&order);
        selection
    }
    pub fn reorder_to(&mut self, order: &[usize]) -> bool {
        if order.len() != self.items.len()
            || order.iter().any(|&i| i >= self.items.len())
            || order.iter().collect::<std::collections::HashSet<_>>().len() != order.len()
        {
            return false;
        }
        let mut inverse = vec![0; order.len()];
        for (new, &old) in order.iter().enumerate() {
            inverse[old] = new;
        }
        self.index = self.index.map(|old| inverse[old]);
        self.history = self.history.iter().map(|&old| inverse[old]).collect();
        self.visited = self.visited.iter().map(|&old| inverse[old]).collect();
        self.items = order.iter().map(|&i| self.items[i].clone()).collect();
        true
    }
    pub fn current(&self) -> Option<&Media> {
        self.index.and_then(|i| self.items.get(i))
    }
    pub fn replace(&mut self, items: Vec<Media>, index: usize) {
        self.items = items;
        self.history.clear();
        self.visited.clear();
        self.index = None;
        self.select(index);
    }
    pub fn select(&mut self, index: usize) {
        self.index = if self.items.is_empty() {
            None
        } else {
            Some(index.min(self.items.len() - 1))
        };
        if let Some(i) = self.index {
            self.visited.insert(i);
        }
    }
    pub fn advance(&mut self, automatic: bool) -> Option<Media> {
        let current = self.index?;
        if automatic && self.repeat == Repeat::One {
            return self.current().cloned();
        }
        let next = if self.shuffle {
            let mut choices: Vec<_> = (0..self.items.len())
                .filter(|i| !self.visited.contains(i))
                .collect();
            if choices.is_empty() {
                if self.repeat != Repeat::All {
                    return None;
                }
                self.visited.clear();
                self.visited.insert(current);
                choices = (0..self.items.len()).filter(|i| *i != current).collect();
            }
            choices.choose(&mut rand::rng()).copied().unwrap_or(current)
        } else if current + 1 < self.items.len() {
            current + 1
        } else if self.repeat == Repeat::All {
            0
        } else {
            return None;
        };
        self.history.push(current);
        self.select(next);
        self.current().cloned()
    }
    pub fn previous(&mut self) -> Option<Media> {
        let i = self.history.pop().unwrap_or(self.index?.saturating_sub(1));
        self.select(i);
        self.current().cloned()
    }
    pub fn move_item(&mut self, from: usize, to: usize) {
        if from >= self.items.len() || to >= self.items.len() {
            return;
        }
        let current = self.current().map(|m| m.path.clone());
        let item = self.items.remove(from);
        self.items.insert(to, item);
        self.index = current.and_then(|p| self.items.iter().position(|m| m.path == p));
        self.reset_navigation();
    }
    pub fn remove(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }
        let current = self.current().map(|m| m.path.clone());
        self.items.remove(index);
        self.index = current.and_then(|p| self.items.iter().position(|m| m.path == p));
        self.reset_navigation();
    }
    fn reset_navigation(&mut self) {
        self.history.clear();
        self.visited.clear();
        if let Some(i) = self.index {
            self.visited.insert(i);
        }
    }
}
pub fn time(seconds: f64) -> String {
    let s = seconds.max(0.) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}
