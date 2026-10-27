//! 结果使用一个判别字和重叠载荷；分支与传播沿原返回域和资源所有者。
use super::*;

impl Builder<'_> {
    pub(super) fn variant_constructor_type(&self,name:&str,explicit:Option<&[GenericArgument]>,expected:Option<&Ty>,span:Span)->Result<Option<(Ty,usize)>> {
        let Some((parent,member))=name.rsplit_once("::") else{return Ok(None)};
        let Some(key)=self.world.resolve(parent,&self.definition.module,&self.definition.imports,false) else{return Ok(None)};
        if self.world.records[&key].0.variants.is_empty(){return Ok(None);}
        let mut syntax=TypeName::named(parent);
        if let Some(arguments)=explicit {syntax.arguments=arguments.to_vec();}
        else if let Some(expected)=expected.filter(|ty|ty.name.split_once('<').map_or(ty.name.as_str(),|(root,_)|root)==key) {
            syntax.arguments=expected.syntax().arguments;
        }
        let ty=self.expected_storage(&syntax,span)?;
        let Some(index)=ty.variants.iter().position(|variant|variant.name==member) else{return self.fail(span,"B269",format!("{}没有变体分支{member}",ty.name));};
        Ok(Some((ty,index)))
    }
    pub(super) fn expr_expected(
        &mut self,
        e: &Expr,
        expected: Option<&Ty>,
    ) -> Result<Option<Value>> {
        let constructor=match &e.kind {
            ExprKind::Call(name,_)=>self.variant_constructor_type(name,None,expected,e.span)?,
            ExprKind::Specialize(name,types,_)=>self.variant_constructor_type(name,Some(types),expected,e.span)?,
            _=>None,
        };
        if constructor.is_some(){let value=self.evaluate_returned(e,expected)?;return self.ordinary_return(value,e.span);}
        match &e.kind {
            ExprKind::Call(name, args) if matches!(name.as_str(), "Ok" | "Err") => {
                let Some(ty) = expected.filter(|ty| ty.result) else {
                    return self.fail(
                        e.span,
                        "B269",
                        "Ok／Err 需要已声明的 result<T,E> 上下文，或显式给出两个类型实参",
                    );
                };
                self.result_constructor(ty.clone(), name == "Ok", args, e.span)
            }
            ExprKind::Select(c, yes, no) => self.selection_expected(c, yes, no, e.span, expected),
            _ => self.expr(e),
        }
    }
    pub(super) fn result_constructor(
        &mut self,
        ty: Ty,
        success: bool,
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        self.value_variant_constructor(ty,usize::from(!success),args,span)
    }
    pub(super) fn value_variant_constructor(&mut self,ty:Ty,index:usize,args:&[Expr],span:Span)->Result<Option<Value>> {
        let payload=(*ty.variants[index].ty).clone();
        let value = if payload.name == "void" {
            if !args.is_empty() {
                return self.fail(span, "B269", "void 结果分支不接受载荷");
            }
            None
        } else {
            if args.len() != 1 {
                return self.fail(span, "B269", "结果分支需要一个匹配类型的载荷");
            }
            let Some(value) = self.expr_expected(&args[0], Some(&payload))? else {
                return Ok(None);
            };
            self.require_same(&payload, &value.ty, span)?;
            Some(value)
        };
        Ok(Some(self.variant_value(ty,index,value)))
    }
    fn result_word(&mut self, value: &Value, index: usize, scalar: Scalar) -> Value {
        let source = value.ty.components[index].0;
        let mut word = Value {
            ty: Ty::scalar(source),
            regs: vec![value.regs[index]],
            proof: value.projected_proof(&[index]),
            uniform: value.uniform,
            literal: false,
            constant: value.constant.as_ref().map(|bits| vec![bits[index]]),
        };
        if source == Scalar::F32 || scalar == Scalar::F32 {
            if source != scalar {
                let reg = self.reg(scalar);
                self.code.push(Inst::Unary {
                    dst: reg,
                    op: UnaryOp::Bitcast,
                    a: word.regs[0],
                });
                word.ty = Ty::scalar(scalar);
                word.regs = vec![reg];
                word.proof = bounds::Proof::default();
            }
            word
        } else if matches!(
            (source, scalar),
            (Scalar::Bool, Scalar::U32) | (Scalar::U32, Scalar::Bool)
        ) {
            // Result 的普通字存储只改变逻辑类型，不撤回活动 bool 载荷的事实。
            let proof = bounds::Proof::capture(&word);
            let mut word = self.cast(word, scalar);
            word.proof = proof;
            word
        } else {
            self.cast(word, scalar)
        }
    }
    pub(super) fn result_value(&mut self, ty: Ty, success: bool, payload: Option<Value>) -> Value {
        self.variant_value(ty,usize::from(!success),payload)
    }
    pub(super) fn variant_value(&mut self, ty: Ty, index: usize, payload: Option<Value>) -> Value {
        let mut value = self.constant_value(ty.clone(), vec![0; ty.components.len()]);
        let tag = self.constant(ty.components[0].0,ty.variants[index].discriminant);
        value.regs[0] = tag.regs[0];
        value
            .proof
            .assign(&[0], value.regs.len(), bounds::Proof::capture(&tag));
        if let Some(payload) = payload {
            let offset = ir::align_up(4, ty.align);
            for (index, (_, position)) in payload.ty.components.iter().enumerate() {
                let component = ty
                    .components
                    .iter()
                    .position(|(_, at)| *at == offset + position)
                    .unwrap();
                let word = self.result_word(&payload, index, Scalar::U32);
                value.regs[component] = word.regs[0];
                value.proof.assign(
                    &[component],
                    value.regs.len(),
                    bounds::Proof::capture(&word),
                );
                if let (Some(bits), Some(source)) = (&mut value.constant, &word.constant) {
                    bits[component] = source[0];
                } else {
                    value.constant = None;
                }
            }
            value.uniform = payload.uniform;
        }
        if let Some(bits) = &mut value.constant {
            bits[0] = ty.variants[index].discriminant;
        }
        value.proof.retain_view_borrows(&ty);
        let offset=ir::align_up(4,ty.align);
        let active=view_contracts::components(&ty.variants[index].ty).into_iter().map(|component|ty.components.iter().position(|(_,at)|*at==offset+ty.variants[index].ty.components[component].1).unwrap()).collect::<BTreeSet<_>>();
        for component in view_contracts::components(&ty).difference(&active){value.proof=value.proof.clone().with_view_sources(*component,BTreeSet::from([view::Source::Inactive]));}
        value.literal = false;
        value
    }
    pub(super) fn result_payload(&mut self, value: &Value, success: bool) -> Option<Value> {
        self.variant_payload(value,usize::from(!success))
    }
    pub(super) fn variant_payload(&mut self, value: &Value, index: usize) -> Option<Value> {
        let ty = (*value.ty.variants[index].ty).clone();
        if ty.name == "void" {
            return None;
        }
        let offset = ir::align_up(4, value.ty.align);
        let words: Vec<_> = ty
            .components
            .iter()
            .map(|(_, at)| {
                value
                    .ty
                    .components
                    .iter()
                    .position(|(_, word)| *word == offset + at)
                    .unwrap()
            })
            .collect();
        let mut proof = bounds::Proof::default();
        let mut regs = Vec::new();
        let mut constant = Some(Vec::new());
        for (index, ((scalar, _), word)) in ty.components.iter().zip(words).enumerate() {
            let fresh = self.result_word(value, word, *scalar);
            regs.push(fresh.regs[0]);
            proof.assign(
                &[index],
                ty.components.len(),
                bounds::Proof::capture(&fresh),
            );
            if let (Some(bits), Some(source)) = (&mut constant, fresh.constant) {
                bits.push(source[0]);
            } else {
                constant = None;
            }
        }
        proof.retain_view_borrows(&ty);
        Some(Value {
            ty,
            regs,
            proof,
            literal: false,
            uniform: value.uniform,
            constant,
        })
    }
    pub(super) fn result_condition(&mut self, value: &Value, span: Span) -> Result<Value> {
        self.variant_condition(value,0,span)
    }
    pub(super) fn variant_condition(&mut self,value:&Value,index:usize,span:Span)->Result<Value> {
        let scalar=value.ty.components[0].0;
        let tag=self.result_word(value,0,scalar);
        let expected=self.constant(scalar,value.ty.variants[index].discriminant);
        self.binary("==",tag,expected,span)
    }
    pub(super) fn check_variant_patterns<'b>(
        &self, ty: &Ty, arms: &'b [MatchArm], span: Span,
    ) -> Result<Vec<&'b MatchArm>> {
        if !ty.has_variants() { return self.fail(span, "B269", "match需要带载荷变体或result值"); }
        let mut selected = vec![None; ty.variants.len()];
        for arm in arms {
            let (owner, name) = arm.variant.rsplit_once("::").map_or((None, arm.variant.as_str()), |(owner, name)| (Some(owner), name));
            if let Some(owner) = owner {
                let mut syntax = ty.syntax(); syntax.name = owner.into(); syntax.canonical = false;
                let expected = self.expected_storage(&syntax, arm.span)?;
                self.require_same(ty, &expected, arm.span)?;
            }
            let Some(index) = ty.variants.iter().position(|v| v.name == name) else { return self.fail(arm.span,"B269",format!("{}没有变体分支{name}",ty.name)); };
            if selected[index].replace(arm).is_some() { return self.fail(arm.span,"B269","同一变体分支不能重复"); }
            let unit = ty.variants[index].ty.name == "void";
            if arm.binding.is_none() && !unit { return self.fail(arm.span,"B269","有载荷分支需要名称或_模式"); }
            if unit && arm.binding.as_ref().is_some_and(|name| name!="_") { return self.fail(arm.span,"B269","无载荷分支没有可绑定的数据"); }
        }
        if selected.iter().any(Option::is_none) { return self.fail(span,"B269","match必须处理声明的全部变体分支"); }
        Ok(selected.into_iter().map(Option::unwrap).collect())
    }
    pub(super) fn value_variant_arm(&mut self, value: &Value, index: usize, arm: &MatchArm) -> Result<()> {
        let payload = self.variant_payload(value,index);
        if let Some(name) = arm.binding.as_ref().filter(|name| name.as_str()!="_") {
            self.bind_local_value(name,payload.unwrap(),arm.span)?;
            self.block(&arm.body)?; self.env.remove(name); Ok(())
        } else { self.block(&arm.body) }
    }
    pub(super) fn explicit_result(
        &mut self,
        name: &str,
        explicit: &[GenericArgument],
        args: &[Expr],
        span: Span,
    ) -> Result<Option<Value>> {
        let mut ty = TypeName::named("result");
        ty.arguments = explicit.to_vec();
        let ty = self.world.ty(
            &ty,
            &self.definition.source,
            span,
            &self.definition.module,
            &self.definition.imports,
        )?;
        self.result_constructor(ty, name == "Ok", args, span)
    }
}
