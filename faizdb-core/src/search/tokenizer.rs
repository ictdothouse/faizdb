//! Full-Text Search Tokenizer and Text Normalizer.

use std::collections::HashSet;
use std::sync::LazyLock;

/// Standard Stop Words for English and Malay
static STOP_WORDS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    let words = [
        // English
        "a",
        "about",
        "above",
        "after",
        "again",
        "against",
        "all",
        "am",
        "an",
        "and",
        "any",
        "are",
        "as",
        "at",
        "be",
        "because",
        "been",
        "before",
        "being",
        "below",
        "between",
        "both",
        "but",
        "by",
        "could",
        "did",
        "do",
        "does",
        "doing",
        "down",
        "during",
        "each",
        "few",
        "for",
        "from",
        "further",
        "had",
        "has",
        "have",
        "having",
        "he",
        "her",
        "here",
        "hers",
        "herself",
        "him",
        "himself",
        "his",
        "how",
        "i",
        "if",
        "in",
        "into",
        "is",
        "it",
        "its",
        "itself",
        "just",
        "me",
        "more",
        "most",
        "my",
        "myself",
        "no",
        "nor",
        "not",
        "now",
        "of",
        "off",
        "on",
        "once",
        "only",
        "or",
        "other",
        "our",
        "ours",
        "ourselves",
        "out",
        "over",
        "own",
        "same",
        "she",
        "should",
        "so",
        "some",
        "such",
        "than",
        "that",
        "the",
        "their",
        "theirs",
        "them",
        "themselves",
        "then",
        "there",
        "these",
        "they",
        "this",
        "those",
        "through",
        "to",
        "too",
        "under",
        "until",
        "up",
        "very",
        "was",
        "we",
        "were",
        "what",
        "when",
        "where",
        "which",
        "while",
        "who",
        "whom",
        "why",
        "with",
        "would",
        "you",
        "your",
        "yours",
        "yourself",
        "yourselves",
        // Malay
        "ada",
        "adalah",
        "akan",
        "antara",
        "atau",
        "bagi",
        "bahkan",
        "bahawa",
        "banyak",
        "bila",
        "boleh",
        "dalam",
        "dan",
        "dapat",
        "dari",
        "daripada",
        "dengan",
        "di",
        "dia",
        "ia",
        "ini",
        "itu",
        "jika",
        "juga",
        "kami",
        "kamu",
        "ke",
        "kerana",
        "kita",
        "lagi",
        "lalu",
        "mana",
        "mereka",
        "oleh",
        "pada",
        "para",
        "saja",
        "sama",
        "sangat",
        "saya",
        "sebagai",
        "sedang",
        "seperti",
        "sudah",
        "supaya",
        "tak",
        "telah",
        "tentang",
        "tidak",
        "untuk",
        "yang",
    ];
    words.into_iter().collect()
});

/// Tokenize and normalize text into clean terms
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .map(|token| token.trim().to_lowercase())
        .filter(|token| token.len() > 1 && !STOP_WORDS.contains(token.as_str()))
        .collect()
}

/// Compute Levenshtein Edit Distance for fuzzy typo-tolerance
pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let v1: Vec<char> = s1.chars().collect();
    let v2: Vec<char> = s2.chars().collect();
    let len1 = v1.len();
    let len2 = v2.len();

    let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

    for (i, row) in matrix.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in matrix[0].iter_mut().enumerate() {
        *cell = j;
    }

    for i in 1..=len1 {
        for j in 1..=len2 {
            let cost = if v1[i - 1] == v2[j - 1] { 0 } else { 1 };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[len1][len2]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer() {
        let text = "FaizDB is the revolutionary AI-Native NoSQL database in Malaysia!";
        let tokens = tokenize(text);
        assert!(tokens.contains(&"faizdb".to_string()));
        assert!(tokens.contains(&"revolutionary".to_string()));
        assert!(tokens.contains(&"database".to_string()));
        assert!(!tokens.contains(&"is".to_string())); // Stop word filtered
        assert!(!tokens.contains(&"the".to_string())); // Stop word filtered
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein_distance("databse", "database"), 1);
        assert_eq!(levenshtein_distance("faiz", "faiz"), 0);
        assert_eq!(levenshtein_distance("nosql", "mysql"), 2);
    }
}
