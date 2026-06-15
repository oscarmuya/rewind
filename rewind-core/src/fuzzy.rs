use crate::entry::Entry;
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};

/// Runs a fuzzy search over history using nucleo as the fuzzy ranking layer on top.
/// Returns entries ordered by fuzzy match score descending.
pub fn search_fuzzy<'a>(candidates: &'a [Entry], term: &str, limit: usize) -> Vec<&'a Entry> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let indices = search_fuzzy_indices(&mut matcher, candidates, term, limit);

    indices
        .into_iter()
        .map(|index| &candidates[index])
        .collect()
}
/// Scores history entries against `term` using nucleo fuzzy matching and
/// returns the indexes of the best matches in descending score order.
///
/// The returned indexes refer to positions in `candidates`, allowing callers
/// to keep UI state as indexes without cloning or moving `Entry` values.
pub fn search_fuzzy_indices(
    matcher: &mut Matcher,
    candidates: &[Entry],
    term: &str,
    limit: usize,
) -> Vec<usize> {
    let pattern = Pattern::new(
        term,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
    );

    let mut buf = Vec::new();

    let mut scored: Vec<(u32, usize)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            buf.clear();

            pattern
                .score(Utf32Str::new(&entry.command, &mut buf), matcher)
                .map(|score| (score, index))
        })
        .collect();

    scored.sort_unstable_by(|(score_a, index_a), (score_b, index_b)| {
        score_b
            .cmp(score_a)
            // Deterministic tie-breaker: keep earlier history order first.
            .then_with(|| index_a.cmp(index_b))
    });

    scored
        .into_iter()
        .take(limit)
        .map(|(_, index)| index)
        .collect()
}
