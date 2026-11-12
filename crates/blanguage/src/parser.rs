use crate::{
    ast::{Diagnostic,Result,Source,Span},
    syntax::{self,wire},
    lexer::{self,Token},
};

pub fn parse(source:Source)->Result<syntax::Module>{
    let mut host=ParseHost::new(source)?;
    let module=host.invoke(|input,state|unsafe{b_module(input,state)})?;
    Ok(syntax::Module{source:host.source,root:host.storage.finish(module.offset)})
}
pub(crate) fn entry(source:Source)->Result<(String,Option<syntax::List<wire::ArgumentNode>>)>{
    let mut host=ParseHost::new(source)?;
    let entry=host.invoke(|input,state|unsafe{b_entry(input,state)})?;
    let name=host.storage.read_text(entry.name).to_owned();
    let arguments=if entry.arguments.present!=0{Some(host.storage.finish_list(entry.arguments.value))}else{None};
    Ok((name,arguments))
}
pub(crate) fn type_query(source:Source)->Result<syntax::Type>{
    let mut host=ParseHost::new(source)?;
    let ty=host.invoke(|input,state|unsafe{b_type_query(input,state)})?;
    Ok(host.storage.finish(ty.offset))
}
// The host owns Source, lexer tokens and the unpublished B store for one call.
// All cursor movement and syntax production belong to the B parser.
struct ParseHost{source:Source,tokens:Vec<Token>,storage:syntax::Builder}
impl ParseHost{
    fn new(source:Source)->Result<Self>{
        let tokens=lexer::lex(&source)?;
        let storage=syntax::Builder::new(Some(source.clone())).map_err(|error|literal_failure(&source,error))?;
        Ok(Self{source,tokens,storage})
    }
    fn invoke<T>(&mut self,call:impl FnOnce(&ParseInputs,&mut ParseState)->T)->Result<T>{
        let input=ParseInputs{source:self.source.text.as_ptr(),tokens:self.tokens.as_mut_ptr(),count:self.tokens.len()as u64,storage:self.storage.raw_for_parser()};
        let mut state=ParseState{at:0,error:no_error()};
        let value=call(&input,&mut state);
        if state.error.kind!=0{return Err(parse_error(&state,&self.source,&self.storage));}
        Ok(value)
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
struct ParsedEntry{name:wire::Text,arguments:wire::Optional<wire::Slice>}
unsafe extern "C"{
 #[link_name="bootstrap.parser::parse_module"]fn b_module(input:*const ParseInputs,state:*mut ParseState)->wire::ModuleRef;
 #[link_name="bootstrap.parser::parse_entry"]fn b_entry(input:*const ParseInputs,state:*mut ParseState)->ParsedEntry;
 #[link_name="bootstrap.parser::parse_type_query"]fn b_type_query(input:*const ParseInputs,state:*mut ParseState)->wire::TypeRef;
}
fn no_error()->ParseError{ParseError{kind:0,detail:0,storage:wire::StorageError{kind:0,native_code:0},cleanup:wire::StorageError{kind:0,native_code:0},span:wire::Span{start:0,end:0},expected:wire::Text{kind:0,range:wire::Span{start:0,end:0}}}}
fn parse_error(state:&ParseState,source:&Source,builder:&syntax::Builder)->Diagnostic{

 let e=state.error;let span=Span{start:e.span.start as usize,end:e.span.end as usize};
 let(code,message)=match e.kind{
  12=>("B012","执行目标只能为 cpu、gpu 或 compute".into()),
  100=>("B100",format!("重复参数 {}",builder.read_text(e.expected))),
  263=>("B263",match e.detail{1=>"C ABI 与链接名称使用双引号字符串",2=>"链接字符串只支持 \\\\ 与 \\\" 转义",3=>"PE 链接名称需要非 NUL 的 ASCII 字符",4=>"链接名称不能为空",5=>"外部函数使用 C ABI",6=>"extern C 的链接符号需要具体类型，泛型包装写为普通 B 函数",_=>panic!("unknown link detail {}",e.detail)}.into()),
  13=>("B013","代码块没有结束".into()),
  10=>("B010",format!("需要 `{}`，实际为 `{}`",builder.read_text(e.expected),&source.text[span.start..span.end])),
  11=>("B011","需要标识符".into()),14=>("B014","命名空间限定需要命名路径".into()),
  225=>("B225",match e.detail{1=>"cleanup条件使用纯输入快照与资源长度",2=>"memory使用none、read、write或inout",3=>"initialized 条件使用输入值、returned、字段、布尔和纯数值运算；不执行调用或存储访问",4=>"range端点使用类型化编译期常量及纯数值运算",5=>"未知接口约束；使用 prefix、nonzero、nonempty、index、range、contiguous、same_length、cleanup、memory 或 initialized",6=>"cfn合同使用requires nonzero、ensures initialized及ensures memory",_=>panic!("unknown predicate detail {}",e.detail)}.into()),
  231=>("B231","重复的泛型参数".into()),232=>("B232","资源包装不能直接嵌套；子资源通过记录字段表达".into()),
  235=>("B235",match e.detail{1=>"编译期值参数当前只支持 u32",2=>"const 参数值域当前使用直接 u32 字面量清单",3=>"类型参数必须写在 const 参数之前",_=>panic!("unknown parameter detail")}.into()),
  266=>("B266","target 的实际执行位置为 cpu 或 gpu".into()),
  385=>("B385",match e.detail{1=>"Unicode 转义使用 \\u{十六进制标量}",2=>"Unicode 转义缺少 }",3=>"Unicode 转义必须是有效的十六进制 Unicode 标量",_=>"文本转义支持 \\\\、\\\"、\\n、\\r、\\t、\\0 和 \\u{标量}"}.into()),
  386=>("B386",{let primary=if e.storage.kind!=0{e.storage}else{e.cleanup};let mut message=syntax::StorageFailure{kind:primary.kind,native_code:primary.native_code}.to_string();if e.storage.kind!=0&&e.cleanup.kind!=0{message.push_str(&format!("; cleanup {}",syntax::StorageFailure{kind:e.cleanup.kind,native_code:e.cleanup.native_code}));}message}),
  _=>panic!("unknown parse error {}",e.kind),
 };let mut message=message;if e.kind!=386&&e.cleanup.kind!=0{message.push_str(&format!("; cleanup {}",syntax::StorageFailure{kind:e.cleanup.kind,native_code:e.cleanup.native_code}));}Diagnostic{source:source.clone(),span,code,message}
}

fn literal_failure(source:&Source,error:syntax::StorageFailure)->Diagnostic {
    Diagnostic{source:source.clone(),span:Span::default(),code:"B386",message:error.to_string()}
}
