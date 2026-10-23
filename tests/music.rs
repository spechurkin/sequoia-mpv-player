use sequoia::{
    model::{Media, Queue},
    music::{self, SortKey},
};
fn rows() -> Vec<Media> {
    (0..6)
        .map(|i| Media {
            path: format!("{i}.mp3").into(),
            title: format!("Track {i}"),
            artist: format!("Artist {}", 5 - i),
            album: "Album".into(),
            track: i + 1,
            duration: (i * 10) as f64,
            ..Default::default()
        })
        .collect()
}
#[test]
fn multi_row_drag_reorders_in_original_order() {
    let mut items = rows();
    let selection = music::move_rows(&mut items, &[3, 1], 6);
    assert_eq!(
        items.iter().map(|m| m.track).collect::<Vec<_>>(),
        [1, 3, 5, 6, 2, 4]
    );
    assert_eq!(selection, [4, 5]);
}
#[test]
fn drop_inside_selected_block_is_stable() {
    let mut items = rows();
    let original = items.clone();
    music::move_rows(&mut items, &[1, 2, 3], 2);
    assert_eq!(items, original);
}
#[test]
fn invalid_drag_indices_and_end_position_are_safe() {
    let mut items = rows();
    music::move_rows(&mut items, &[99, 1, 1], 999);
    assert_eq!(items.last().unwrap().track, 2);
    assert_eq!(items.len(), 6);
}
#[test]
fn insertion_preserves_exact_playing_duplicate_and_previous_history() {
    let mut items = rows();
    items[3] = items[0].clone();
    let mut q = Queue::default();
    q.replace(items, 2);
    q.advance(false);
    assert_eq!(q.index, Some(3));
    q.insert(1, vec![rows()[5].clone()]);
    assert_eq!(q.index, Some(4));
    assert_eq!(q.previous().unwrap().track, 3);
}
#[test]
fn moving_rows_keeps_exact_duplicate_current_index() {
    let mut items = rows();
    items[3] = items[0].clone();
    let mut q = Queue::default();
    q.replace(items, 3);
    q.move_rows(&[3], 0);
    assert_eq!(q.index, Some(0));
    q.move_rows(&[2, 4], 0);
    assert_eq!(q.index, Some(2));
}
#[test]
fn shuffle_history_survives_dragging() {
    let mut q = Queue::default();
    q.replace(rows(), 0);
    q.shuffle = true;
    q.advance(false);
    let current = q.current().unwrap().path.clone();
    q.move_rows(&[0, 1], 6);
    assert_eq!(q.current().unwrap().path, current);
    assert_eq!(q.previous().unwrap().track, 1);
}
#[test]
fn all_sort_fields_and_descending_work() {
    for &(key, _) in SortKey::ALL {
        let ascending = music::sorted(rows(), key, false);
        let descending = music::sorted(rows(), key, true);
        assert_eq!(
            ascending.iter().rev().map(|m| &m.path).collect::<Vec<_>>(),
            descending.iter().map(|m| &m.path).collect::<Vec<_>>()
        );
    }
}
#[test]
fn bad_sort_permutation_does_not_mutate_queue() {
    let mut q = Queue::default();
    q.replace(rows(), 3);
    let previous = q.items.clone();
    assert!(!q.reorder_to(&[0, 0, 1, 2, 3, 4]));
    assert_eq!(q.items, previous);
    assert_eq!(q.index, Some(3));
}

#[test]
fn collaborations_share_primary_artist_without_changing_metadata() {
    for credit in [
        "Артист; Гость",
        "Артист;Гость;Ещё гость",
        "Артист/Гость",
        "Артист / Гость; Ещё гость",
        "Артист; Гость/Ещё гость",
        "Big Russian Boss/Young P&H",
        "  Артист ; Гость, Ещё гость  ",
    ] {
        let item = Media {
            artist: credit.into(),
            ..Default::default()
        };
        let expected = if credit.starts_with("Big Russian Boss") {
            "Big Russian Boss"
        } else {
            "Артист"
        };
        assert_eq!(item.primary_artist(), expected, "{credit}");
        assert_eq!(item.artist, credit);
        assert_eq!(item.subtitle(), credit);
    }
    for credit in [
        "Earth, Wind & Fire",
        "Simon & Garfunkel",
        "Певица",
        "Jay-Z",
        "",
    ] {
        let item = Media {
            artist: credit.into(),
            ..Default::default()
        };
        assert_eq!(item.primary_artist(), credit);
    }
}

#[test]
fn featured_artists_do_not_affect_artist_sort_order() {
    let items: Vec<_> = [("Артист; А-гость", 3), ("Артист/Я-гость", 1), ("Артист", 2)]
        .into_iter()
        .map(|(artist, track)| Media {
            artist: artist.into(),
            track,
            album: "Альбом".into(),
            ..Default::default()
        })
        .collect();
    for descending in [false, true] {
        let sorted = music::sorted(items.clone(), SortKey::Artist, descending);
        let expected = if descending {
            vec![3, 2, 1]
        } else {
            vec![1, 2, 3]
        };
        assert_eq!(sorted.iter().map(|m| m.track).collect::<Vec<_>>(), expected);
        assert_eq!(sorted[1].artist, "Артист");
    }
}
