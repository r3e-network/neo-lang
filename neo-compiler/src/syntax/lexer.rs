//! Lexer for neo-lang (ASCII-oriented, matches README token set).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Contract,          // contract
    Package,           // package
    Struct,            // struct
    Import,            // import
    From,              // from
    Const,             // const
    Event,             // event
    Emit,              // emit
    Return,            // return
    If,                // if
    Else,              // else
    For,               // for
    In,                // in
    While,             // while
    Var,               // var
    As,                // as
    Void,              // void
    Bool,              // bool
    Int,               // int
    String,            // string
    Hash160,           // hash160
    Hash256,           // hash256
    Map,               // map
    Buffer,            // buffer
    Any,               // any
    Null,              // null
    True,              // true
    False,             // false
    Self_,             // self
    ShrEq,             // >>=
    ShlEq,             // <<=
    Shr,               // >>
    Shl,               // <<
    Ge,                // >=
    Le,                // <=
    EqEq,              // ==
    Ne,                // !=
    AmpAmp,            // &&
    PipePipe,          // ||
    PlusEq,            // +=
    MinusEq,           // -=
    StarEq,            // *=
    SlashEq,           // /=
    PercentEq,         // %=
    AmpEq,             // &=
    PipeEq,            // |=
    CaretEq,           // ^=
    Plus,              // +
    Minus,             // -
    Star,              // *
    Slash,             // /
    Percent,           // %
    Bang,              // !
    Tilde,             // ~
    Amp,               // &
    Pipe,              // |
    Caret,             // ^
    Lt,                // <
    Gt,                // >
    Eq,                // =
    LParen,            // (
    RParen,            // )
    LBracket,          // [
    RBracket,          // ]
    LBrace,            // {
    RBrace,            // }
    Semi,              // ;
    Comma,             // ,
    Dot,               // .
    Colon,             // :
    AttrOpen,          // #[attr(arg1, arg2, ...)]
    Ident(String),     // identifier
    StringLit(String), // "string"
    BufferLit(String), // b"string"
    IntLit(String),    // 123, 0x123, 0b1010, etc.
    Eof,               // end of file
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub line: usize,
    pub message: &'static str,
}

impl LexError {
    pub fn new(line: usize, message: &'static str) -> Self {
        Self { line, message }
    }
}

/// Lex the source code into a vector of (line number, token) pairs.
/// The last token is always (line count, Token::Eof).
pub fn lex(src: &str) -> Result<Vec<(usize, Token)>, LexError> {
    let mut out = Vec::new();
    let mut line = 1usize;
    let buf = src.as_bytes();
    let mut index = 0usize;
    while index < buf.len() {
        match buf[index] {
            b'\n' => {
                line += 1;
                index += 1;
            }
            b' ' | b'\t' | b'\r' => {
                index += 1;
            }
            b'/' if index + 1 < buf.len() && buf[index + 1] == b'/' => {
                // skip line comments
                index += 2;
                while index < buf.len() && buf[index] != b'\n' {
                    index += 1;
                }
            }
            b'#' if index + 1 < buf.len() && buf[index + 1] == b'[' => {
                out.push((line, Token::AttrOpen));
                index += 2;
            }
            b'>' if index + 1 < buf.len()
                && buf[index + 1] == b'>'
                && index + 2 < buf.len()
                && buf[index + 2] == b'=' =>
            {
                out.push((line, Token::ShrEq));
                index += 3;
            }
            b'<' if index + 1 < buf.len()
                && buf[index + 1] == b'<'
                && index + 2 < buf.len()
                && buf[index + 2] == b'=' =>
            {
                out.push((line, Token::ShlEq));
                index += 3;
            }
            b'>' if index + 1 < buf.len() && buf[index + 1] == b'>' => {
                out.push((line, Token::Shr));
                index += 2;
            }
            b'<' if index + 1 < buf.len() && buf[index + 1] == b'<' => {
                out.push((line, Token::Shl));
                index += 2;
            }
            b'>' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::Ge));
                index += 2;
            }
            b'<' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::Le));
                index += 2;
            }
            b'=' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::EqEq));
                index += 2;
            }
            b'!' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::Ne));
                index += 2;
            }
            b'&' if index + 1 < buf.len() && buf[index + 1] == b'&' => {
                out.push((line, Token::AmpAmp));
                index += 2;
            }
            b'|' if index + 1 < buf.len() && buf[index + 1] == b'|' => {
                out.push((line, Token::PipePipe));
                index += 2;
            }
            b'+' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::PlusEq));
                index += 2;
            }
            b'-' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::MinusEq));
                index += 2;
            }
            b'*' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::StarEq));
                index += 2;
            }
            b'/' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::SlashEq));
                index += 2;
            }
            b'%' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::PercentEq));
                index += 2;
            }
            b'&' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::AmpEq));
                index += 2;
            }
            b'|' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::PipeEq));
                index += 2;
            }
            b'^' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                out.push((line, Token::CaretEq));
                index += 2;
            }
            b'(' => {
                out.push((line, Token::LParen));
                index += 1;
            }
            b')' => {
                out.push((line, Token::RParen));
                index += 1;
            }
            b'[' => {
                out.push((line, Token::LBracket));
                index += 1;
            }
            b']' => {
                out.push((line, Token::RBracket));
                index += 1;
            }
            b'{' => {
                out.push((line, Token::LBrace));
                index += 1;
            }
            b'}' => {
                out.push((line, Token::RBrace));
                index += 1;
            }
            b';' => {
                out.push((line, Token::Semi));
                index += 1;
            }
            b',' => {
                out.push((line, Token::Comma));
                index += 1;
            }
            b'.' => {
                out.push((line, Token::Dot));
                index += 1;
            }
            b':' => {
                out.push((line, Token::Colon));
                index += 1;
            }
            b'+' => {
                out.push((line, Token::Plus));
                index += 1;
            }
            b'-' => {
                out.push((line, Token::Minus));
                index += 1;
            }
            b'*' => {
                out.push((line, Token::Star));
                index += 1;
            }
            b'/' => {
                out.push((line, Token::Slash));
                index += 1;
            }
            b'%' => {
                out.push((line, Token::Percent));
                index += 1;
            }
            b'!' => {
                out.push((line, Token::Bang));
                index += 1;
            }
            b'~' => {
                out.push((line, Token::Tilde));
                index += 1;
            }
            b'&' => {
                out.push((line, Token::Amp));
                index += 1;
            }
            b'|' => {
                out.push((line, Token::Pipe));
                index += 1;
            }
            b'^' => {
                out.push((line, Token::Caret));
                index += 1;
            }
            b'<' => {
                out.push((line, Token::Lt));
                index += 1;
            }
            b'>' => {
                out.push((line, Token::Gt));
                index += 1;
            }
            b'=' => {
                out.push((line, Token::Eq));
                index += 1;
            }
            b'"' => {
                let (s, ni) = lex_string(src, line, index)?;
                out.push((line, Token::StringLit(s)));
                index = ni;
            }
            b'b' if index + 1 < buf.len() && buf[index + 1] == b'"' => {
                let (s, ni) = lex_string(src, line, index + 1)?;
                out.push((line, Token::BufferLit(s)));
                index = ni;
            }
            c if c.is_ascii_digit() => {
                let start = index;
                if buf[index] == b'0' && index + 1 < buf.len() {
                    let n = buf[index + 1];
                    if n == b'x' || n == b'X' {
                        index += 2;
                        let d0 = index;
                        while index < buf.len()
                            && (buf[index].is_ascii_hexdigit() || buf[index] == b'_')
                        {
                            index += 1;
                        }
                        if index == d0 {
                            return Err(LexError::new(line, "empty hex int"));
                        }
                        out.push((line, Token::IntLit(src[start..index].into())));
                        continue;
                    }
                    if n == b'b' || n == b'B' {
                        index += 2;
                        let d0 = index;
                        while index < buf.len()
                            && (buf[index] == b'0' || buf[index] == b'1' || buf[index] == b'_')
                        {
                            index += 1;
                        }
                        if index == d0 {
                            return Err(LexError::new(line, "empty binary int"));
                        }
                        out.push((line, Token::IntLit(src[start..index].into())));
                        continue;
                    }
                }
                while index < buf.len() && (buf[index].is_ascii_digit() || buf[index] == b'_') {
                    index += 1;
                }
                out.push((line, Token::IntLit(src[start..index].into())));
            }
            c if is_ident_start(c) => {
                let start = index;
                index += 1;
                while index < buf.len() && is_ident_cont(buf[index]) {
                    index += 1;
                }
                let name = &src[start..index];
                out.push((line, keyword_or_ident(name)));
            }
            _ => {
                return Err(LexError::new(line, "unexpected character"));
            }
        }
    }
    out.push((line, Token::Eof));
    Ok(out)
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_cont(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn keyword_or_ident(s: &str) -> Token {
    match s {
        "contract" => Token::Contract,
        "package" => Token::Package,
        "struct" => Token::Struct,
        "import" => Token::Import,
        "from" => Token::From,
        "const" => Token::Const,
        "event" => Token::Event,
        "emit" => Token::Emit,
        "return" => Token::Return,
        "if" => Token::If,
        "else" => Token::Else,
        "for" => Token::For,
        "in" => Token::In,
        "while" => Token::While,
        "var" => Token::Var,
        "as" => Token::As,
        "void" => Token::Void,
        "bool" => Token::Bool,
        "int" => Token::Int,
        "string" => Token::String,
        "hash160" => Token::Hash160,
        "hash256" => Token::Hash256,
        "map" => Token::Map,
        "buffer" => Token::Buffer,
        "any" => Token::Any,
        "null" => Token::Null,
        "true" => Token::True,
        "false" => Token::False,
        "self" => Token::Self_,
        _ => Token::Ident(s.to_string()),
    }
}

fn hex_escape_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + (b - b'a')),
        b'A'..=b'F' => Some(10 + (b - b'A')),
        _ => None,
    }
}

fn lex_string(src: &str, line: usize, quote_at: usize) -> Result<(String, usize), LexError> {
    let b = src.as_bytes();
    debug_assert!(b.get(quote_at) == Some(&b'"'));
    let mut index = quote_at + 1;
    let mut out = String::new();
    while index < b.len() {
        match b[index] {
            b'"' => return Ok((out, index + 1)),
            b'\\' => {
                if index + 1 >= b.len() {
                    return Err(LexError::new(line, "unterminated string"));
                }
                match b[index + 1] {
                    b'n' => {
                        out.push('\n');
                        index += 2;
                    }
                    b'r' => {
                        out.push('\r');
                        index += 2;
                    }
                    b't' => {
                        out.push('\t');
                        index += 2;
                    }
                    b'\\' => {
                        out.push('\\');
                        index += 2;
                    }
                    b'"' => {
                        out.push('"');
                        index += 2;
                    }
                    b'0' => {
                        out.push('\0');
                        index += 2;
                    }
                    b'a' => {
                        out.push('\u{7}');
                        index += 2;
                    }
                    b'b' => {
                        out.push('\u{8}');
                        index += 2;
                    }
                    b'f' => {
                        out.push('\u{c}');
                        index += 2;
                    }
                    b'v' => {
                        out.push('\u{b}');
                        index += 2;
                    }
                    b'x' => {
                        if index + 4 > b.len() {
                            return Err(LexError::new(line, "invalid hex escape in string"));
                        }
                        let h1 = hex_escape_digit(b[index + 2])
                            .ok_or(LexError::new(line, "invalid hex escape in string"))?;
                        let h2 = hex_escape_digit(b[index + 3])
                            .ok_or(LexError::new(line, "invalid hex escape in string"))?;
                        let byte = (h1 << 4) | h2;
                        out.push(char::from_u32(u32::from(byte)).unwrap());
                        index += 4;
                    }
                    _ => return Err(LexError::new(line, "invalid escape in string")),
                }
            }
            c => {
                out.push(c as char);
                index += 1;
            }
        }
    }
    Err(LexError::new(line, "unterminated string"))
}
