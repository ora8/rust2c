//! Lexer: zerlegt den Quelltext in Tokens.

use std::fmt;

use crate::error::{CompileError, Result, Span};
use crate::types::IntTy;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// Ganzzahl-Literal mit optionalem Typsuffix (`10u8`).
    Int(u128, Option<IntTy>),
    Str(String),
    Ident(String),
    // Schlüsselwörter
    Fn,
    Let,
    Mut,
    If,
    Else,
    While,
    Loop,
    For,
    In,
    Break,
    Continue,
    Return,
    True,
    False,
    As,
    // Klammern und Trennzeichen
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Semi,
    Colon,
    Arrow,
    Dot,
    DotDot,
    DotDotEq,
    // Operatoren
    Bang,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    Pipe,
    Caret,
    Shl,
    Shr,
    AndAnd,
    OrOr,
    Eq,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,
    Eof,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Tok::Int(v, _) => return write!(f, "Zahl `{v}`"),
            Tok::Str(_) => return f.write_str("Zeichenkette"),
            Tok::Ident(n) => return write!(f, "Bezeichner `{n}`"),
            Tok::Eof => return f.write_str("Dateiende"),
            Tok::Fn => "fn",
            Tok::Let => "let",
            Tok::Mut => "mut",
            Tok::If => "if",
            Tok::Else => "else",
            Tok::While => "while",
            Tok::Loop => "loop",
            Tok::For => "for",
            Tok::In => "in",
            Tok::Break => "break",
            Tok::Continue => "continue",
            Tok::Return => "return",
            Tok::True => "true",
            Tok::False => "false",
            Tok::As => "as",
            Tok::LParen => "(",
            Tok::RParen => ")",
            Tok::LBrace => "{",
            Tok::RBrace => "}",
            Tok::LBracket => "[",
            Tok::RBracket => "]",
            Tok::Comma => ",",
            Tok::Semi => ";",
            Tok::Colon => ":",
            Tok::Arrow => "->",
            Tok::Dot => ".",
            Tok::DotDot => "..",
            Tok::DotDotEq => "..=",
            Tok::Bang => "!",
            Tok::Plus => "+",
            Tok::Minus => "-",
            Tok::Star => "*",
            Tok::Slash => "/",
            Tok::Percent => "%",
            Tok::Amp => "&",
            Tok::Pipe => "|",
            Tok::Caret => "^",
            Tok::Shl => "<<",
            Tok::Shr => ">>",
            Tok::AndAnd => "&&",
            Tok::OrOr => "||",
            Tok::Eq => "=",
            Tok::EqEq => "==",
            Tok::Ne => "!=",
            Tok::Lt => "<",
            Tok::Le => "<=",
            Tok::Gt => ">",
            Tok::Ge => ">=",
            Tok::PlusEq => "+=",
            Tok::MinusEq => "-=",
            Tok::StarEq => "*=",
            Tok::SlashEq => "/=",
            Tok::PercentEq => "%=",
            Tok::AmpEq => "&=",
            Tok::PipeEq => "|=",
            Tok::CaretEq => "^=",
            Tok::ShlEq => "<<=",
            Tok::ShrEq => ">>=",
        };
        write!(f, "`{s}`")
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// Zerlegt `src` in Tokens. Das letzte Token ist immer [`Tok::Eof`].
pub fn lex(src: &str) -> Result<Vec<Token>> {
    let mut lx = Lexer {
        chars: src.chars().collect(),
        pos: 0,
        line: 1,
        col: 1,
    };
    let mut tokens = Vec::new();
    loop {
        lx.skip_trivia()?;
        let span = lx.span();
        let Some(c) = lx.peek() else {
            tokens.push(Token {
                tok: Tok::Eof,
                span,
            });
            return Ok(tokens);
        };
        let tok = if c.is_ascii_digit() {
            lx.number()?
        } else if c.is_ascii_alphabetic() || c == '_' {
            lx.ident_or_keyword()
        } else if c == '"' {
            lx.string()?
        } else {
            lx.punct()?
        };
        tokens.push(Token { tok, span });
    }
}

struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    fn span(&self) -> Span {
        Span::new(self.line, self.col)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.chars.get(self.pos + n).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    /// Überspringt Leerraum sowie `//`- und (verschachtelte) `/* */`-Kommentare.
    fn skip_trivia(&mut self) -> Result<()> {
        loop {
            match (self.peek(), self.peek_at(1)) {
                (Some(c), _) if c.is_whitespace() => {
                    self.bump();
                }
                (Some('/'), Some('/')) => {
                    while let Some(c) = self.peek() {
                        if c == '\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                (Some('/'), Some('*')) => {
                    let start = self.span();
                    self.bump();
                    self.bump();
                    let mut depth = 1;
                    while depth > 0 {
                        match (self.peek(), self.peek_at(1)) {
                            (Some('/'), Some('*')) => {
                                self.bump();
                                self.bump();
                                depth += 1;
                            }
                            (Some('*'), Some('/')) => {
                                self.bump();
                                self.bump();
                                depth -= 1;
                            }
                            (Some(_), _) => {
                                self.bump();
                            }
                            (None, _) => {
                                return Err(CompileError::new(
                                    "nicht abgeschlossener Blockkommentar",
                                    start,
                                ));
                            }
                        }
                    }
                }
                _ => return Ok(()),
            }
        }
    }

    fn word(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                s.push(c);
                self.bump();
            } else {
                break;
            }
        }
        s
    }

    fn number(&mut self) -> Result<Tok> {
        let span = self.span();
        let radix = match (self.peek(), self.peek_at(1)) {
            (Some('0'), Some('x')) => 16,
            (Some('0'), Some('o')) => 8,
            (Some('0'), Some('b')) => 2,
            _ => 10,
        };
        if radix != 10 {
            self.bump();
            self.bump();
        }
        let mut value: u128 = 0;
        let mut digits = 0;
        while let Some(c) = self.peek() {
            if c == '_' {
                self.bump();
                continue;
            }
            let Some(d) = c.to_digit(radix) else { break };
            value = value
                .checked_mul(u128::from(radix))
                .and_then(|v| v.checked_add(u128::from(d)))
                .ok_or_else(|| CompileError::new("Zahl ist zu gross", span))?;
            digits += 1;
            self.bump();
        }
        if digits == 0 {
            return Err(CompileError::new("Ziffern erwartet", span));
        }
        let suffix = if self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            let s = self.word();
            match IntTy::from_name(&s) {
                Some(it) => Some(it),
                None => {
                    return Err(CompileError::new(
                        format!("unbekanntes Zahlensuffix `{s}`"),
                        span,
                    ));
                }
            }
        } else {
            None
        };
        Ok(Tok::Int(value, suffix))
    }

    fn ident_or_keyword(&mut self) -> Tok {
        let w = self.word();
        match w.as_str() {
            "fn" => Tok::Fn,
            "let" => Tok::Let,
            "mut" => Tok::Mut,
            "if" => Tok::If,
            "else" => Tok::Else,
            "while" => Tok::While,
            "loop" => Tok::Loop,
            "for" => Tok::For,
            "in" => Tok::In,
            "break" => Tok::Break,
            "continue" => Tok::Continue,
            "return" => Tok::Return,
            "true" => Tok::True,
            "false" => Tok::False,
            "as" => Tok::As,
            _ => Tok::Ident(w),
        }
    }

    fn string(&mut self) -> Result<Tok> {
        let start = self.span();
        self.bump(); // "
        let mut s = String::new();
        loop {
            let esc_span = self.span();
            match self.bump() {
                None => {
                    return Err(CompileError::new(
                        "nicht abgeschlossene Zeichenkette",
                        start,
                    ));
                }
                Some('"') => return Ok(Tok::Str(s)),
                Some('\\') => match self.bump() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('r') => s.push('\r'),
                    Some('0') => s.push('\0'),
                    Some('\\') => s.push('\\'),
                    Some('"') => s.push('"'),
                    Some('\'') => s.push('\''),
                    Some('\n') => {
                        // Zeilenfortsetzung: führenden Leerraum der nächsten Zeile überspringen
                        while self.peek().is_some_and(char::is_whitespace) {
                            self.bump();
                        }
                    }
                    Some(c) => {
                        return Err(CompileError::new(
                            format!("unbekannte Escape-Sequenz `\\{c}`"),
                            esc_span,
                        ));
                    }
                    None => {
                        return Err(CompileError::new(
                            "nicht abgeschlossene Zeichenkette",
                            start,
                        ));
                    }
                },
                Some(c) => s.push(c),
            }
        }
    }

    /// Liest ein Zeichen; folgt `second`, wird auch dieses gelesen und `yes` geliefert.
    fn pick(&mut self, second: char, yes: Tok, no: Tok) -> Tok {
        if self.peek() == Some(second) {
            self.bump();
            yes
        } else {
            no
        }
    }

    fn punct(&mut self) -> Result<Tok> {
        let span = self.span();
        let c = self
            .bump()
            .expect("punct wird nur vor einem Zeichen aufgerufen");
        let tok = match c {
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '{' => Tok::LBrace,
            '}' => Tok::RBrace,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            ',' => Tok::Comma,
            ';' => Tok::Semi,
            ':' => Tok::Colon,
            '.' => {
                if self.peek() == Some('.') {
                    self.bump();
                    self.pick('=', Tok::DotDotEq, Tok::DotDot)
                } else {
                    Tok::Dot
                }
            }
            '-' => {
                if self.peek() == Some('>') {
                    self.bump();
                    Tok::Arrow
                } else {
                    self.pick('=', Tok::MinusEq, Tok::Minus)
                }
            }
            '+' => self.pick('=', Tok::PlusEq, Tok::Plus),
            '*' => self.pick('=', Tok::StarEq, Tok::Star),
            '/' => self.pick('=', Tok::SlashEq, Tok::Slash),
            '%' => self.pick('=', Tok::PercentEq, Tok::Percent),
            '^' => self.pick('=', Tok::CaretEq, Tok::Caret),
            '!' => self.pick('=', Tok::Ne, Tok::Bang),
            '=' => self.pick('=', Tok::EqEq, Tok::Eq),
            '&' => {
                if self.peek() == Some('&') {
                    self.bump();
                    Tok::AndAnd
                } else {
                    self.pick('=', Tok::AmpEq, Tok::Amp)
                }
            }
            '|' => {
                if self.peek() == Some('|') {
                    self.bump();
                    Tok::OrOr
                } else {
                    self.pick('=', Tok::PipeEq, Tok::Pipe)
                }
            }
            '<' => {
                if self.peek() == Some('<') {
                    self.bump();
                    self.pick('=', Tok::ShlEq, Tok::Shl)
                } else {
                    self.pick('=', Tok::Le, Tok::Lt)
                }
            }
            '>' => {
                if self.peek() == Some('>') {
                    self.bump();
                    self.pick('=', Tok::ShrEq, Tok::Shr)
                } else {
                    self.pick('=', Tok::Ge, Tok::Gt)
                }
            }
            other => {
                return Err(CompileError::new(
                    format!("unerwartetes Zeichen `{other}`"),
                    span,
                ));
            }
        };
        Ok(tok)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        lex(src).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn operatoren_und_schluesselwoerter() {
        assert_eq!(
            toks("fn x -> a <<= b..=c && !d"),
            vec![
                Tok::Fn,
                Tok::Ident("x".into()),
                Tok::Arrow,
                Tok::Ident("a".into()),
                Tok::ShlEq,
                Tok::Ident("b".into()),
                Tok::DotDotEq,
                Tok::Ident("c".into()),
                Tok::AndAnd,
                Tok::Bang,
                Tok::Ident("d".into()),
                Tok::Eof,
            ]
        );
    }

    #[test]
    fn zahlen_mit_suffix_und_basis() {
        assert_eq!(
            toks("1_000 0xffu8 0b1010 7usize"),
            vec![
                Tok::Int(1000, None),
                Tok::Int(255, Some(IntTy::U8)),
                Tok::Int(10, None),
                Tok::Int(7, Some(IntTy::Usize)),
                Tok::Eof,
            ]
        );
        assert!(lex("12abc").is_err());
    }

    #[test]
    fn bereich_ist_keine_kommazahl() {
        assert_eq!(
            toks("0..10"),
            vec![Tok::Int(0, None), Tok::DotDot, Tok::Int(10, None), Tok::Eof]
        );
    }

    #[test]
    fn kommentare_und_zeichenketten() {
        assert_eq!(
            toks("/* a /* verschachtelt */ b */ \"x\\n\\\"y\" // Rest"),
            vec![Tok::Str("x\n\"y".into()), Tok::Eof]
        );
        assert!(lex("/* offen").is_err());
        assert!(lex("\"offen").is_err());
    }

    #[test]
    fn positionen() {
        let t = lex("fn\n  main").unwrap();
        assert_eq!(t[1].span, Span::new(2, 3));
    }
}
