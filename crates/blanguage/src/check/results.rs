//! 结果使用一个判别字和重叠载荷；分支与传播沿原返回域和资源所有者。
use super::*;

impl Builder<'_> {
    #[inline(never)]
    pub(super) async fn variant_constructor_type(&self,name:&str,explicit:Option<&[GenericArgument]>,expected:Option<&Ty>,span:Span)->Result<Option<(Ty,usize)>> {
        unsafe{self.world.call(self.variant_constructor_type_with(name,expected,span,|parent|{
            explicit.map(|arguments|{let mut syntax=TypeName::named(parent);syntax.arguments=arguments.to_vec();syntax})
        })).await}
    }
    pub(super) async fn variant_constructor_type_with(&self,name:&str,expected:Option<&Ty>,span:Span,
        explicit:impl FnOnce(&str)->Option<TypeName>)->Result<Option<(Ty,usize)>> {
        let Some((parent,member))=name.rsplit_once("::") else{return Ok(None)};
        let Some(key)=self.world.resolve(parent,self.scope.module(),self.scope.imports(),false).unique() else{return Ok(None)};
        if self.world.record(&key).0.variants().is_empty(){return Ok(None);}
        let syntax=explicit(parent).unwrap_or_else(||{
            let mut syntax=TypeName::named(parent);
            if let Some(expected)=expected.filter(|ty|ty.name.split_once('<').map_or(ty.name.as_str(),|(root,_)|root)==key){
                syntax.arguments=expected.syntax().arguments;
            }
            syntax
        });
        let ty=unsafe { self.world.call(self.expected_storage(&syntax,span)).await }?;
        let Some(index)=ty.variants.iter().position(|variant|variant.name==member) else{return self.fail(span,"B269",format!("{}没有变体分支{member}",ty.name));};
        Ok(Some((ty,index)))
    }
    #[inline(never)]
    pub(super) async fn expr_expected(
        &mut self,
        e: &Expr,
        expected: Option<&Ty>,
    ) -> Result<Option<Value>> {
        unsafe{self.world.call(source_call::expected(self,e,expected)).await}
    }

    pub(super) async fn result_constructor(
        &mut self,
        ty: Ty,
        success: bool,
        args: &Expressions,
        span: Span,
    ) -> Result<Option<Value>> {
        unsafe { self.world.call(self.value_variant_constructor(ty,usize::from(!success),args,span)).await }
    }
    #[inline(never)]
    pub(super) async fn value_variant_constructor(&mut self,ty:Ty,index:usize,args:&Expressions,span:Span)->Result<Option<Value>> {
        unsafe { self.world.call(self.variant_values(ty,index,args.len(),span,async |builder,expected|unsafe { builder.world.call(builder.expr_expected(&args.at(0),Some(expected))).await })).await }
    }
    pub(super) async fn variant_values(&mut self,ty:Ty,index:usize,count:usize,span:Span,
        argument:impl std::ops::AsyncFnOnce(&mut Self,&Ty)->Result<Option<Value>>)->Result<Option<Value>> {
        let payload=(*ty.variants[index].ty).clone();
        let value = if payload.name == "void" {
            if count!=0 {
                return self.fail(span, "B269", "void 结果分支不接受载荷");
            }
            None
        } else {
            if count != 1 {
                return self.fail(span, "B269", "结果分支需要一个匹配类型的载荷");
            }
            let Some(value) = argument(self,&payload).await? else {
                return Ok(None);
            };
            self.require_same(&payload, &value.ty, span)?;
            Some(value)
        };
        Ok(Some(self.variant_value(ty,index,value)))
    }
    fn result_component(&mut self, value: &Value, index: usize, scalar: Scalar) -> Value {
        let source = value.ty.components[index].0;
        let mut word = Value {
            ty: Ty::scalar(source),
            regs: vec![value.regs[index]],
            proof: value.projected_proof(&[index]),
            uniform: value.uniform,
            literal: false,
            constant: value.constant.as_ref().map(|bits| vec![bits[index]]),
        };
        if source.is_float() || scalar.is_float() {
            if source == scalar { return word; }
            if source.is_float() {
                let raw = if source.size() == 8 { Scalar::U64 } else { Scalar::U32 };
                let reg = self.reg(raw);
                self.emit(InstKind::Unary {
                    dst: reg,
                    op: UnaryOp::Bitcast,
                    a: word.regs[0],
                });
                word.ty = Ty::scalar(raw);
                word.regs = vec![reg];
                word.proof = bounds::Proof::default();
            }
            let raw = if scalar.is_float() {
                if scalar.size() == 8 { Scalar::U64 } else { Scalar::U32 }
            } else { scalar };
            word = self.cast(word, raw);
            if scalar.is_float() {
                let reg = self.reg(scalar);
                self.emit(InstKind::Unary { dst: reg, op: UnaryOp::Bitcast, a: word.regs[0] });
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
    fn result_word(&mut self, value: &Value, offset: u32, scalar: Scalar) -> Value {
        let raw = if scalar.size() == 8 { Scalar::U64 } else { Scalar::U32 };
        let mut joined: Option<Value> = None;
        for (index, (source, at)) in value.ty.components.iter().enumerate() {
            let begin = offset.max(*at);
            let end = (offset + scalar.size()).min(at + source.size());
            if begin >= end { continue; }
            if offset == *at && scalar.size() == source.size() {
                return self.result_component(value, index, scalar);
            }
            let source_raw = if source.size() == 8 { Scalar::U64 } else { Scalar::U32 };
            let mut word = self.result_component(value, index, source_raw);
            let mask = raw.bit_mask() >> (raw.bit_width() - (end - begin) * 8);
            for (name, op, target, bits) in [
                (">>", BinaryOp::ShiftRightLogical, source_raw, Some(Bits::from((begin - at) * 8))),
                ("&", BinaryOp::And, raw, Some(mask)),
                ("<<", BinaryOp::ShiftLeft, raw, Some(Bits::from((begin - offset) * 8))),
                ("|", BinaryOp::Or, raw, None),
            ] {
                word = self.cast(word, target);
                let rhs = if let Some(bits) = bits {
                    if (op == BinaryOp::And && bits == target.bit_mask()) || (op != BinaryOp::And && bits == 0) { continue; }
                    self.constant(target, bits)
                } else {
                    let Some(previous) = joined.take() else { continue; };
                    previous
                };
                let constant = word.constant.as_ref().zip(rhs.constant.as_ref())
                    .and_then(|(a,b)| constant::binary(op, target, a[0], b[0]).map(|bits| vec![bits]));
                let proof = bounds::arithmetic(self, name, &word, &rhs);
                let dst = self.reg(target);
                self.emit(InstKind::Binary { dst, op, a: word.regs[0], b: rhs.regs[0] });
                word = Value { ty: Ty::scalar(target), regs: vec![dst], proof, uniform: word.uniform && rhs.uniform,
                    literal: false, constant };
            }
            joined = Some(word);
        }
        let word = joined.unwrap_or_else(|| self.constant(raw, 0));
        let mut word = self.result_component(&word, 0, scalar);
        if scalar.is_integer() && value.nonzero_bytes(offset, scalar.size(), &self.residency_path()) {
            word.proof.mark_nonzero(0);
        }
        word
    }
    pub(super) fn result_value(&mut self, ty: Ty, success: bool, payload: Option<Value>) -> Value {
        self.variant_value(ty,usize::from(!success),payload)
    }
    pub(super) fn variant_value(&mut self, ty: Ty, index: usize, payload: Option<Value>) -> Value {
        let mut value = self.constant_value(ty.clone(), vec![0; ty.components.len()],None);
        let tag = self.constant(ty.components[0].0,Bits::from(ty.variants[index].discriminant));
        value.regs[0] = tag.regs[0];
        value
            .proof
            .assign(&[0], value.regs.len(), bounds::Proof::capture(&tag));
        if let Some(bits) = &mut value.constant {
            bits[0] = Bits::from(ty.variants[index].discriminant);
        }
        if let Some(payload) = payload {
            let offset = ty.variant_payload_offset();
            for (component, (scalar, position)) in ty.components.iter().enumerate().skip(1) {
                let word = self.result_word(&payload, position - offset, *scalar);
                value.regs[component] = word.regs[0];
                value.proof.assign(&[component], value.regs.len(), bounds::Proof::capture(&word));
                if let (Some(bits), Some(source)) = (&mut value.constant, &word.constant) {
                    bits[component] = source[0];
                } else {
                    value.constant = None;
                }
            }
            for component in view_contracts::components(&payload.ty) {
                let word=offset+payload.ty.components[component].1;
                let target=ty.components.iter().position(|(_,at)|*at==word).unwrap();
                value.proof=value.proof.with_view_borrow(target,payload.proof.view_borrow(component));
            }
            value.retain_payload_nonzero(&payload, offset);
            value.uniform = payload.uniform;
        }
        value.proof.retain_view_borrows(&ty);
        let offset=ty.variant_payload_offset();
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
        let offset = value.ty.variant_payload_offset();
        let mut proof = bounds::Proof::default();
        let mut regs = Vec::new();
        let mut constant = Some(Vec::new());
        for (index, (scalar, at)) in ty.components.iter().enumerate() {
            let fresh = self.result_word(value, offset + at, *scalar);
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
        for component in view_contracts::components(&ty) {
            let word=offset+ty.components[component].1;
            let source=value.ty.components.iter().position(|(_,at)|*at==word).unwrap();
            proof=proof.with_view_borrow(component,value.proof.view_borrow(source));
        }
        proof.retain_view_borrows(&ty);
        let proof = self.path_proof(&ty, proof);
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
        self.variant_condition(value,0,span,None)
    }
    pub(super) fn variant_condition(&mut self,value:&Value,index:usize,span:Span,origin:Option<Arc<GeneratedOrigin>>)->Result<Value> {
        let scalar=value.ty.components[0].0;
        let tag=self.result_word(value,0,scalar);
        let expected=self.constant_with_origin(scalar,Bits::from(value.ty.variants[index].discriminant),origin.clone());
        self.binary("==",tag,expected,span,origin)
    }
    pub(super) async fn check_variant_patterns(
        &self, ty: &Ty, arms: &MatchArms, span: Span,
    ) -> Result<Vec<MatchArm>> {
        if !ty.has_variants() { return self.fail(span, "B269", "match需要带载荷变体或result值"); }
        let mut selected = vec![None; ty.variants.len()];
        for arm in arms {
            let (owner, name) = arm.variant().rsplit_once("::").map_or((None, arm.variant()), |(owner, name)| (Some(owner), name));
            if let Some(owner) = owner {
                let mut syntax = ty.syntax(); syntax.name = owner.into(); syntax.canonical = false;
                let expected = unsafe { self.world.call(self.expected_storage(&syntax, arm.span())).await }?;
                self.require_same(ty, &expected, arm.span())?;
            }
            let Some(index) = ty.variants.iter().position(|v| v.name == name) else { return self.fail(arm.span(),"B269",format!("{}没有变体分支{name}",ty.name)); };
            if selected[index].replace(arm.clone()).is_some() { return self.fail(arm.span(),"B269","同一变体分支不能重复"); }
            let unit = ty.variants[index].ty.name == "void";
            if arm.binding().is_none() && !unit { return self.fail(arm.span(),"B269","有载荷分支需要名称或_模式"); }
            if unit && arm.binding().is_some_and(|name| name!="_") { return self.fail(arm.span(),"B269","无载荷分支没有可绑定的数据"); }
        }
        if selected.iter().any(Option::is_none) { return self.fail(span,"B269","match必须处理声明的全部变体分支"); }
        Ok(selected.into_iter().map(Option::unwrap).collect())
    }
    pub(super) async fn value_variant_arm(&mut self, value: &Value, index: usize, arm: &MatchArm) -> Result<()> {
        let payload = self.variant_payload(value,index);
        if let Some(name) = arm.binding().filter(|name| *name!="_") {
            self.bind_local_value(name,payload.unwrap(),arm.span())?;
            unsafe { self.world.call(self.block(&arm.body())).await }?; self.env.remove(name); Ok(())
        } else { unsafe { self.world.call(self.block(&arm.body())).await } }
    }
}
