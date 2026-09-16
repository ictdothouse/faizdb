//! SQL Tokenizer / Lexer for FaizDB Query Engine.
//!
//! Converts a raw SQL string into a stream of typed tokens for the parser.
//! Handles:
//! - SQL keywords (SELECT, INSERT, UPDATE, DELETE, WHERE, FROM, etc.)
//! - Identifiers (column names, table names) — unquoted, double-quoted, backtick-quoted
//! - String literals (single-quoted with escape support)
//! - Numeric literals (integer, float, scientific notation)
//! - Operators (=, !=, <>, <, >, <=, >=)
//! - Punctuation (, ; . ( ) *)
//! - SQL comments (-- line, /* */ block)

/// Position in source for error reporting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    pub offset: usize,
}

/// Token types produced by the lexer
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    Keyword(SqlKeyword),
    // Identifiers (column, table names)
    Identifier(String),
    // Literal values
    StringLiteral(String),
    IntegerLiteral(i64),
    FloatLiteral(f64),
    BooleanLiteral(bool),
    NullLiteral,
    // Operators
    Eq,         // =
    Neq,        // != or <>
    Lt,         // <
    Gt,         // >
    Lte,        // <=
    Gte,        // >=
    Plus,       // +
    Minus,      // -
    Star,       // *
    Slash,      // /
    // Punctuation
    Comma,      // ,
    Semicolon,  // ;
    Dot,        // .
    LParen,     // (
    RParen,     // )
    // End of input
    Eof,
}

/// SQL Keywords recognized by FaizDB
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SqlKeyword {
    Select,
    Insert,
    Into,
    Update,
    Delete,
    From,
    Where,
    And,
    Or,
    Not,
    In,
    Between,
    Like,
    Is,
    Null,
    Set,
    Values,
    Create,
    Drop,
    Alter,
    Table,
    Collection,
    Index,
    On,
    Unique,
    If,
    Exists,
    Limit,
    Offset,
    Skip_,
    Order,
    By,
    Asc,
    Desc,
    As,
    Count,
    Join,
    Inner,
    Left,
    Outer,
    Explain,
    Analyze,
    Verbose,
    Begin,
    Commit,
    Rollback,
    Transaction,
    Add,
    Column,
    Rename,
    To,
    Distinct,
    Show,
    Describe,
}

/// A single token with its kind, source text, and position
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn is_keyword(&self, kw: SqlKeyword) -> bool {
        self.kind == TokenKind::Keyword(kw)
    }

    pub fn is_eof(&self) -> bool {
        self.kind == TokenKind::Eof
    }

    pub fn as_identifier(&self) -> Option<&str> {
        match &self.kind {
            TokenKind::Identifier(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_string_literal(&self) -> Option<&str> {
        match &self.kind {
            TokenKind::StringLiteral(s) => Some(s),
            _ => None,
        }
    }
}

/// SQL Tokenizer
pub struct Tokenizer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Tokenizer {
    /// Create a new tokenizer for the given SQL input
    pub fn new(input: &str) -> Self {
        Self {
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    /// Tokenize the entire input into a vector of tokens
    pub fn tokenize(input: &str) -> Result<Vec<Token>, String> {
        let mut tokenizer = Self::new(input);
        let mut tokens = Vec::new();

        loop {
            let tok = tokenizer.next_token()?;
            let is_eof = tok.is_eof();
            tokens.push(tok);
            if is_eof {
                break;
            }
        }

        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_ahead(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn span(&self) -> Span {
        Span {
            line: self.line,
            col: self.col,
            offset: self.pos,
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(ch) = self.advance() {
            if ch == '\n' {
                break;
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), String> {
        let start_span = self.span();
        // Already consumed /*
        loop {
            match self.advance() {
                Some('*') => {
                    if self.peek() == Some('/') {
                        self.advance();
                        return Ok(());
                    }
                }
                None => {
                    return Err(format!(
                        "Unterminated block comment starting at line {} col {}",
                        start_span.line, start_span.col
                    ));
                }
                _ => {}
            }
        }
    }

    /// Get the next token
    pub fn next_token(&mut self) -> Result<Token, String> {
        self.skip_whitespace();

        let span = self.span();

        let ch = match self.peek() {
            Some(c) => c,
            None => {
                return Ok(Token {
                    kind: TokenKind::Eof,
                    span,
                });
            }
        };

        // Line comments
        if ch == '-' && self.peek_ahead(1) == Some('-') {
            self.advance();
            self.advance();
            self.skip_line_comment();
            return self.next_token();
        }

        // Block comments
        if ch == '/' && self.peek_ahead(1) == Some('*') {
            self.advance();
            self.advance();
            self.skip_block_comment()?;
            return self.next_token();
        }

        // Single-character tokens
        match ch {
            ',' => { self.advance(); return Ok(Token { kind: TokenKind::Comma, span }); }
            ';' => { self.advance(); return Ok(Token { kind: TokenKind::Semicolon, span }); }
            '.' => { self.advance(); return Ok(Token { kind: TokenKind::Dot, span }); }
            '(' => { self.advance(); return Ok(Token { kind: TokenKind::LParen, span }); }
            ')' => { self.advance(); return Ok(Token { kind: TokenKind::RParen, span }); }
            '*' => { self.advance(); return Ok(Token { kind: TokenKind::Star, span }); }
            '+' => { self.advance(); return Ok(Token { kind: TokenKind::Plus, span }); }
            '/' => { self.advance(); return Ok(Token { kind: TokenKind::Slash, span }); }
            _ => {}
        }

        // Multi-character operators
        if ch == '!' && self.peek_ahead(1) == Some('=') {
            self.advance(); self.advance();
            return Ok(Token { kind: TokenKind::Neq, span });
        }
        if ch == '<' && self.peek_ahead(1) == Some('>') {
            self.advance(); self.advance();
            return Ok(Token { kind: TokenKind::Neq, span });
        }
        if ch == '<' && self.peek_ahead(1) == Some('=') {
            self.advance(); self.advance();
            return Ok(Token { kind: TokenKind::Lte, span });
        }
        if ch == '>' && self.peek_ahead(1) == Some('=') {
            self.advance(); self.advance();
            return Ok(Token { kind: TokenKind::Gte, span });
        }
        if ch == '<' {
            self.advance();
            return Ok(Token { kind: TokenKind::Lt, span });
        }
        if ch == '>' {
            self.advance();
            return Ok(Token { kind: TokenKind::Gt, span });
        }
        if ch == '=' {
            self.advance();
            return Ok(Token { kind: TokenKind::Eq, span });
        }

        // Minus (could be negative number or operator)
        if ch == '-' {
            self.advance();
            // Check if this is a negative number
            if let Some(next) = self.peek() {
                if next.is_ascii_digit() {
                    return self.read_number(span, true);
                }
            }
            return Ok(Token { kind: TokenKind::Minus, span });
        }

        // String literals
        if ch == '\'' {
            return self.read_string_literal(span);
        }

        // Double-quoted identifiers
        if ch == '"' {
            return self.read_quoted_identifier(span, '"');
        }

        // Backtick-quoted identifiers
        if ch == '`' {
            return self.read_quoted_identifier(span, '`');
        }

        // Numbers
        if ch.is_ascii_digit() {
            return self.read_number(span, false);
        }

        // Keywords and identifiers
        if ch.is_alphabetic() || ch == '_' {
            return self.read_word(span);
        }

        Err(format!(
            "Unexpected character '{}' at line {} col {}",
            ch, span.line, span.col
        ))
    }

    fn read_string_literal(&mut self, span: Span) -> Result<Token, String> {
        self.advance(); // consume opening '
        let mut s = String::new();

        loop {
            match self.advance() {
                Some('\'') => {
                    // Check for escaped quote ''
                    if self.peek() == Some('\'') {
                        s.push('\'');
                        self.advance();
                    } else {
                        break;
                    }
                }
                Some('\\') => {
                    // C-style escapes
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('\\') => s.push('\\'),
                        Some('\'') => s.push('\''),
                        Some(c) => { s.push('\\'); s.push(c); }
                        None => return Err(format!(
                            "Unterminated string literal at line {} col {}",
                            span.line, span.col
                        )),
                    }
                }
                Some(c) => s.push(c),
                None => {
                    return Err(format!(
                        "Unterminated string literal at line {} col {}",
                        span.line, span.col
                    ));
                }
            }
        }

        Ok(Token {
            kind: TokenKind::StringLiteral(s),
            span,
        })
    }

    fn read_quoted_identifier(&mut self, span: Span, quote: char) -> Result<Token, String> {
        self.advance(); // consume opening quote
        let mut s = String::new();

        loop {
            match self.advance() {
                Some(c) if c == quote => break,
                Some(c) => s.push(c),
                None => {
                    return Err(format!(
                        "Unterminated quoted identifier at line {} col {}",
                        span.line, span.col
                    ));
                }
            }
        }

        Ok(Token {
            kind: TokenKind::Identifier(s),
            span,
        })
    }

    fn read_number(&mut self, span: Span, negative: bool) -> Result<Token, String> {
        let mut s = String::new();
        if negative {
            s.push('-');
        }

        let mut is_float = false;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                s.push(ch);
                self.advance();
            } else if ch == '.' && !is_float {
                // Check next char is a digit (not a method call like 1.method)
                if let Some(next) = self.peek_ahead(1) {
                    if next.is_ascii_digit() {
                        is_float = true;
                        s.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else if (ch == 'e' || ch == 'E') && !s.contains('e') && !s.contains('E') {
                is_float = true;
                s.push(ch);
                self.advance();
                // Optional +/- after e
                if let Some(next) = self.peek() {
                    if next == '+' || next == '-' {
                        s.push(next);
                        self.advance();
                    }
                }
            } else {
                break;
            }
        }

        if is_float {
            match s.parse::<f64>() {
                Ok(f) => Ok(Token {
                    kind: TokenKind::FloatLiteral(f),
                    span,
                }),
                Err(_) => Err(format!("Invalid float literal '{}' at line {} col {}", s, span.line, span.col)),
            }
        } else {
            match s.parse::<i64>() {
                Ok(i) => Ok(Token {
                    kind: TokenKind::IntegerLiteral(i),
                    span,
                }),
                Err(_) => Err(format!("Invalid integer literal '{}' at line {} col {}", s, span.line, span.col)),
            }
        }
    }

    fn read_word(&mut self, span: Span) -> Result<Token, String> {
        let mut word = String::new();

        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' {
                word.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        // Check if it's a keyword
        let upper = word.to_uppercase();
        let kind = match upper.as_str() {
            "SELECT" => TokenKind::Keyword(SqlKeyword::Select),
            "INSERT" => TokenKind::Keyword(SqlKeyword::Insert),
            "INTO" => TokenKind::Keyword(SqlKeyword::Into),
            "UPDATE" => TokenKind::Keyword(SqlKeyword::Update),
            "DELETE" => TokenKind::Keyword(SqlKeyword::Delete),
            "FROM" => TokenKind::Keyword(SqlKeyword::From),
            "WHERE" => TokenKind::Keyword(SqlKeyword::Where),
            "AND" => TokenKind::Keyword(SqlKeyword::And),
            "OR" => TokenKind::Keyword(SqlKeyword::Or),
            "NOT" => TokenKind::Keyword(SqlKeyword::Not),
            "IN" => TokenKind::Keyword(SqlKeyword::In),
            "BETWEEN" => TokenKind::Keyword(SqlKeyword::Between),
            "LIKE" => TokenKind::Keyword(SqlKeyword::Like),
            "IS" => TokenKind::Keyword(SqlKeyword::Is),
            "NULL" => TokenKind::NullLiteral,
            "TRUE" => TokenKind::BooleanLiteral(true),
            "FALSE" => TokenKind::BooleanLiteral(false),
            "SET" => TokenKind::Keyword(SqlKeyword::Set),
            "VALUES" => TokenKind::Keyword(SqlKeyword::Values),
            "CREATE" => TokenKind::Keyword(SqlKeyword::Create),
            "DROP" => TokenKind::Keyword(SqlKeyword::Drop),
            "ALTER" => TokenKind::Keyword(SqlKeyword::Alter),
            "TABLE" => TokenKind::Keyword(SqlKeyword::Table),
            "COLLECTION" => TokenKind::Keyword(SqlKeyword::Collection),
            "INDEX" => TokenKind::Keyword(SqlKeyword::Index),
            "ON" => TokenKind::Keyword(SqlKeyword::On),
            "UNIQUE" => TokenKind::Keyword(SqlKeyword::Unique),
            "IF" => TokenKind::Keyword(SqlKeyword::If),
            "EXISTS" => TokenKind::Keyword(SqlKeyword::Exists),
            "LIMIT" => TokenKind::Keyword(SqlKeyword::Limit),
            "OFFSET" => TokenKind::Keyword(SqlKeyword::Offset),
            "SKIP" => TokenKind::Keyword(SqlKeyword::Skip_),
            "ORDER" => TokenKind::Keyword(SqlKeyword::Order),
            "BY" => TokenKind::Keyword(SqlKeyword::By),
            "ASC" => TokenKind::Keyword(SqlKeyword::Asc),
            "DESC" => TokenKind::Keyword(SqlKeyword::Desc),
            "AS" => TokenKind::Keyword(SqlKeyword::As),
            "COUNT" => TokenKind::Keyword(SqlKeyword::Count),
            "JOIN" => TokenKind::Keyword(SqlKeyword::Join),
            "INNER" => TokenKind::Keyword(SqlKeyword::Inner),
            "LEFT" => TokenKind::Keyword(SqlKeyword::Left),
            "OUTER" => TokenKind::Keyword(SqlKeyword::Outer),
            "EXPLAIN" => TokenKind::Keyword(SqlKeyword::Explain),
            "ANALYZE" => TokenKind::Keyword(SqlKeyword::Analyze),
            "VERBOSE" => TokenKind::Keyword(SqlKeyword::Verbose),
            "BEGIN" => TokenKind::Keyword(SqlKeyword::Begin),
            "COMMIT" => TokenKind::Keyword(SqlKeyword::Commit),
            "ROLLBACK" => TokenKind::Keyword(SqlKeyword::Rollback),
            "TRANSACTION" => TokenKind::Keyword(SqlKeyword::Transaction),
            "ADD" => TokenKind::Keyword(SqlKeyword::Add),
            "COLUMN" => TokenKind::Keyword(SqlKeyword::Column),
            "RENAME" => TokenKind::Keyword(SqlKeyword::Rename),
            "TO" => TokenKind::Keyword(SqlKeyword::To),
            "DISTINCT" => TokenKind::Keyword(SqlKeyword::Distinct),
            "SHOW" => TokenKind::Keyword(SqlKeyword::Show),
            "DESCRIBE" => TokenKind::Keyword(SqlKeyword::Describe),
            _ => TokenKind::Identifier(word),
        };

        Ok(Token { kind, span })
    }
}

/// Token stream cursor for the parser
pub struct TokenStream {
    tokens: Vec<Token>,
    pos: usize,
}

impl TokenStream {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn from_sql(input: &str) -> Result<Self, String> {
        Ok(Self::new(Tokenizer::tokenize(input)?))
    }

    pub fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    pub fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    pub fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    pub fn expect_keyword(&mut self, kw: SqlKeyword) -> Result<&Token, String> {
        let tok = self.peek();
        if tok.kind == TokenKind::Keyword(kw) {
            Ok(self.advance())
        } else {
            Err(format!(
                "Expected keyword {:?} but got {:?} at line {} col {}",
                kw, tok.kind, tok.span.line, tok.span.col
            ))
        }
    }

    pub fn expect_identifier(&mut self) -> Result<String, String> {
        let tok = self.peek().clone();
        match &tok.kind {
            TokenKind::Identifier(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            // Allow keywords to be used as identifiers in certain contexts
            TokenKind::Keyword(_) => {
                let s = format!("{:?}", tok.kind);
                // Extract the keyword name to use as identifier
                let ident = match &tok.kind {
                    TokenKind::Keyword(kw) => format!("{:?}", kw).to_lowercase(),
                    _ => unreachable!(),
                };
                self.advance();
                Ok(ident)
            }
            _ => Err(format!(
                "Expected identifier but got {:?} at line {} col {}",
                tok.kind, tok.span.line, tok.span.col
            )),
        }
    }

    pub fn check_keyword(&self, kw: SqlKeyword) -> bool {
        self.peek().kind == TokenKind::Keyword(kw)
    }

    pub fn consume_if_keyword(&mut self, kw: SqlKeyword) -> bool {
        if self.check_keyword(kw) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub fn is_eof(&self) -> bool {
        self.peek().is_eof()
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn reset(&mut self, pos: usize) {
        self.pos = pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_select() {
        let tokens = Tokenizer::tokenize("SELECT * FROM users").unwrap();
        assert_eq!(tokens.len(), 5); // SELECT, *, FROM, users, EOF
        assert_eq!(tokens[0].kind, TokenKind::Keyword(SqlKeyword::Select));
        assert_eq!(tokens[1].kind, TokenKind::Star);
        assert_eq!(tokens[2].kind, TokenKind::Keyword(SqlKeyword::From));
        assert_eq!(tokens[3].kind, TokenKind::Identifier("users".to_string()));
        assert_eq!(tokens[4].kind, TokenKind::Eof);
    }

    #[test]
    fn test_string_literals() {
        let tokens = Tokenizer::tokenize("SELECT * FROM t WHERE name = 'hello world'").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::StringLiteral("hello world".to_string())));
    }

    #[test]
    fn test_escaped_quotes() {
        let tokens = Tokenizer::tokenize("SELECT 'it''s escaped'").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::StringLiteral("it's escaped".to_string())));
    }

    #[test]
    fn test_numeric_literals() {
        let tokens = Tokenizer::tokenize("SELECT 42, 3.14, -100, 1e5").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::IntegerLiteral(42)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::FloatLiteral(3.14)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::IntegerLiteral(-100)));
        assert!(tokens.iter().any(|t| matches!(&t.kind, TokenKind::FloatLiteral(f) if (*f - 1e5).abs() < 1.0)));
    }

    #[test]
    fn test_operators() {
        let tokens = Tokenizer::tokenize("a = 1 AND b != 2 AND c <> 3 AND d >= 4 AND e <= 5").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Eq));
        assert!(tokens.iter().filter(|t| t.kind == TokenKind::Neq).count() == 2);
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Gte));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Lte));
    }

    #[test]
    fn test_quoted_identifiers() {
        let tokens = Tokenizer::tokenize(r#"SELECT "user name" FROM `my table`"#).unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Identifier("user name".to_string())));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Identifier("my table".to_string())));
    }

    #[test]
    fn test_comments() {
        let tokens = Tokenizer::tokenize("SELECT 1 -- this is a comment\n FROM t").unwrap();
        // Comments should be skipped
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword(SqlKeyword::From)));
    }

    #[test]
    fn test_block_comments() {
        let tokens = Tokenizer::tokenize("SELECT /* skip this */ 1 FROM t").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::IntegerLiteral(1)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword(SqlKeyword::From)));
    }

    #[test]
    fn test_between_like_keywords() {
        let tokens = Tokenizer::tokenize("WHERE age BETWEEN 18 AND 65 AND name LIKE '%faiz%'").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword(SqlKeyword::Between)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword(SqlKeyword::Like)));
    }

    #[test]
    fn test_is_null() {
        let tokens = Tokenizer::tokenize("WHERE email IS NULL AND name IS NOT NULL").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword(SqlKeyword::Is)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword(SqlKeyword::Not)));
        assert!(tokens.iter().filter(|t| t.kind == TokenKind::NullLiteral).count() == 2);
    }

    #[test]
    fn test_token_stream() {
        let mut ts = TokenStream::from_sql("SELECT 1").unwrap();
        assert!(ts.check_keyword(SqlKeyword::Select));
        ts.advance();
        assert_eq!(*ts.peek_kind(), TokenKind::IntegerLiteral(1));
    }

    #[test]
    fn test_alter_table_keywords() {
        let tokens = Tokenizer::tokenize("ALTER TABLE users ADD COLUMN email").unwrap();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword(SqlKeyword::Alter)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword(SqlKeyword::Add)));
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Keyword(SqlKeyword::Column)));
    }
}
