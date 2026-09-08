//! Lexer for neo-lang (ASCII-oriented, matches README token set).

use crate::diagnostic::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Contract,          // contract
    Declare,           // declare
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
    pub message: String,
    pub span: Span,
}

impl LexError {
    pub fn new(line: usize, message: impl Into<String>, span: Span) -> Self {
        Self {
            line,
            message: message.into(),
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexed {
    pub tokens: Vec<(usize, Token)>,
    pub spans: Vec<Span>,
}

fn push_token(
    out: &mut Vec<(usize, Token)>,
    spans: &mut Vec<Span>,
    line: usize,
    token: Token,
    span: Span,
) {
    out.push((line, token));
    spans.push(span);
}

/// Lex the source code into line-numbered tokens and byte spans.
pub fn lex(src: &str) -> Result<Lexed, LexError> {
    let mut out = Vec::new();
    let mut spans = Vec::new();
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
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::AttrOpen,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'>' if index + 1 < buf.len()
                && buf[index + 1] == b'>'
                && index + 2 < buf.len()
                && buf[index + 2] == b'=' =>
            {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::ShrEq,
                    Span::new(start, start + 3),
                );
                index += 3;
            }
            b'<' if index + 1 < buf.len()
                && buf[index + 1] == b'<'
                && index + 2 < buf.len()
                && buf[index + 2] == b'=' =>
            {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::ShlEq,
                    Span::new(start, start + 3),
                );
                index += 3;
            }
            b'>' if index + 1 < buf.len() && buf[index + 1] == b'>' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Shr,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'<' if index + 1 < buf.len() && buf[index + 1] == b'<' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Shl,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'>' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Ge,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'<' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Le,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'=' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::EqEq,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'!' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Ne,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'&' if index + 1 < buf.len() && buf[index + 1] == b'&' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::AmpAmp,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'|' if index + 1 < buf.len() && buf[index + 1] == b'|' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::PipePipe,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'+' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::PlusEq,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'-' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::MinusEq,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'*' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::StarEq,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'/' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::SlashEq,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'%' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::PercentEq,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'&' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::AmpEq,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'|' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::PipeEq,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'^' if index + 1 < buf.len() && buf[index + 1] == b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::CaretEq,
                    Span::new(start, start + 2),
                );
                index += 2;
            }
            b'(' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::LParen,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b')' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::RParen,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'[' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::LBracket,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b']' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::RBracket,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'{' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::LBrace,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'}' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::RBrace,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b';' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Semi,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b',' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Comma,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'.' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Dot,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b':' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Colon,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'+' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Plus,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'-' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Minus,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'*' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Star,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'/' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Slash,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'%' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Percent,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'!' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Bang,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'~' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Tilde,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'&' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Amp,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'|' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Pipe,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'^' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Caret,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'<' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Lt,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'>' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Gt,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'=' => {
                let start = index;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::Eq,
                    Span::new(start, start + 1),
                );
                index += 1;
            }
            b'"' => {
                let start = index;
                let (s, ni) = lex_string(src, line, index)?;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::StringLit(s),
                    Span::new(start, ni),
                );
                index = ni;
            }
            b'b' if index + 1 < buf.len() && buf[index + 1] == b'"' => {
                let start = index;
                let (s, ni) = lex_string(src, line, index + 1)?;
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::BufferLit(s),
                    Span::new(start, ni),
                );
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
                            return Err(LexError::new(
                                line,
                                "empty hex int",
                                Span::new(start, index),
                            ));
                        }
                        push_token(
                            &mut out,
                            &mut spans,
                            line,
                            Token::IntLit(src[start..index].into()),
                            Span::new(start, index),
                        );
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
                            return Err(LexError::new(
                                line,
                                "empty binary int",
                                Span::new(start, index),
                            ));
                        }
                        push_token(
                            &mut out,
                            &mut spans,
                            line,
                            Token::IntLit(src[start..index].into()),
                            Span::new(start, index),
                        );
                        continue;
                    }
                }
                while index < buf.len() && (buf[index].is_ascii_digit() || buf[index] == b'_') {
                    index += 1;
                }
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    Token::IntLit(src[start..index].into()),
                    Span::new(start, index),
                );
            }
            c if is_ident_start(c) => {
                let start = index;
                index += 1;
                while index < buf.len() && is_ident_cont(buf[index]) {
                    index += 1;
                }
                let name = &src[start..index];
                push_token(
                    &mut out,
                    &mut spans,
                    line,
                    keyword_or_ident(name),
                    Span::new(start, index),
                );
            }
            _ => {
                return Err(LexError::new(
                    line,
                    "unexpected character",
                    Span::new(index, index + 1),
                ));
            }
        }
    }
    push_token(
        &mut out,
        &mut spans,
        line,
        Token::Eof,
        Span::empty(src.len()),
    );
    Ok(Lexed { tokens: out, spans })
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
        "declare" => Token::Declare,
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
                    return Err(LexError::new(
                        line,
                        "unterminated string",
                        Span::new(quote_at, b.len()),
                    ));
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
                            return Err(LexError::new(
                                line,
                                "invalid hex escape in string",
                                Span::new(index, b.len()),
                            ));
                        }
                        let h1 = hex_escape_digit(b[index + 2]).ok_or_else(|| {
                            LexError::new(
                                line,
                                "invalid hex escape in string",
                                Span::new(index, index + 4),
                            )
                        })?;
                        let h2 = hex_escape_digit(b[index + 3]).ok_or_else(|| {
                            LexError::new(
                                line,
                                "invalid hex escape in string",
                                Span::new(index, index + 4),
                            )
                        })?;
                        let byte = (h1 << 4) | h2;
                        out.push(char::from_u32(u32::from(byte)).unwrap());
                        index += 4;
                    }
                    _ => {
                        return Err(LexError::new(
                            line,
                            "invalid escape in string",
                            Span::new(index, index + 2),
                        ))
                    }
                }
            }
            c => {
                out.push(c as char);
                index += 1;
            }
        }
    }
    Err(LexError::new(
        line,
        "unterminated string",
        Span::new(quote_at, b.len()),
    ))
}
