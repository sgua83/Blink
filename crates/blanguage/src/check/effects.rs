//! 从已声明的 write／inout 参数追踪调用方位置写入，覆盖表达式与各控制流入口。
use super::*;
mod sources;
use sources::{Local, Scope};

type ComponentProjection = (Ty, Vec<Vec<usize>>);

#[derive(Clone, Copy)]
enum Write<'e> {
    Position {
        place: &'e Expr,
        call: Option<&'e Expr>,
        contents: bool,
        iterator: Option<(&'e str, &'e Expr)>,
    },
    External(&'e Expr),
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
                let (ty, selected) = member_type(&ty, field, &self.definition.source, expr.span)?;
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
            ExprKind::Specialize(name,explicit,args) if name=="unsafe_position"=>{
                let ([GenericArgument::Type(syntax)],[arg])=(explicit.as_slice(),args.as_slice()) else{return Ok(None);};
                let ty=self.world.ty(syntax,&self.definition.source,expr.span,&self.definition.module,&self.definition.imports)?;
                let known=if let ExprKind::Name(name)=&arg.kind {
                    self.env.get(name).and_then(Binding::view_parts).and_then(|(address,proof)|{
                        if !address.is_address(){return None;}
                        let borrow=address::borrow_in_path(&self.current_address_borrow(&proof.view_borrow(0)),&self.path);
                        (!borrow.pending()&&!address::external_memory(&borrow)&&borrow.sources.iter().all(|source|matches!(source,view::Source::Position(_)))).then(||{
                            address::borrowed_positions(&borrow,&address::Layout::of(&ty),&ty.components.iter().map(|(_,word)|*word).collect::<Vec<_>>())
                        })
                    })
                }else{None};
                let components=match known {
                    Some(positions)=>{
                        let actual=self.positions(owner);
                        positions.iter().map(|written|actual.iter().enumerate().filter_map(|(i,position)|
                            (owned_presence::Presence::selected(written,position)!=owned_presence::Presence::None).then_some(i)).collect()).collect()
                    }
                    None=>vec![(0..owner.ty().components.len()).collect();ty.components.len()],
                };
                Ok(Some((ty,components)))
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
                    .any(|write| match write {
                        Write::External(_)=>true,
                        Write::Position{contents:false,place,..}=>assigned_root(place)==Some(name)||address_write(place),
                        _=>false,
                    }) =>
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
        if *identity != data.root.identity() {
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
                (borrowed_data != Some(data.root.identity())
                    && !data
                        .root
                        .proof
                        .known_identity(0)
                        .is_some_and(|id| self.application_borrowed(id))
                    && writes.iter().any(|write| matches!(write,
                        Write::Position{contents:false,place,..} if matches!(&place.kind, ExprKind::Name(target) if target == name))))
                .then(|| name.clone())
            })
            .collect();
        for name in resources {
            if let Some(Binding::ApplicationData(data)) = self.env.get_mut(&name) {
                data.root.proof = bounds::Proof::default();
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
                    let components = match *write {
                        Write::External(_)=>vec![(0..owner.ty().components.len()).collect()],
                        Write::Position{place,contents,iterator,..}=>{
                            let projection=if contents {self.application_write_projection(place,&owner)}
                                else {self.written_projection(place,name,&owner,iterator,writes)?};
                            projection.map(|(_,components)|components).unwrap_or_default()
                        },
                    };
                    for component in components.into_iter().flatten() {
                        modified.entry(name.clone()).or_default().insert(component);
                    }
                }
            }
        }
        let raw=writes.iter().any(|write|match write {
            Write::External(_)=>true,
            Write::Position{contents:false,place,..}=>address_write(place),
            _=>false,
        });
        for (name, binding) in &mut self.env {
            if let Some(components) = modified.get(name) {
                let components: Vec<_> = components.iter().copied().collect();
                if raw && let Some((proof,_,storage))=binding.proof_storage() {
                    for(offset,field)in storage.ty().borrow_fields() {
                        if field.is_address() {
                            let component=storage.ty().components.iter().position(|(_,at)|*at==offset).unwrap();
                            if components.contains(&component){
                                let borrow=proof.view_borrow(component).joined(view::Borrow::source(view::Source::External));
                                *proof=std::mem::take(proof).with_view_borrow(component,borrow);
                            }
                        }
                    }
                }
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
        expression_writes(self, expr, &mut |place| {
            writes.push(place)
        })?;
        self.invalidate_writes(&writes, None)
    }
    pub(super) fn invalidate_assigned(
        &mut self,
        body: &[Stmt],
        borrowed_data: Option<ValueIdentity>,
    ) -> Result<()> {
        let mut writes = Vec::new();
        statement_writes(self, body, &mut |place| {
            writes.push(place)
        })?;
        self.invalidate_writes(&writes, borrowed_data)
    }
}

fn address_write(expr:&Expr)->bool {
    match &expr.kind {
        ExprKind::Specialize(name,_,_)=>name=="unsafe_position",
        ExprKind::Member(base,_)|ExprKind::Index(base,_)=>address_write(base),
        _=>false,
    }
}
fn assigned_root(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Name(name) => Some(name),
        ExprKind::Member(base, _) | ExprKind::Index(base, _) => assigned_root(base),
        _ => None,
    }
}
impl<'b, 'w, 'e> Scope<'b, 'w, 'e> {
    fn function_writes(&self, ty: Ty, args: &'e [Expr], call: &'e Expr, writes: &mut dyn FnMut(Write<'e>)) -> Result<()> {
        let Some(function) = &ty.function else {
            return self.definition.source.fail(call.span, "B263", "间接调用目标需要cfn原生C签名值");
        };
        writes(Write::External(call));
        for ((access, _), arg) in function.parameters.iter().zip(args) {
            if matches!(access, Access::Write | Access::InOut) {
                writes(Write::Position { place: arg, call: Some(call), contents: false, iterator: None });
            }
        }
        Ok(())
    }
    fn writes_expression(&self, expr: &'e Expr, writes: &mut dyn FnMut(Write<'e>)) -> Result<()> {
        match &expr.kind {
            ExprKind::Invoke(callee, args) => {
                let ty = self.expression(callee)?.ok_or_else(|| Diagnostic {
                    source: self.definition.source.clone(), span: callee.span, code: "B263",
                    message: "不能从原声明与词法来源取得间接调用签名".into(),
                })?;
                self.function_writes(ty, args, expr, writes)?;
                self.writes_expression(callee, writes)?;
                for arg in args { self.writes_expression(arg, writes)?; }
            }
            ExprKind::Call(name, args) | ExprKind::Cooperate(name, _, args) | ExprKind::Specialize(name, _, args) => {
                if self.builder.is_none() && name == "address_of" && let [place] = args.as_slice() {
                    writes(Write::Position { place, call: Some(expr), contents: false, iterator: None });
                }
                let local = if matches!(expr.kind, ExprKind::Call(..)) { self.named(name, expr.span)? } else { None };
                if let Some(ty) = local { self.function_writes(ty, args, expr, writes)?; }
                else {
                    let record = matches!(expr.kind, ExprKind::Specialize(..))
                        && self.world.resolve(name, &self.definition.module, &self.definition.imports, false).is_some();
                    if !record && let Some(key) = self.world.resolve(name, &self.definition.module, &self.definition.imports, true) {
                        if self.world.functions[&key].function.c_abi {writes(Write::External(expr));}
                        for (param, arg) in self.world.functions[&key].function.params.iter().zip(args) {
                            if matches!(param.access, Access::InOut | Access::Write) && !param.ty.tile {
                                writes(Write::Position { place: arg, call: Some(expr), contents: param.ty.dataset, iterator: None });
                            }
                        }
                    }
                }
                for arg in args { self.writes_expression(arg, writes)?; }
            }
            ExprKind::Construct(_, args) => for arg in args { self.writes_expression(arg, writes)?; },
            ExprKind::Target(_, base) | ExprKind::Member(base, _) | ExprKind::Unary(_, base) => self.writes_expression(base, writes)?,
            ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => { self.writes_expression(a, writes)?; self.writes_expression(b, writes)?; }
            ExprKind::Select(a, b, c) => { self.writes_expression(a, writes)?; self.writes_expression(b, writes)?; self.writes_expression(c, writes)?; }
            _ => {}
        }
        Ok(())
    }
    fn writes_statements(&mut self, code: &'e [Stmt], writes: &mut dyn FnMut(Write<'e>)) -> Result<()> {
        let outer = self.bindings.len();
        for statement in code {
            match &statement.kind {
                StmtKind::Declare(ty, name) => self.bindings.push((name, Local::Typed(ty))),
                StmtKind::Let(ty, name, expr) => {
                    self.writes_expression(expr, writes)?;
                    self.bindings.push((name, ty.as_ref().map_or(Local::Inferred(expr), Local::Typed)));
                }
                StmtKind::Assign(lhs, _, rhs) => {
                    writes(Write::Position { place: lhs, call: None, contents: false, iterator: None });
                    self.writes_expression(lhs, writes)?;
                    self.writes_expression(rhs, writes)?;
                }
                StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) | StmtKind::Seek(_, expr) => self.writes_expression(expr, writes)?,
                StmtKind::If(expr, yes, no) => {
                    self.writes_expression(expr, writes)?;
                    self.writes_statements(yes, writes)?; self.writes_statements(no, writes)?;
                }
                StmtKind::Match(value, arms) => {
                    self.writes_expression(value, writes)?;
                    for arm in arms {
                        if let Some(name) = &arm.binding { self.bindings.push((name, Local::Payload(value, &arm.variant))); }
                        self.writes_statements(&arm.body, writes)?;
                        if arm.binding.is_some() { self.bindings.pop(); }
                    }
                }
                StmtKind::While(expr, body) => { self.writes_expression(expr, writes)?; self.writes_statements(body, writes)?; }
                StmtKind::For(name, start, end, body) => {
                    self.writes_expression(start, writes)?;
                    if let Some(end) = end { self.writes_expression(end, writes)?; }
                    self.bindings.push((name, if end.is_some() { Local::Range } else { Local::Element(start) }));
                    self.writes_statements(body, &mut |mut write| {
                        if let Write::Position{place,iterator,..}=&mut write
                            && end.is_none() && assigned_root(place)==Some(name) {*iterator=Some((name,start));}
                        writes(write);
                    })?;
                    self.bindings.pop();
                }
                StmtKind::Parallel(name, domain, body) => {
                    if let Some(count) = &domain.count { self.writes_expression(count, writes)?; }
                    if let Some(selection) = &domain.selection { self.writes_expression(selection, writes)?; }
                    self.bindings.push((name, Local::NamedElement(&domain.dataset)));
                    self.writes_statements(body, writes)?; self.bindings.pop();
                }
                _ => {}
            }
        }
        self.bindings.truncate(outer);
        Ok(())
    }
}
fn expression_writes<'e>(builder: &Builder<'_>, expr: &'e Expr, writes: &mut dyn FnMut(Write<'e>)) -> Result<()> {
    Scope::current(builder).writes_expression(expr, writes)
}
fn statement_writes<'e>(builder: &Builder<'_>, code: &'e [Stmt], writes: &mut dyn FnMut(Write<'e>)) -> Result<()> {
    Scope::current(builder).writes_statements(code, writes)
}
pub(super) fn value_writes<'e>(builder: &Builder<'_>, body: &'e [Stmt]) -> Result<Vec<&'e Expr>> {
    let mut values = Vec::new();
    statement_writes(builder, body, &mut |write| {
        if let Write::Position{contents:false,iterator:None,place,..}=write {values.push(place);}
    })?;
    Ok(values)
}
pub(super) fn expression_assignments(builder: &Builder<'_>, expr: &Expr, names: &mut BTreeSet<String>) -> Result<()> {
    expression_writes(builder, expr, &mut |write| {
        if let Write::Position{contents:false,place,..}=write
            && let Some(root)=assigned_root(place) {names.insert(root.to_owned());}
    })
}
pub(super) fn assigned_names(world: &World, definition: &Definition, code: &[Stmt], names: &mut BTreeSet<String>) -> Result<()> {
    Scope::declared(world, definition).writes_statements(code, &mut |write| {
        if let Write::Position{contents:false,place,..}=write
            && let Some(root)=assigned_root(place) {names.insert(root.to_owned());}
    })
}
fn contains_expression(root: &Expr, selected: &Expr) -> bool {
    if std::ptr::eq(root, selected) { return true; }
    match &root.kind {
        ExprKind::Invoke(callee, args) => contains_expression(callee, selected) || args.iter().any(|arg| contains_expression(arg, selected)),
        ExprKind::Call(_, args) | ExprKind::Specialize(_, _, args) | ExprKind::Cooperate(_, _, args) | ExprKind::Construct(_, args) =>
            args.iter().any(|arg| contains_expression(arg, selected)),
        ExprKind::Target(_, base) | ExprKind::Member(base, _) | ExprKind::Unary(_, base) => contains_expression(base, selected),
        ExprKind::Index(a, b) | ExprKind::Binary(_, a, b) => contains_expression(a, selected) || contains_expression(b, selected),
        ExprKind::Select(a, b, c) => contains_expression(a, selected) || contains_expression(b, selected) || contains_expression(c, selected),
        _ => false,
    }
}
fn contains_statement(statement: &Stmt, selected: &Expr) -> bool {
    let expression = |expr: &Expr| contains_expression(expr, selected);
    let block = |body: &[Stmt]| body.iter().any(|statement| contains_statement(statement, selected));
    match &statement.kind {
        StmtKind::Let(_, _, expr) | StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) | StmtKind::Seek(_, expr) => expression(expr),
        StmtKind::Assign(a, _, b) => expression(a) || expression(b),
        StmtKind::If(expr, yes, no) => expression(expr) || block(yes) || block(no),
        StmtKind::Match(expr, arms) => expression(expr) || arms.iter().any(|arm| block(&arm.body)),
        StmtKind::While(expr, body) => expression(expr) || block(body),
        StmtKind::For(_, start, end, body) => expression(start) || end.as_ref().is_some_and(expression) || block(body),
        StmtKind::Parallel(_, domain, body) => domain.count.as_ref().is_some_and(expression) || domain.selection.as_ref().is_some_and(expression) || block(body),
        _ => false,
    }
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
        .map(|data| data.root.identity())
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


// 同一位置写效果的位移与独立赋值。Unknown仍是未知，不改变实际整数回绕语义。
#[derive(Clone)]
enum NumericEffect {
    Unknown,
    Assign(bounds::Proof),
    Shift { low: i128, high: i128, least: i128, most: i128 },
}
impl NumericEffect {
    fn shift(amount: i128) -> Self { Self::Shift { low: amount, high: amount, least: amount.min(0), most: amount.max(0) } }
    fn then(self, next: Self) -> Self {
        match (&self,&next) {
            (_,Self::Assign(_))=>return next,
            (Self::Assign(_),Self::Shift{low:0,high:0,least:0,most:0})=>return self,
            _=>{},
        }
        let (Self::Shift {low:a,high:b,least:c,most:d}, Self::Shift {low:e,high:f,least:g,most:h}) = (self,next) else { return Self::Unknown; };
        let (Some(low),Some(high),Some(least),Some(most)) = (a.checked_add(e),b.checked_add(f),a.checked_add(g),b.checked_add(h)) else { return Self::Unknown; };
        Self::Shift {low,high,least:c.min(least),most:d.max(most)}
    }
    fn join(self, other: Self) -> Self {
        match (self,other) {
            (Self::Assign(mut a),Self::Assign(b))=>{a.join(&b,1,None);Self::Assign(a)},
            (a@Self::Assign(_),Self::Shift{low:0,high:0,least:0,most:0})
            |(Self::Shift{low:0,high:0,least:0,most:0},a@Self::Assign(_))=>a,
            (Self::Shift{low:a,high:b,least:c,most:d},Self::Shift{low:e,high:f,least:g,most:h})=>
                Self::Shift{low:a.min(e),high:b.max(f),least:c.min(g),most:d.max(h)},
            _=>Self::Unknown,
        }
    }
}
fn join_numeric_paths(a: Option<NumericEffect>, b: Option<NumericEffect>) -> Option<NumericEffect> {
    match (a,b) { (Some(a),Some(b))=>Some(a.join(b)), (a,None)|(None,a)=>a }
}
impl Builder<'_> {
    fn numeric_write(&self, write: Write<'_>, name: &str, owner: &Place, component: usize, writes: &[Write<'_>]) -> Result<bool> {
        let Write::Position{place,contents,iterator,..}=write else{return Ok(true);};
        let projection = if contents { self.application_write_projection(place, owner) }
            else { self.written_projection(place, name, owner, iterator, writes)? };
        Ok(projection.is_some_and(|(_,parts)|parts.iter().any(|part|part.contains(&component))))
    }
    fn numeric_expression_effect(&self, expr: &Expr, name: &str, owner: &Place, component: usize, writes: &[Write<'_>]) -> Result<NumericEffect> {
        for write in writes.iter().filter(|write|match write {
            Write::External(call)=>contains_expression(expr,call),
            Write::Position{call,..}=>call.is_some_and(|call|contains_expression(expr,call)),
        }) {
            if self.numeric_write(*write,name,owner,component,writes)? { return Ok(NumericEffect::Unknown); }
        }
        Ok(NumericEffect::shift(0))
    }
    fn numeric_assignment_effect(&self, lhs: &Expr, op: &str, rhs: &Expr, name: &str, owner: &Place, component: usize, writes: &[Write<'_>], absolute: bool) -> Result<NumericEffect> {
        let Some((ty,parts))=self.written_projection(lhs,name,owner,None,writes)? else { return Ok(NumericEffect::shift(0)); };
        if !parts.iter().any(|part|part.contains(&component)) { return Ok(NumericEffect::shift(0)); }
        // 未知地址可能写此分量；单个候选分量不表示该次写必然选中了它。
        if address_write(lhs){return Ok(NumericEffect::Unknown);}
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
                _ => {
                    let value=if let Some(value)=self.constant_index(rhs,writes)? {bounds::Proof::unsigned(value,value)}
                    else if absolute && let ExprKind::Name(source)=&rhs.kind {
                        match self.env.get(source) {
                            Some(Binding::Value(value)) if value.ty==Ty::scalar(Scalar::U32)=>bounds::Proof::capture(value),
                            Some(Binding::Owned(value)) if value.place.ty()==&Ty::scalar(Scalar::U32)=>{
                                for write in writes {if self.numeric_write(*write,source,&value.place,0,writes)? {return Ok(NumericEffect::Unknown);}}
                                value.proof.clone()
                            },
                            _=>return Ok(NumericEffect::Unknown),
                        }
                    } else {return Ok(NumericEffect::Unknown);};
                    return Ok(NumericEffect::Assign(value));
                },
            },
            _ => return Ok(NumericEffect::Unknown),
        };
        Ok(amount.map(|amount|NumericEffect::shift(i128::from(amount)*sign)).unwrap_or(NumericEffect::Unknown))
    }
    fn numeric_loop_paths(&self, body:&[Stmt], mut next:Option<NumericEffect>, continues:&mut Option<NumericEffect>, name:&str, owner:&Place, component:usize, writes:&[Write<'_>], absolute:bool)->Result<Option<NumericEffect>> {
        for statement in body {
            let Some(before)=next.take() else { break; };
            next=match &statement.kind {
                StmtKind::Assign(lhs,op,rhs)=>Some(before
                    .then(self.numeric_expression_effect(lhs,name,owner,component,writes)?)
                    .then(self.numeric_expression_effect(rhs,name,owner,component,writes)?)
                    .then(self.numeric_assignment_effect(lhs,op,rhs,name,owner,component,writes,absolute)?)),
                StmtKind::Let(_,_,expr)|StmtKind::Expr(expr)=>Some(before.then(self.numeric_expression_effect(expr,name,owner,component,writes)?)),
                StmtKind::If(condition,yes,no)=>{
                    let before=before.then(self.numeric_expression_effect(condition,name,owner,component,writes)?);
                    let yes=self.numeric_loop_paths(yes,Some(before.clone()),continues,name,owner,component,writes,absolute)?;
                    let no=self.numeric_loop_paths(no,Some(before),continues,name,owner,component,writes,absolute)?;
                    join_numeric_paths(yes,no)
                },
                StmtKind::Match(value,arms)=>{
                    let before=before.then(self.numeric_expression_effect(value,name,owner,component,writes)?);let mut next=None;
                    for arm in arms { next=join_numeric_paths(next,self.numeric_loop_paths(&arm.body,Some(before.clone()),continues,name,owner,component,writes,absolute)?); }next
                },
                StmtKind::Seek(_,expr)=>{
                    let before=before.then(self.numeric_expression_effect(expr,name,owner,component,writes)?);
                    *continues=join_numeric_paths(continues.take(),Some(before));None
                },
                StmtKind::Continue=>{*continues=join_numeric_paths(continues.take(),Some(before));None},
                StmtKind::Break|StmtKind::Return(_)=>None,
                StmtKind::While(..)|StmtKind::For(..)|StmtKind::Parallel(..)=>{
                    let mut effect=NumericEffect::shift(0);
                    for write in writes.iter().filter(|write|contains_statement(statement,match write {
                        Write::External(call)=>call,
                        Write::Position{call,place,..}=>call.unwrap_or(place),
                    })) {
                        if self.numeric_write(*write,name,owner,component,writes)? {effect=NumericEffect::Unknown;break;}
                    }
                    Some(before.then(effect))
                },
                _=>Some(before),
            };
        }
        Ok(next)
    }
    pub(super) fn retain_loop_bounds(&mut self, condition:Option<&Expr>, body:&[Stmt], initial:&BTreeMap<String,Binding>, header:&mut BTreeMap<String,Binding>)->Result<()> {
        let mut writes=Vec::new();statement_writes(self,body,&mut|write|writes.push(write))?;if let Some(condition)=condition {expression_writes(self,condition,&mut|write|writes.push(write))?;}
        let mut retained=Vec::new();
        for (name,binding) in initial {
            let (owner,proof)=match binding { Binding::Owned(owner)=>(owner.place.clone(),&owner.proof),Binding::OwnedValue(owner)=>{let Some(place)=owner.place()else{continue};(place,&owner.value.proof)},_=>continue };
            // 这里只解释有确定位置身份的本地／独占参数；数据候选别名仍由原写效果撤回。
            if !matches!(owner.storage(),Place::Local(..)|Place::Reference{..}) {continue;}
            for (component,(scalar,_)) in owner.ty().components.iter().enumerate() {
                if *scalar!=Scalar::U32 {continue;}
                let mut written=false;for write in &writes {if self.numeric_write(*write,name,&owner,component,&writes)? {written=true;break;}}
                if !written {continue;}
                if let Some(condition)=condition && matches!(self.numeric_expression_effect(condition,name,&owner,component,&writes)?,NumericEffect::Unknown) {continue;}
                let mut input=proof.project(&[component],owner.ty().components.len());
                if let Some(identity)=input.known_identity(0) {
                    let mut domains=input.domains(0);domains.insert(IndexDomain::Upper(identity,0));input=input.with_domains(domains);
                }
                let mut continues=None;let next=self.numeric_loop_paths(body,Some(NumericEffect::shift(0)),&mut continues,name,&owner,component,&writes,!bounds::seeks(body))?;
                let (lower,upper,least,most)=match join_numeric_paths(next,continues) {
                    None=>(true,true,0,0),
                    Some(NumericEffect::Shift{low,high,least,most})=>(low>=0,high<=0,least,most),
                    Some(NumericEffect::Assign(repeated))=>{
                        let mut invariant=input;invariant.join(&repeated,1,None);
                        retained.push((name.clone(),component,invariant,true,true));
                        continue;
                    },
                    Some(NumericEffect::Unknown)=>continue,
                };
                if !lower&&!upper {continue;}
                let guard=match self.env.get(name) {Some(Binding::Owned(value))=>&value.proof,Some(Binding::OwnedValue(value))=>&value.value.proof,_=>continue};
                let low=guard.unsigned_minimum(component).max(if lower{proof.unsigned_minimum(component)}else{0});
                let high=guard.unsigned_maximum(component).min(if upper{proof.unsigned_maximum(component)}else{u32::MAX});
                if i128::from(low)+least<0||i128::from(high)+most>i128::from(u32::MAX) {continue;}
                retained.push((name.clone(),component,input,lower,upper));
            }
        }
        for (name,component,proof,lower,upper) in retained {
            for environment in [&mut self.env,&mut *header] {if let Some(binding)=environment.get_mut(&name)&&let Some((current,_,_))=binding.proof_storage(){current.retain_loop_bounds(&proof,component,lower,upper);}}
        }
        Ok(())
    }
}
