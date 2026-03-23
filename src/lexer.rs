#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Keywords
    Fn,
    Let,
    Return,
    If,
    Else,
    While,
    For,
    In,
    Print,
    Input,

    // Types
    I32,
    I64,
    F32,
    F64,
    Bool,
    Str,

    // Literals
    IntLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    True,
    False,
    Identifier(String),

    // Symbols
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Semicolon,
    Comma,
    Arrow,
    Assign,
    DotDot,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Equal,
    NotEqual,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    And,
    Or,
    Bang,
    Percent,
    Ampersand,
    Pipe,
    Caret,
    LShift,
    RShift,

    // Compound assignment
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,

    // Special
    As,
    Break,
    Continue,
    Map,
    Extern,
    Void,
    Struct,
    Dot,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub typ: TokenType,
    pub line: usize,
    pub column: usize,
}

pub fn tokenize(input: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = input.chars().collect();
    let mut pos = 0;
    let mut line = 1;
    let mut column = 1;

    while pos < chars.len() {
        let start_line = line;
        let start_column = column;

        // Skip whitespace
        if chars[pos].is_whitespace() {
            if chars[pos] == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
            pos += 1;
            continue;
        }

        // Skip comments
        if chars[pos] == '/' && pos + 1 < chars.len() && chars[pos + 1] == '/' {
            while pos < chars.len() && chars[pos] != '\n' {
                pos += 1;
            }
            continue;
        }

        // Numbers (int or float)
        if chars[pos].is_ascii_digit() {
            let mut num = String::new();
            while pos < chars.len() && chars[pos].is_ascii_digit() {
                num.push(chars[pos]);
                pos += 1;
                column += 1;
            }
            // Check for float: digits '.' digits
            if pos < chars.len() && chars[pos] == '.'
                && pos + 1 < chars.len() && chars[pos + 1].is_ascii_digit()
            {
                num.push('.');
                pos += 1;
                column += 1;
                while pos < chars.len() && chars[pos].is_ascii_digit() {
                    num.push(chars[pos]);
                    pos += 1;
                    column += 1;
                }
                tokens.push(Token {
                    typ: TokenType::FloatLiteral(num.parse().unwrap()),
                    line: start_line,
                    column: start_column,
                });
            } else {
                tokens.push(Token {
                    typ: TokenType::IntLiteral(num.parse().unwrap()),
                    line: start_line,
                    column: start_column,
                });
            }
            continue;
        }

        // Identifiers and keywords
        if chars[pos].is_alphabetic() || chars[pos] == '_' {
            let mut ident = String::new();
            while pos < chars.len() && (chars[pos].is_alphanumeric() || chars[pos] == '_') {
                ident.push(chars[pos]);
                pos += 1;
                column += 1;
            }

            let typ = match ident.as_str() {
                "fn" => TokenType::Fn,
                "let" => TokenType::Let,
                "return" => TokenType::Return,
                "if" => TokenType::If,
                "else" => TokenType::Else,
                "while"    => TokenType::While,
                "for"      => TokenType::For,
                "in"       => TokenType::In,
                "print"    => TokenType::Print,
                "input"    => TokenType::Input,
                "break"    => TokenType::Break,
                "continue" => TokenType::Continue,
                "as"       => TokenType::As,
                "map"      => TokenType::Map,
                "extern"   => TokenType::Extern,
                "void"     => TokenType::Void,
                "struct"   => TokenType::Struct,
                "i32" => TokenType::I32,
                "i64" => TokenType::I64,
                "f32" => TokenType::F32,
                "f64" => TokenType::F64,
                "bool" => TokenType::Bool,
                "str" => TokenType::Str,
                "true" => TokenType::True,
                "false" => TokenType::False,
                _ => TokenType::Identifier(ident),
            };

            tokens.push(Token {
                typ,
                line: start_line,
                column: start_column,
            });
            continue;
        }

        // String literals
        if chars[pos] == '"' {
            pos += 1;
            column += 1;
            let mut s = String::new();
            while pos < chars.len() && chars[pos] != '"' {
                if chars[pos] == '\\' && pos + 1 < chars.len() {
                    pos += 1;
                    column += 1;
                    match chars[pos] {
                        'n' => s.push('\n'),
                        'r' => s.push('\r'),
                        't' => s.push('\t'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        c => { s.push('\\'); s.push(c); }
                    }
                } else {
                    if chars[pos] == '\n' {
                        line += 1;
                        column = 0;
                    }
                    s.push(chars[pos]);
                }
                pos += 1;
                column += 1;
            }
            if pos >= chars.len() {
                panic!("Unterminated string literal at {}:{}", start_line, start_column);
            }
            pos += 1; // skip closing '"'
            column += 1;
            tokens.push(Token {
                typ: TokenType::StringLiteral(s),
                line: start_line,
                column: start_column,
            });
            continue;
        }

        // Symbols and operators
        let typ = match chars[pos] {
            '(' => TokenType::LParen,
            ')' => TokenType::RParen,
            '{' => TokenType::LBrace,
            '}' => TokenType::RBrace,
            '[' => TokenType::LBracket,
            ']' => TokenType::RBracket,
            ':' => TokenType::Colon,
            ';' => TokenType::Semicolon,
            ',' => TokenType::Comma,
            '+' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                    pos += 1; column += 1; TokenType::PlusAssign
                } else { TokenType::Plus }
            }
            '*' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                    pos += 1; column += 1; TokenType::StarAssign
                } else { TokenType::Star }
            }
            '/' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                    pos += 1; column += 1; TokenType::SlashAssign
                } else { TokenType::Slash }
            }
            '%' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                    pos += 1; column += 1; TokenType::PercentAssign
                } else { TokenType::Percent }
            }
            '^' => TokenType::Caret,
            '=' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                    pos += 1;
                    column += 1;
                    TokenType::Equal
                } else {
                    TokenType::Assign
                }
            }
            '!' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                    pos += 1;
                    column += 1;
                    TokenType::NotEqual
                } else {
                    TokenType::Bang
                }
            }
            '&' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '&' {
                    pos += 1; column += 1; TokenType::And
                } else {
                    TokenType::Ampersand
                }
            }
            '|' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '|' {
                    pos += 1; column += 1; TokenType::Or
                } else {
                    TokenType::Pipe
                }
            }
            '<' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '<' {
                    pos += 1; column += 1; TokenType::LShift
                } else if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                    pos += 1; column += 1; TokenType::LessEqual
                } else {
                    TokenType::Less
                }
            }
            '>' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '>' {
                    pos += 1; column += 1; TokenType::RShift
                } else if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                    pos += 1; column += 1; TokenType::GreaterEqual
                } else {
                    TokenType::Greater
                }
            }
            '.' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '.' {
                    pos += 1;
                    column += 1;
                    TokenType::DotDot
                } else {
                    TokenType::Dot
                }
            }
            '-' => {
                if pos + 1 < chars.len() && chars[pos + 1] == '>' {
                    pos += 1; column += 1; TokenType::Arrow
                } else if pos + 1 < chars.len() && chars[pos + 1] == '=' {
                    pos += 1; column += 1; TokenType::MinusAssign
                } else {
                    TokenType::Minus
                }
            }
            _ => panic!("Unexpected character '{}' at {}:{}", chars[pos], line, column),
        };

        tokens.push(Token {
            typ,
            line: start_line,
            column: start_column,
        });

        pos += 1;
        column += 1;
    }

    tokens.push(Token {
        typ: TokenType::Eof,
        line,
        column,
    });

    tokens
}
