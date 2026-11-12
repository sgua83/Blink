use crate::{
    ast::{Access,Diagnostic,ExternalName,FunctionKind,Result,Source,Span,Target},
    syntax::{self,wire,build::*},
    lexer::{self, Token},
};

// These discriminants are produced by bootstrap/parser.b through its C ABI.
#[repr(u32)]
#[allow(dead_code)]
enum LinkStringError {
    None,
    Escape,
    Ascii,
    Empty,
}
#[repr(C)]
struct LinkStringResult {
    count: usize,
    error: LinkStringError,
}
unsafe extern "C" {
    #[link_name = "bootstrap.parser::decode_link_string"]
    fn decode_link_string(source: *const u8, length: usize, output: *mut u8) -> LinkStringResult;
}


// A completed child is stored before it enters its parent. Construction drafts
// contain lexical fields and B indices, never a second recursive syntax tree.
macro_rules! store {
    ($parser:expr,$method:ident,$value:expr) => {{
        let value=$value;
        let result=$parser.storage.$method(value);
        $parser.storage_result(result)?
    }};
}
pub fn parse(source: Source) -> Result<syntax::Module> {
    Parser::new(source)?.module()
}
pub(crate) fn entry(source: Source) -> Result<(String, Option<syntax::List<wire::ArgumentNode>>)> {
    let mut parser=Parser::new(source)?;
    let name=parser.type_name()?;
    let arguments=if parser.peek()=="<" {Some(parser.generic_arguments()?)}else{None};
    parser.expect("")?;
    let name=parser.storage.read_text(name).to_owned();
    let arguments=match arguments {
        Some(range)=>Some(parser.storage.finish_list(range)),
        None=>None,
    };
    Ok((name,arguments))
}
pub(crate) fn type_query(source: Source) -> Result<syntax::Type> {
    let mut parser=Parser::new(source)?;
    let ty=parser.ty()?;
    parser.expect("")?;
        Ok(parser.storage.finish(ty.offset))
}
struct Parser {
    source:Source,
    tokens:Vec<Token>,
    at:usize,
    storage:syntax::Builder,
}
impl Parser {
    fn new(source:Source)->Result<Self> {
        let tokens=lexer::lex(&source)?;
        let storage=syntax::Builder::new(Some(source.clone())).map_err(|error|Diagnostic {
            source:source.clone(),span:Span::default(),code:"B386",message:error.to_string(),
        })?;
        Ok(Self{source,tokens,at:0,storage})
    }
    fn storage_result<T>(&self,result:std::result::Result<T,syntax::StorageFailure>)->Result<T> {
        result.map_err(|error|Diagnostic {source:self.source.clone(),span:Span::default(),code:"B386",message:error.to_string()})
    }
    fn expr_ref(&mut self,min:u8,type_argument:bool)->Result<wire::ExprRef> {
        self.invoke(|input,state,context|unsafe{b_expression(input,state,u32::from(min),u32::from(type_argument),literal,context)})
    }
    fn peek(&self) -> &str {
        self.tokens[self.at].text(&self.source)
    }
    fn span(&self) -> Span {
        self.tokens[self.at].span
    }
    fn take(&mut self, text: &str) -> bool {
        if self.peek() == text {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, text: &str) -> Result<()> {
        if self.take(text) {
            Ok(())
        } else {
            self.source.fail(
                self.span(),
                "B010",
                format!("需要 `{text}`，实际为 `{}`", self.peek()),
            )
        }
    }
    fn ident(&mut self) -> Result<String> {
        let t = self.tokens[self.at];
        if t.text(&self.source)
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        {
            self.at += 1;
            Ok(t.text(&self.source).to_owned())
        } else {
            self.source.fail(t.span, "B011", "需要标识符")
        }
    }
    fn link_string(&mut self) -> Result<String> {
        let span = self.span();
        let text = self.peek();
        if !text.starts_with('"') {
            return self
                .source
                .fail(span, "B263", "C ABI 与链接名称使用双引号字符串");
        }
        let input = &text.as_bytes()[1..text.len() - 1];
        // Each output byte consumes at least one input byte. B writes synchronously
        // into this caller-owned buffer and retains neither address.
        let mut output = Vec::<u8>::with_capacity(input.len());
        let result = unsafe {
            decode_link_string(input.as_ptr(), input.len(), output.as_mut_ptr())
        };
        let message = match result.error {
            LinkStringError::None => {
                // Successful B decoding initialized exactly count non-NUL ASCII bytes.
                unsafe { output.set_len(result.count); }
                self.at += 1;
                return Ok(unsafe { String::from_utf8_unchecked(output) });
            }
            LinkStringError::Escape => "链接字符串只支持 \\\\ 与 \\\" 转义",
            LinkStringError::Ascii => "PE 链接名称需要非 NUL 的 ASCII 字符",
            LinkStringError::Empty => "链接名称不能为空",
        };
        self.source.fail(span, "B263", message)
    }
    fn module(mut self) -> Result<syntax::Module> {
        self.expect("module")?;
        let name = self.qualified()?;
        self.expect(";")?;
        let mut imports = Vec::new();
        let mut records = Vec::new();
        let mut type_sets = Vec::new();
        let mut functions = Vec::new();
        while !self.peek().is_empty() {
            let span = self.span();
            if self.take("import") {
                imports.push(self.qualified()?);
                self.expect(";")?;
                continue;
            }
            if self.take("const") {
                let result = self.ty()?;
                let name = self.ident()?;
                self.expect("=")?;
                let value = self.expr_ref(0,false)?;
                self.expect(";")?;
                // const 初始化沿同一表达式与返回类型分析，不生成运行期函数入口。
                functions.push(store!(self,make_function,Function {
                    name,
                    types: wire::Slice::default(),
                    values: wire::Slice::default(),
                    params: Vec::new(),
                    result,
                    kind: FunctionKind::Constant,
                    target: Target::Compute,
                    contracts: Vec::new(),
                    body: Some(store!(self,refs,&[store!(self,make_return,ReturnStmt {span,value})])),
                    c_abi: false,
                    external: None,
                    signature_end: self.tokens[self.at - 1].span.end,
                    span: Span { start: span.start, end: self.tokens[self.at - 1].span.end },
                }));
                continue;
            }
            if self.take("typeset") {
                let name = self.ident()?;
                self.expect("{")?;
                let mut members = Vec::new();
                while !self.take("}") {
                    members.push(self.ty()?);
                    self.expect(";")?;
                }
                self.expect(";")?;
                type_sets.push(store!(self,make_type_set,TypeSet {
                    name,
                    members,
                    span: Span { start: span.start, end: self.tokens[self.at - 1].span.end },
                }));
                continue;
            }
            if self.take("enum") {
                let name = self.ident()?;
                let (types, values) = self.parameters()?;
                self.expect(":")?;
                let representation = self.ty()?;
                self.expect("{")?;
                let mut variants = Vec::new();
                let mut payloads = types.count!=0 || values.count!=0;
                while !self.take("}") {
                    let at = self.span();
                    let member = self.ident()?;
                    let payload = if self.take("(") {
                        payloads = true;
                        if self.take(")") { self.named_type("void")? }
                        else { let ty = self.ty()?; self.expect(")")?; ty }
                    } else { self.named_type("void")? };
                    self.expect("=")?;
                    let discriminant = self.expr_ref(0,false)?;
                    self.expect(";")?;
                    variants.push(VariantDeclaration { name: member, payload, discriminant, span: at });
                }
                self.expect(";")?;
                if !payloads {
                    for variant in &variants {
                        let span = variant.span;
                        functions.push(store!(self,make_function,Function {
                            name: format!("{name}::{}", variant.name),
                            types: wire::Slice::default(), values: wire::Slice::default(), params: Vec::new(),
                            result: self.named_type(&name)?, kind: FunctionKind::Constant,
                            target: Target::Compute, contracts: Vec::new(), c_abi: false, external: None, signature_end: span.end, span,
                            body: Some(store!(self,refs,&[store!(self,make_return,ReturnStmt { span, value:store!(self,make_expr,Expr {
                                span, kind: ExprKind::Call(name.clone(), vec![variant.discriminant]),
                            }) })])),
                        }));
                    }
                    variants.clear();
                }
                let mut stored_variants=Vec::with_capacity(variants.len());
                for variant in variants {stored_variants.push(store!(self,make_variant,variant));}
                records.push(store!(self,make_record,Record { name, types, values, stride: None,
                    enumeration: Some(representation), variants:stored_variants, fields: Vec::new(),
                    span: Span { start: span.start, end: self.tokens[self.at - 1].span.end } }));
                continue;
            }
            if self.take("struct") {
                let name = self.ident()?;
                let (types, values) = self.parameters()?;
                let stride = if self.take("stride") {
                    self.expect("(")?;
                    let value = self.shape_argument()?;
                    self.expect(")")?;
                    Some(value)
                } else {
                    None
                };
                self.expect("{")?;
                let mut fields = Vec::new();
                while !self.take("}") {
                    let span = self.span();
                    let ty = self.ty()?;
                    let name = self.ident()?;
                    self.expect(";")?;
                    fields.push(store!(self,make_field,Field { name, ty, span }));
                }
                self.expect(";")?;
                records.push(store!(self,make_record,Record {
                    name,
                    types,
                    values,
                    stride,
                    enumeration: None,
                    variants: Vec::new(),
                    fields,
                    span: Span { start: span.start, end: self.tokens[self.at - 1].span.end },
                }));
                continue;
            }
            let c_abi = if self.take("extern") {
                if self.link_string()? != "C" {
                    return self.source.fail(span, "B263", "外部函数使用 C ABI");
                }
                true
            } else {
                false
            };
            let kind = if self.take("cooperative") {
                FunctionKind::Cooperative
            } else {
                FunctionKind::Value
            };
            let result = self.ty()?;
            let name = self.ident()?;
            let (types, values) = self.parameters()?;
            self.expect("(")?;
            let mut params:Vec<wire::ParamRef> = Vec::new();
            if !self.take(")") {
                loop {
                    let span = self.span();
                    let access = if self.take("read") {
                        Access::Read
                    } else if self.take("write") {
                        Access::Write
                    } else if self.take("inout") {
                        Access::InOut
                    } else if self.take("atomic") {
                        Access::Atomic
                    } else {
                        Access::Value
                    };
                    let ty = self.ty()?;
                    let name = self.ident()?;
                    if params.iter().any(|p| self.storage.read_text(self.storage.read::<wire::ParamNode>(p.offset).name)==name) {
                        return self.source.fail(span, "B100", format!("重复参数 {name}"));
                    }
                    params.push(store!(self,make_param,Param {
                        name,
                        ty,
                        access,
                        span,
                    }));
                    if self.take(")") {
                        break;
                    }
                    self.expect(",")?;
                }
            }
            let target = if self.take("on") {
                match self.ident()?.as_str() {
                    "cpu" => Target::Cpu,
                    "gpu" => Target::Gpu,
                    "compute" => Target::Compute,
                    _ => {
                        return self.source.fail(
                            span,
                            "B012",
                            "执行目标只能为 cpu、gpu 或 compute",
                        );
                    }
                }
            } else {
                Target::Compute
            };
            let mut contracts = Vec::new();
            while matches!(self.peek(), "requires" | "ensures") {
                contracts.push(self.contract()?);
            }
            let external = if c_abi && self.peek() == "from" {
                if types.count!=0 || values.count!=0 {
                    return self.source.fail(
                        span,
                        "B263",
                        "extern C 的链接符号需要具体类型，泛型包装写为普通 B 函数",
                    );
                }
                self.expect("from")?;
                let library = self.link_string()?;
                let symbol = if self.take("as") {
                    self.link_string()?
                } else {
                    name.clone()
                };
                Some(ExternalName { library, symbol })
            } else {
                None
            };
            let signature_end = self.span().start;
            let body = if self.take(";") {
                None
            } else {
                Some(self.block()?)
            };
            functions.push(store!(self,make_function,Function {
                name,
                types,
                values,
                params,
                result,
                kind,
                target,
                contracts,
                body,
                c_abi,
                external,
                signature_end,
                span: Span { start: span.start, end: self.tokens[self.at - 1].span.end },
            }));
        }
        let module=store!(self,make_module,Module {
            name,
            imports,
            records,
            type_sets,
            functions,
        });
        Ok(syntax::Module{source:self.source,root:self.storage.finish(module.offset)})
    }
    fn block(&mut self)->Result<wire::Slice> {
        self.invoke(|input,state,context|unsafe{b_block(input,state,literal,context)})
    }
}

#[repr(C)]
#[derive(Clone,Copy)]
struct ParseError{kind:u32,detail:u32,storage:wire::StorageError,cleanup:wire::StorageError,span:wire::Span,expected:wire::Text}
#[repr(C)]
struct ParseInputs{source:*const u8,tokens:*mut Token,count:u64,storage:*mut std::ffi::c_void}
#[repr(C)]
struct ParseState{at:u64,error:ParseError}
#[repr(C)]
struct LiteralResult{expression:wire::ExprNode,error:ParseError}
#[repr(C)]
struct ParsedParameters{types:wire::Slice,values:wire::Slice}
type Literal=unsafe extern "C" fn(*mut std::ffi::c_void,wire::Span)->LiteralResult;
unsafe extern "C"{
 #[link_name="bootstrap.parser::parse_block"]fn b_block(input:*const ParseInputs,state:*mut ParseState,literal:Literal,context:*mut std::ffi::c_void)->wire::Slice;
 #[link_name="bootstrap.parser::parse_qualified"]fn b_qualified(input:*const ParseInputs,state:*mut ParseState)->wire::Text;
 #[link_name="bootstrap.parser::parse_type_name"]fn b_type_name(input:*const ParseInputs,state:*mut ParseState)->wire::Text;
 #[link_name="bootstrap.parser::parse_expression"]fn b_expression(input:*const ParseInputs,state:*mut ParseState,minimum:u32,type_argument:u32,literal:Literal,context:*mut std::ffi::c_void)->wire::ExprRef;
 #[link_name="bootstrap.parser::parse_type"]fn b_type(input:*const ParseInputs,state:*mut ParseState,literal:Literal,context:*mut std::ffi::c_void)->wire::TypeRef;
 #[link_name="bootstrap.parser::parse_shape"]fn b_shape(input:*const ParseInputs,state:*mut ParseState,literal:Literal,context:*mut std::ffi::c_void)->wire::ShapeRef;
 #[link_name="bootstrap.parser::parse_contract"]fn b_contract(input:*const ParseInputs,state:*mut ParseState,literal:Literal,context:*mut std::ffi::c_void)->wire::ContractRef;
 #[link_name="bootstrap.parser::parse_parameters"]fn b_parameters(input:*const ParseInputs,state:*mut ParseState,literal:Literal,context:*mut std::ffi::c_void)->ParsedParameters;
 #[link_name="bootstrap.parser::parse_generic_arguments"]fn b_arguments(input:*const ParseInputs,state:*mut ParseState,literal:Literal,context:*mut std::ffi::c_void)->wire::Slice;
}
fn no_error()->ParseError{ParseError{kind:0,detail:0,storage:wire::StorageError{kind:0,native_code:0},cleanup:wire::StorageError{kind:0,native_code:0},span:wire::Span{start:0,end:0},expected:wire::Text{kind:0,range:wire::Span{start:0,end:0}}}}
// This synchronous callback is only the original ordinary text algorithm. B
// owns grammar selection and every completed node in the original store.
struct LiteralContext<'a>{source:&'a Source,storage:*mut syntax::Builder,error:Option<Diagnostic>}
unsafe extern "C" fn literal(context:*mut std::ffi::c_void,span:wire::Span)->LiteralResult{unsafe{
 let context=&mut *context.cast::<LiteralContext<'_>>();
 let span=Span{start:span.start as usize,end:span.end as usize};let storage=&mut *context.storage;
 let result=literal_expression(context.source,span,storage).and_then(|value|storage.parser_expr_row(value).map_err(|error|literal_failure(context.source,error)));
 match result{
  Ok(expression)=>LiteralResult{expression,error:no_error()},
  Err(error)=>{let mut record=no_error();record.kind=error.code[1..].parse().unwrap();record.span=wire::Span{start:error.span.start as u64,end:error.span.end as u64};context.error=Some(error);LiteralResult{expression:wire::ExprNode{kind:wire::ExprKind::Bool(0),span:record.span},error:record}}
 }
}}
impl Parser{
 fn inputs(&mut self)->ParseInputs{ParseInputs{source:self.source.text.as_ptr(),tokens:self.tokens.as_mut_ptr(),count:self.tokens.len()as u64,storage:self.storage.raw_for_parser()}}
 fn invoke<T>(&mut self,call:impl FnOnce(&ParseInputs,&mut ParseState,*mut std::ffi::c_void)->T)->Result<T>{
  let input=self.inputs();let mut state=ParseState{at:self.at as u64,error:no_error()};
  let mut context=LiteralContext{source:&self.source,storage:&mut self.storage,error:None};
  let value=call(&input,&mut state,(&mut context as *mut LiteralContext<'_>).cast());
  self.at=state.at as usize;
  if state.error.kind!=0{return Err(parse_error(&state,&self.source,&self.storage,context.error.take()));}
  Ok(value)
 }
 fn qualified(&mut self)->Result<wire::Text>{self.invoke(|input,state,_|unsafe{b_qualified(input,state)})}
 fn type_name(&mut self)->Result<wire::Text>{self.invoke(|input,state,_|unsafe{b_type_name(input,state)})}
 fn ty(&mut self)->Result<wire::TypeRef>{self.invoke(|input,state,context|unsafe{b_type(input,state,literal,context)})}
 fn shape_argument(&mut self)->Result<wire::ShapeRef>{self.invoke(|input,state,context|unsafe{b_shape(input,state,literal,context)})}
 fn contract(&mut self)->Result<wire::ContractRef>{self.invoke(|input,state,context|unsafe{b_contract(input,state,literal,context)})}
 fn parameters(&mut self)->Result<(wire::Slice,wire::Slice)>{let value=self.invoke(|input,state,context|unsafe{b_parameters(input,state,literal,context)})?;Ok((value.types,value.values))}
 fn generic_arguments(&mut self)->Result<wire::Slice>{self.invoke(|input,state,context|unsafe{b_arguments(input,state,literal,context)})}
 fn named_type(&mut self,name:&str)->Result<wire::TypeRef>{Ok(store!(self,make_type,TypeName::named(name)))}
}

fn parse_error(state:&ParseState,source:&Source,builder:&syntax::Builder,host:Option<Diagnostic>)->Diagnostic{
 if let Some(mut error)=host{if state.error.cleanup.kind!=0{error.message.push_str(&format!("; cleanup {}",syntax::StorageFailure{kind:state.error.cleanup.kind,native_code:state.error.cleanup.native_code}));}return error;}
 let e=state.error;let span=Span{start:e.span.start as usize,end:e.span.end as usize};
 let(code,message)=match e.kind{
  13=>("B013","代码块没有结束".into()),
  10=>("B010",format!("需要 `{}`，实际为 `{}`",builder.read_text(e.expected),&source.text[span.start..span.end])),
  11=>("B011","需要标识符".into()),14=>("B014","命名空间限定需要命名路径".into()),
  225=>("B225",match e.detail{1=>"cleanup条件使用纯输入快照与资源长度",2=>"memory使用none、read、write或inout",3=>"initialized 条件使用输入值、returned、字段、布尔和纯数值运算；不执行调用或存储访问",4=>"range端点使用类型化编译期常量及纯数值运算",5=>"未知接口约束；使用 prefix、nonzero、nonempty、index、range、contiguous、same_length、cleanup、memory 或 initialized",6=>"cfn合同使用requires nonzero、ensures initialized及ensures memory",_=>panic!("unknown predicate detail {}",e.detail)}.into()),
  231=>("B231","重复的泛型参数".into()),232=>("B232","资源包装不能直接嵌套；子资源通过记录字段表达".into()),
  235=>("B235",match e.detail{1=>"编译期值参数当前只支持 u32",2=>"const 参数值域当前使用直接 u32 字面量清单",3=>"类型参数必须写在 const 参数之前",_=>panic!("unknown parameter detail")}.into()),
  266=>("B266","target 的实际执行位置为 cpu 或 gpu".into()),
  386=>("B386",{let primary=if e.storage.kind!=0{e.storage}else{e.cleanup};let mut message=syntax::StorageFailure{kind:primary.kind,native_code:primary.native_code}.to_string();if e.storage.kind!=0&&e.cleanup.kind!=0{message.push_str(&format!("; cleanup {}",syntax::StorageFailure{kind:e.cleanup.kind,native_code:e.cleanup.native_code}));}message}),
  _=>panic!("unknown parse error {}",e.kind),
 };let mut message=message;if e.kind!=386&&e.cleanup.kind!=0{message.push_str(&format!("; cleanup {}",syntax::StorageFailure{kind:e.cleanup.kind,native_code:e.cleanup.native_code}));}Diagnostic{source:source.clone(),span,code,message}
}

macro_rules! literal_store {($storage:expr,$source:expr,$method:ident,$value:expr)=>{{
    let value=$value;let result=$storage.$method(value);
    result.map_err(|error|literal_failure($source,error))?
}};}
fn literal_failure(source:&Source,error:syntax::StorageFailure)->Diagnostic {
    Diagnostic{source:source.clone(),span:Span::default(),code:"B386",message:error.to_string()}
}
fn literal_number(storage:&mut syntax::Builder,source:&Source,value:String,span:Span)->Result<wire::ExprRef>{
    Ok(literal_store!(storage,source,make_expr,Expr{kind:ExprKind::Number(value),span}))
}
pub(crate) fn literal_expression(source:&Source,span:Span,storage:&mut syntax::Builder)->Result<Expr>{
Ok({

            let text = &source.text[span.start..span.end];
            let mut chars = text[1..text.len() - 1].chars();
            let mut value = String::new();
            while let Some(c) = chars.next() {
                value.push(if c == '\\' {
                    match chars.next() {
                        Some('\\') => '\\',
                        Some('"') => '"',
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('0') => '\0',
                        Some('u') => {
                            if chars.next() != Some('{') {
                                return source.fail(span, "B385", "Unicode 转义使用 \\u{十六进制标量}");
                            }
                            let Some((digits, rest)) = chars.as_str().split_once('}') else {
                                return source.fail(span, "B385", "Unicode 转义缺少 }");
                            };
                            let Some(scalar) = u32::from_str_radix(digits, 16).ok()
                                .filter(|_| digits.bytes().all(|c| c.is_ascii_hexdigit()))
                                .and_then(char::from_u32) else {
                                return source.fail(span, "B385", "Unicode 转义必须是有效的十六进制 Unicode 标量");
                            };
                            chars = rest.chars();
                            scalar
                        }
                        _ => return source.fail(span, "B385", "文本转义支持 \\\\、\\\"、\\n、\\r、\\t、\\0 和 \\u{标量}"),
                    }
                } else { c });
            }
            let mut ty = TypeName::named("array");
            let byte_type=literal_store!(storage,source,make_argument,GenericArgument::Type(TypeName::named("u8")));
            let length=literal_number(storage,source,value.len().to_string(),span)?;
            let count=literal_store!(storage,source,make_argument,GenericArgument::Value(length));
            ty.arguments=vec![byte_type,count];
            let mut values=Vec::with_capacity(value.len());
            for byte in value.bytes() {
                let number=literal_number(storage,source,byte.to_string(),span)?;
                values.push(literal_store!(storage,source,make_expr,Expr {
                    kind:ExprKind::Construct(TypeName::named("u8"),vec![number]),span,
                }));
            }
            Expr { kind: ExprKind::Construct(ty, values), span }

})
}
