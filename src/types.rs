//! Typen der Quellsprache und ihre Abbildung auf C.

use std::fmt;

/// Ganzzahltypen wie in Rust. `isize`/`usize` werden als 64 Bit behandelt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    Isize,
    U8,
    U16,
    U32,
    U64,
    Usize,
}

impl IntTy {
    pub const ALL: [IntTy; 10] = [
        IntTy::I8,
        IntTy::I16,
        IntTy::I32,
        IntTy::I64,
        IntTy::Isize,
        IntTy::U8,
        IntTy::U16,
        IntTy::U32,
        IntTy::U64,
        IntTy::Usize,
    ];

    pub fn from_name(name: &str) -> Option<IntTy> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            IntTy::I8 => "i8",
            IntTy::I16 => "i16",
            IntTy::I32 => "i32",
            IntTy::I64 => "i64",
            IntTy::Isize => "isize",
            IntTy::U8 => "u8",
            IntTy::U16 => "u16",
            IntTy::U32 => "u32",
            IntTy::U64 => "u64",
            IntTy::Usize => "usize",
        }
    }

    pub fn is_signed(self) -> bool {
        matches!(
            self,
            IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::Isize
        )
    }

    pub fn bits(self) -> u32 {
        match self {
            IntTy::I8 | IntTy::U8 => 8,
            IntTy::I16 | IntTy::U16 => 16,
            IntTy::I32 | IntTy::U32 => 32,
            IntTy::I64 | IntTy::U64 | IntTy::Isize | IntTy::Usize => 64,
        }
    }

    /// Grösster darstellbarer Wert.
    pub fn max_value(self) -> u128 {
        if self.is_signed() {
            (1u128 << (self.bits() - 1)) - 1
        } else {
            (1u128 << self.bits()) - 1
        }
    }

    /// Betrag des kleinsten darstellbaren Werts (0 bei vorzeichenlosen Typen).
    pub fn min_magnitude(self) -> u128 {
        if self.is_signed() {
            1u128 << (self.bits() - 1)
        } else {
            0
        }
    }

    pub fn c_type(self) -> &'static str {
        match self {
            IntTy::I8 => "int8_t",
            IntTy::I16 => "int16_t",
            IntTy::I32 => "int32_t",
            IntTy::I64 | IntTy::Isize => "int64_t",
            IntTy::U8 => "uint8_t",
            IntTy::U16 => "uint16_t",
            IntTy::U32 => "uint32_t",
            IntTy::U64 | IntTy::Usize => "uint64_t",
        }
    }

    /// Vorzeichenloser C-Typ gleicher Breite (für Linksverschiebungen).
    pub fn c_unsigned(self) -> &'static str {
        match self.bits() {
            8 => "uint8_t",
            16 => "uint16_t",
            32 => "uint32_t",
            _ => "uint64_t",
        }
    }

    /// C-Makro für den kleinsten Wert eines vorzeichenbehafteten Typs.
    pub fn c_min(self) -> &'static str {
        match self.bits() {
            8 => "INT8_MIN",
            16 => "INT16_MIN",
            32 => "INT32_MIN",
            _ => "INT64_MIN",
        }
    }
}

impl fmt::Display for IntTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int(IntTy),
    Bool,
    /// `()`
    Unit,
    /// `!` – Ausdrücke, die nie einen Wert liefern (`return`, `break`, ...).
    Never,
    /// `[T; N]`
    Array(Box<Type>, u64),
}

impl Type {
    /// Ganzzahl oder `bool`.
    pub fn is_scalar(&self) -> bool {
        matches!(self, Type::Int(_) | Type::Bool)
    }

    /// `!` lässt sich in jeden Typ umwandeln.
    pub fn coerces_to(&self, target: &Type) -> bool {
        self == target || *self == Type::Never
    }

    pub fn as_int(&self) -> Option<IntTy> {
        match self {
            Type::Int(it) => Some(*it),
            _ => None,
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Int(it) => write!(f, "{it}"),
            Type::Bool => f.write_str("bool"),
            Type::Unit => f.write_str("()"),
            Type::Never => f.write_str("!"),
            Type::Array(elem, n) => write!(f, "[{elem}; {n}]"),
        }
    }
}
