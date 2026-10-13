// SPDX-License-Identifier: AGPL-3.0-only

use muse_library::Track;

#[derive(Clone, Copy)]
pub enum Direction {
    Forward,
    Backward,
}

impl Direction {
    pub fn marker(self) -> char {
        match self {
            Self::Forward => '/',
            Self::Backward => '?',
        }
    }

    fn reversed(self) -> Self {
        match self {
            Self::Forward => Self::Backward,
            Self::Backward => Self::Forward,
        }
    }
}

/// Prepared row indices keep matching independent of viewport and rendering.
pub struct Search {
    pub query: String,
    pub direction: Direction,
    matches: Vec<usize>,
}

impl Search {
    pub fn new(query: &str, direction: Direction, tracks: &[Track]) -> Self {
        let mut search = Self {
            query: query.into(),
            direction,
            matches: Vec::new(),
        };
        search.refresh(tracks);
        search
    }

    pub fn refresh(&mut self, tracks: &[Track]) {
        let query = self.query.to_lowercase();
        self.matches = tracks
            .iter()
            .enumerate()
            .filter(|(_, track)| {
                !query.is_empty()
                    && [
                        Some(track.title.as_str()),
                        track.artist.as_deref(),
                        track.album.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|text| text.to_lowercase().contains(&query))
            })
            .map(|(index, _)| index)
            .collect();
    }

    pub fn contains(&self, row: usize) -> bool {
        self.matches.binary_search(&row).is_ok()
    }

    /// Preview from a fixed origin, including the song at that row.
    pub fn preview(&self, origin: usize) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        let index = match self.direction {
            Direction::Forward => {
                self.matches.partition_point(|row| *row < origin) % self.matches.len()
            }
            Direction::Backward => self
                .matches
                .partition_point(|row| *row <= origin)
                .checked_sub(1)
                .unwrap_or(self.matches.len() - 1),
        };
        Some(self.matches[index])
    }

    /// Repeat strictly beyond the current row, wrapping counts in constant time.
    pub fn next(&self, current: usize, opposite: bool, count: usize) -> Option<usize> {
        let length = self.matches.len();
        if length == 0 {
            return None;
        }
        let extra = (count.max(1) - 1) % length;
        let direction = if opposite {
            self.direction.reversed()
        } else {
            self.direction
        };
        let index = match direction {
            Direction::Forward => {
                let first = self.matches.partition_point(|row| *row <= current) % length;
                (first + extra) % length
            }
            Direction::Backward => {
                let first = self
                    .matches
                    .partition_point(|row| *row < current)
                    .checked_sub(1)
                    .unwrap_or(length - 1);
                (first + length - extra) % length
            }
        };
        Some(self.matches[index])
    }

    pub fn label(&self, selected: Option<usize>) -> String {
        let position = selected
            .and_then(|row| self.matches.binary_search(&row).ok())
            .map_or(0, |index| index + 1);
        format!(
            "{}{} | {position}/{} matches",
            self.direction.marker(),
            self.query,
            self.matches.len()
        )
    }
}
