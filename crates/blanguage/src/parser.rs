use crate::{
    ast::*,
    lexer::{self, Token},
};

#[repr(C)]
struct AngleEnd {
    end: usize,
    present: u32,
}
unsafe extern "C" {
    #[link_name = "bootstrap.parser::angle_end"]
    fn angle_end(source: *const u8, tokens: *const Token, count: usize, at: usize) -> AngleEnd;
}

pub fn parse(source: Source) -> Result<Module> {
    let tokens = lexer::lex(&source)?;
    Parser {
        source,
        tokens,
        at: 0,
    }
    .module()
}
pub(crate) fn entry(source: Source) -> Result<(String, Option<Vec<GenericArgument>>)> {
    let tokens = lexer::lex(&source)?;
    let mut parser = Parser {
        source,
        tokens,
        at: 0,
    };
    let name = parser.type_name()?;
    let arguments = if parser.peek() == "<" {
        Some(parser.generic_arguments()?)
    } else {
        None
    };
    parser.expect("")?;
    Ok((name, arguments))
}
pub(crate) fn type_query(source: Source) -> Result<TypeName> {
    let tokens = lexer::lex(&source)?;
    let mut parser = Parser { source, tokens, at: 0 };
    let ty = parser.ty()?;
    parser.expect("")?;
    Ok(ty)
}
struct Parser {
    source: Source,
    tokens: Vec<Token>,
    at: usize,
}
impl Parser {
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
        let text = self.peek().to_owned();
        if !text.starts_with('"') {
            return self
                .source
                .fail(span, "B263", "C ABI 与链接名称使用双引号字符串");
        }
        let mut result = String::new();
        let mut chars = text[1..text.len() - 1].chars();
        while let Some(c) = chars.next() {
            let c = if c == '\\' {
                match chars.next() {
                    Some('\\') => '\\',
                    Some('"') => '"',
                    _ => {
                        return self.source.fail(
                            span,
                            "B263",
                            "链接字符串只支持 \\\\ 与 \\\" 转义",
                        );
                    }
                }
            } else {
                c
            };
            if !c.is_ascii() || c == '\0' {
                return self
                    .source
                    .fail(span, "B263", "PE 链接名称需要非 NUL 的 ASCII 字符");
            }
            result.push(c);
        }
        if result.is_empty() {
            return self.source.fail(span, "B263", "链接名称不能为空");
        }
        self.at += 1;
        Ok(result)
    }
    fn qualified(&mut self, sep: &str) -> Result<String> {
        let mut s = self.ident()?;
        while self.take(sep) {
            s.push_str(sep);
            s.push_str(&self.ident()?);
        }
        Ok(s)
    }
    fn type_close(&mut self) -> Result<()> {
        if self.peek() == ">>" {
            // 类型上下文消费首个 >，保留第二个；表达式中的右移 token 不变。
            self.tokens[self.at].span.start += 1;
            Ok(())
        } else {
            self.expect(">")
        }
    }
    fn angle_end(&self, at: usize) -> Option<usize> {
        // Source and the lexer-owned spans remain borrowed for this call.
        let result = unsafe {
            angle_end(self.source.text.as_ptr(), self.tokens.as_ptr(), self.tokens.len(), at)
        };
        (result.present != 0).then_some(result.end)
    }
    fn contract_value_path(&mut self) -> Result<String> {
        let mut path=self.qualified(".")?;
        while self.take("[") {
            self.expect("*")?;self.expect("]")?;path.push_str("[*]");
            if self.take("."){path.push('.');path.push_str(&self.qualified(".")?);}
        }
        Ok(path)
    }
    fn contract(&mut self) -> Result<FunctionContract> {
        let span = self.span();
        let kind = if self.take("requires") {
            ContractKind::Requires
        } else {
            self.expect("ensures")?;
            ContractKind::Ensures
        };
        let predicate = self.ident()?;
        self.expect("(")?;
        let predicate = match predicate.as_str() {
            "prefix" => {
                let data = self.ident()?;
                self.expect(",")?;
                let count = self.ident()?;
                self.expect(".")?;
                let field = self.qualified(".")?;
                ContractPredicate::Prefix { data, count, field }
            }
            "nonzero" => ContractPredicate::Nonzero(self.contract_value_path()?),
            "nonempty" => ContractPredicate::Nonempty(self.contract_value_path()?),
            "contiguous" => ContractPredicate::Contiguous(self.qualified(".")?),
            "initialized" => {
                let value = self.qualified(".")?;
                self.expect(",")?;
                let expression = self.expr(0,false)?;
                let condition = OutputCondition::from_expr(&expression).ok_or_else(|| Diagnostic {
                    source: self.source.clone(), span: expression.span, code: "B225",
                    message: "initialized 条件使用输入值、returned、字段、布尔和纯数值运算；不执行调用或存储访问".into(),
                })?;
                ContractPredicate::Initialized { value, condition }
            }
            "same_length" => {
                let left = self.qualified(".")?;
                self.expect(",")?;
                let right = self.qualified(".")?;
                let (left, right) = if left <= right { (left, right) } else { (right, left) };
                ContractPredicate::SameLength { left, right }
            }
            "index" => {
                let value = self.qualified(".")?;
                self.expect(",")?;
                let resource = self.qualified(".")?;
                ContractPredicate::Index { value, resource }
            }
            _ => {
                return self.source.fail(
                    span,
                    "B225",
                    "未知接口约束；使用 prefix、nonzero、nonempty、index、contiguous、same_length 或 initialized",
                );
            }
        };
        self.expect(")")?;
        Ok(FunctionContract { kind, predicate, span })
    }
    fn function_clause(&self, at: usize) -> bool {
        self.tokens.get(at).is_some_and(|t| matches!(t.text(&self.source), "requires" | "ensures"))
            && self.tokens.get(at + 2).is_some_and(|t| t.text(&self.source) == "(")
    }
    fn function_suffix_end(&self, mut at: usize) -> Option<usize> {
        while self.function_clause(at) {
            at += 3;
            let mut depth = 1usize;
            while depth != 0 {
                match self.tokens.get(at)?.text(&self.source) {
                    "(" => depth += 1, ")" => depth -= 1, _ => {}
                }
                at += 1;
            }
        }
        Some(at)
    }
    fn function_contracts(&mut self) -> Result<Option<Box<Vec<ContractPredicate>>>> {
        let mut contracts = Vec::new();
        while self.function_clause(self.at) {
            let contract = self.contract()?;
            if !matches!((&contract.kind, &contract.predicate),
                (ContractKind::Requires, ContractPredicate::Nonzero(_))
                | (ContractKind::Ensures, ContractPredicate::Initialized { .. })) {
                return self.source.fail(contract.span, "B225", "cfn合同使用requires nonzero输入前提及ensures initialized输出条件");
            }
            contracts.push(contract.predicate);
        }
        Ok(if contracts.is_empty() { None } else { Some(Box::new(contracts)) })
    }
    fn arguments(&mut self) -> Result<Vec<Expr>> {
        self.expect("(")?;
        let mut args = Vec::new();
        if !self.take(")") {
            loop {
                args.push(self.expr(0,false)?);
                if self.take(")") { break; }
                self.expect(",")?;
            }
        }
        Ok(args)
    }
    fn ty(&mut self) -> Result<TypeName> {
        let name = self.type_name()?;
        if matches!(name.as_str(), "dataset" | "view" | "tile") {
            let view = name == "view";
            let tile = name == "tile";
            self.expect("<")?;
            let mut inner = self.ty()?;
            if inner.dataset || inner.view || inner.tile {
                return self.source.fail(
                    self.span(),
                    "B232",
                    "资源包装不能直接嵌套；子资源通过记录字段表达",
                );
            }
            let count = if !view && !tile && self.take(",") {
                Some(self.shape_argument()?)
            } else {
                None
            };
            self.type_close()?;
            inner.dataset = !view && !tile;
            inner.view = view;
            inner.tile = tile;
            inner.count = count;
            Ok(inner)
        } else {
            let arguments = if self.peek() == "<" {
                self.generic_arguments()?
            } else {
                Vec::new()
            };
            let contracts = if name == "cfn" { self.function_contracts()? } else { None };
            Ok(TypeName {
                name,
                canonical: false,
                arguments,
                dataset: false,
                view: false,
                tile: false,
                count: None,
                contracts,
            })
        }
    }
    fn shape_argument(&mut self) -> Result<Shape> {
        let expression=self.expr(0,true)?;
        Ok(match &expression.kind {
            ExprKind::Number(number) if number.parse::<u32>().is_ok()=>Shape::Literal(number.parse().unwrap()),
            ExprKind::StaticU32(value)=>Shape::Literal(*value),
            ExprKind::Name(name)=>Shape::Constant(name.clone()),
            _=>Shape::Expression(Box::new(expression)),
        })
    }
    fn value_argument(&self)->bool {
        let first=self.peek();
        if first.as_bytes().first().is_some_and(u8::is_ascii_digit)
            || first.starts_with('"')
            || matches!(first,"("|"+"|"-"|"!"|"~"|"true"|"false") {return true;}
        let mut at=self.at+1;
        while self.tokens.get(at).is_some_and(|token|matches!(token.text(&self.source),"."|"::")) {at+=2;}
        if self.tokens.get(at).is_some_and(|token|token.text(&self.source)=="<") {
            let Some(end)=self.angle_end(at)else{return false;};at=end;
        }
        self.tokens.get(at).is_some_and(|token|matches!(token.text(&self.source),
            "("|"+"|"-"|"*"|"/"|"%"|"&"|"|"|"^"|"<<"|"=="|"!="|"<="|">="|"&&"|"||"|"?"))
    }
    fn generic_arguments(&mut self) -> Result<Vec<GenericArgument>> {
        self.expect("<")?;
        let mut result = Vec::new();
        loop {
            result.push(
                if self.value_argument()
                {
                    GenericArgument::Value(self.shape_argument()?)
                } else {
                    GenericArgument::Type(self.ty()?)
                },
            );
            if matches!(self.peek(), ">" | ">>") {
                self.type_close()?;
                break;
            }
            self.expect(",")?;
        }
        Ok(result)
    }
    fn parameters(&mut self) -> Result<(Vec<TypeParameter>, Vec<ValueParameter>)> {
        let mut types: Vec<TypeParameter> = Vec::new();
        let mut values: Vec<ValueParameter> = Vec::new();
        if self.take("<") {
            loop {
                let span = self.span();
                let is_value = self.take("const");
                let name = self.ident()?;
                if types.iter().any(|t| t.name == name) || values.iter().any(|v| v.name == name) {
                    return self.source.fail(span, "B231", "重复的泛型参数");
                }
                if is_value {
                    self.expect(":")?;
                    let ty = self.ty()?;
                    if !matches!(ty.name.as_str(), "u32" | "uint")
                        || !ty.arguments.is_empty()
                        || ty.dataset
                        || ty.view
                        || ty.tile
                    {
                        return self.source.fail(span, "B235", "编译期值参数当前只支持 u32");
                    }
                    let restricted = self.take("in");
                    if restricted {
                        self.expect("{")?;
                    }
                    let mut choices = Vec::new();
                    if restricted && !self.take("}") {
                        loop {
                            let choice = self.shape_argument()?;
                            let Shape::Literal(value) = choice else {
                                return self.source.fail(
                                    span,
                                    "B235",
                                    "const 参数值域当前使用直接 u32 字面量清单",
                                );
                            };
                            choices.push(value);
                            if self.take("}") {
                                break;
                            }
                            self.expect(",")?;
                        }
                    }
                    choices.sort();
                    choices.dedup();
                    values.push(ValueParameter {
                        name,
                        choices: restricted.then_some(choices),
                    });
                } else {
                    if !values.is_empty() {
                        return self
                            .source
                            .fail(span, "B235", "类型参数必须写在 const 参数之前");
                    }
                    let family = if self.take(":") {
                        let bound = self.type_name()?;
                        Some(match bound.as_str() {
                            "numeric" => TypeFamily::Numeric,
                            "floating" => TypeFamily::Floating,
                            "integer" => TypeFamily::Integer,
                            "scalar_numeric" => TypeFamily::ScalarNumeric,
                            "scalar_integer" => TypeFamily::ScalarInteger,
                            _ => TypeFamily::Named(bound),
                        })
                    } else {
                        None
                    };
                    types.push(TypeParameter { name, family });
                }
                if self.take(">") {
                    break;
                }
                self.expect(",")?;
            }
        }
        Ok((types, values))
    }
    fn type_name(&mut self) -> Result<String> {
        let mut name = self.qualified(".")?;
        while self.take("::") {
            name.push_str("::");
            name.push_str(&self.ident()?);
        }
        Ok(name)
    }
    fn declaration_type(&self) -> bool {
        let ident = |at: usize| {
            self.tokens.get(at).is_some_and(|t| {
                t.text(&self.source)
                    .as_bytes()
                    .first()
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
            })
        };
        if self.peek() == "cooperate" {
            return false;
        }
        if matches!(self.peek(), "view" | "tile")
            && self.tokens.get(self.at + 1).is_some_and(|t| t.text(&self.source) == "<")
        {
            return true;
        }
        if !ident(self.at) {
            return false;
        }
        let mut at = self.at + 1;
        while self
            .tokens
            .get(at)
            .is_some_and(|t| matches!(t.text(&self.source), "." | "::"))
            && ident(at + 1)
        {
            at += 2;
        }
        if self.tokens.get(at).is_some_and(|t| t.text(&self.source) == "<") {
            let Some(end) = self.angle_end(at) else {
                return false;
            };
            at = end;
        }
        let Some(at) = self.function_suffix_end(at) else { return false; };
        ident(at)
    }
    fn module(mut self) -> Result<Module> {
        self.expect("module")?;
        let name = self.qualified(".")?;
        self.expect(";")?;
        let mut imports = Vec::new();
        let mut records = Vec::new();
        let mut type_sets = Vec::new();
        let mut functions = Vec::new();
        while !self.peek().is_empty() {
            let span = self.span();
            if self.take("import") {
                imports.push(self.qualified(".")?);
                self.expect(";")?;
                continue;
            }
            if self.take("const") {
                let result = self.ty()?;
                let name = self.ident()?;
                self.expect("=")?;
                let value = self.expr(0,false)?;
                self.expect(";")?;
                // const 初始化沿同一表达式与返回类型分析，不生成运行期函数入口。
                functions.push(Function {
                    name,
                    types: Vec::new(),
                    values: Vec::new(),
                    params: Vec::new(),
                    result,
                    kind: FunctionKind::Constant,
                    target: Target::Compute,
                    contracts: Vec::new(),
                    body: Some(vec![Stmt {
                        span,
                        kind: StmtKind::Return(Some(value)),
                    }]),
                    c_abi: false,
                    external: None,
                    span: Span { start: span.start, end: self.tokens[self.at - 1].span.end },
                });
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
                type_sets.push(TypeSet {
                    name,
                    members,
                    span: Span { start: span.start, end: self.tokens[self.at - 1].span.end },
                });
                continue;
            }
            if self.take("enum") {
                let name = self.ident()?;
                let (types, values) = self.parameters()?;
                self.expect(":")?;
                let representation = self.ty()?;
                self.expect("{")?;
                let mut variants = Vec::new();
                let mut payloads = !types.is_empty() || !values.is_empty();
                while !self.take("}") {
                    let at = self.span();
                    let member = self.ident()?;
                    let payload = if self.take("(") {
                        payloads = true;
                        if self.take(")") { TypeName::named("void") }
                        else { let ty = self.ty()?; self.expect(")")?; ty }
                    } else { TypeName::named("void") };
                    self.expect("=")?;
                    let discriminant = self.expr(0,false)?;
                    self.expect(";")?;
                    variants.push(VariantDeclaration { name: member, payload, discriminant, span: at });
                }
                self.expect(";")?;
                if !payloads {
                    for variant in &variants {
                        let span = variant.span;
                        functions.push(Function {
                            name: format!("{name}::{}", variant.name),
                            types: Vec::new(), values: Vec::new(), params: Vec::new(),
                            result: TypeName::named(&name), kind: FunctionKind::Constant,
                            target: Target::Compute, contracts: Vec::new(), c_abi: false, external: None, span,
                            body: Some(vec![Stmt { span, kind: StmtKind::Return(Some(Expr {
                                span, kind: ExprKind::Call(name.clone(), vec![variant.discriminant.clone()]),
                            })) }]),
                        });
                    }
                    variants.clear();
                }
                records.push(Record { name, types, values, stride: None,
                    enumeration: Some(representation), variants, fields: Vec::new(),
                    span: Span { start: span.start, end: self.tokens[self.at - 1].span.end } });
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
                    fields.push(Field { name, ty, span });
                }
                self.expect(";")?;
                records.push(Record {
                    name,
                    types,
                    values,
                    stride,
                    enumeration: None,
                    variants: Vec::new(),
                    fields,
                    span: Span { start: span.start, end: self.tokens[self.at - 1].span.end },
                });
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
            let mut params = Vec::new();
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
                    if params.iter().any(|p: &Param| p.name == name) {
                        return self.source.fail(span, "B100", format!("重复参数 {name}"));
                    }
                    params.push(Param {
                        name,
                        ty,
                        access,
                        span,
                    });
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
                if !types.is_empty() || !values.is_empty() {
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
            let body = if self.take(";") {
                None
            } else {
                Some(self.block()?)
            };
            functions.push(Function {
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
                span: Span { start: span.start, end: self.tokens[self.at - 1].span.end },
            });
        }
        Ok(Module {
            name,
            imports,
            records,
            type_sets,
            functions,
            source: self.source,
        })
    }
    fn block(&mut self) -> Result<Vec<Stmt>> {
        self.expect("{")?;
        let mut out = Vec::new();
        while !self.take("}") {
            if self.peek().is_empty() {
                return self.source.fail(self.span(), "B013", "代码块没有结束");
            }
            out.push(self.stmt()?);
        }
        Ok(out)
    }
    fn stmt(&mut self) -> Result<Stmt> {
        let span = self.span();
        let kind = if self.take("parallel") {
            self.expect("(")?;
            let name = self.ident()?;
            self.expect("in")?;
            let selected = self.peek() == "where"
                && self.tokens.get(self.at + 1).is_some_and(|t| t.text(&self.source) == "(");
            if selected {
                self.at += 1;
                self.expect("(")?;
            }
            let prefix = self.peek() == "prefix"
                && self.tokens.get(self.at + 1).is_some_and(|t| t.text(&self.source) == "(");
            let mut domain = if prefix {
                self.at += 1;
                self.expect("(")?;
                let dataset = self.ident()?;
                self.expect(",")?;
                let count = self.expr(0,false)?;
                self.expect(")")?;
                ParallelDomain {
                    dataset,
                    count: Some(count),
                    selection: None,
                }
            } else {
                ParallelDomain {
                    dataset: self.ident()?,
                    count: None,
                    selection: None,
                }
            };
            if selected {
                self.expect(",")?;
                domain.selection = Some(self.expr(0,false)?);
                self.expect(")")?;
            }
            self.expect(")")?;
            StmtKind::Parallel(name, domain, self.block()?)
        } else if self.take("if") {
            self.expect("(")?;
            let c = self.expr(0,false)?;
            self.expect(")")?;
            let a = self.block()?;
            let b = if self.take("else") {
                if self.peek() == "if" {
                    vec![self.stmt()?]
                } else {
                    self.block()?
                }
            } else {
                Vec::new()
            };
            StmtKind::If(c, a, b)
        } else if self.take("match") {
            self.expect("(")?;
            let value = self.expr(0,false)?;
            self.expect(")")?;
            self.expect("{")?;
            let mut arms=Vec::new();
            while !self.take("}") {
                let at=self.span();let variant=self.type_name()?;self.expect("(")?;
                let binding=if self.take(")"){None}else{let name=self.ident()?;self.expect(")")?;Some(name)};
                let body=self.block()?;arms.push(MatchArm {variant,binding,body,span:at});
            }
            StmtKind::Match(value,arms)
        } else if self.take("while") {
            self.expect("(")?;
            let c = self.expr(0,false)?;
            self.expect(")")?;
            StmtKind::While(c, self.block()?)
        } else if self.take("for") {
            self.expect("(")?;
            let name = self.ident()?;
            self.expect("in")?;
            let start = self.expr(0,false)?;
            let end = if self.take("..") {
                Some(self.expr(0,false)?)
            } else {
                None
            };
            self.expect(")")?;
            StmtKind::For(name, start, end, self.block()?)
        } else if self.take("return") {
            if self.take(";") {
                StmtKind::Return(None)
            } else {
                let e = self.expr(0,false)?;
                self.expect(";")?;
                StmtKind::Return(Some(e))
            }
        } else if self.take("break") {
            self.expect(";")?;
            StmtKind::Break
        } else if self.take("continue") {
            self.expect(";")?;
            StmtKind::Continue
        } else if self.peek() == "phase" && self.tokens[self.at + 1].text(&self.source) == ";" {
            self.at += 2;
            StmtKind::Phase
        } else if self.take("seek") {
            self.expect("(")?;
            let name = self.ident()?;
            self.expect(",")?;
            let position = self.expr(0,false)?;
            self.expect(")")?;
            self.expect(";")?;
            StmtKind::Seek(name, position)
        } else if self.take("let") {
            let name = self.ident()?;
            self.expect("=")?;
            let e = self.expr(0,false)?;
            self.expect(";")?;
            StmtKind::Let(None, name, e)
        } else {
            let save = self.at;
            let looks_type = self.declaration_type();
            if looks_type {
                let ty = self.ty()?;
                let name = self.ident()?;
                if self.take(";") {
                    StmtKind::Declare(ty, name)
                } else {
                    self.expect("=")?;
                    let e = self.expr(0,false)?;
                    self.expect(";")?;
                    StmtKind::Let(Some(ty), name, e)
                }
            } else {
                self.at = save;
                let lhs = self.expr(0,false)?;
                if [
                    "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=",
                ]
                .contains(&self.peek())
                {
                    let op = self.peek().to_owned();
                    self.at += 1;
                    let rhs = self.expr(0,false)?;
                    self.expect(";")?;
                    StmtKind::Assign(lhs, op, rhs)
                } else {
                    self.expect(";")?;
                    StmtKind::Expr(lhs)
                }
            }
        };
        Ok(Stmt { kind, span })
    }
    fn expr(&mut self, min: u8, type_argument: bool) -> Result<Expr> {
        let span = self.span();
        let mut lhs = if ["-", "+", "!", "~"].contains(&self.peek()) {
            let op = self.peek().to_owned();
            self.at += 1;
            Expr {
                kind: ExprKind::Unary(op, Box::new(self.expr(12,type_argument)?)),
                span,
            }
        } else if self.peek() == "target" && self.tokens[self.at + 1].text(&self.source) == "(" {
            self.at += 2;
            let target = match self.ident()?.as_str() {
                "cpu" => Target::Cpu,
                "gpu" => Target::Gpu,
                _ => {
                    return self
                        .source
                        .fail(span, "B266", "target 的实际执行位置为 cpu 或 gpu");
                }
            };
            self.expect(",")?;
            let call = self.expr(0,false)?;
            self.expect(")")?;
            Expr {
                kind: ExprKind::Target(target, Box::new(call)),
                span,
            }
        } else if self.take("cooperate") {
            let mut name = self.qualified(".")?;
            while self.take("::") {
                name.push_str("::");
                name.push_str(&self.ident()?);
            }
            let explicit = if self.peek() == "<" {
                Some(self.generic_arguments()?)
            } else {
                None
            };
            let args = self.arguments()?;
            Expr {
                kind: ExprKind::Cooperate(name, explicit, args),
                span,
            }
        } else if self.peek() == "cfn"
            && self.tokens.get(self.at + 1).is_some_and(|t| t.text(&self.source) == "<")
            && self.angle_end(self.at + 1).is_some_and(|at| {
                self.tokens.get(at).is_some_and(|t| t.text(&self.source) == "(")
                    || self.function_clause(at)
            }) {
            let ty = self.ty()?;
            let args = self.arguments()?;
            Expr { kind: ExprKind::Construct(ty, args), span }
        } else if self.take("(") {
            let e = self.expr(0,false)?;
            self.expect(")")?;
            e
        } else if self.take("true") {
            Expr {
                kind: ExprKind::Bool(true),
                span,
            }
        } else if self.take("false") {
            Expr {
                kind: ExprKind::Bool(false),
                span,
            }
        } else if self.peek().starts_with('"') {
            let text = self.peek();
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
                                return self.source.fail(span, "B385", "Unicode 转义使用 \\u{十六进制标量}");
                            }
                            let Some((digits, rest)) = chars.as_str().split_once('}') else {
                                return self.source.fail(span, "B385", "Unicode 转义缺少 }");
                            };
                            let Some(scalar) = u32::from_str_radix(digits, 16).ok()
                                .filter(|_| digits.bytes().all(|c| c.is_ascii_hexdigit()))
                                .and_then(char::from_u32) else {
                                return self.source.fail(span, "B385", "Unicode 转义必须是有效的十六进制 Unicode 标量");
                            };
                            chars = rest.chars();
                            scalar
                        }
                        _ => return self.source.fail(span, "B385", "文本转义支持 \\\\、\\\"、\\n、\\r、\\t、\\0 和 \\u{标量}"),
                    }
                } else { c });
            }
            let mut ty = TypeName::named("array");
            ty.arguments = vec![
                GenericArgument::Type(TypeName::named("u8")),
                GenericArgument::Value(Shape::Expression(Box::new(Expr {
                    kind: ExprKind::Number(value.len().to_string()), span,
                }))),
            ];
            let values = value.bytes().map(|byte| Expr {
                kind: ExprKind::Construct(TypeName::named("u8"), vec![Expr {
                    kind: ExprKind::Number(byte.to_string()), span,
                }]), span,
            }).collect();
            self.at += 1;
            Expr { kind: ExprKind::Construct(ty, values), span }
        } else if self
            .peek()
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
        {
            let s = self.peek().to_owned();
            self.at += 1;
            Expr {
                kind: ExprKind::Number(s),
                span,
            }
        } else {
            Expr {
                kind: ExprKind::Name(self.qualified("::")?),
                span,
            }
        };
        loop {
            if self.take("::") {
                fn path(e: &Expr) -> Option<String> {
                    match &e.kind {
                        ExprKind::Name(n) => Some(n.clone()),
                        ExprKind::Member(base, n) => Some(format!("{}.{n}", path(base)?)),
                        _ => None,
                    }
                }
                let Some(mut name) = path(&lhs) else {
                    return self.source.fail(span, "B014", "命名空间限定需要命名路径");
                };
                name.push_str("::");
                name.push_str(&self.ident()?);
                lhs = Expr {
                    kind: ExprKind::Name(name),
                    span,
                };
                continue;
            }
            if self.take(".") {
                let name = self.ident()?;
                lhs = Expr {
                    kind: ExprKind::Member(Box::new(lhs), name),
                    span,
                };
                continue;
            }
            if self.take("[") {
                let index = self.expr(0,false)?;
                self.expect("]")?;
                lhs = Expr {
                    kind: ExprKind::Index(Box::new(lhs), Box::new(index)),
                    span,
                };
                continue;
            }
            if let ExprKind::Name(name) = &lhs.kind
                && self.peek() == "<"
                && self.angle_end(self.at).is_some_and(|at| {
                    self.tokens
                        .get(at)
                        .is_some_and(|t| matches!(t.text(&self.source), "(" | "," | ")" | ";"))
                })
            {
                let name = name.clone();
                let arguments = self.generic_arguments()?;
                lhs = Expr {
                    kind: ExprKind::FunctionRef(name, arguments),
                    span,
                };
                continue;
            }
            if self.peek() == "(" {
                let args = self.arguments()?;
                lhs = Expr {
                    kind: match lhs.kind {
                        ExprKind::Name(name) => ExprKind::Call(name, args),
                        ExprKind::FunctionRef(name, arguments) => {
                            ExprKind::Specialize(name, arguments, args)
                        }
                        kind => ExprKind::Invoke(Box::new(Expr { kind, span }), args),
                    },
                    span,
                };
                continue;
            }
            if self.peek() == "?" && min == 0 {
                self.at += 1;
                let a = self.expr(0,false)?;
                self.expect(":")?;
                let b = self.expr(0,type_argument)?;
                lhs = Expr {
                    kind: ExprKind::Select(Box::new(lhs), Box::new(a), Box::new(b)),
                    span,
                };
                continue;
            }
            if type_argument && matches!(self.peek(),">"|">>") {break;}
            let prec = match self.peek() {
                "||" => 1,
                "&&" => 2,
                "|" => 3,
                "^" => 4,
                "&" => 5,
                "==" | "!=" => 6,
                "<" | ">" | "<=" | ">=" => 7,
                "<<" | ">>" => 8,
                "+" | "-" => 9,
                "*" | "/" | "%" => 10,
                _ => 0,
            };
            if prec == 0 || prec < min {
                break;
            }
            let op = self.peek().to_owned();
            self.at += 1;
            let rhs = self.expr(prec + 1,type_argument)?;
            lhs = Expr {
                kind: ExprKind::Binary(op, Box::new(lhs), Box::new(rhs)),
                span,
            };
        }
        Ok(lhs)
    }
}
