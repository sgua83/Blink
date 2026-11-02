use crate::ast::*;

#[repr(transparent)]
#[derive(Clone, Copy, Debug)]
pub struct Token {
    pub span: Span,
}
impl Token {
    pub fn text<'a>(&self, source: &'a Source) -> &'a str {
        &source.text[self.span.start..self.span.end]
    }
}

// These discriminants are produced by bootstrap/lexer.b through its C ABI.
#[repr(u32)]
#[allow(dead_code)]
enum ScanError {
    None,
    BlockComment,
    SourceCharacter,
    QuotedString,
    RemovedProgram,
}
#[repr(C)]
struct ScanResult {
    count: usize,
    start: usize,
    end: usize,
    error: ScanError,
}
unsafe extern "C" {
    #[link_name = "bootstrap.lexer::scan"]
    fn scan(source: *const u8, length: usize, tokens: *mut Token) -> ScanResult;
}
pub fn lex(source: &Source) -> Result<Vec<Token>> {
    // Every non-EOF token consumes at least one byte. B writes into this
    // caller-owned storage synchronously and retains neither address.
    let mut out = Vec::<Token>::with_capacity(source.text.len() + 1);
    let result = unsafe { scan(source.text.as_ptr(), source.text.len(), out.as_mut_ptr()) };
    let span = Span { start: result.start, end: result.end };
    let (code, message) = match result.error {
        ScanError::None => {
            // The scanner initialized exactly count C-layout spans, including EOF.
            unsafe { out.set_len(result.count); }
            return Ok(out);
        }
        ScanError::BlockComment => ("B001", "块注释没有结束"),
        ScanError::SourceCharacter => ("B002", "不支持的源字符；标识符使用 ASCII，中文可写在文本字面量或注释中"),
        ScanError::QuotedString => ("B003", "字符串没有结束"),
        ScanError::RemovedProgram => ("B004", "`program` 已从源语言移除，不能用作声明关键字或标识符；请用返回类型定义函数（无返回值用 `void`），并用 `parallel` 表达并行域"),
    };
    source.fail(span, code, message)
}
