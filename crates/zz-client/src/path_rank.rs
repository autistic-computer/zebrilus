use std::{cmp::Ordering, ops::Range};

use crate::pick::{self, Candidate, Kind, Prior};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ranked {
    pub index: usize,
    pub score: i64,
    pub matches: Vec<Range<usize>>,
}

#[must_use]
pub fn split(label: &str) -> (&str, &str) {
    let cut = label
        .trim_end_matches('/')
        .rfind('/')
        .map_or(0, |at| at + 1);
    (&label[..cut], &label[cut..])
}

pub fn ranked(query: &str, labels: &[&str], prior: impl Fn(usize) -> Prior) -> Vec<Ranked> {
    let candidates = labels
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let (path, name) = split(label);
            Candidate {
                kind: Kind::File,
                label: name,
                path,
                prior: Prior {
                    depth: u32::try_from(depth(label)).unwrap_or(u32::MAX),
                    ..prior(index)
                },
            }
        })
        .collect::<Vec<_>>();
    pick::rank(query, &candidates)
        .into_iter()
        .map(|hit| {
            let cut = candidates[hit.index].path.len();
            let mut matches = hit.path;
            for range in hit.label {
                let range = range.start + cut..range.end + cut;
                match matches.last_mut() {
                    Some(last) if last.end == range.start => last.end = range.end,
                    _ => matches.push(range),
                }
            }
            Ranked {
                index: hit.index,
                score: hit.score,
                matches,
            }
        })
        .collect()
}

pub fn rank(
    query: &str,
    labels: &[&str],
    prior: impl Fn(usize) -> u32,
    limit: usize,
) -> Vec<usize> {
    let priors = (0..labels.len()).map(&prior).collect::<Vec<_>>();
    let mut found = if query.is_empty() {
        (0..labels.len())
            .map(|index| (index, 0))
            .collect::<Vec<_>>()
    } else {
        ranked(query, labels, |_| Prior::default())
            .into_iter()
            .map(|hit| (hit.index, hit.score))
            .collect()
    };
    found.sort_by(|(left, left_score), (right, right_score)| {
        right_score
            .cmp(left_score)
            .then(priors[*right].cmp(&priors[*left]))
            .then_with(|| labels[*left].cmp(labels[*right]))
    });
    found.truncate(limit);
    found.into_iter().map(|(index, _)| index).collect()
}

pub fn depth(label: &str) -> usize {
    label.trim_end_matches('/').matches('/').count()
}

pub fn browse_order(left: (&str, bool), right: (&str, bool)) -> Ordering {
    right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highlights(query: &str, kept: &[&str]) -> Vec<Vec<Range<usize>>> {
        kept.iter()
            .map(|label| {
                ranked(query, &[label], |_| Prior::default())
                    .pop()
                    .map(|hit| hit.matches)
                    .unwrap_or_default()
            })
            .collect()
    }

    fn ordered<'a>(query: &str, labels: &[&'a str], prior: impl Fn(usize) -> u32) -> Vec<&'a str> {
        rank(query, labels, prior, usize::MAX)
            .into_iter()
            .map(|index| labels[index])
            .collect()
    }

    #[test]
    fn empty_query_orders_by_prior_then_label() {
        let labels = ["b", "a", "c"];
        assert_eq!(
            ordered("", &labels, |index| u32::from(index == 2)),
            ["c", "a", "b"]
        );
        assert_eq!(rank("", &labels, |_| 0, 2), [1, 0]);
    }

    #[test]
    fn prior_breaks_score_ties_and_label_breaks_prior_ties() {
        let labels = ["x/foo/b", "x/foo/a", "x/foo/c"];
        assert_eq!(
            ordered("foo", &labels, |index| u32::from(index == 2)),
            ["x/foo/c", "x/foo/a", "x/foo/b"]
        );
    }

    #[test]
    fn score_outranks_prior() {
        let labels = ["src/m_a_i_n.rs", "src/main.rs"];
        let order = ordered("main", &labels, |index| if index == 0 { 1000 } else { 0 });
        assert_eq!(order, ["src/main.rs", "src/m_a_i_n.rs"]);
    }

    #[test]
    fn misses_are_dropped() {
        let labels = ["alpha", "beta"];
        assert_eq!(ordered("zzz", &labels, |_| 0), Vec::<&str>::new());
        assert_eq!(ordered("alx", &labels, |_| 0), Vec::<&str>::new());
    }

    #[test]
    fn basename_matches_win_over_directory_matches_on_equal_scores() {
        let labels = ["x/foo/a", "x/a/foo"];
        let query = "foo";
        let order = ordered(query, &labels, |_| 0);
        assert_eq!(order, ["x/a/foo", "x/foo/a"]);
    }

    #[test]
    fn changed_beats_shallow_and_depth_counts_folders() {
        let labels = ["q", "deep/a/b/q"];
        let order = ranked("q", &labels, |index| Prior {
            changed: index == 1,
            ..Prior::default()
        })
        .into_iter()
        .map(|hit| labels[hit.index])
        .collect::<Vec<_>>();
        assert_eq!(order, ["deep/a/b/q", "q"]);
        assert_eq!(depth("a/b/"), 1);
    }

    #[test]
    fn highlights_are_byte_ranges_on_the_kept_rows() {
        let kept = ["src/main.rs", "héllo/wörld.txt", "nothing"];
        let rows = highlights("main", &kept);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], vec![4..8]);
        assert!(rows[2].is_empty());
        let rows = highlights("wö", &kept);
        let label = kept[1];
        for range in &rows[1] {
            assert!(label.is_char_boundary(range.start) && label.is_char_boundary(range.end));
        }
        let matched = rows[1]
            .iter()
            .map(|range| &label[range.clone()])
            .collect::<String>();
        assert_eq!(matched, "wö");
    }

    #[test]
    fn multibyte_query_characters_cover_whole_characters() {
        let kept = ["dir/wörld"];
        let rows = highlights("wö", &kept);
        assert_eq!(rows[0], vec![4..7]);
        let label = kept[0];
        let matched = rows[0]
            .iter()
            .map(|range| &label[range.clone()])
            .collect::<String>();
        assert_eq!(matched, "wö");
    }

    #[test]
    fn browse_order_puts_dirs_first_then_names() {
        let mut entries = [("b", false), ("z", true), ("a", false), ("c", true)];
        entries.sort_by(|left, right| browse_order(*left, *right));
        assert_eq!(
            entries,
            [("c", true), ("z", true), ("a", false), ("b", false)]
        );
    }
}
