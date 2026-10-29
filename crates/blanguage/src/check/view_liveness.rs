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
#[derive(Default)]
pub(super) struct State {
    points: BTreeMap<Key, Point>,
    current: Live,
    outputs:Live,
}
impl State {
    #[inline(never)]
    pub(super) fn new(world: &World, definition: &Definition) -> Self {
        let mut analysis = Analysis {
            world,
            definition,
            points: BTreeMap::new(),
            outputs: Live::default(),
        };
        for param in &definition.function.params {
            if !param.ty.dataset && matches!(param.access, Access::Write | Access::InOut) {
                analysis.outputs.read(Location {
                    name: param.name.clone(),
                    path: Vec::new(),
                });
            }
        }
        let current = analysis.block(
            definition.function.body.as_ref().unwrap(),
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
    pub(super) fn borrow(&self,name:&str,binding:&Binding,transferred:&[Place])->view::Borrow {
        if name.starts_with('\0') {
            return binding.view_borrow();
        }
        let uses = self
            .current
            .0
            .iter()
            .filter(|(location, _)| location.name == name)
            .collect::<Vec<_>>();
        if uses.is_empty() && !matches!(binding,Binding::OwnedValue(value)if value.value.ty.contains_cleanup()&&value.value.ty.contains_views()) {
            return view::Borrow::default();
        }
        let (ty, proof) = match binding {
            Binding::Value(v) => (&v.ty, &v.proof),
            Binding::Owned(v) => (v.place.ty(), &v.proof),
            Binding::OwnedValue(v) => (&v.value.ty,&v.value.proof),
            Binding::ViewElement { view, .. } => return view.proof.borrowed_view(),
            _ => return view::Borrow::default(),
        };
        let mut omitted=BTreeSet::new();
        if let Some(root)=binding.stored_place(){for place in transferred{if let Some(components)=place.components_in(&root){omitted.extend(components);}}}
        let mut result = user_owned::cleanup_borrow(binding,&omitted);
        for (location, excluded) in uses {
            let mut selected = components(ty, &location.path, 0);
            for path in excluded {
                selected.retain(|i| !components(ty, path, 0).contains(i));
            }
            selected.retain(|component|!omitted.contains(component));
            let views=view_contracts::components(ty);
            for component in selected.intersection(&views) {
                result.union(&proof.view_borrow(*component));
            }
        }
        result
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
    definition: &'a Definition,
    points: BTreeMap<Key, Point>,
    outputs: Live,
}
impl Analysis<'_> {
    fn literal_u32(&self,expr:&Expr)->Option<u32> {
        match &expr.kind {
            ExprKind::StaticU32(value)=>Some(*value),
            ExprKind::Number(text)=>number(&self.definition.source,expr.span,text).ok().and_then(|(ty,value)|(ty==Ty::scalar(Scalar::U32)).then_some(value)),
            _=>None,
        }
    }

    fn location(&self, expr: &Expr) -> Option<Location> {
        match &expr.kind {
            ExprKind::Name(name) => Some(Location {
                name: name.clone(),
                path: Vec::new(),
            }),
            ExprKind::Member(base, field) => {
                let mut location = self.location(base)?;
                location.path.push(Step::Field(field.clone()));
                Some(location)
            }
            ExprKind::Index(base, index) => {
                let mut location = self.location(base)?;
                let index = self.literal_u32(index);
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
        let before = match &expr.kind {
            ExprKind::Name(_) => after.clone(),
            ExprKind::Member(base, _) => self.address(base, after.clone(), edges),
            ExprKind::Index(base, index) => {
                let live = self.expression(index, after.clone(), edges);
                self.address(base, live, edges)
            }
            _ => return self.expression(expr, after, edges),
        };
        self.point(expr.span, before.clone(), after);
        before
    }
    fn expression(&mut self, expr: &Expr, after: Live, edges: Option<&Edges>) -> Live {
        let before = if let Some(location) = self.location(expr) {
            let mut live = after.clone();
            live.read(location);
            self.address(expr, live, edges)
        } else {
            match &expr.kind {
                ExprKind::Call(name, args)
                | ExprKind::Specialize(name, _, args)
                | ExprKind::Cooperate(name, _, args) => {
                    let function = self
                        .world
                        .resolve(
                            name,
                            &self.definition.module,
                            &self.definition.imports,
                            true,
                        )
                        .and_then(|key| self.world.functions.get(&key));
                    let params = function
                        .map(|d| d.function.params.clone())
                        .unwrap_or_default();
                    let mut invocation = if function.is_some_and(|d| d.function.result.is_never()) {
                        Live::default()
                    } else {
                        after.clone()
                    };
                    for (param, arg) in params.iter().zip(args) {
                        if !param.ty.dataset
                            && param.access == Access::Write
                            && let Some(place) = self.location(arg)
                        {
                            invocation.kill(&place);
                        }
                    }
                    for (param, arg) in params.iter().zip(args) {
                        if !param.ty.dataset
                            && param.access == Access::InOut
                            && let Some(place) = self.location(arg)
                        {
                            invocation.read(place);
                        }
                    }
                    if name == "try" {
                        invocation.union(&self.outputs);
                    }
                    self.points.insert(
                        key(expr.span),
                        Point {
                            before: Live::default(),
                            after: after.clone(),
                            invocation: Some(invocation.clone()),
                        },
                    );
                    let mut live = invocation;
                    for (index, arg) in args.iter().enumerate().rev() {
                        let place_only = name == "ordinal"
                            || params
                                .get(index)
                                .is_some_and(|p| !p.ty.dataset && p.access == Access::Write);
                        live = if place_only {
                            self.address(arg, live, edges)
                        } else {
                            self.expression(arg, live, edges)
                        };
                    }
                    live
                }
                ExprKind::Construct(_, args) => {
                    let mut live = after.clone();
                    for arg in args.iter().rev() {
                        live = self.expression(arg, live, edges);
                    }
                    live
                }
                ExprKind::Target(_, expr)
                | ExprKind::Unary(_, expr)
                | ExprKind::Member(expr, _) => self.expression(expr, after.clone(), edges),
                ExprKind::Index(base, index) => {
                    let live = self.expression(index, after.clone(), edges);
                    self.expression(base, live, edges)
                }
                ExprKind::Binary(op, left, right) if matches!(op.as_str(), "&&" | "||") => {
                    let mut live = self.expression(right, after.clone(), edges);
                    live.union(&after);
                    self.expression(left, live, edges)
                }
                ExprKind::Binary(_, left, right) => {
                    let live = self.expression(right, after.clone(), edges);
                    self.expression(left, live, edges)
                }
                ExprKind::Select(condition, yes, no) => {
                    let mut live = self.expression(yes, after.clone(), edges);
                    let other = self.expression(no, after.clone(), edges);
                    live.union(&other);
                    self.expression(condition, live, edges)
                }
                _ => after.clone(),
            }
        };
        self.point(expr.span, before.clone(), after);
        before
    }
    fn block(&mut self, body: &[Stmt], mut live: Live, edges: Option<&Edges>) -> Live {
        for stmt in body.iter().rev() {
            live = self.statement(stmt, live, edges);
        }
        live
    }
    fn statement(&mut self, stmt: &Stmt, after: Live, edges: Option<&Edges>) -> Live {
        let before = match &stmt.kind {
            StmtKind::Declare(_, name) => {
                let mut live = after.clone();
                live.kill_name(name);
                live
            }
            StmtKind::Let(_, name, expr) => {
                let mut live = after.clone();
                live.kill_name(name);
                self.expression(expr, live, edges)
            }
            StmtKind::Assign(place, op, expr) => {
                let mut live = after.clone();
                if let Some(place) = self.location(place) {
                    live.kill(&place);
                }
                live = self.expression(expr, live, edges);
                if op != "=" {
                    if let Some(place) = self.location(place) {
                        live.read(place);
                    }
                }
                self.address(place, live, edges)
            }
            StmtKind::Expr(expr) => self.expression(expr, after.clone(), edges),
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expression(value, self.outputs.clone(), edges)
                } else {
                    self.outputs.clone()
                }
            }
            StmtKind::Break => edges.map(|e| e.broken.clone()).unwrap_or_default(),
            StmtKind::Continue => edges.map(|e| e.continued.clone()).unwrap_or_default(),
            StmtKind::Seek(_, expr) => self.expression(
                expr,
                edges.map(|e| e.continued.clone()).unwrap_or_default(),
                edges,
            ),
            StmtKind::If(condition, yes, no) => {
                let a = self.block(yes, after.clone(), edges);
                let b = self.block(no, after.clone(), edges);
                let live = match condition.kind {
                    ExprKind::Bool(true) => a,
                    ExprKind::Bool(false) => b,
                    _ => {
                        let mut live = a;
                        live.union(&b);
                        live
                    }
                };
                self.expression(condition, live, edges)
            }
            StmtKind::Match(value, arms) => {
                let mut live = Live::default();
                for arm in arms {
                    let mut branch = self.block(&arm.body, after.clone(), edges);
                    if let Some(name) = &arm.binding {
                        branch.kill_name(name);
                    }
                    live.union(&branch);
                }
                self.expression(value, live, edges)
            }
            StmtKind::While(condition, _) if matches!(condition.kind, ExprKind::Bool(false)) => self.expression(condition,after.clone(),edges),
            StmtKind::While(condition, body) => {
                let mut head = after.clone();
                loop {
                    let exits = Edges {
                        broken: after.clone(),
                        continued: head.clone(),
                    };
                    let mut live = self.block(body, head.clone(), Some(&exits));
                    if !matches!(condition.kind, ExprKind::Bool(true)) {
                        live.union(&after);
                    }
                    live = self.expression(condition, live, edges);
                    if live == head {
                        break live;
                    }
                    head = live;
                }
            }
            StmtKind::For(_,begin,Some(end),_) if self.literal_u32(begin).zip(self.literal_u32(end)).is_some_and(|(a,b)|a>=b)=>{
                let live=self.expression(end,after.clone(),edges);self.expression(begin,live,edges)
            }
            StmtKind::For(name, begin, end, body) => {
                let mut head = after.clone();
                loop {
                    let exits = Edges {
                        broken: after.clone(),
                        continued: head.clone(),
                    };
                    let mut live = self.block(body, head.clone(), Some(&exits));
                    live.union(&after);
                    if live == head {
                        break;
                    }
                    head = live;
                }
                head.kill_name(name);
                if let Some(end) = end {
                    head = self.expression(end, head, edges);
                }
                self.expression(begin, head, edges)
            }
            StmtKind::Parallel(name, domain, body) => {
                let mut live = self.block(body, after.clone(), None);
                live.kill_name(name);
                if let Some(selection) = &domain.selection {
                    live = self.expression(selection, live, edges);
                }
                if let Some(count) = &domain.count {
                    live = self.expression(count, live, edges);
                }
                live.read(Location {
                    name: domain.dataset.clone(),
                    path: Vec::new(),
                });
                live
            }
            StmtKind::Phase => after.clone(),
        };
        self.point(stmt.span, before.clone(), after);
        before
    }
}
impl Builder<'_> {
    pub(super) fn live_view_borrow(&self,name:&str,binding:&Binding)->view::Borrow {self.view_uses.borrow(name,binding,&[])}

    pub(super) fn live_view_sources(
        &self,
        name: &str,
        binding: &Binding,
    ) -> BTreeSet<view::Source> {
        self.view_uses.borrow(name,binding,&[]).restricted(&self.path).sources.entries().filter_map(|(source,when)|when.possible().then_some(*source)).collect()
    }
}
