//! 从已声明的 write／inout 参数追踪调用方位置写入，覆盖表达式与各控制流入口。
use super::*;

type ComponentProjection = (Ty, Vec<Vec<usize>>);

#[derive(Clone, Copy)]
struct Write<'e> {
    place: &'e Expr,
    contents: bool,
    iterator: Option<(&'e str, &'e Expr)>,
}

impl Builder<'_> {
    // 只投影可能改写的现有所有者，不求值下标或再次生成表达式代码。
    fn written_projection(
        &self,
        expr: &Expr,
        name: &str,
        owner: &Place,
        iterator: Option<(&str, &Expr)>,
        writes: &[Write<'_>],
    ) -> Result<Option<ComponentProjection>> {
        match &expr.kind {
            ExprKind::Name(root) => {
                if let Some((iterator, source)) = iterator
                    && root == iterator
                {
                    return Ok(self.application_write_projection(source, owner));
                }
                if root == name {
                    let ty = owner.ty().clone();
                    let components = (0..ty.components.len()).map(|i| vec![i]).collect();
                    Ok(Some((ty, components)))
                } else {
                    Ok(None)
                }
            }
            ExprKind::Member(base, field) => {
                let Some((ty, components)) =
                    self.written_projection(base, name, owner, iterator, writes)?
                else {
                    return Ok(None);
                };
                let (ty, selected) = self.member(&ty, field, expr.span)?;
                Ok(Some((
                    ty,
                    selected.iter().map(|i| components[*i].clone()).collect(),
                )))
            }
            ExprKind::Index(base, index) => {
                if let Some((ty, mut components)) = self.application_write_projection(base, owner) {
                    if let (Some(index), Place::ApplicationElement { index_range, .. }) =
                        (self.constant_index(index, writes)?, owner.storage())
                        && (index < index_range.0 || index > index_range.1)
                    {
                        for selected in &mut components {
                            selected.clear();
                        }
                    }
                    return Ok(Some((ty, components)));
                }
                if let ExprKind::Name(root) = &base.kind
                    && let Some(Binding::Data(view)) = self.env.get(root)
                {
                    if let Place::Element { binding, words, .. } = owner
                        && *binding == view.binding
                    {
                        let (base, ty) = self.dataset_layout(view);
                        let ty = ty.clone();
                        let components = ty
                            .components
                            .iter()
                            .map(|(_, offset)| {
                                words
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(i, word)| (*word == base + offset).then_some(i))
                                    .collect()
                            })
                            .collect();
                        return Ok(Some((ty, components)));
                    }
                    return Ok(None);
                }
                let Some((ty, components)) =
                    self.written_projection(base, name, owner, iterator, writes)?
                else {
                    return Ok(None);
                };
                let Some(element) = &ty.array else {
                    return Ok(None);
                };
                let constant = self.constant_index(index, writes)?;
                let width = element.components.len();
                let elements: Vec<_> = (0..ty.values[0] as usize)
                    .filter(|i| constant.is_none_or(|value| *i == value as usize))
                    .collect();
                let selected = (0..width)
                    .map(|component| {
                        elements
                            .iter()
                            .flat_map(|i| components[*i * width + component].iter().copied())
                            .collect()
                    })
                    .collect();
                Ok(Some(((**element).clone(), selected)))
            }
            _ => Ok(None),
        }
    }
    fn constant_index(&self, index: &Expr, writes: &[Write<'_>]) -> Result<Option<u32>> {
        Ok(match &index.kind {
            ExprKind::StaticU32(value) => Some(*value),
            ExprKind::Number(text) => {
                let (ty, value) = number(&self.definition.source, index.span, text)?;
                (ty == Ty::scalar(Scalar::U32)).then_some(value as u32)
            }
            ExprKind::Name(name)
                if !writes
                    .iter()
                    .any(|write| !write.contents && assigned_root(write.place) == Some(name)) =>
            {
                match self.env.get(name) {
                    Some(Binding::Value(value)) if value.ty == Ty::scalar(Scalar::U32) => {
                        value.constant.as_ref().map(|bits| bits[0] as u32)
                    }
                    Some(Binding::Owned(owner)) if *owner.place.ty() == Ty::scalar(Scalar::U32) => {
                        owner
                            .proof
                            .known_value(owner.place.ty())
                            .map(|bits| bits[0] as u32)
                    }
                    _ => None,
                }
            }
            _ => None,
        })
    }
    fn written_application_data(&self, expr: &Expr) -> Option<&application::Data> {
        if let ExprKind::Name(name) = &expr.kind
            && let Some(Binding::ApplicationData(data)) = self.env.get(name)
        {
            return Some(data);
        }
        let projection = self.owned_projection(expr)?;
        let Binding::OwnedValue(owner) = self.env.get(&projection.root)? else {
            return None;
        };
        let mut owner = owner;
        for index in projection.path {
            match owner.payloads[index].as_deref()? {
                returns::Returned::Dataset(data) => return Some(data),
                returns::Returned::OwnedValue(child) => owner = child,
                _ => return None,
            }
        }
        None
    }
    fn application_write_projection(
        &self,
        expr: &Expr,
        owner: &Place,
    ) -> Option<ComponentProjection> {
        let data = self.written_application_data(expr)?;
        let Place::ApplicationElement {
            identity, words, ..
        } = owner.storage()
        else {
            return None;
        };
        if *identity != data.handle.identity() {
            return None;
        }
        let components = data
            .ty
            .components
            .iter()
            .map(|(_, offset)| {
                if matches!(owner, Place::ApplicationElement { .. }) {
                    words
                        .iter()
                        .enumerate()
                        .filter_map(|(i, word)| (*word == *offset).then_some(i))
                        .collect()
                } else if words.contains(&(*offset)) {
                    (0..owner.ty().components.len()).collect()
                } else {
                    Vec::new()
                }
            })
            .collect();
        Some((data.ty.clone(), components))
    }
    fn invalidate_writes(
        &mut self,
        writes: &[Write<'_>],
        borrowed_data: Option<ValueIdentity>,
    ) -> Result<()> {
        let resources: Vec<_> = self
            .env
            .iter()
            .filter_map(|(name, binding)| {
                let Binding::ApplicationData(data) = binding else {
                    return None;
                };
                (borrowed_data != Some(data.handle.identity())
                    && !data
                        .handle
                        .proof
                        .known_identity(0)
                        .is_some_and(|id| self.application_borrowed(id))
                    && writes.iter().any(|write| {
                        !write.contents
                            && matches!(&write.place.kind, ExprKind::Name(target) if target == name)
                    }))
                .then(|| name.clone())
            })
            .collect();
        for name in resources {
            if let Some(Binding::ApplicationData(data)) = self.env.get_mut(&name) {
                data.handle.proof = bounds::Proof::default();
                data.length.proof = data
                    .count
                    .map(|count| bounds::Proof::unsigned(count, count))
                    .unwrap_or_default();
                data.length.constant = None;
                data.length.uniform = false;
            }
        }
        let mut modified: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        for (name, binding) in &self.env {
            if let Some(owner) = binding.stored_place() {
                for write in writes {
                    let projection = if write.contents {
                        self.application_write_projection(write.place, &owner)
                    } else {
                        self.written_projection(write.place, name, &owner, write.iterator, writes)?
                    };
                    if let Some((_, components)) = projection {
                        for component in components.into_iter().flatten() {
                            modified.entry(name.clone()).or_default().insert(component);
                        }
                    }
                }
            }
        }
        for (name, binding) in &mut self.env {
            if let Some(components) = modified.get(name) {
                let components: Vec<_> = components.iter().copied().collect();
                match binding {
                    Binding::Owned(owner) => {
                        owner.uniform = false;
                        owner.proof.forget_value(&components,owner.place.ty().components.len());
                    }
                    Binding::OwnedValue(owner) => {
                        owner.invalidate_components(&components, borrowed_data)
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
    pub(super) fn invalidate_expression(&mut self, expr: &Expr) -> Result<()> {
        let mut writes = Vec::new();
        expression_writes(self.world, &self.definition, expr, &mut |place| {
            writes.push(place)
        });
        self.invalidate_writes(&writes, None)
    }
    pub(super) fn invalidate_assigned(
        &mut self,
        body: &[Stmt],
        borrowed_data: Option<ValueIdentity>,
    ) -> Result<()> {
        let mut writes = Vec::new();
        statement_writes(self.world, &self.definition, body, &mut |place| {
            writes.push(place)
        });
        self.invalidate_writes(&writes, borrowed_data)
    }
}

fn assigned_root(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Name(name) => Some(name),
        ExprKind::Member(base, _) | ExprKind::Index(base, _) => assigned_root(base),
        _ => None,
    }
}
fn expression_writes<'e>(
    world: &World,
    definition: &Definition,
    expr: &'e Expr,
    writes: &mut dyn FnMut(Write<'e>),
) {
    match &expr.kind {
        ExprKind::Call(name, args)
        | ExprKind::Cooperate(name, _, args)
        | ExprKind::Specialize(name, _, args) => {
            let record = matches!(expr.kind, ExprKind::Specialize(..))
                && world
                    .resolve(name, &definition.module, &definition.imports, false)
                    .is_some();
            if !record
                && let Some(key) =
                    world.resolve(name, &definition.module, &definition.imports, true)
            {
                for (param, arg) in world.functions[&key].function.params.iter().zip(args) {
                    if matches!(param.access, Access::InOut | Access::Write) && !param.ty.tile {
                        writes(Write {
                            place: arg,
                            contents: param.ty.dataset,
                            iterator: None,
                        });
                    }
                }
            }
            for arg in args {
                expression_writes(world, definition, arg, writes);
            }
        }
        ExprKind::Construct(_, args) => {
            for arg in args {
                expression_writes(world, definition, arg, writes);
            }
        }
        ExprKind::Target(_, base) | ExprKind::Member(base, _) | ExprKind::Unary(_, base) => {
            expression_writes(world, definition, base, writes)
        }
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => {
            expression_writes(world, definition, a, writes);
            expression_writes(world, definition, b, writes);
        }
        ExprKind::Select(a, b, c) => {
            expression_writes(world, definition, a, writes);
            expression_writes(world, definition, b, writes);
            expression_writes(world, definition, c, writes);
        }
        _ => {}
    }
}
pub(super) fn value_writes<'e>(world:&World,definition:&Definition,body:&'e[Stmt])->Vec<&'e Expr>{
    let mut values=Vec::new();statement_writes(world,definition,body,&mut|write|{if !write.contents&&write.iterator.is_none(){values.push(write.place);}});values
}
fn statement_writes<'e>(
    world: &World,
    definition: &Definition,
    code: &'e [Stmt],
    writes: &mut dyn FnMut(Write<'e>),
) {
    for s in code {
        match &s.kind {
            StmtKind::Assign(lhs, _, rhs) => {
                writes(Write {
                    place: lhs,
                    contents: false,
                    iterator: None,
                });
                expression_writes(world, definition, lhs, writes);
                expression_writes(world, definition, rhs, writes);
            }
            StmtKind::Let(_, _, expr)
            | StmtKind::Expr(expr)
            | StmtKind::Return(Some(expr))
            | StmtKind::Seek(_, expr) => expression_writes(world, definition, expr, writes),
            StmtKind::If(expr, a, b) => {
                expression_writes(world, definition, expr, writes);
                statement_writes(world, definition, a, writes);
                statement_writes(world, definition, b, writes);
            }
            StmtKind::Match(value, arms) => {
                expression_writes(world,definition,value,writes);
                for arm in arms {statement_writes(world,definition,&arm.body,writes);}
            }
            StmtKind::While(expr, body) => {
                expression_writes(world, definition, expr, writes);
                statement_writes(world, definition, body, writes);
            }
            StmtKind::For(name, start, end, body) => {
                expression_writes(world, definition, start, writes);
                if let Some(end) = end {
                    expression_writes(world, definition, end, writes);
                }
                statement_writes(world, definition, body, &mut |mut write| {
                    if end.is_none() && assigned_root(write.place) == Some(name) {
                        write.iterator = Some((name, start));
                    }
                    writes(write);
                });
            }
            StmtKind::Parallel(_, domain, body) => {
                if let Some(count) = &domain.count {
                    expression_writes(world, definition, count, writes);
                }
                if let Some(selection) = &domain.selection {
                    expression_writes(world, definition, selection, writes);
                }
                statement_writes(world, definition, body, writes);
            }
            _ => {}
        }
    }
}
pub(super) fn expression_assignments(
    world: &World,
    definition: &Definition,
    expr: &Expr,
    names: &mut BTreeSet<String>,
) {
    expression_writes(world, definition, expr, &mut |write| {
        if !write.contents
            && let Some(root) = assigned_root(write.place)
        {
            names.insert(root.to_owned());
        }
    });
}
pub(super) fn assigned_names(
    world: &World,
    definition: &Definition,
    code: &[Stmt],
    names: &mut BTreeSet<String>,
) {
    statement_writes(world, definition, code, &mut |write| {
        if !write.contents
            && let Some(root) = assigned_root(write.place)
        {
            names.insert(root.to_owned());
        }
    });
}
pub(super) fn modified_owners(
    env: &BTreeMap<String, Binding>,
    names: &BTreeSet<String>,
) -> BTreeSet<String> {
    let data: BTreeSet<_> = names
        .iter()
        .filter_map(|name| {
            if let Some(Binding::Data(view)) = env.get(name) {
                Some(view.binding)
            } else {
                None
            }
        })
        .collect();
    let application: BTreeSet<_> = names
        .iter()
        .flat_map(|name| match env.get(name) {
            Some(Binding::ApplicationData(data)) => vec![data],
            Some(Binding::OwnedValue(owner)) => owner.resources(),
            _ => Vec::new(),
        })
        .map(|data| data.handle.identity())
        .collect();
    env.iter()
        .filter_map(|(name, binding)| {
            let owner = binding.stored_place()?;
            let through_data =
                matches!(owner,Place::Element{binding,..} if data.contains(&binding));
            let through_application = matches!(owner.storage(), Place::ApplicationElement { identity, .. } if application.contains(identity));
            (names.contains(name) || through_data || through_application).then(|| name.clone())
        })
        .collect()
}


// 同一位置写效果的数学位移。Unknown仍是未知，不改变实际整数回绕语义。
#[derive(Clone, Copy)]
enum NumericEffect {
    Unknown,
    Shift { low: i128, high: i128, least: i128, most: i128 },
}
impl NumericEffect {
    fn shift(amount: i128) -> Self { Self::Shift { low: amount, high: amount, least: amount.min(0), most: amount.max(0) } }
    fn then(self, next: Self) -> Self {
        let (Self::Shift {low:a,high:b,least:c,most:d}, Self::Shift {low:e,high:f,least:g,most:h}) = (self,next) else { return Self::Unknown; };
        let (Some(low),Some(high),Some(least),Some(most)) = (a.checked_add(e),b.checked_add(f),a.checked_add(g),b.checked_add(h)) else { return Self::Unknown; };
        Self::Shift {low,high,least:c.min(least),most:d.max(most)}
    }
    fn join(self, other: Self) -> Self {
        let (Self::Shift {low:a,high:b,least:c,most:d}, Self::Shift {low:e,high:f,least:g,most:h}) = (self,other) else { return Self::Unknown; };
        Self::Shift {low:a.min(e),high:b.max(f),least:c.min(g),most:d.max(h)}
    }
}
fn join_numeric_paths(a: Option<NumericEffect>, b: Option<NumericEffect>) -> Option<NumericEffect> {
    match (a,b) { (Some(a),Some(b))=>Some(a.join(b)), (a,None)|(None,a)=>a }
}
impl Builder<'_> {
    fn numeric_write(&self, write: Write<'_>, name: &str, owner: &Place, component: usize, writes: &[Write<'_>]) -> Result<bool> {
        let projection = if write.contents { self.application_write_projection(write.place, owner) }
            else { self.written_projection(write.place, name, owner, write.iterator, writes)? };
        Ok(projection.is_some_and(|(_,parts)|parts.iter().any(|part|part.contains(&component))))
    }
    fn numeric_expression_effect(&self, expr: &Expr, name: &str, owner: &Place, component: usize, writes: &[Write<'_>]) -> Result<NumericEffect> {
        let mut changes=Vec::new();expression_writes(self.world,&self.definition,expr,&mut|write|changes.push(write));
        for write in changes { if self.numeric_write(write,name,owner,component,writes)? { return Ok(NumericEffect::Unknown); } }
        Ok(NumericEffect::shift(0))
    }
    fn numeric_assignment_effect(&self, lhs: &Expr, op: &str, rhs: &Expr, name: &str, owner: &Place, component: usize, writes: &[Write<'_>]) -> Result<NumericEffect> {
        let Some((ty,parts))=self.written_projection(lhs,name,owner,None,writes)? else { return Ok(NumericEffect::shift(0)); };
        if !parts.iter().any(|part|part.contains(&component)) { return Ok(NumericEffect::shift(0)); }
        // 动态候选位置不被当作同一实际分量。
        if parts.iter().any(|part|part.contains(&component)&&part.len()!=1) { return Ok(NumericEffect::Unknown); }
        let relative=|base:&Expr|->Result<bool>{Ok(self.written_projection(base,name,owner,None,writes)?.is_some_and(|(source,selected)|source==ty&&selected==parts))};
        let (sign,amount)=match op {
            "+=" => (1,self.constant_index(rhs,writes)?),
            "-=" => (-1,self.constant_index(rhs,writes)?),
            "=" => match &rhs.kind {
                ExprKind::Binary(operation,base,amount) if matches!(operation.as_str(),"+"|"-")&&relative(base)? => (if operation=="+"{1}else{-1},self.constant_index(amount,writes)?),
                ExprKind::Binary(operation,amount,base) if operation=="+"&&relative(base)? => (1,self.constant_index(amount,writes)?),
                _ if relative(rhs)? => (1,Some(0)),
                _ => return Ok(NumericEffect::Unknown),
            },
            _ => return Ok(NumericEffect::Unknown),
        };
        Ok(amount.map(|amount|NumericEffect::shift(i128::from(amount)*sign)).unwrap_or(NumericEffect::Unknown))
    }
    fn numeric_loop_paths(&self, body:&[Stmt], mut next:Option<NumericEffect>, continues:&mut Option<NumericEffect>, name:&str, owner:&Place, component:usize, writes:&[Write<'_>])->Result<Option<NumericEffect>> {
        for statement in body {
            let Some(before)=next.take() else { break; };
            next=match &statement.kind {
                StmtKind::Assign(lhs,op,rhs)=>Some(before
                    .then(self.numeric_expression_effect(lhs,name,owner,component,writes)?)
                    .then(self.numeric_expression_effect(rhs,name,owner,component,writes)?)
                    .then(self.numeric_assignment_effect(lhs,op,rhs,name,owner,component,writes)?)),
                StmtKind::Let(_,_,expr)|StmtKind::Expr(expr)|StmtKind::Seek(_,expr)=>Some(before.then(self.numeric_expression_effect(expr,name,owner,component,writes)?)),
                StmtKind::If(condition,yes,no)=>{
                    let before=before.then(self.numeric_expression_effect(condition,name,owner,component,writes)?);
                    let yes=self.numeric_loop_paths(yes,Some(before),continues,name,owner,component,writes)?;
                    let no=self.numeric_loop_paths(no,Some(before),continues,name,owner,component,writes)?;
                    join_numeric_paths(yes,no)
                },
                StmtKind::Match(value,arms)=>{
                    let before=before.then(self.numeric_expression_effect(value,name,owner,component,writes)?);let mut next=None;
                    for arm in arms { next=join_numeric_paths(next,self.numeric_loop_paths(&arm.body,Some(before),continues,name,owner,component,writes)?); }next
                },
                StmtKind::Continue=>{*continues=join_numeric_paths(*continues,Some(before));None},
                StmtKind::Break|StmtKind::Return(_)=>None,
                StmtKind::While(..)|StmtKind::For(..)|StmtKind::Parallel(..)=>{
                    let mut changes=Vec::new();statement_writes(self.world,&self.definition,std::slice::from_ref(statement),&mut|write|changes.push(write));
                    let mut effect=NumericEffect::shift(0);for write in changes { if self.numeric_write(write,name,owner,component,writes)? {effect=NumericEffect::Unknown;break;} }
                    Some(before.then(effect))
                },
                _=>Some(before),
            };
        }
        Ok(next)
    }
    pub(super) fn retain_while_bounds(&mut self, condition:&Expr, body:&[Stmt], initial:&BTreeMap<String,Binding>, header:&mut BTreeMap<String,Binding>)->Result<()> {
        let mut writes=Vec::new();statement_writes(self.world,&self.definition,body,&mut|write|writes.push(write));expression_writes(self.world,&self.definition,condition,&mut|write|writes.push(write));
        let mut retained=Vec::new();
        for (name,binding) in initial {
            let (owner,proof)=match binding { Binding::Owned(owner)=>(owner.place.clone(),&owner.proof),Binding::OwnedValue(owner)=>{let Some(place)=owner.place()else{continue};(place,&owner.value.proof)},_=>continue };
            // 这里只解释有确定位置身份的本地／独占参数；数据候选别名仍由原写效果撤回。
            if !matches!(owner.storage(),Place::Local(..)|Place::Reference{..}) {continue;}
            for (component,(scalar,_)) in owner.ty().components.iter().enumerate() {
                if *scalar!=Scalar::U32 {continue;}
                let mut written=false;for write in &writes {if self.numeric_write(*write,name,&owner,component,&writes)? {written=true;break;}}
                if !written {continue;}
                if matches!(self.numeric_expression_effect(condition,name,&owner,component,&writes)?,NumericEffect::Unknown) {continue;}
                let mut continues=None;let next=self.numeric_loop_paths(body,Some(NumericEffect::shift(0)),&mut continues,name,&owner,component,&writes)?;
                let (lower,upper,least,most)=match join_numeric_paths(next,continues) {
                    None=>(true,true,0,0),
                    Some(NumericEffect::Shift{low,high,least,most})=>(low>=0,high<=0,least,most),
                    Some(NumericEffect::Unknown)=>continue,
                };
                if !lower&&!upper {continue;}
                let guard=match self.env.get(name) {Some(Binding::Owned(value))=>&value.proof,Some(Binding::OwnedValue(value))=>&value.value.proof,_=>continue};
                let low=guard.unsigned_minimum(component).max(if lower{proof.unsigned_minimum(component)}else{0});
                let high=guard.unsigned_maximum(component).min(if upper{proof.unsigned_maximum(component)}else{u32::MAX});
                if i128::from(low)+least<0||i128::from(high)+most>i128::from(u32::MAX) {continue;}
                retained.push((name.clone(),component,proof.clone(),lower,upper));
            }
        }
        for (name,component,proof,lower,upper) in retained {
            for environment in [&mut self.env,&mut *header] {if let Some(binding)=environment.get_mut(&name)&&let Some((current,_,_))=binding.proof_storage(){current.retain_loop_bounds(&proof,component,lower,upper);}}
        }
        Ok(())
    }
}
