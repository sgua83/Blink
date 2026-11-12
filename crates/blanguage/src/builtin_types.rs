use crate::ir::Scalar;

// These discriminants are produced by the B implementation of the static C ABI.
#[allow(dead_code)]
#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind { Missing = 0, Numeric = 1, Address = 2, Handle = 3, RayHit = 4 }

#[repr(u32)]
#[derive(Clone, Copy)]
pub(crate) enum ScalarCode { F32 = 0, U32 = 1, I32 = 2, Bool = 3, U64 = 4, I64 = 5, F64 = 6, U8 = 7, I8 = 8, U16 = 9, I16 = 10 }


impl From<Scalar> for ScalarCode {
    fn from(scalar: Scalar) -> Self {
        match scalar {
            Scalar::F32 => Self::F32, Scalar::U32 => Self::U32, Scalar::I32 => Self::I32,
            Scalar::Bool => Self::Bool, Scalar::U64 => Self::U64, Scalar::I64 => Self::I64,
            Scalar::F64 => Self::F64, Scalar::U8 => Self::U8, Scalar::I8 => Self::I8,
            Scalar::U16 => Self::U16, Scalar::I16 => Self::I16,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Builtin {
    pub kind: Kind,
    scalar: ScalarCode,
    pub rows: u32,
    pub columns: u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.type_families::recognize"]
    fn recognize(name: *const u8, length: u64) -> Builtin;
}
impl Builtin {
    pub fn recognize(name: &str) -> Self { unsafe { recognize(name.as_ptr(), name.len() as u64) } }
    pub fn is_present(self) -> bool { self.kind != Kind::Missing }
    pub fn is_scalar(self) -> bool {
        self.kind == Kind::Address || (self.kind == Kind::Numeric && self.rows == 1 && self.columns == 1)
    }
    pub fn scalar(self) -> Scalar {
        match self.scalar {
            ScalarCode::F32 => Scalar::F32,
            ScalarCode::U32 => Scalar::U32,
            ScalarCode::I32 => Scalar::I32,
            ScalarCode::Bool => Scalar::Bool,
            ScalarCode::U64 => Scalar::U64,
            ScalarCode::I64 => Scalar::I64,
            ScalarCode::F64 => Scalar::F64,
            ScalarCode::U8 => Scalar::U8,
            ScalarCode::I8 => Scalar::I8,
            ScalarCode::U16 => Scalar::U16,
            ScalarCode::I16 => Scalar::I16,
        }
    }
    pub fn canonical(self, name: &str) -> Option<&str> {
        if self.kind == Kind::Missing { None }
        else if self.kind == Kind::Numeric && self.rows == 1 && self.columns == 1 { Some(self.scalar().name()) }
        else { Some(name) }
    }
}
