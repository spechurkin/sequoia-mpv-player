use chrono::NaiveDate;

pub fn filename(title: &str, date: NaiveDate, position: f64) -> String {
    let title = title
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    let title: String = title
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let title = title.trim_end_matches([' ', '.']);
    let title = if title.is_empty() { "video" } else { title };
    let seconds = if position.is_finite() && position > 0.0 {
        position.floor() as u64
    } else {
        0
    };
    format!(
        "Sequoia-{title}-{}-{:02}.{:02}.{:02}.png",
        date.format("%d.%m.%Y"),
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
    }

    #[test]
    fn uses_two_words_and_elapsed_video_time() {
        assert_eq!(
            filename("  Первые   два слова фильма", date(), 3723.9),
            "Sequoia-Первые два-23.10.2026-01.02.03.png"
        );
        assert_eq!(
            filename("Movie", date(), 90061.0),
            "Sequoia-Movie-23.10.2026-25.01.01.png"
        );
    }

    #[test]
    fn handles_invalid_filename_characters_and_missing_values() {
        assert_eq!(
            filename("Film: Part/One rest", date(), 0.0),
            "Sequoia-Film_ Part_One-23.10.2026-00.00.00.png"
        );
        for position in [-1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                filename(" ", date(), position),
                "Sequoia-video-23.10.2026-00.00.00.png"
            );
        }
    }
}
