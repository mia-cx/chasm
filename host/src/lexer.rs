//! Turns source text into tokens, inserting statement-ending newlines.

use crate::diag::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Ident(String),
    /// Magnitude of a decimal literal, at most 2^31 so that `-2147483648` fits.
    Int(u32),
    Str(String),
    Fn,
    Let,
    If,
    Else,
    While,
    Return,
    True,
    False,
    And,
    Or,
    Not,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Arrow,
    Assign,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    /// A newline that ends a statement.
    Newline,
    Eof,
    /// Text the lexer rejects. Lexing stops here and the parser reports the
    /// message when it reaches this token, so errors surface in source order.
    Invalid(String),
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Largest literal magnitude: `2147483648`, valid only after a unary minus.
pub const MAX_LITERAL_MAGNITUDE: u32 = 1 << 31;

const RESERVED: [&str; 15] = [
    "struct", "enum", "match", "for", "in", "break", "continue", "try", "import", "export",
    "async", "await", "region", "trait", "mut",
];

/// Lexes the whole file. The returned list always ends in `Eof` or `Invalid`.
pub fn lex(src: &str) -> Vec<Token> {
    let mut lexer = Lexer {
        chars: src.chars().collect(),
        pos: 0,
        line: 1,
        col: 1,
        paren_depth: 0,
        tokens: Vec::new(),
    };
    lexer.run();
    lexer.tokens
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    col: u32,
    paren_depth: u32,
    tokens: Vec<Token>,
}

impl Lexer {
    fn run(&mut self) {
        loop {
            let mut span = self.span();
            let Some(c) = self.peek(0) else {
                self.push(Tok::Eof, span);
                return;
            };
            let tok = match c {
                ' ' | '\t' | '\r' => {
                    self.bump();
                    continue;
                }
                '\n' => {
                    self.bump();
                    if self.paren_depth == 0 && self.last_ends_statement() {
                        self.push(Tok::Newline, span);
                    }
                    continue;
                }
                '/' if self.peek(1) == Some('/') => {
                    while self.peek(0).is_some_and(|c| c != '\n') {
                        self.bump();
                    }
                    continue;
                }
                '"' => self.string(&mut span),
                '0'..='9' => self.number(),
                c if c.is_ascii_alphabetic() || c == '_' => self.word(),
                _ => self.punct(c),
            };
            let stop = matches!(tok, Tok::Invalid(_));
            self.push(tok, span);
            if stop {
                return;
            }
        }
    }

    fn punct(&mut self, c: char) -> Tok {
        let next = self.peek(1);
        let (tok, len) = match (c, next) {
            ('-', Some('>')) => (Tok::Arrow, 2),
            ('=', Some('=')) => (Tok::EqEq, 2),
            ('!', Some('=')) => (Tok::NotEq, 2),
            ('<', Some('=')) => (Tok::Le, 2),
            ('>', Some('=')) => (Tok::Ge, 2),
            (':', Some(':')) => return unsupported("namespace paths (`::`)"),
            ('&', Some('&')) => return Tok::Invalid("use `and` instead of `&&`".into()),
            ('|', Some('|')) => return Tok::Invalid("use `or` instead of `||`".into()),
            ('(', _) => (Tok::LParen, 1),
            (')', _) => (Tok::RParen, 1),
            ('{', _) => (Tok::LBrace, 1),
            ('}', _) => (Tok::RBrace, 1),
            (',', _) => (Tok::Comma, 1),
            (':', _) => (Tok::Colon, 1),
            ('=', _) => (Tok::Assign, 1),
            ('+', _) => (Tok::Plus, 1),
            ('-', _) => (Tok::Minus, 1),
            ('*', _) => (Tok::Star, 1),
            ('/', _) => (Tok::Slash, 1),
            ('%', _) => (Tok::Percent, 1),
            ('<', _) => (Tok::Lt, 1),
            ('>', _) => (Tok::Gt, 1),
            ('!', _) => return Tok::Invalid("use `not` instead of `!`".into()),
            ('&', _) => return unsupported("borrows (`&`)"),
            ('#', _) => return unsupported("callable paths (`#`)"),
            ('.', _) => return unsupported("field access (`.`)"),
            ('[' | ']', _) => return unsupported("arrays and indexing"),
            (';', _) => {
                return Tok::Invalid(
                    "semicolons are not used; end a statement with a newline".into(),
                );
            }
            _ if c.is_alphabetic() => {
                return Tok::Invalid(format!(
                    "unexpected character `{c}`; identifiers are ASCII only"
                ));
            }
            _ => return Tok::Invalid(format!("unexpected character `{}`", c.escape_debug())),
        };
        match tok {
            Tok::LParen => self.paren_depth += 1,
            Tok::RParen => self.paren_depth = self.paren_depth.saturating_sub(1),
            _ => {}
        }
        for _ in 0..len {
            self.bump();
        }
        tok
    }

    fn word(&mut self) -> Tok {
        let mut word = String::new();
        while let Some(c) = self
            .peek(0)
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        {
            word.push(c);
            self.bump();
        }
        match word.as_str() {
            "fn" => Tok::Fn,
            "let" => Tok::Let,
            "if" => Tok::If,
            "else" => Tok::Else,
            "while" => Tok::While,
            "return" => Tok::Return,
            "true" => Tok::True,
            "false" => Tok::False,
            "and" => Tok::And,
            "or" => Tok::Or,
            "not" => Tok::Not,
            w if RESERVED.contains(&w) => unsupported(&format!("`{w}`")),
            _ => Tok::Ident(word),
        }
    }

    fn number(&mut self) -> Tok {
        let mut text = String::new();
        while let Some(c) = self
            .peek(0)
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        {
            text.push(c);
            self.bump();
        }
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            return unsupported("floating-point numbers");
        }
        if !text.bytes().all(|b| b.is_ascii_digit()) {
            return Tok::Invalid(format!("invalid integer literal `{text}`"));
        }
        if text.len() > 1 && text.starts_with('0') {
            return Tok::Invalid(format!("integer literal `{text}` has a leading zero"));
        }
        match text.parse::<u32>() {
            Ok(n) if n <= MAX_LITERAL_MAGNITUDE => Tok::Int(n),
            _ => Tok::Invalid(format!("integer literal `{text}` is out of range for Int")),
        }
    }

    /// Lexes a string literal. An invalid escape moves `span` to its backslash.
    fn string(&mut self, span: &mut Span) -> Tok {
        self.bump();
        let mut value = String::new();
        loop {
            let here = self.span();
            match self.peek(0) {
                None | Some('\n') => return Tok::Invalid("unterminated string literal".into()),
                Some('\0') => {
                    *span = here;
                    return Tok::Invalid("string literals cannot contain NUL".into());
                }
                Some('"') => {
                    self.bump();
                    return Tok::Str(value);
                }
                Some('\\') => {
                    self.bump();
                    let escaped = match self.peek(0) {
                        Some('n') => '\n',
                        Some('t') => '\t',
                        Some('r') => '\r',
                        Some('\\') => '\\',
                        Some('"') => '"',
                        _ => {
                            *span = here;
                            return Tok::Invalid(
                                "unknown escape; use \\n, \\t, \\r, \\\\, or \\\"".into(),
                            );
                        }
                    };
                    self.bump();
                    value.push(escaped);
                }
                Some(c) => {
                    self.bump();
                    value.push(c);
                }
            }
        }
    }

    fn last_ends_statement(&self) -> bool {
        self.tokens.last().is_some_and(|t| {
            matches!(
                t.tok,
                Tok::Ident(_)
                    | Tok::Int(_)
                    | Tok::Str(_)
                    | Tok::True
                    | Tok::False
                    | Tok::Return
                    | Tok::RParen
                    | Tok::RBrace
            )
        })
    }

    fn push(&mut self, tok: Tok, span: Span) {
        self.tokens.push(Token { tok, span });
    }

    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.pos + ahead).copied()
    }

    fn bump(&mut self) {
        if self.peek(0) == Some('\n') {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        self.pos += 1;
    }

    fn span(&self) -> Span {
        Span {
            line: self.line,
            col: self.col,
        }
    }
}

fn unsupported(what: &str) -> Tok {
    Tok::Invalid(format!("this alpha does not support {what}"))
}
