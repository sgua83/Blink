//! CPU借用值的实际后续使用；只从原AST导出，生命周期关系仍由Proof持有。
use super::*;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    Field(String),
    Index(Option<u32>),
}
type Path = Vec<Step>;
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Location {
    name: String,
    path: Path,
}
#[derive(Clone, Default, PartialEq, Eq)]
struct Live(BTreeMap<Location, BTreeSet<Path>>);
fn prefix(a: &[Step], b: &[Step]) -> bool {
    b.starts_with(a)
}
impl Live {
    fn read(&mut self, location: Location) {
        self.0.insert(location, BTreeSet::new());
    }
    fn union(&mut self, other: &Self) {
        for (location, excluded) in &other.0 {
            self.0
                .entry(location.clone())
                .and_modify(|current| {
                    *current = current
                        .iter()
                        .flat_map(|a| {
                            excluded.iter().filter_map(move |b| {
                                if prefix(a, b) {
                                    Some(b.clone())
                                } else if prefix(b, a) {
                                    Some(a.clone())
                                } else {
                                    None
                                }
                            })
                        })
                        .collect();
                })
                .or_insert_with(|| excluded.clone());
        }
    }
    fn kill(&mut self, written: &Location) {
        // 不确定的动态位置不等于覆盖全部候选元素。
        if written.path.iter().any(|s| matches!(s, Step::Index(None))) {
            return;
        }
        self.0.retain(|location, excluded| {
            if location.name != written.name {
                return true;
            }
            if prefix(&written.path, &location.path) {
                return false;
            }
            if prefix(&location.path, &written.path) {
                excluded.insert(written.path.clone());
            }
            true
        });
    }
    fn kill_name(&mut self, name: &str) {
        self.0.retain(|location, _| location.name != name);
    }
}
#[derive(Clone)]
struct Point {
    before: Live,
    after: Live,
    invocation: Option<Live>,
}
type Key = (usize, usize);
fn key(span: Span) -> Key {
    (span.start, span.end)
}
pub(super) enum Transfer<'a> {
    Owned { value:&'a owned_values::OwnedValue, place:Option<&'a Place> },
    BorrowedData(&'a application::DataReference),
    OwnedData(&'a application::Data),
}
#[derive(Default)]
pub(super) struct State {
    points: BTreeMap<Key, Point>,
    current: Live,
    outputs:Live,
}
impl State {
    #[inline(never)]
    pub(super) fn new(world: &World, instance: &crate::generics::Instance) -> Self {
        let definition = &instance.definition;
        let mut analysis = Analysis {
            world,
            instance,
            points: BTreeMap::new(),
            outputs: Live::default(),
        };
        for param in &definition.function.params() {
            if matches!(param.access(), Access::Write | Access::InOut) {
                analysis.outputs.read(Location {
                    name: param.name().to_owned(),
                    path: Vec::new(),
                });
            }
        }
        let current = analysis.block(
            definition.function.body().as_ref().unwrap(),
            analysis.outputs.clone(),
            None,
        );
        Self {
            points: analysis.points,
            current,
            outputs:analysis.outputs,
        }
    }
    pub(super) fn used(&self,name:&str)->bool{name.starts_with('\0')||self.current.0.keys().any(|location|location.name==name)}
    pub(super) fn returning(&mut self){self.current=self.outputs.clone();}
    pub(super) fn before(&mut self, span: Span) {
        if let Some(point) = self.points.get(&key(span)) {
            self.current = point.before.clone();
        }
    }
    pub(super) fn after(&mut self, span: Span) {
        if let Some(point) = self.points.get(&key(span)) {
            self.current = point.after.clone();
        }
    }
    pub(super) fn invocation(&mut self, span: Span) {
        if let Some(point) = self.points.get(&key(span)) {
            if let Some(live) = &point.invocation {
                self.current = live.clone();
            }
        }
    }
    pub(super) fn all(&self,builder:&Builder<'_>,transferred:&[Transfer<'_>])->Result<view::Borrow> {
        let mut borrow=view::Borrow::default();
        for(name,binding)in &builder.env{borrow.union(&self.borrow(builder,name,binding,transferred)?);}
        Ok(borrow)
    }
    pub(super) fn borrow(&self,builder:&Builder<'_>,name:&str,binding:&Binding,transferred:&[Transfer<'_>])->Result<view::Borrow> {
        let mut omitted=BTreeMap::<usize,owned_presence::Presence>::new();
        for transfer in transferred{if let Transfer::Owned{value,place}=transfer{
            let components=match (binding.stored_place(),*place){
                (Some(root),Some(Place::Indexed{initialization,..}))=>root.owned_positions().into_iter().flatten().enumerate().map(|(component,position)|{
                    let when=initialization.iter().fold(owned_presence::Presence::None,|when,selected|when.or(owned_presence::Presence::selected(selected,&position)));
                    (component,when)
                }).collect::<Vec<_>>(),
                (Some(root),Some(place))=>place.components_in(&root).into_iter().flatten().map(|component|(component,owned_presence::Presence::All)).collect(),
                (_,None)if matches!(binding,Binding::OwnedValue(stored)if stored.value.ty==value.value.ty&&stored.value.regs==value.value.regs)=>(0..value.value.regs.len()).map(|component|(component,owned_presence::Presence::All)).collect(),
                _=>Vec::new(),
            };
            for(component,when)in components{if when!=owned_presence::Presence::None{omitted.entry(component).and_modify(|old|*old=old.clone().or(when.clone())).or_insert(when);}}
        }}

        if name.starts_with('\0') {
            if matches!(binding,Binding::DataReference(_)|Binding::ViewElement{..})||matches!(binding,Binding::OwnedValue(value)if value.ownership==application::Ownership::Borrowed){return binding.view_borrow(builder);}
            let mut result=stored_element_borrow(builder,binding,&[],&BTreeSet::new(),&omitted,transferred,true)?;
            if let Some((ty,proof))=binding.view_parts(){for component in view_contracts::components(ty){result.union(&retained_component(proof.view_borrow(component),component,&omitted));}}
            return Ok(result);
        }
        let uses = self
            .current
            .0
            .iter()
            .filter(|(location, _)| location.name == name)
            .collect::<Vec<_>>();
        let mut result=stored_element_borrow(builder,binding,&[],&BTreeSet::new(),&omitted,transferred,true)?;
        for(location,excluded)in &uses{result.union(&stored_element_borrow(builder,binding,&location.path,excluded,&omitted,transferred,false)?);}
        if uses.is_empty(){return Ok(result);}
        let (ty, proof) = match binding {
            Binding::Value(v) => (&v.ty, &v.proof),
            Binding::Owned(v) => (v.place.ty(), &v.proof),
            Binding::OwnedValue(v) => (&v.value.ty,&v.value.proof),
            Binding::ViewElement { view, .. } => return Ok(view.proof.borrowed_view()),
            _ => return Ok(result),
        };
        for (location, excluded) in uses {
            let mut selected = components(ty, &location.path, 0);
            for path in excluded {
                selected.retain(|i| !components(ty, path, 0).contains(i));
            }
            let views=view_contracts::components(ty);
            for component in selected.intersection(&views) {
                result.union(&retained_component(proof.view_borrow(*component),*component,&omitted));
            }
        }
        Ok(result)
    }
}
fn components(ty: &Ty, path: &[Step], first: usize) -> BTreeSet<usize> {
    // view是一个借用值；length或元素读取仍消费该引用。
    if path.is_empty() || ty.view.is_some() {
        return (first..first + ty.components.len()).collect();
    }
    match &path[0] {
        Step::Field(name) => {
            if let Some((_, offset, field)) = ty.fields.iter().find(|(field, _, _)| field == name) {
                let at = ty
                    .components
                    .iter()
                    .position(|(_, at)| at == offset)
                    .unwrap();
                components(field, &path[1..], first + at)
            } else {
                BTreeSet::new()
            }
        }
        Step::Index(index) => {
            let Some(element) = &ty.array else {
                return BTreeSet::new();
            };
            (0..ty.values[0])
                .filter(|i| index.is_none_or(|index| index == *i))
                .flat_map(|i| {
                    components(
                        element,
                        &path[1..],
                        first + i as usize * element.components.len(),
                    )
                })
                .collect()
        }
    }
}
#[derive(Clone)]
struct Edges {
    broken: Live,
    continued: Live,
}
struct Analysis<'a> {
    world: &'a World,
    instance: &'a crate::generics::Instance,
    points: BTreeMap<Key, Point>,
    outputs: Live,
}
impl Analysis<'_> {
    fn literal_u32(&self,expr:&Expr)->Option<u32> {
        match expr.kind() {
            ExprKind::StaticU32(value)=>Some(value),
            ExprKind::Name(name)=>self.instance.bindings.values.get(name).copied(),
            ExprKind::Number(text)=>number(&self.instance.definition.source,expr.span(),text).ok().and_then(|(ty,value)|(ty==Ty::scalar(Scalar::U32)).then_some(value as u32)),
            _=>None,
        }
    }

    fn location(&self, expr: &Expr) -> Option<Location> {
        match expr.kind() {
            ExprKind::Name(name) => Some(Location {
                name: name.to_string(),
                path: Vec::new(),
            }),
            ExprKind::Member(base, field) => {
                let mut location = self.location(&base)?;
                location.path.push(Step::Field(field.to_string()));
                Some(location)
            }
            ExprKind::Index(base, index) => {
                let mut location = self.location(&base)?;
                let index = self.literal_u32(&index);
                location.path.push(Step::Index(index));
                Some(location)
            }
            _ => None,
        }
    }
    fn point(&mut self, span: Span, before: Live, after: Live) {
        let invocation = self
            .points
            .get(&key(span))
            .and_then(|point| point.invocation.clone());
        self.points.insert(
            key(span),
            Point {
                before,
                after,
                invocation,
            },
        );
    }
    fn address(&mut self, expr: &Expr, after: Live, edges: Option<&Edges>) -> Live {
        let before = match expr.kind() {
            ExprKind::Name(_) => after.clone(),
            ExprKind::Member(base, _) => self.address(&base, after.clone(), edges),
            ExprKind::Index(base, index) => {
                let live = self.expression(&index, after.clone(), edges);
                self.address(&base, live, edges)
            }
            _ => return self.expression(expr, after, edges),
        };
        self.point(expr.span(), before.clone(), after);
        before
    }
    fn expression(&mut self, expr: &Expr, after: Live, edges: Option<&Edges>) -> Live {
        let before = if let Some(location) = self.location(expr) {
            let mut live = after.clone();
            live.read(location);
            self.address(expr, live, edges)
        } else {
            match expr.kind() {
                ExprKind::Call(name, args)
                | ExprKind::Specialize(name, _, args)
                | ExprKind::Cooperate(name, _, args) => {
                    let function = self
                        .world
                        .resolve(
                            name,
                            &self.instance.definition.module,
                            &self.instance.definition.imports,
                            true,
                        ).unique()
                        .and_then(|key| self.world.functions.get(&key));
                    let params = function.map(|d| d.function.params());
                    let mut invocation = if function.is_some_and(|d|{
                        // Only the explicit declaration is a known terminator in
                        // this source pass; concrete calls are checked by Instance.
                        let result=d.function.result();
                        result.name()=="never"&&result.arguments().is_empty()&&!result.dataset()
                            &&!result.tile()&&!result.view()&&result.count().is_none()&&result.contracts().is_none()
                    }) {
                        Live::default()
                    } else {
                        after.clone()
                    };
                    for (param, arg) in params.iter().flat_map(|params|params.iter()).zip(&args) {
                        if !param.ty().dataset()
                            && param.access() == Access::Write
                            && let Some(place) = self.location(&arg)
                        {
                            invocation.kill(&place);
                        }
                    }
                    for (param, arg) in params.iter().flat_map(|params|params.iter()).zip(&args) {
                        if !param.ty().dataset()
                            && param.access() == Access::InOut
                            && let Some(place) = self.location(&arg)
                        {
                            invocation.read(place);
                        }
                    }
                    if name == "try" {
                        invocation.union(&self.outputs);
                    }
                    self.points.insert(
                        key(expr.span()),
                        Point {
                            before: Live::default(),
                            after: after.clone(),
                            invocation: Some(invocation.clone()),
                        },
                    );
                    let mut live = invocation;
                    for (index, arg) in args.iter().enumerate().rev() {
                        let place_only = name == "ordinal"
                            || params.as_ref().and_then(|params|params.get(index))
                                .is_some_and(|p| !p.ty().dataset() && p.access() == Access::Write);
                        live = if place_only {
                            self.address(&arg, live, edges)
                        } else {
                            self.expression(&arg, live, edges)
                        };
                    }
                    if function.is_none() {
                        live.read(Location { name: name.to_string(), path: Vec::new() });
                    }
                    live
                }
                ExprKind::Invoke(callee, args) => {
                    self.points.insert(key(expr.span()), Point {
                        before: Live::default(), after: after.clone(), invocation: Some(after.clone()),
                    });
                    let mut live = after.clone();
                    for arg in args.iter().rev() { live = self.expression(&arg, live, edges); }
                    self.expression(&callee, live, edges)
                }
                ExprKind::Construct(_, args) => {
                    let mut live = after.clone();
                    for arg in args.iter().rev() {
                        live = self.expression(&arg, live, edges);
                    }
                    live
                }
                ExprKind::Target(_, expr)
                | ExprKind::Unary(_, expr)
                | ExprKind::Member(expr, _) => self.expression(&expr, after.clone(), edges),
                ExprKind::Index(base, index) => {
                    let live = self.expression(&index, after.clone(), edges);
                    self.expression(&base, live, edges)
                }
                ExprKind::Binary(op, left, right) if matches!(op, "&&" | "||") => {
                    let mut live = self.expression(&right, after.clone(), edges);
                    live.union(&after);
                    self.expression(&left, live, edges)
                }
                ExprKind::Binary(_, left, right) => {
                    let live = self.expression(&right, after.clone(), edges);
                    self.expression(&left, live, edges)
                }
                ExprKind::Select(condition, yes, no) => {
                    let mut live = self.expression(&yes, after.clone(), edges);
                    let other = self.expression(&no, after.clone(), edges);
                    live.union(&other);
                    self.expression(&condition, live, edges)
                }
                _ => after.clone(),
            }
        };
        self.point(expr.span(), before.clone(), after);
        before
    }
    fn block(&mut self, body: &Block, mut live: Live, edges: Option<&Edges>) -> Live {
        for stmt in body.iter().rev() {
            live = self.statement(&stmt, live, edges);
        }
        live
    }
    fn statement(&mut self, stmt: &Stmt, after: Live, edges: Option<&Edges>) -> Live {
        let before = match stmt.kind() {
            StmtKind::Declare(_, name) => {
                let mut live = after.clone();
                live.kill_name(name);
                live
            }
            StmtKind::Let(_, name, expr) => {
                let mut live = after.clone();
                live.kill_name(name);
                self.expression(&expr, live, edges)
            }
            StmtKind::Assign(place, op, expr) => {
                let mut live = after.clone();
                if let Some(place) = self.location(&place) {
                    live.kill(&place);
                }
                live = self.expression(&expr, live, edges);
                if op != "=" {
                    if let Some(place) = self.location(&place) {
                        live.read(place);
                    }
                }
                self.address(&place, live, edges)
            }
            StmtKind::Expr(expr) => self.expression(&expr, after.clone(), edges),
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expression(&value, self.outputs.clone(), edges)
                } else {
                    self.outputs.clone()
                }
            }
            StmtKind::Break => edges.map(|e| e.broken.clone()).unwrap_or_default(),
            StmtKind::Continue => edges.map(|e| e.continued.clone()).unwrap_or_default(),
            StmtKind::Seek(_, expr) => self.expression(
                &expr,
                edges.map(|e| e.continued.clone()).unwrap_or_default(),
                edges,
            ),
            StmtKind::If(condition, yes, no) => {
                let a = self.block(&yes, after.clone(), edges);
                let b = self.block(&no, after.clone(), edges);
                let live = match condition.kind() {
                    ExprKind::Bool(true) => a,
                    ExprKind::Bool(false) => b,
                    _ => {
                        let mut live = a;
                        live.union(&b);
                        live
                    }
                };
                self.expression(&condition, live, edges)
            }
            StmtKind::Match(value, arms) => {
                let mut live = Live::default();
                for arm in arms {
                    let mut branch = self.block(&arm.body(), after.clone(), edges);
                    if let Some(name) = &arm.binding() {
                        branch.kill_name(name);
                    }
                    live.union(&branch);
                }
                self.expression(&value, live, edges)
            }
            StmtKind::While(condition, _) if matches!(condition.kind(), ExprKind::Bool(false)) => self.expression(&condition,after.clone(),edges),
            StmtKind::While(condition, body) => {
                let mut head = after.clone();
                loop {
                    let exits = Edges {
                        broken: after.clone(),
                        continued: head.clone(),
                    };
                    let mut live = self.block(&body, head.clone(), Some(&exits));
                    if !matches!(condition.kind(), ExprKind::Bool(true)) {
                        live.union(&after);
                    }
                    live = self.expression(&condition, live, edges);
                    if live == head {
                        break live;
                    }
                    head = live;
                }
            }
            StmtKind::For(_,begin,Some(end),_) if self.literal_u32(&begin).zip(self.literal_u32(&end)).is_some_and(|(a,b)|a>=b)=>{
                let live=self.expression(&end,after.clone(),edges);self.expression(&begin,live,edges)
            }
            StmtKind::For(name, begin, end, body) => {
                let mut head = after.clone();
                loop {
                    let exits = Edges {
                        broken: after.clone(),
                        continued: head.clone(),
                    };
                    let mut live = self.block(&body, head.clone(), Some(&exits));
                    live.union(&after);
                    if live == head {
                        break;
                    }
                    head = live;
                }
                head.kill_name(name);
                if let Some(end) = end {
                    head = self.expression(&end, head, edges);
                }
                self.expression(&begin, head, edges)
            }
            StmtKind::Parallel(name, domain, body) => {
                let mut live = self.block(&body, after.clone(), None);
                live.kill_name(name);
                if let Some(selection) = &domain.selection() {
                    live = self.expression(selection, live, edges);
                }
                if let Some(count) = &domain.count() {
                    live = self.expression(count, live, edges);
                }
                live.read(Location {
                    name: domain.dataset().to_string(),
                    path: Vec::new(),
                });
                live
            }
            StmtKind::Phase => after.clone(),
        };
        self.point(stmt.span(), before.clone(), after);
        before
    }
}
impl Builder<'_> {
    pub(super) fn live_view_borrow(&self,name:&str,binding:&Binding)->Result<view::Borrow> {self.view_uses.borrow(self,name,binding,&[])}
}

// Reuse the original AST locations and stored Proof; only real indexed element
// consumers expose content borrows, while metadata-only use stays metadata.
fn retained_component(borrow:view::Borrow,component:usize,omitted:&BTreeMap<usize,owned_presence::Presence>)->view::Borrow {
    match omitted.get(&component){Some(when)=>borrow.restricted(&when.condition().assume(&bounds::Clause::default(),false)),None=>borrow}
}
fn stored_element_borrow(builder:&Builder<'_>,binding:&Binding,path:&[Step],excluded:&BTreeSet<Path>,omitted:&BTreeMap<usize,owned_presence::Presence>,transferred:&[Transfer<'_>],cleanup:bool)->Result<view::Borrow> {
    fn external(borrow:view::Borrow,depth:Option<usize>)->view::Borrow{
        borrow.resources().map_sources(|source|match source{
            view::Source::Data(ValueIdentity::ElementComponent(_)) if depth.is_some()=>view::Borrow::default(),
            view::Source::Data(ValueIdentity::ElementParent(parent,_)) if depth.is_some_and(|depth|parent<depth)=>view::Borrow::default(),
            view::Source::Element{parent,fields,index}=>view::Borrow::element(external(*parent,depth),fields,index),
            view::Source::Projection{parent,fields}=>view::Borrow::projection(external(*parent,depth),fields),
            source=>view::Borrow::source(source),
        })
    }
    fn without(excluded:&BTreeSet<Path>,step:&Step)->BTreeSet<Path>{
        excluded.iter().filter(|path|path.first()==Some(step)).map(|path|path[1..].to_vec()).collect()
    }
    fn projected(value:&Value,path:&[Step],excluded:&BTreeSet<Path>,bindings:&BTreeMap<ValueIdentity,bounds::Proof>,omitted:&BTreeMap<usize,owned_presence::Presence>,passed:&[(usize,Vec<u32>)])->view::Borrow{
        let mut selected=components(&value.ty,path,0);
        for path in excluded{selected.retain(|i|!components(&value.ty,path,0).contains(i));}
        let mut out=view::Borrow::default();
        for component in selected.intersection(&view_contracts::components(&value.ty)){
            let depth=passed.iter().filter(|(depth,words)|*depth!=0||words.contains(&value.ty.components[*component].1)).map(|(depth,_)|*depth).max();
            out.union(&external(retained_component(value.proof.view_borrow(*component),*component,omitted),depth).at_call(bindings));
        }
        out
    }
    fn data(builder:&Builder<'_>,data:&application::Data,path:&[Step],excluded:&BTreeSet<Path>,parent:view::Borrow,mut bindings:BTreeMap<ValueIdentity,bounds::Proof>,transferred:&[Transfer<'_>],cleanup:bool)->Result<view::Borrow>{
        if data.ownership==application::Ownership::Moved||data.length.maximum()==0||cleanup&&(!data.ownership.may_own()||matches!(data.elements.known(),Some(returns::Returned::Value(_)))){return Ok(view::Borrow::default());}

        let mut passed=Vec::new();let mut omitted=BTreeMap::new();
        for transfer in transferred{match transfer{
            Transfer::BorrowedData(reference)=>{
                for(scope,complete,_)in builder.application_element_scopes(&[],&parent,&reference.data,(&builder.path,&builder.env)){
                    if scope.first().is_none_or(|fields|fields.starts_with(&reference.projection)){
                        let words=reference.words();
                        if scope.is_empty()&&complete{for(component,(_,word))in data.ty.components.iter().enumerate(){if words.contains(word){omitted.insert(component,owned_presence::Presence::All);}}}
                        passed.push((scope.len(),words));
                    }
                }
            },
            Transfer::Owned{value,place}=>{
                if let Some(Place::ApplicationElement{sources,resource,words,index_range,..})=place{
                    for(scope,complete,_)in builder.application_element_scopes(sources,resource,data,(&builder.path,&builder.env)){if scope.is_empty(){
                        passed.push((0,words.clone()));
                        if complete&&*index_range==(0,0)&&data.length.minimum()==1&&data.length.maximum()==1{
                            for(component,(_,word))in data.ty.components.iter().enumerate(){if words.contains(word){omitted.insert(component,owned_presence::Presence::All);}}
                        }
                    }}
                }
                for resource in value.resources(){for(scope,_,_)in builder.application_element_scopes(&[],&parent,resource,(&builder.path,&builder.env)){passed.push((scope.len()+1,Vec::new()));}}
            },
            Transfer::OwnedData(owner)=>{
                if data.ownership.may_own()&&builder.application_element_scopes(&[],&parent,owner,(&builder.path,&builder.env)).iter().any(|(scope,complete,_)|scope.is_empty()&&*complete){return Ok(view::Borrow::default());}
            },
        }}
        if let Some(Step::Index(Some(index)))=path.first(){
            let selected=bounds::Operand::Constant(Scalar::U32,Bits::from(*index));
            bindings.insert(ValueIdentity::ElementIndex(0),bounds::Proof::element_operand(selected,(*index,*index)));
        }
        builder.bind_element_resource_borrows(data,&parent,&mut bindings);
        let out=match data.elements.known(){
            Some(elements)if path.is_empty()=>model(builder,elements,path,excluded,true,&omitted,Some(&bindings),&passed,transferred,cleanup)?,
            Some(elements)=>if let Some(step@Step::Index(_))=path.first(){model(builder,elements,&path[1..],&without(excluded,step),true,&omitted,Some(&bindings),&passed,transferred,cleanup)?}else{view::Borrow::default()},
            None if builder.world.stored_borrows(&data.ty,builder.scope.source(),builder.scope.span(),builder.scope.module(),builder.scope.imports(),&mut BTreeSet::new(),cleanup)?=>view::Borrow::source(view::Source::Unresolved),
            None=>view::Borrow::default(),
        };
        Ok(if let application::Ownership::Conditional(when)=&data.ownership{out.restricted(&when.condition().at_call(&bindings).assume(&bounds::Clause::default(),true))}else{out})
    }
    fn owned(builder:&Builder<'_>,value:&owned_values::OwnedValue,path:&[Step],excluded:&BTreeSet<Path>,stored:bool,omitted:&BTreeMap<usize,owned_presence::Presence>,bindings:Option<&BTreeMap<ValueIdentity,bounds::Proof>>,passed:&[(usize,Vec<u32>)],transferred:&[Transfer<'_>],cleanup:bool,root:bool)->Result<view::Borrow>{
        let mut out=if cleanup{
            if root{
                let mut borrow=view::Borrow::default();value.cleanup_views(&BTreeSet::new(),&mut borrow,&|component,_,borrow|{
                    let borrow=retained_component(borrow,component,omitted);
                    if stored{let depth=passed.iter().filter(|(depth,words)|*depth!=0||words.contains(&value.value.ty.components[component].1)).map(|(depth,_)|*depth).max();external(borrow,depth).at_call(bindings.unwrap())}else{borrow}
                });borrow
            }else{view::Borrow::default()}
        }else if stored{projected(&value.value,path,excluded,bindings.unwrap(),omitted,passed)}else{view::Borrow::default()};
        let payload_omitted=|index:usize|{
            let(offset,ty)=if value.value.ty.has_variants(){(value.value.ty.variant_payload_offset(),value.value.ty.variants[index].ty.as_ref())}else{value.value.ty.aggregate_part(index)};
            ty.components.iter().enumerate().filter_map(|(index,(_,word))|{
                let component=value.value.ty.components.iter().position(|(_,at)|*at==offset+word).unwrap();
                omitted.get(&component).map(|when|(index,when.clone()))
            }).collect::<BTreeMap<_,_>>()
        };
        if let Some(step)=path.first(){
            let indices=match step{
                Step::Field(name)=>value.value.ty.fields.iter().position(|(field,_,_)|field==name).into_iter().collect::<Vec<_>>(),
                Step::Index(index)if value.value.ty.array.is_some()=>(0..value.value.ty.aggregate_len()).filter(|i|index.is_none_or(|index|index as usize==*i)).collect(),
                _=>Vec::new(),
            };
            let excluded=without(excluded,step);
            for index in indices{if let Some(payload)=value.payloads.get(index).and_then(Option::as_deref){out.union(&model(builder,payload,&path[1..],&excluded,false,&payload_omitted(index),bindings,passed,transferred,cleanup)?);}}
        }else{
            for(index,payload)in value.payloads.iter().enumerate(){if let Some(payload)=payload{
                let excluded=if value.value.ty.has_variants(){excluded.clone()}
                    else if value.value.ty.array.is_some(){without(excluded,&Step::Index(Some(index as u32)))}
                    else{without(excluded,&Step::Field(value.value.ty.fields[index].0.clone()))};
                out.union(&model(builder,payload,path,&excluded,false,&payload_omitted(index),bindings,passed,transferred,cleanup)?);
            }}
        }
        Ok(out)
    }
    fn model(builder:&Builder<'_>,model:&returns::Returned,path:&[Step],excluded:&BTreeSet<Path>,stored:bool,omitted:&BTreeMap<usize,owned_presence::Presence>,bindings:Option<&BTreeMap<ValueIdentity,bounds::Proof>>,passed:&[(usize,Vec<u32>)],transferred:&[Transfer<'_>],cleanup:bool)->Result<view::Borrow>{match model{
        returns::Returned::Dataset(value)=>{
            let(parent,bindings)=if let Some(bindings)=bindings{
                (view::resource_borrow(value).at_call(bindings),application::element_scope_bindings(bindings,0,true))
            }else{(view::resource_borrow(value),builder.element_owner_bindings(value))};
            Ok(retained_component(data(builder,value,path,excluded,parent,bindings,transferred,cleanup)?,0,omitted))
        },
        returns::Returned::OwnedValue(value)=>owned(builder,value,path,excluded,stored,omitted,bindings,passed,transferred,cleanup,stored),
        returns::Returned::Value(v)if stored&&!cleanup=>Ok(projected(v,path,excluded,bindings.unwrap(),omitted,passed)),
        _=>Ok(view::Borrow::default()),
    }}
    match binding{
        Binding::ApplicationData(value)=>data(builder,value,path,excluded,view::resource_borrow(value),builder.element_owner_bindings(value),transferred,cleanup),
        Binding::OwnedValue(value)=>owned(builder,value,path,excluded,false,omitted,None,&[],transferred,cleanup,true),
        _=>Ok(view::Borrow::default()),
    }
}
