//! Parser Fuzz & Malformed Query Resilience Test Suite
//!
//! Fuzzes the query parser and expression evaluator with hostile, malformed,
//! and extreme inputs to guarantee zero panics, zero stack overflows, and clean error handling.

use faizdb_query::parse_query;
use faizdb_query::tokenizer::ExprParser;
use std::panic::catch_unwind;

#[test]
fn test_deeply_nested_parentheses_resilience() {
    // Generate 500 levels of nested parentheses: (((((...x = 1...)))))
    let depth = 500;
    let mut input = String::with_capacity(depth * 2 + 10);
    for _ in 0..depth {
        input.push('(');
    }
    input.push_str("age = 25");
    for _ in 0..depth {
        input.push(')');
    }

    let result = catch_unwind(|| {
        let _ = ExprParser::parse_from_str(&input);
    });

    assert!(
        result.is_ok(),
        "Parser must not panic or stack overflow on 500 nested parentheses"
    );
}

#[test]
fn test_unbalanced_parentheses_fuzzing() {
    let inputs = [
        "(((age > 10)",
        "(age > 10)))",
        "(((((((()))))))",
        "((age = 1) AND (status = 'active')",
        ") AND (",
        "(((((((((((((((((((((((((((((((((((((((((age = 1)",
    ];

    for query in inputs {
        let result = catch_unwind(|| {
            let res = ExprParser::parse_from_str(query);
            // Must either parse or return a graceful Err, never panic
            let _ = res;
        });
        assert!(
            result.is_ok(),
            "Parser panicked on unbalanced query: {query}"
        );
    }
}

#[test]
fn test_malformed_sql_string_literals_and_injections() {
    let malformed_sqls = [
        "SELECT * FROM users WHERE name = 'unterminated string literal",
        "SELECT * FROM users WHERE id = 100 AND name = 'escaped \\' quote without closing",
        "SELECT * FROM users WHERE name = '''",
        "SELECT * FROM users WHERE \"unclosed_identifier = 1",
        "SELECT * FROM users WHERE ;;;;;;;;;;;;;;;;;;;",
        "SELECT * FROM users WHERE AND OR NOT IS LIKE BETWEEN",
        "SELECT * FROM users WHERE age BETWEEN AND",
        "SELECT * FROM users WHERE age IN ()",
        "SELECT * FROM users WHERE age IN (1, 2, 3,",
        "SELECT * FROM users WHERE age IN (,1, 2)",
        "SELECT * FROM users WHERE age IS NOT",
        "SELECT * FROM users WHERE age LIKE",
        "SELECT * FROM users WHERE 1 = = = = 1",
        "SELECT * FROM users WHERE ? ! @ # $ % ^ & *",
    ];

    for sql in malformed_sqls {
        let result = catch_unwind(|| {
            let _ = parse_query(sql);
        });
        assert!(
            result.is_ok(),
            "Query parser panicked on malformed SQL: {sql}"
        );
    }
}

#[test]
fn test_pseudo_random_query_fuzzing() {
    let seed_chars = [
        'S', 'E', 'L', 'C', 'T', '*', 'F', 'R', 'O', 'M', 'W', 'H', 'R', '\'', '"', '(', ')', '=',
        '<', '>', '!', '0', '1', '9', ';', '-', ' ', '\t', '\n', '\0', 'A', 'N', 'D', 'O', 'R',
        'N', 'T', 'I', 'S', 'L', 'K', 'E', 'B', 'U',
    ];

    let mut state: u64 = 0xfa12_db20_2609_1600;
    let mut next_rand = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (state >> 33) as usize
    };

    // Run 1,000 randomized fuzzed query strings
    for _ in 0..1000 {
        let len = (next_rand() % 60) + 1;
        let mut random_str = String::with_capacity(len);
        for _ in 0..len {
            let idx = next_rand() % seed_chars.len();
            random_str.push(seed_chars[idx]);
        }

        let result = catch_unwind(|| {
            let _ = parse_query(&random_str);
            let _ = ExprParser::parse_from_str(&random_str);
        });

        assert!(
            result.is_ok(),
            "Parser panicked on random fuzzed input: {:?}",
            random_str
        );
    }
}
