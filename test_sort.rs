use std::cmp::Ordering;

fn collect_similar_resources(query: &str, candidates: &[&str]) -> Vec<String> {
    let query_lower = query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split(|c| c == '.' || c == '_').collect();
    let query_last_word = *query_words.last().unwrap_or(&"");

    let mut scored = candidates
        .iter()
        .filter(|&&candidate| candidate.to_lowercase() != query_lower)
        .filter_map(|&candidate| {
            let candidate_lower = candidate.to_lowercase();
            let is_substring = candidate_lower.contains(&query_lower);
            // mocked distance
            let distance = candidate_lower.len().abs_diff(query_lower.len()) + 5;

            let candidate_words: Vec<&str> =
                candidate_lower.split(|c| c == '.' || c == '_').collect();
            let has_core_word =
                !query_last_word.is_empty() && candidate_words.contains(&query_last_word);
            let shared_words_count = candidate_words
                .iter()
                .filter(|w| query_words.contains(w))
                .count();

            if !is_substring && !has_core_word && distance > 10 {
                return None;
            }

            let len_diff = candidate.len().abs_diff(query.len());
            Some((
                candidate.to_string(),
                is_substring,
                has_core_word,
                shared_words_count,
                len_diff,
                distance,
            ))
        })
        .collect::<Vec<_>>();

    scored.sort_by(|a, b| {
        (!a.1)
            .cmp(&!b.1)
            .then((!a.2).cmp(&!b.2))
            .then(b.3.cmp(&a.3)) // reversed for descending
            .then(a.4.cmp(&b.4))
            .then(a.5.cmp(&b.5))
            .then(a.0.cmp(&b.0))
    });

    scored.into_iter().map(|(c, _, _, _, _, _)| c).collect()
}

fn main() {
    let candidates = vec![
        "wm.binding.down",
        "wm.binding.left",
        "wm.binding.help",
        "wm.program.lock_sway",
        "wm.program.lock_i3",
        "regolith.lockscreen.wallpaper",
    ];
    let res = collect_similar_resources("wm.binding.lock", &candidates);
    for r in res {
        println!("{}", r);
    }
}
