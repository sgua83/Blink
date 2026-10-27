use crate::ast::*;

#[derive(Clone, Debug)]
pub struct Token {
    pub text: String,
    pub span: Span,
}
pub fn lex(source: &Source) -> Result<Vec<Token>> {
    let bytes = source.text.as_bytes();
    let mut at = 0;
    let mut out = Vec::new();
    while at < bytes.len() {
        let start = at;
        let c = bytes[at];
        if c.is_ascii_whitespace() {
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"//") {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            at += 2;
            while at + 1 < bytes.len() && !bytes[at..].starts_with(b"*/") {
                at += 1;
            }
            if at + 1 >= bytes.len() {
                return source.fail(Span { start, end: at }, "B001", "块注释没有结束");
            }
            at += 2;
            continue;
        }
        if c == b'"' {
            at += 1;
            while at < bytes.len() && bytes[at] != b'"' {
                if bytes[at] == b'\\' {
                    at += 1;
                }
                at += 1;
            }
            if at >= bytes.len() {
                return source.fail(
                    Span {
                        start,
                        end: bytes.len(),
                    },
                    "B003",
                    "链接字符串没有结束",
                );
            }
            at += 1;
        } else if c.is_ascii_alphabetic() || c == b'_' {
            at += 1;
            while at < bytes.len() && (bytes[at].is_ascii_alphanumeric() || bytes[at] == b'_') {
                at += 1;
            }
        } else if c.is_ascii_digit()
            || (c == b'.' && bytes.get(at + 1).is_some_and(u8::is_ascii_digit))
        {
            let hexadecimal = bytes[start..].starts_with(b"0x");
            at += 1;
            while at < bytes.len() {
                let x = bytes[at];
                if x.is_ascii_alphanumeric()
                    || (!hexadecimal && x == b'.' && !bytes[at..].starts_with(b".."))
                    || (!hexadecimal
                        && (x == b'+' || x == b'-')
                        && matches!(bytes[at - 1], b'e' | b'E'))
                {
                    at += 1;
                } else {
                    break;
                }
            }
        } else if let Some(op) = [
            "<<=", ">>=", "::", "..", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "==", "!=",
            "<=", ">=", "&&", "||", "<<", ">>",
        ]
        .iter()
        .find(|op| bytes[at..].starts_with(op.as_bytes()))
        {
            at += op.len();
        } else if b"{}()[];,.?:+-*/%!=<>&|^~".contains(&c) {
            at += 1;
        } else {
            return source.fail(
                Span {
                    start,
                    end: start + 1,
                },
                "B002",
                "不支持的源字符；标识符使用 ASCII，中文可写在注释中",
            );
        }
        out.push(Token {
            text: source.text[start..at].into(),
            span: Span { start, end: at },
        });
    }
    out.push(Token {
        text: String::new(),
        span: Span { start: at, end: at },
    });
    Ok(out)
}
