//! 运算类型、字面量转换与形状由声明检查和原生降低共用。
use super::*;

pub(super) fn same(a: &Ty, b: &Ty, source: &Source, span: Span) -> Result<()> {
    if a == b { Ok(()) } else {
        source.fail(span, "B124", format!("类型不匹配：需要 {}，实际为 {}", a.name, b.name))
    }
}
pub(super) fn negative_number(source: &Source, span: Span, text: &str) -> Result<(Ty, Bits)> {
    let (ty, bits) = number(source, span, text)?;
    let (scalar, bits) = match ty.components[0].0 {
        Scalar::F32 => (Scalar::F32, bits ^ 0x80000000),
        Scalar::F64 => (Scalar::F64, bits ^ 0x8000000000000000),
        Scalar::U32 if bits <= 0x80000000 =>
            (Scalar::I32, Scalar::I32.truncate(0u64.wrapping_sub(bits))),
        Scalar::U64 if bits <= 0x8000000000000000 =>
            (Scalar::I64, 0u64.wrapping_sub(bits)),
        _ => return source.fail(span, "B180", "负整数字面量超出其有符号类型范围"),
    };
    Ok((Ty::scalar(scalar), bits))
}
pub(super) struct Numeric {
    pub scalar: Scalar,
    pub cast_left: bool,
    pub cast_right: bool,
}
pub(super) fn numeric(a: &Ty, a_literal: bool, b: &Ty, b_literal: bool,
    source: &Source, span: Span) -> Result<Numeric> {
    let left = a.components[0].0;
    let right = b.components[0].0;
    let (scalar, cast_left, cast_right) = if left == right { (left, false, false) }
        else if a_literal && b_literal {
            (if left == Scalar::F32 || right == Scalar::F32 { Scalar::F32 } else { Scalar::I32 }, true, true)
        } else if a_literal && right != Scalar::Bool { (right, true, false) }
        else if b_literal && left != Scalar::Bool { (left, false, true) }
        else { return source.fail(span, "B148", "不同数值类型必须显式转换"); };
    if a.components.len() != b.components.len() && a.components.len() != 1 && b.components.len() != 1 {
        return source.fail(span, "B149", "向量维度不匹配");
    }
    Ok(Numeric { scalar, cast_left, cast_right })
}
pub(super) enum Binary {
    Identity,
    Handle,
    Matrix(matrix::Binary),
    Numeric { operands: Numeric, comparison: bool, operation: Option<BinaryOp> },
}
pub(super) fn binary(op: &str, a: &Ty, a_literal: bool, b: &Ty, b_literal: bool,
    source: &Source, span: Span) -> Result<(Ty, Binary)> {
    if a.is_c_function() || b.is_c_function() || a.is_address() || b.is_address()
        || a.is_handle() || b.is_handle() {
        same(a, b, source, span)?;
        if !matches!(op, "==" | "!=") {
            let message = if a.is_c_function() {
                "C函数地址只比较同签名的完整身份，不参与数值运算或次序比较"
            } else if a.is_address() {
                "address只比较完整地址身份；数值运算须显式取得address_bits"
            } else { "handle 只支持完整身份的相等或不等比较" };
            return source.fail(span, "B147", message);
        }
        return Ok((Ty::scalar(Scalar::Bool),
            if a.is_handle() { Binary::Handle } else { Binary::Identity }));
    }
    if a.has_variants() || b.has_variants() {
        return source.fail(span, "B269", "结果值的运算先通过 match 或 try 取得对应载荷");
    }
    let comparison = matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=");
    if a.enumeration || b.enumeration {
        same(a, b, source, span)?;
        if !comparison {
            return source.fail(span, "B268", "编号枚举只直接比较同类型值；数值运算先显式取得声明的整数表示");
        }
    }
    if a.matrix_shape().is_some() || b.matrix_shape().is_some() {
        let (ty, kind) = matrix::binary_type(op, a, a_literal, b, b_literal, source, span)?;
        return Ok((ty, Binary::Matrix(kind)));
    }
    if a.array.is_some() || b.array.is_some() || !a.fields.is_empty() || !b.fields.is_empty()
        || a.contains_views() || b.contains_views() {
        return source.fail(span, "B147", "结构体和资源视图不能作为数值进行运算");
    }
    let operands = numeric(a, a_literal, b, b_literal, source, span)?;
    let scalar = operands.scalar;
    if scalar == Scalar::Bool && !matches!(op, "==" | "!=") {
        return source.fail(span, "B150", "bool 只支持逻辑或相等运算");
    }
    if scalar.is_float() && matches!(op, "%" | "&" | "|" | "^" | "<<" | ">>") {
        return source.fail(span, "B151", "位运算与取余需要整数");
    }
    let operation = if matches!(op, "/" | "%") && !scalar.is_float() { None } else {
        Some(BinaryOp::from_source(op, scalar).ok_or_else(|| Diagnostic {
            source: source.clone(), span, code: "B178", message: format!("未知二元运算 {op}"),
        })?)
    };
    Ok((Ty::vector(if comparison { Scalar::Bool } else { scalar },
        a.components.len().max(b.components.len())), Binary::Numeric { operands, comparison, operation }))
}
pub(super) fn unary(op: &str, ty: &Ty, source: &Source, span: Span) -> Result<Option<UnaryOp>> {
    if op == "+" {
        if ty.is_c_function() || ty.is_handle() || ty.is_address() || ty.array.is_some()
            || ty.enumeration || ty.has_variants() {
            return source.fail(span, "B146", "数值一元运算需要数值类型");
        }
        return Ok(None);
    }
    if op == "!" { same(&Ty::scalar(Scalar::Bool), ty, source, span)?; }
    else if !ty.is_numeric() && !(op == "-" && ty.matrix_shape().is_some()) {
        return source.fail(span, "B146", "数值一元运算需要数值类型");
    }
    let scalar = ty.components[0].0;
    let operation = match op {
        "-" if scalar.is_float() => UnaryOp::FloatNegate,
        "-" if scalar.is_signed() => UnaryOp::SignedNegate,
        "-" => UnaryOp::UnsignedNegate,
        "!" => UnaryOp::LogicalNot,
        "~" if !scalar.is_float() => UnaryOp::BitNot,
        _ => return source.fail(span, "B177", "当前类型不支持该一元运算"),
    };
    Ok(Some(operation))
}
