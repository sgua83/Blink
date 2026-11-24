use super::*;
use std::borrow::Cow;

#[derive(Clone)]
pub(in crate::check) enum Epoch { Input, Local { memory:usize, snapshots:Option<Arc<[ValueSnapshot]>> } }
impl Epoch { fn key(&self)->Option<usize>{match self{Self::Input=>None,Self::Local{memory,..}=>Some(*memory)}} }
impl PartialEq for Epoch{fn eq(&self,b:&Self)->bool{self.key()==b.key()}}
impl Eq for Epoch{}
impl PartialOrd for Epoch{fn partial_cmp(&self,b:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(b))}}
impl Ord for Epoch{fn cmp(&self,b:&Self)->std::cmp::Ordering{self.key().cmp(&b.key())}}


#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
enum Op { Identity, Position(address::Origin), Initial{origin:address::Origin,frame:Option<usize>}, Read{word:u32,epoch:Epoch,frame:Option<usize>}, Element{fields:Vec<usize>,index:Option<bounds::Selector>}, Projection(Vec<usize>), Resources, Owners, Select(Vec<Option<bounds::Selector>>) }

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
struct Edge { from:usize, op:Op, when:bounds::Condition }

#[derive(Clone,Default,PartialEq,Eq,PartialOrd,Ord)]
struct Node { seeds:Borrow, edges:Vec<Edge> }

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
struct Write { target:usize, fields:Option<Vec<(u32,usize)>>, when:bounds::Condition }

#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
struct Frame { parent:Option<usize>, writes:Vec<Write> }

#[derive(Clone,Default,PartialEq,Eq,PartialOrd,Ord)]
pub(in crate::check) struct Graph { nodes:Vec<Node>, variables:BTreeSet<usize>, frames:Vec<Frame>, inputs:BTreeMap<LoopInput,usize> }


#[derive(Clone,PartialEq,Eq,PartialOrd,Ord)]
enum Key { Loop(LoopInput), Read(ContentRead), Call(CallResult) }
struct Builder<'a> { function:&'a Function,state:LoopScope<'a>,known:&'a States<'a>,keys:BTreeMap<Key,usize>,prefix:Vec<Option<usize>>,graph:Graph,parameters:BTreeMap<ValueIdentity,usize> }
impl Builder<'_> {
    fn empty(&mut self)->usize{let id=self.graph.nodes.len();self.graph.nodes.push(Node::default());id}
    fn edge(&mut self,to:usize,from:usize,op:Op,when:bounds::Clause){self.graph.nodes[to].edges.push(Edge{from,op,when:bounds::Condition::guard(when)})}
    fn borrow(&mut self,value:&Borrow,epoch:&Epoch)->usize{let id=self.empty();self.fill(id,value,epoch);id}
    fn read(&mut self,to:usize,from:usize,word:u32,epoch:Epoch){let frame=match &epoch{Epoch::Input=>None,Epoch::Local{memory,..}=>self.read_frame(*memory)};self.edge(to,from,Op::Read{word,epoch,frame},bounds::Clause::default());}
    fn fill(&mut self,id:usize,value:&Borrow,epoch:&Epoch){
        self.fill_with_seeds(id, value, epoch, |seeds, source, when| {
            seeds.sources.insert_when(source.clone(), when);
        });
    }
    fn fill_with_seeds(
        &mut self, id: usize, value: &Borrow, epoch: &Epoch,
        mut add_seed: impl FnMut(&mut Borrow, &Origin, bounds::Clause),
    ) {
        for(source,when)in value.sources.entries(){
            let target=match source {
                Origin::Parameter(input,indices)if self.parameters.contains_key(input)=>{let from=self.parameters[input];let n=self.empty();self.edge(n,from,Op::Select(indices.clone()),bounds::Clause::default());Some(n)},
                Origin::Position(origin)=>Some(self.position(origin,epoch)),
                Origin::Contents{origin,word}=>{let from=self.position(origin,epoch);let n=self.empty();self.read(n,from,*word,epoch.clone());Some(n)},
                Origin::Element{parent,fields,index}=>{let from=self.borrow(parent,epoch);let n=self.empty();self.edge(n,from,Op::Element{fields:fields.clone(),index:*index},bounds::Clause::default());Some(n)},
                Origin::Projection{parent,fields}=>{let from=self.borrow(parent,epoch);let n=self.empty();self.edge(n,from,Op::Projection(fields.clone()),bounds::Clause::default());Some(n)},
                Origin::Relation(graph,root)=>Some(self.import(graph,*root,None,&Epoch::Input)),
                _=>None,
            };
            if let Some(from) = target {
                self.edge(id, from, Op::Identity, when);
            } else {
                add_seed(&mut self.graph.nodes[id].seeds, source, when);
            }
        }
        for(input,when)in value.inputs.entries(){let from=self.key(Key::Loop(*input));self.edge(id,from,Op::Identity,when);}
        for(read,when)in value.reads.entries(){let from=self.key(Key::Read(read.clone()));self.edge(id,from,Op::Identity,when);}
        for(call,when)in value.calls.entries(){let from=self.key(Key::Call(call.clone()));self.edge(id,from,Op::Identity,when);}
    }
    fn position(&mut self,value:&address::Origin,epoch:&Epoch)->usize{
        if let address::Root::Parameter(parameter)=&value.root && let Some(&from)=self.parameters.get(&ValueIdentity::ParameterStorage(*parameter)){
            let id=self.empty();let mut template=value.clone();template.root=address::Root::Derived(Borrow::default());template.positions=Arc::from([]);self.edge(id,from,Op::Position(template),bounds::Clause::default());return id;
        }
        let parent=match &value.root{address::Root::Derived(b)|address::Root::Resource{borrow:b,..}|address::Root::Dataset{borrow:b,..}|address::Root::External(address::Identity::Storage(b))=>Some(b),_=>None};
        let id=self.empty();
        if let Some(parent)=parent{let from=self.borrow(parent,epoch);let mut template=value.map_borrows(|_|Borrow::default());template.positions=Arc::from([]);self.edge(id,from,Op::Position(template),bounds::Clause::default());}
        else{self.graph.nodes[id].seeds.sources.insert(Origin::Position(Arc::new(value.clone())));}
        id
    }
    fn writes(&mut self, memory: usize, names: &BTreeMap<ValueIdentity, ValueIdentity>) -> Vec<Write> {
        let usage = &self.function.uses[memory];
        let Requirement::Memory { borrow, access, written, address_fields, .. } = &usage.requirement else {
            return Vec::new();
        };
        let mut writes = Vec::new();
        let bind = |value: Borrow| {
            if names.is_empty() { value } else { value.output_bound(names) }
        };
        if let Some(call) = self.function.calls.iter().find(|call| call.memory == memory) {
            for access in &self.known[&call.function].interface.access {
                if let Access::Stored { borrows, word, value, when } = access {
                    let target = self.borrow(&bind(call_content_sources(borrows, call, &call.arguments)
                        .at_call(&call.arguments)), &Epoch::Input);
                    let value = self.borrow(&bind(call_content_sources(value, call, &call.arguments)
                        .at_call(&call.arguments)), &Epoch::Input);
                    writes.push(Write {
                        target,
                        fields: Some(vec![(*word, value)]),
                        when: when.at_call(&call.arguments).output_bound(names),
                    });
                }
            }
        } else if access.is_some_and(super::super::Access::writes) {
            let target = if names.is_empty() {
                self.borrow(borrow, &Epoch::Input)
            } else {
                self.borrow(&borrow.output_bound(names), &Epoch::Input)
            };
            let fields = written.as_ref().map(|value| {
                address_fields.iter().map(|&(word, component)| {
                    (word, self.borrow(&bind(value.view_borrow(component)), &Epoch::Input))
                }).collect()
            });
            writes.push(Write { target, fields, when: bounds::Condition::constant(true) });
        }
        writes
    }

    fn frame(&mut self, memory: usize) -> Option<usize> {
        while self.prefix.len() <= memory {
            let at = self.prefix.len() - 1;
            let parent = *self.prefix.last().unwrap();
            self.prefix.push(parent);
            let mut writes = self.writes(at, &BTreeMap::new());
            if writes.is_empty() { continue; }
            let execution = bounds::Condition::execution(&self.function.uses[at].path);
            for write in &mut writes {
                write.when = write.when.clone().logical(execution.clone(), false);
            }
            let id = self.graph.frames.len();
            self.graph.frames.push(Frame { parent, writes });
            self.prefix[at + 1] = Some(id);
        }
        self.prefix[memory]
    }
    fn read_frame(&mut self,memory:usize)->Option<usize>{
        let mut at=self.frame(memory);let mut frames=Vec::new();while let Some(id)=at{frames.push(id);at=self.graph.frames[id].parent;}
        let function = self.function;
        let path = &function.uses[memory].path;
        let mut parent = None;
        let mut normal = bounds::Normalizer::new();
        for (scope, at, prior) in function.carried_writes(memory) {
            let names = function.history_names(at, memory, Some(scope));
            let prior = prior.output_bound(&names);
            let mut writes = self.writes(at, &names);
            for write in &mut writes {
                // An earlier iteration may contribute this write. The current
                // read can still be reached without that iteration's write.
                write.when = bounds::Condition::paths(
                    write.when.assume(&prior, true), bounds::Clause::default(),
                );
            }
            if !writes.is_empty() {
                let id = self.graph.frames.len();
                self.graph.frames.push(Frame { parent, writes });
                parent = Some(id);
            }
        }
        for id in frames.into_iter().rev() {
            // The lexical prefix first names this frame just after its write.
            let at = self.prefix.partition_point(|frame| *frame < Some(id)) - 1;
            let names = function.history_names(at, memory, None);
            let mut writes = if names.is_empty() {
                self.graph.frames[id].writes.clone()
            } else {
                let execution = bounds::Condition::execution(&function.uses[at].path).output_bound(&names);
                let mut writes = self.writes(at, &names);
                for write in &mut writes {
                    write.when = write.when.clone().logical(execution.clone(), false);
                }
                writes
            };
            for write in &mut writes {
                write.when = normal.condition(&bounds::Condition::paths(
                    write.when.assume(path, true), write.when.assume(path, false),
                ));
            }
            let frame=Frame{parent,writes};let id=match self.graph.frames.iter().position(|f|*f==frame){Some(id)=>id,None=>{let id=self.graph.frames.len();self.graph.frames.push(frame);id}};parent=Some(id);
        }
        if let Some(observe) = self.known.observe {
            observe(self.state.name, crate::compiler::BorrowSolveStorage::ReadFrame {
                memory, tables: normal.storage(),
            });
        }
        parent
    }
    fn key(&mut self,key:Key)->usize{
        if let Some(&id)=self.keys.get(&key){return id;}
        let id=self.empty();self.keys.insert(key.clone(),id);
        if let Key::Read(ContentRead::Value { memory, .. } | ContentRead::Call { memory, .. }) = &key
            && self.function.repeated_read(*memory)
        {
            self.graph.variables.insert(id);
        }
        match key{
            Key::Loop(input)=>{self.graph.inputs.insert(input,id);self.graph.variables.insert(id);self.fill(id,&self.function.loops[&input].borrow.clone(),&Epoch::Input)},
            Key::Read(ContentRead::Relation { memory, graph, root, snapshots }) => {
                let prefix = self.read_frame(memory);
                let from = self.import(&graph, root, prefix, &Epoch::Local { memory, snapshots });
                self.edge(id, from, Op::Identity, bounds::Clause::default());
            },
            Key::Read(ContentRead::Value{memory,word,borrow,snapshots})=>{let from=self.borrow(&borrow,&Epoch::Input);self.read(id,from,word,Epoch::Local{memory,snapshots:Some(snapshots)});self.graph.nodes[id].edges.last_mut().unwrap().when=bounds::Condition::execution(&self.function.uses[memory].path);},
            Key::Read(ContentRead::Call{memory,origin,word,snapshots})=>{let from=self.position(&origin,&Epoch::Input);self.read(id,from,word,Epoch::Local{memory,snapshots});self.graph.nodes[id].edges.last_mut().unwrap().when=bounds::Condition::execution(&self.function.uses[memory].path);},
            Key::Read(ContentRead::Snapshot{memory,position:Position::Local(slot),initial,..})=>{
                let from=self.borrow(&initial.view_borrow(0),&Epoch::Input);
                let writes = self.writes(memory, &BTreeMap::new());
                let frame = if writes.is_empty() {
                    None
                } else {
                    let id = self.graph.frames.len();
                    self.graph.frames.push(Frame { parent: None, writes });
                    Some(id)
                };
                let layout=address::Layout::of(&Ty::scalar(Scalar::U64));
                let origin=address::Origin{root:address::Root::Local{base:slot,len:1},layout:layout.clone(),base:layout,words:Arc::from([0]),positions:Arc::from([Position::Local(slot)]),writable:true,initialized:Arc::from([true])};
                let path = self.function.uses[memory].path.clone();
                self.graph.nodes[id].edges.push(Edge {
                    from,
                    op: Op::Initial { origin, frame },
                    when: bounds::Condition::paths(path, bounds::Clause::impossible()),
                });
            },
            Key::Read(read)=>{
                let epoch=match &read{ContentRead::Snapshot{memory,..}|ContentRead::Returned{memory}=>Epoch::Local{memory:*memory,snapshots:None},_=>Epoch::Input};
                let mut evaluation=BorrowEvaluation{function:self.function,state:self.state,known:self.known,expand_loops:false,deferred_reads:true,loop_bindings:Vec::new(),values:BTreeMap::new(),stored: None, exit_writes: None,stored_paths:Default::default()};let value=evaluation.content_read(&read,LoopSources::Deferred);self.fill(id,&value,&epoch);
            },
            Key::Call(result)=>{let call=&self.function.calls[result.call];let value=self.known[&call.function].interface.outputs.get(&result.output).map(|output|call_content_sources(output,call,&call.arguments).at_call(&call.arguments)).unwrap_or_else(||Borrow::source(Origin::Unresolved));self.fill(id,&value,&Epoch::Local{memory:call.memory,snapshots:call.snapshots.clone()});},
        }id
    }
    fn import(&mut self, graph: &Graph, root: usize, prefix: Option<usize>, entry: &Epoch) -> usize {
        let offset=self.graph.nodes.len();self.graph.nodes.resize_with(offset+graph.nodes.len(),Node::default);let frame_offset=self.graph.frames.len();
        self.graph.frames.extend(graph.frames.iter().map(|f|Frame{parent:f.parent.map(|p|frame_offset+p).or(prefix),writes:f.writes.iter().map(|w|Write{target:offset+w.target,fields:w.fields.as_ref().map(|fields|fields.iter().map(|(word,id)|(*word,offset+id)).collect()),when:w.when.clone()}).collect()}));
        if matches!(entry, Epoch::Input) {
            for (input, id) in &graph.inputs {
                self.graph.inputs.entry(*input).or_insert(offset + id);
            }
        }
        for(i,node)in graph.nodes.iter().enumerate(){
            if graph.variables.contains(&i){self.graph.variables.insert(offset+i);}
            self.fill(offset+i,&node.seeds,&Epoch::Input);
            self.graph.nodes[offset + i].edges.extend(node.edges.iter().map(|edge| {
                let mut op = edge.op.clone();
                match &mut op {
                    Op::Read { epoch, frame, .. } => {
                        *frame = frame.map(|f| frame_offset + f).or(prefix);
                        if let Epoch::Local { .. } = entry {
                            *epoch = entry.clone();
                        }
                    }
                    Op::Initial { frame, .. } => {
                        *frame = frame.map(|f| frame_offset + f).or(prefix);
                    }
                    _ => {}
                }
                Edge { from: offset + edge.from, op, when: edge.when.clone() }
            }));
        }offset+root
    }
}
impl Graph {
    fn builder<'a>(function:&'a Function,state:LoopScope<'a>,known:&'a States<'a>)->Builder<'a>{Builder{function,state,known,keys:BTreeMap::new(),prefix:vec![None],graph:Self::default(),parameters:BTreeMap::new()}}
    pub(super) fn recursive_inputs(name:&str,function:&Function,state:LoopScope<'_>,known:&States)->Option<BTreeMap<ValueIdentity,Borrow>>{
        if !function.calls.iter().any(|call|call.function==name){return None;}
        let mut b=Self::builder(function,state,known);let mut roots=BTreeMap::new();
        for(&local,&formal)in &function.parameters{if function.input(formal){
            let id=*roots.entry(formal).or_insert_with(||{let id=b.empty();b.graph.variables.insert(id);b.graph.nodes[id].seeds=Borrow::source(Origin::Parameter(local,Vec::new()));id});
            b.parameters.insert(local,id);
        }}
        // Feedback depends only on edges and writes. Merge the seeds once
        // the graph has a reader, in the original argument order.
        let mut pending_seeds = Vec::new();
        for call in function.calls.iter().filter(|call| call.function == name) {
            for (input, value) in &call.arguments {
                if let Some(&id) = roots.get(input) {
                    let value = value.view_borrow(0).restricted(&call.path);
                    let epoch = Epoch::Local { memory: call.memory, snapshots: call.snapshots.clone() };
                    b.fill_with_seeds(id, &value, &epoch, |_, source, when| {
                        pending_seeds.push((id, source.clone(), when));
                    });
                }
            }
        }
        if !roots.values().any(|root| b.graph.read_feedback(*root)) {
            return None;
        }
        for (id, source, when) in pending_seeds {
            b.graph.nodes[id].seeds.sources.insert_when(source, when);
        }
        let graph=Arc::new(b.graph);let mut evaluation=BorrowEvaluation{function,state,known,expand_loops:true,deferred_reads:false,loop_bindings:Vec::new(),values:BTreeMap::new(),stored: None, exit_writes: None,stored_paths:Default::default()};
        let parameters=residency::input_parameters(&function.parameters,function.input_width);
        Some(roots.into_iter().map(|(input,root)|{let value=graph.solve(root,&mut evaluation);(input,function.address_borrows(&parameters,value).unwrap_or_default())}).collect())
    }
    pub(super) fn component_inputs<'name, 'state>(
        entry: &'name str,
        members: &[&'name str],
        functions: &'state BTreeMap<String, &Function>,
        known: &'state States<'state>,
    ) -> Vec<(&'name str, BTreeMap<ValueIdentity, bounds::Proof>, BTreeMap<ValueIdentity, Borrow>)> {
        fn roots(builder: &mut Builder<'_>, scalars: &BTreeMap<ValueIdentity, bounds::Proof>, initial: bool)
            -> BTreeMap<ValueIdentity,usize>
        {
            scalars.keys().map(|&input| {
                let root = builder.empty();
                builder.graph.variables.insert(root);
                if initial {
                    builder.graph.nodes[root].seeds = Borrow::source(Origin::Parameter(input, Vec::new()));
                }
                (input, root)
            }).collect()
        }

        let function = functions[entry];
        let state = known[entry].loop_scope();
        let mut builder = Self::builder(function, state, known);
        let formals = members.iter().map(|&name| {
            let function = functions[name];
            (name, function.parameters.values().copied().filter(|&formal| function.input(formal)).collect::<BTreeSet<_>>())
        }).collect::<BTreeMap<_,_>>();
        let entry_scalars = formals[entry].iter().map(|&input| {
            (input, bounds::Proof::default().with_identity(input))
        }).collect::<BTreeMap<_,_>>();
        let entry_roots = roots(&mut builder, &entry_scalars, true);
        let mut ports = vec![(entry, entry_scalars, entry_roots)];
        let mut port = 0;
        while port < ports.len() {
            let (caller, scalars, parameters) = ports[port].clone();
            port += 1;
            let function = functions[caller];
            let state = known[caller].loop_scope();
            let public = residency::input_parameters(&function.parameters, function.input_width);
            let mut local = Self::builder(function, state, known);
            let mut operands = Vec::new();
            let empty = BTreeMap::new();
            for call in &function.calls {
                let Some((&callee, inputs)) = formals.get_key_value(call.function.as_str()) else { continue; };
                let when = call.path.project_guard(&public).project_guard(&scalars);
                if !when.possible() { continue; }
                let incoming = inputs.iter().map(|&input| {
                    let scalar = call.arguments.get(&input).map_or_else(bounds::Proof::default, |proof| {
                        let local = proof.numeric_at_call(&empty);
                        if local.known_boolean_value(1).is_some() {
                            return local;
                        }
                        proof.known_identity(0).and_then(|id| function.parameters.get(&id))
                            .and_then(|id| scalars.get(id)).cloned().unwrap_or(local)
                    });
                    (input, scalar)
                }).collect::<BTreeMap<_,_>>();
                let target = match ports.iter().position(|(name, known, _)| *name == callee && *known == incoming) {
                    Some(index) => index,
                    None => {
                        let nodes = roots(&mut builder, &incoming, false);
                        let index = ports.len();
                        ports.push((callee, incoming, nodes));
                        index
                    }
                };
                let epoch = Epoch::Local { memory: call.memory, snapshots: call.snapshots.clone() };
                for (input, proof) in &call.arguments {
                    let Some(&node) = ports[target].2.get(input) else { continue; };
                    let actual = proof.view_borrow(0).restricted(&call.path);
                    if actual.active() {
                        let source = local.borrow(&actual, &epoch);
                        operands.push((node, source, when.clone()));
                    }
                }
            }
            let local = Arc::new(local.graph);
            let mut evaluation = BorrowEvaluation {
                function, state, known, expand_loops: true, deferred_reads: false,
                loop_bindings: Vec::new(), values: BTreeMap::new(), stored: None, exit_writes: None,
                stored_paths: Default::default(),
            };
            let identities = scalars.into_iter().map(|(input, mut proof)| {
                proof.set_view_borrow(0, Borrow::source(Origin::Parameter(input, Vec::new())));
                (input, proof)
            }).collect::<BTreeMap<_,_>>();
            builder.parameters = parameters;
            for (target, source, when) in operands {
                let value = local.solve(source, &mut evaluation);
                if let Some(actual) = function.address_borrows(&public, value) {
                    let source = builder.borrow(&actual.at_call(&identities), &Epoch::Input);
                    builder.edge(target, source, Op::Identity, when);
                }
            }
        }
        let graph = Arc::new(builder.graph);
        let mut evaluation = BorrowEvaluation {
            function, state, known, expand_loops: true, deferred_reads: false,
            loop_bindings: Vec::new(), values: BTreeMap::new(), stored: None, exit_writes: None,
            stored_paths: Default::default(),
        };
        ports.into_iter().map(|(name, scalars, nodes)| {
            let inputs = nodes.into_iter().map(|(input, node)| (input, graph.solve(node, &mut evaluation))).collect();
            (name, scalars, inputs)
        }).collect()
    }

    pub(super) fn loops(
        inputs: &BTreeSet<LoopInput>, function: &Function, name: &str, known: &States,
    ) -> Option<Arc<Self>> {
        if inputs.is_empty() {
            return None;
        }
        let mut builder = Self::builder(function, LoopScope { name, loops: None }, known);
        for &input in inputs {
            builder.key(Key::Loop(input));
        }
        builder.graph.remove_identity_feedback();
        Some(Arc::new(builder.graph))
    }

    pub(super) fn input_root(&self, input: LoopInput) -> usize {
        self.inputs[&input]
    }
    pub(super) fn bind(&self,root:usize,function:&Function,state:LoopScope<'_>,known:&States)->(Arc<Self>,usize){let mut b=Self::builder(function,state,known);let root=b.import(self,root,None,&Epoch::Input);(Arc::new(b.graph),root)}
    pub(super) fn loop_content(read: &ContentRead, function: &Function, state: LoopScope<'_>, known: &States)
        -> (Arc<Self>, usize)
    {
        let mut builder = Self::builder(function, state, known);
        let root = builder.key(Key::Read(read.clone()));
        (Arc::new(builder.graph), root)
    }

    pub(super) fn bind_incoming(
        &self, root: usize, memory: usize, snapshots: Option<Arc<[ValueSnapshot]>>,
        function: &Function, state: LoopScope<'_>, known: &States,
    ) -> (Arc<Self>, usize) {
        let mut builder = Self::builder(function, state, known);
        let prefix = builder.read_frame(memory);
        let root = builder.import(self, root, prefix, &Epoch::Local { memory, snapshots });
        (Arc::new(builder.graph), root)
    }
    fn remove_identity_feedback(&mut self) {
        for &variable in &self.variables {
            let mut edges = std::mem::take(&mut self.nodes[variable].edges);
            edges.retain(|edge| {
                let mut edge = edge;
                while edge.op == Op::Identity {
                    if edge.from == variable {
                        return false;
                    }
                    if self.variables.contains(&edge.from) {
                        break;
                    }
                    let next = &self.nodes[edge.from];
                    let [next_edge] = next.edges.as_slice() else { break; };
                    if !next.seeds.empty() {
                        break;
                    }
                    edge = next_edge;
                }
                true
            });
            self.nodes[variable].edges = edges;
        }
    }

    fn visit_frames(
        &self,
        node: &Node,
        visited: &mut BTreeSet<usize>,
        mut visit: impl FnMut(&Frame),
    ) {
        for edge in &node.edges {
            let (Op::Read { frame, .. } | Op::Initial { frame, .. }) = &edge.op else {
                continue;
            };
            let mut next = *frame;
            while let Some(id) = next {
                if !visited.insert(id) {
                    break;
                }
                let frame = &self.frames[id];
                visit(frame);
                next = frame.parent;
            }
        }
    }

    fn frames_at(&self, id: usize) -> BTreeSet<usize> {
        let mut frames = BTreeSet::new();
        self.visit_frames(&self.nodes[id], &mut frames, |_| {});
        frames
    }

    fn read_feedback(&self, root: usize) -> bool {
        let mut todo = vec![(root, false)];
        let mut seen = BTreeSet::new();
        let mut frames = [BTreeSet::new(), BTreeSet::new()];
        while let Some((id, read)) = todo.pop() {
            if id == root && read {
                return true;
            }
            if !seen.insert((id, read)) {
                continue;
            }
            let node = &self.nodes[id];
            let mut frame_read = read;
            for edge in &node.edges {
                let reads = matches!(edge.op, Op::Read { .. });
                todo.push((edge.from, read || reads));
                frame_read |= reads;
            }
            self.visit_frames(node, &mut frames[usize::from(frame_read)], |frame| {
                for write in &frame.writes {
                    todo.push((write.target, frame_read));
                    todo.extend(write.fields.iter().flatten().map(|(_, source)| (*source, frame_read)));
                }
            });
        }
        false
    }

    fn walk_reachable(
        &self,
        root: usize,
        found: &mut BTreeSet<usize>,
        frames: &mut BTreeSet<usize>,
        mut accepts: impl FnMut(&Node) -> bool,
    ) -> bool {
        let mut todo = vec![root];
        while let Some(id) = todo.pop() {
            if !found.insert(id) {
                continue;
            }
            let node = &self.nodes[id];
            if accepts(node) {
                return true;
            }
            todo.extend(node.edges.iter().map(|edge| edge.from));
            self.visit_frames(node, frames, |frame| {
                for write in &frame.writes {
                    todo.push(write.target);
                    todo.extend(write.fields.iter().flatten().map(|(_, source)| *source));
                }
            });
        }
        false
    }

    fn reachable(&self, root: usize) -> (BTreeSet<usize>, BTreeSet<usize>) {
        let mut nodes = BTreeSet::new();
        let mut frames = BTreeSet::new();
        self.walk_reachable(root, &mut nodes, &mut frames, |_| false);
        (nodes, frames)
    }

    fn any_reachable(&self, root: usize, accepts: impl FnMut(&Node) -> bool) -> bool {
        self.walk_reachable(root, &mut BTreeSet::new(), &mut BTreeSet::new(), accepts)
    }

    fn evaluation_dependencies(&self, id: usize) -> impl Iterator<Item = usize> + '_ {
        let sources = self.nodes[id].edges.iter().map(|edge| edge.from);
        let writes = self.frames_at(id).into_iter().flat_map(|frame| self.frames[frame].writes.iter());
        sources.chain(writes.flat_map(|write| {
            std::iter::once(write.target).chain(write.fields.iter().flatten().map(|(_, value)| *value))
        }))
    }

    fn order(
        &self, reachable: &BTreeSet<usize>, fixed_inputs: Option<&mut BTreeSet<usize>>,
    ) -> Vec<(usize, bool)> {
        let mut counts = BTreeMap::new();
        let mut users: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for &id in reachable.difference(&self.variables) {
            let edges = &self.nodes[id].edges;
            let mut dependencies: BTreeSet<_> = self.evaluation_dependencies(id).collect();

            // A read can also use the evaluation scope's current loop bindings.
            let mut feedback = edges.iter().any(|edge| matches!(edge.op, Op::Read { .. }));
            dependencies.retain(|dependency| {
                let variable = self.variables.contains(dependency);
                feedback |= variable;
                !variable
            });
            counts.insert(id, (dependencies.len(), feedback));
            for dependency in dependencies {
                users.entry(dependency).or_default().push(id);
            }
        }

        let mut ready = counts.iter().filter_map(|(&id, &(count, _))| {
            (count == 0).then_some(id)
        }).collect::<Vec<_>>();
        let mut order = Vec::new();
        while let Some(id) = ready.pop() {
            let feedback = counts[&id].1;
            order.push((id, feedback));
            for &user in users.get(&id).into_iter().flatten() {
                let (count, changes) = counts.get_mut(&user).unwrap();
                *changes |= feedback;
                *count -= 1;
                if *count == 0 {
                    ready.push(user);
                }
            }
        }
        if let Some(fixed) = fixed_inputs {
            fixed.extend(self.variables.intersection(reachable).copied().filter(|&id| {
                !self.nodes[id].edges.iter().any(|edge| matches!(edge.op, Op::Read { .. }))
                    && self.evaluation_dependencies(id).all(|dependency| counts.get(&dependency) == Some(&(0, false)))
            }));
        }
        order
    }
    pub(in crate::check) fn selected(&self,root:usize)->Self{
        let (reachable, frames) = self.reachable(root);
        let order: Vec<_> = std::iter::once(root)
            .chain(reachable.into_iter().filter(|&id| id != root)).collect();
        let indices: BTreeMap<_, _> = order.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let fi: BTreeMap<_, _> = frames.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        let nodes=order.iter().map(|&id|Node{seeds:self.nodes[id].seeds.clone(),edges:self.nodes[id].edges.iter().map(|edge|{let mut op=edge.op.clone();if let Op::Read{frame,..}|Op::Initial{frame,..}=&mut op{*frame=frame.map(|id|fi[&id]);}Edge{from:indices[&edge.from],op,when:edge.when.clone()}}).collect()}).collect();
        let variables=self.variables.iter().filter_map(|id|indices.get(id).copied()).collect();let frames=frames.iter().map(|id|{let f=&self.frames[*id];Frame{parent:f.parent.map(|p|fi[&p]),writes:f.writes.iter().map(|w|Write{target:indices[&w.target],fields:w.fields.as_ref().map(|v|v.iter().map(|(word,id)|(*word,indices[id])).collect()),when:w.when.clone()}).collect()}}).collect();
        let inputs = self.inputs.iter().filter_map(|(input, id)| {
            indices.get(id).map(|&id| (*input, id))
        }).collect();
        Self { nodes, variables, frames, inputs }
    }
    pub(in crate::check) fn has_local(&self, root: usize) -> bool {
        self.any_reachable(root, |node| node.seeds.has_local())
    }
    pub(in crate::check) fn unresolved(&self, root: usize) -> bool {
        self.any_reachable(root, |node| node.seeds.unresolved())
    }
    pub(in crate::check) fn symbolic(&self, root: usize) -> bool {
        self.any_reachable(root, |node| {
            node.seeds.sources.iter().any(|source| {
                matches!(source, Origin::Parameter(..) | Origin::Contents { .. } | Origin::Relation(..))
            })
        })
    }
    pub(in crate::check) fn content_inputs(&self, root: usize) -> bool {
        self.any_reachable(root, |node| {
            node.edges.iter().any(|edge| matches!(edge.op, Op::Read { .. }))
                || node.seeds.content_inputs()
        })
    }
    pub(in crate::check) fn pending(&self, root: usize) -> bool {
        self.any_reachable(root, |node| {
            node.seeds.pending() || node.seeds.sources.iter().any(|source| {
                matches!(source, Origin::Relation(..) | Origin::Element { .. } | Origin::Projection { .. })
                    || matches!(source, Origin::Position(position) if matches!(
                        position.root, address::Root::Derived(_) | address::Root::Resource { .. }
                            | address::Root::Dataset { .. } | address::Root::External(address::Identity::Storage(_))
                    ))
            })
        })
    }
    fn project(self: &Arc<Self>, root: usize, op: Op) -> Borrow {
        let node = &self.nodes[root];
        if matches!(op, Op::Resources | Op::Owners)
            && node.seeds.empty()
            && node.edges.len() == 1
            && node.edges[0].op == op
            && node.edges[0].when.boolean_constant() == Some(true)
        {
            return Borrow::source(Origin::Relation(Arc::clone(self), root));
        }

        let selected = Node {
            seeds: Borrow::default(),
            edges: vec![Edge { from: root, op, when: bounds::Condition::constant(true) }],
        };
        if let Some(id) = self.nodes.iter().position(|node| *node == selected) {
            return Borrow::source(Origin::Relation(Arc::clone(self), id));
        }

        let mut graph = self.as_ref().clone();
        let id = graph.nodes.len();
        graph.nodes.push(selected);
        Borrow::source(Origin::Relation(Arc::new(graph), id))
    }
    pub(in crate::check) fn resources(self: &Arc<Self>, root: usize) -> Borrow {
        self.project(root, Op::Resources)
    }
    pub(in crate::check) fn owners(self: &Arc<Self>, root: usize) -> Borrow {
        self.project(root, Op::Owners)
    }
    pub(in crate::check) fn select(self: &Arc<Self>, root: usize, indices: &[Option<bounds::Selector>]) -> Borrow {
        self.project(root, Op::Select(indices.to_vec()))
    }
    pub(in crate::check) fn seed_borrows(&self, root: usize) -> Vec<&Borrow> {
        let (reachable, _) = self.reachable(root);
        reachable.into_iter().map(|id| &self.nodes[id].seeds).collect()
    }
    pub(in crate::check) fn at_call(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{let mut v=self.clone();for n in &mut v.nodes{n.seeds=n.seeds.at_call(values);for e in &mut n.edges{e.when=e.when.guard_at_call(values);match &mut e.op{Op::Position(p)=>*p=p.bound_at_call(values),Op::Element{index,..}=>*index=Origin::index_at_call(*index,values),Op::Select(indices)=>indices.iter_mut().for_each(|index|*index=Origin::index_at_call(*index,values)),_=>{}}}}for f in &mut v.frames{for w in &mut f.writes{w.when=w.when.at_call(values);}}v}
    pub(in crate::check) fn substitute_values(&self,values:&BTreeMap<ValueIdentity,bounds::Proof>)->Self{let mut v=self.clone();for n in &mut v.nodes{n.seeds=n.seeds.substitute_values(values);for e in &mut n.edges{e.when=e.when.substitute_values(values);match &mut e.op{Op::Position(p)=>*p=p.substitute_values(values),Op::Element{index,..}=>*index=Origin::index_substitute(*index,values),Op::Select(indices)=>indices.iter_mut().for_each(|index|*index=Origin::index_substitute(*index,values)),_=>{}}}}for f in &mut v.frames{for w in &mut f.writes{w.when=w.when.substitute_values(values);}}v}
    pub(in crate::check) fn output_bound(&self,values:&BTreeMap<ValueIdentity,ValueIdentity>)->Self{let mut v=self.clone();for n in &mut v.nodes{n.seeds=n.seeds.output_bound(values);for e in &mut n.edges{e.when=bounds::Condition::guard(e.when.assume(&bounds::Clause::default(),true).output_bound(values));if let Op::Position(p)=&mut e.op{*p=p.bind_outputs(values);}}}for f in &mut v.frames{for w in &mut f.writes{w.when=w.when.output_bound(values);}}v}
    pub(in crate::check) fn normalize(&mut self,normal:&mut bounds::Normalizer){for n in &mut self.nodes{n.seeds.normalize_contract(normal);for e in &mut n.edges{e.when=normal.condition(&e.when);if let Op::Position(p)=&mut e.op{normal.origin(p);}}}for f in &mut self.frames{for w in &mut f.writes{w.when=normal.condition(&w.when);}}}
    pub(super) fn canonical(
        &self, function: &Function, exposed: &BTreeMap<Origin, Borrow>,
        guards: &bounds::PublicGuards<'_>,
    ) -> Self {
        let bindings = || residency::input_parameters(&function.parameters, function.input_width);
        let relation = guards.relation(bindings);
        let fallback = std::cell::LazyCell::new(bindings);
        let input_parameters = relation.map_or_else(|| &*fallback, |scope| &scope.bindings);
        let nodes = self.nodes.iter().map(|node| {
            let seeds = canonical_with_guards(node.seeds.clone(), function, None, exposed, guards, BorrowProjection::Input);
            let edges = node.edges.iter().map(|edge| {
                let when = relation.map_or_else(
                    || edge.when.guard_at_call(input_parameters), |scope| scope.edge(&edge.when),
                );
                let op = match &edge.op {
                    Op::Read { word, frame, .. } => Op::Read {
                        word: *word,
                        epoch: Epoch::Input,
                        frame: *frame,
                    },
                    Op::Position(origin) => {
                        let borrow = Borrow::source(Origin::Position(Arc::new(origin.clone())));
                        let value = canonical_with_guards(borrow, function, None, exposed, guards, BorrowProjection::Input);
                        match value.sources.iter().next() {
                            Some(Origin::Position(position)) => Op::Position(position.as_ref().clone()),
                            _ => edge.op.clone(),
                        }
                    }
                    other => other.clone(),
                };
                Edge { from: edge.from, op, when }
            }).collect();
            Node { seeds, edges }
        }).collect();
        let frames = self.frames.iter().map(|frame| Frame {
            parent: frame.parent,
            writes: frame.writes.iter().map(|write| Write {
                target: write.target,
                fields: write.fields.clone(),
                when: write.when.at_call(input_parameters),
            }).collect(),
        }).collect();
        Self { nodes, variables: self.variables.clone(), frames, inputs: BTreeMap::new() }
    }
    fn extension(&self,other:&Graph)->bool{other.nodes.get(..self.nodes.len())==Some(self.nodes.as_slice())&&other.frames==self.frames&&other.variables==self.variables&&other.inputs==self.inputs}
    fn references(&self,source:&Origin)->bool{
        let borrow=|b:&Borrow|b.sources.iter().any(|s|self.references(s));
        match source{
            Origin::Relation(g,_)=>self.extension(g),
            Origin::Element{parent,..}|Origin::Projection{parent,..}=>borrow(parent),
            Origin::Position(p)|Origin::Contents{origin:p,..}=>match &p.root{address::Root::Derived(b)|address::Root::Resource{borrow:b,..}|address::Root::Dataset{borrow:b,..}|address::Root::External(address::Identity::Storage(b))=>borrow(b),_=>false},
            _=>false,
        }
    }
    fn private(&self, source: &Origin) -> bool {
        !self.references(source) && Borrow::source_has_local(source)
    }
    fn residual(&self, query: &Query<'_>) -> Self {
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for &id in query.values.keys() {
            nodes.extend_from_slice(&self.nodes[nodes.len()..id]);
            nodes.push(query.local_definition(id).into_owned());
        }
        nodes.extend_from_slice(&self.nodes[nodes.len()..]);
        let mut residual = Self {
            nodes,
            variables: self.variables.clone(),
            frames: self.frames.clone(),
            inputs: self.inputs.clone(),
        };
        for(id,node)in residual.nodes.iter_mut().enumerate(){
            node.seeds.retain_sources(|source| !self.private(source));
            if self.variables.contains(&id)||self.nodes[id].edges.iter().any(|e|matches!(e.op,Op::Read{..})) {
                if !query.variables.contains_key(&id)&&!query.values.contains_key(&id){continue;}
                for(source,when)in query.node(id).sources.entries(){
                    if !self.references(source) && !Borrow::source_has_local(source) {
                        node.seeds.sources.union_when(source.clone(), &when);
                    }
                }
            }
        }
        let frames=query.values.keys().chain(query.variables.keys()).flat_map(|id|self.frames_at(*id)).collect::<BTreeSet<_>>();
        for id in frames{
            let frame=&mut residual.frames[id];
            frame.writes.retain(|write|query.node(write.target).sources.entries().any(|(source,when)|when.possible()&&!self.private(source)));
            for write in &frame.writes{for(_,id)in write.fields.iter().flatten(){for(source,when)in query.node(*id).sources.entries(){
                if self.private(source){residual.nodes[*id].seeds.sources.union_when(source.clone(),&when);}
            }}}
        }
        residual
    }
    fn rebind<'r>(&self, value: &Borrow, residual: &impl Fn() -> &'r Arc<Graph>) -> Borrow {
        value.map_sources(|source|match source{
            Origin::Relation(graph, root) if self.extension(&graph) => {
                let residual = residual();
                let tail = &graph.nodes[self.nodes.len()..];
                let graph = if tail.is_empty() {
                    Arc::clone(residual)
                } else {
                    Arc::new(Self {
                        nodes: residual.nodes.iter().chain(tail).cloned().collect(),
                        variables: graph.variables.clone(),
                        frames: residual.frames.clone(),
                        inputs: graph.inputs.clone(),
                    })
                };
                Borrow::source(Origin::Relation(Arc::new(graph.selected(root)), 0))
            },
            Origin::Element{parent,fields,index}=>Borrow::selected_element(self.rebind(&parent,residual),fields,index),
            Origin::Projection{parent,fields}=>Borrow::projection(self.rebind(&parent,residual),fields),
            Origin::Position(p)=>p.map_borrows(|b|self.rebind(b,residual)).normalize(),
            Origin::Contents{origin,word}=>Borrow::source(Origin::Contents{origin:Arc::new(origin.map_borrows(|b|self.rebind(b,residual))),word}),
            other=>Borrow::source(other),
        })
    }
    pub(super) fn solve(self: &Arc<Self>, root: usize, evaluation: &mut BorrowEvaluation<'_>) -> Borrow {
        let (reachable, _) = self.reachable(root);
        let loop_scopes = self.inputs.iter().filter_map(|(input, &node)| {
            evaluation.function.history.get(&input.level).map(|history| (node, (input.level, history)))
        }).collect::<BTreeMap<_, _>>();
        let mut fixed_inputs = BTreeSet::new();
        let (mut stable, feedback): (Vec<_>, Vec<_>) =
            self.order(&reachable, Some(&mut fixed_inputs)).into_iter().partition(|&(_, changes)| !changes);
        let mut variables = self.variables.intersection(&reachable)
            .map(|&id| (id, Borrow::default())).collect::<BTreeMap<_, _>>();
        let mut values = BTreeMap::new();
        let mut initial = true;
        loop {
            let mut scope = BorrowEvaluation {
                function: evaluation.function,
                state: evaluation.state,
                known: evaluation.known,
                expand_loops: evaluation.expand_loops,
                deferred_reads: evaluation.deferred_reads,
                loop_bindings: evaluation.loop_bindings.clone(),
                values: BTreeMap::new(), stored: None, exit_writes: None, stored_paths: super::StoredPaths::Borrowed(evaluation.stored_paths.paths()),
            };
            scope.loop_bindings.push(self.inputs.iter().filter_map(|(input, id)| {
                variables.get(id).map(|value| (*input, value.clone()))
            }).collect());

            let mut query = Query {
                graph: self,
                relation: Arc::clone(self),
                variables: &variables,
                values,
            };
            for (id, _) in stable.drain(..).chain(feedback.iter().copied()) {
                let value = query.definition(id, &mut scope);
                query.values.insert(id, value);
            }
            // A fixed input has no reads or dependencies that can change on a backedge.
            let next = variables.keys().copied().filter(|id| initial || !fixed_inputs.contains(id))
                .map(|id| {
                    let mut value = query.definition(id, &mut scope);
                    if let Some((level, history)) = loop_scopes.get(&id) {
                        // A carried source can come from any earlier iteration.
                        // Its local predicates do not describe the current test.
                        let paths = value.sources.entries().map(|(_, when)| when).collect::<Vec<_>>();
                        let bindings = bounds::Clause::scalar_identities(paths.iter()).into_iter()
                            .filter(|identity| !match identity {
                                ValueIdentity::Register(reg) | ValueIdentity::TaskControl(reg)
                                | ValueIdentity::TaskCompletion(reg, _) => history.registers.contains(reg),
                                ValueIdentity::MemoryValue(memory, _) => history.uses.contains(memory),
                                ValueIdentity::LoopValue(scope, _) => scope == level || evaluation.function.history.get(scope)
                                    .is_some_and(|inner| inner.registers.start >= history.registers.start
                                        && inner.registers.end <= history.registers.end),
                                _ => false,
                            })
                            .map(|identity| (identity, bounds::Proof::default().with_identity(identity)))
                            .collect();
                        value = value.guards_at_call(&bindings);
                    }
                    (id, value)
                }).collect::<BTreeMap<_, _>>();
            if let Some(observe) = scope.known.observe {
                use crate::compiler::{AnalysisStorage, BorrowSolveStorage};
                observe(scope.state.name, BorrowSolveStorage::Relation {
                    root,
                    nodes: AnalysisStorage::vector(&self.nodes),
                    frames: AnalysisStorage::vector(&self.frames),
                    resolved: AnalysisStorage::entries::<usize, Borrow>(query.values.len(), None),
                    previous_inputs: AnalysisStorage::entries::<usize, Borrow>(variables.len(), None),
                    next_inputs: AnalysisStorage::entries::<usize, Borrow>(next.len(), None),
                });
            }
            if next.iter().all(|(id, value)| value.sources.same_sources(&variables[id].sources))
                && next.iter().all(|(id, value)| value.sources.equivalent_paths(&variables[id].sources))
            {
                let value = query.node(root);
                let residual = std::cell::OnceCell::new();
                return self.rebind(value, &|| residual.get_or_init(|| Arc::new(self.residual(&query))));
            }
            values = query.values;
            for &(id, _) in &feedback {
                values.remove(&id);
            }
            variables.extend(next);
            initial = false;
        }
    }
}
struct Query<'g> {
    graph: &'g Graph,
    relation: Arc<Graph>,
    variables: &'g BTreeMap<usize, Borrow>,
    values: BTreeMap<usize, Borrow>,
}
impl<'g> Query<'g> {
    fn node(&self, id: usize) -> &Borrow {
        self.variables.get(&id).unwrap_or_else(|| &self.values[&id])
    }

    fn definition(&self, id: usize, evaluation: &mut BorrowEvaluation<'_>) -> Borrow {
        let mut definition = self.local_definition(id);
        let mut sources = match &mut definition {
            Cow::Borrowed(node) => node.seeds.sources.clone(),
            Cow::Owned(node) => std::mem::take(&mut node.seeds.sources),
        };
        for edge in &definition.edges {
            let from = self.node(edge.from);
            let incoming = self.apply(id, edge.from, &edge.op, from, evaluation);
            let path = edge.when.assume(&bounds::Clause::default(), true);
            sources.union_restricted(&incoming.sources, &path);
        }
        Borrow { sources, ..Borrow::default() }
    }
    fn deferred(&self,id:usize)->Borrow{Borrow::source(Origin::Relation(self.relation.clone(),id))}
    fn apply(
        &self, id: usize, from_id: usize, op: &Op, from: &Borrow, evaluation: &mut BorrowEvaluation<'_>,
    ) -> Borrow {
        if let Op::Read { word, epoch, frame } = op {
            return self.read(id, from_id, from, *word, epoch, *frame, evaluation);
        }
        from.map_sources(|source|match op{Op::Identity=>Borrow::source(source),Op::Resources=>Borrow::source(source).resources(),Op::Owners=>Borrow::source(source).owners(),Op::Select(indices)=>Borrow::source(source).select_elements(indices),Op::Element{fields,index}=>Borrow::selected_element(Borrow::source(source),fields.clone(),*index),Op::Projection(fields)=>Borrow::projection(Borrow::source(source),fields.clone()),Op::Position(p)=>p.map_borrows(|_|Borrow::source(source)).normalize(),Op::Read{..}|Op::Initial{..}=>Borrow::default()})
    }
    fn local_definition(&self, id: usize) -> Cow<'g, Node> {
        let node = &self.graph.nodes[id];
        if !node.edges.iter().any(|edge| matches!(edge.op, Op::Initial { .. })) {
            return Cow::Borrowed(node);
        }
        let mut result = Node { seeds: node.seeds.clone(), edges: Vec::with_capacity(node.edges.len()) };
        for edge in &node.edges {
            let Op::Initial{origin,frame}=&edge.op else{result.edges.push(edge.clone());continue;};
            let path = edge.when.assume(&bounds::Clause::default(), true);
            let position=content_position(origin,0);let mut earlier=bounds::Condition::constant(true);let mut at=*frame;
            while let Some(fi)=at{
                let f=&self.graph.frames[fi];at=f.parent;
                for write in f.writes.iter().rev() {
                    let targets = self.node(write.target);
                    let targets = targets.sources.entries().filter_map(|(target, selected)| {
                        let yes = write.when.assume(&bounds::Clause::both(&path, &selected), true);
                        yes.possible().then_some((target, yes))
                    });
                    let Some(fields) = &write.fields else {
                        // A possible external write contributes a source while retaining the earlier value.
                        for (target, yes) in targets {
                            let affected = match target {
                                Origin::Position(target) if target.symbolic_input() && !target.has_local() => false,
                                Origin::Position(target) => address::MemoryEffect::of(
                                    &Borrow::source(Origin::Position(target.clone())), super::super::Access::Write, None,
                                ).written_bytes(position.as_ref().unwrap(), Scalar::U64, None) != 0,
                                Origin::External => true,
                                _ => false,
                            };
                            if affected {
                                result.seeds.sources.union_when(Origin::External, &earlier.assume(&yes, true));
                            }
                        }
                        continue;
                    };

                    let mut hit = bounds::Clause::impossible();
                    let mut miss = bounds::Clause::impossible();
                    for (target, yes) in targets {
                        let mut complete = false;
                        if let Origin::Position(target) = target {
                            // A formal input cannot reach this invocation's private slots.
                            if target.symbolic_input() && !target.has_local() {
                                miss = bounds::Clause::either(&miss, &yes);
                                continue;
                            }
                            for (word, value) in fields {
                                if let Some(full) = content_write_overlap(
                                    Some((origin, 0)), position.as_ref(), target, *word, || Some(&yes),
                                ) {
                                    result.edges.push(Edge {
                                        from: *value,
                                        op: Op::Identity,
                                        when: {
                                            let visible = earlier.clone()
                                                .logical(bounds::Condition::guard(yes.clone()), false);
                                            // The selected write already includes the read path.
                                            bounds::Condition::paths(
                                                visible.assume(&bounds::Clause::default(), true),
                                                bounds::Clause::either(
                                                    &visible.assume(&bounds::Clause::default(), false),
                                                    &edge.when.assume(&bounds::Clause::default(), false),
                                                ),
                                            )
                                        },
                                    });
                                    hit = bounds::Clause::either(&hit, &yes);
                                    complete |= full;
                                }
                            }
                        } else if matches!(target, Origin::External) {
                            result.seeds.sources.union_when(Origin::External, &earlier.assume(&yes, true));
                        }
                        if !complete {
                            miss = bounds::Clause::either(&miss, &yes);
                        }
                    }
                    let no = bounds::Clause::either(&write.when.assume(&path, false), &miss);
                    earlier = earlier.logical(bounds::Condition::paths(hit, no).negate(), false);
                }
            }
            result.edges.push(Edge{from:edge.from,op:Op::Identity,when:earlier.logical(edge.when.clone(),false)});
        }
        Cow::Owned(result)
    }
    fn binding(&self, mut id: usize) -> (usize, Cow<'g, Node>) {
        loop {
            let node = self.local_definition(id);
            if !self.graph.variables.contains(&id)
                && node.seeds.empty()
                && let [edge] = node.edges.as_slice()
                && edge.op == Op::Identity
            {
                id = edge.from;
            } else {
                return (id, node);
            }
        }
    }

    fn location(&self, id: usize) -> Option<(usize, address::Origin)> {
        let (_, node) = self.binding(id);
        if node.seeds.empty()
            && let [edge] = node.edges.as_slice()
            && let Op::Position(origin) = &edge.op
        {
            Some((self.binding(edge.from).0, origin.clone()))
        } else {
            None
        }
    }
    fn read(
        &self, id: usize, from_id: usize, from: &Borrow, word: u32, epoch: &Epoch, frame: Option<usize>,
        evaluation: &mut BorrowEvaluation<'_>,
    ) -> Borrow {
        let mut output=Borrow::default();
        let read_location = std::cell::LazyCell::new(|| {
            let (binding, origin) = self.location(from_id)?;
            let (_, word) = content_word(&origin, word)?;
            Some((binding, origin, word))
        });
        for(source,path)in from.sources.entries(){
            let Origin::Position(origin)=source else{continue;};let position=content_position(origin,word);let mut content=ContentAtRead::default();let mut at=frame;let mut delayed=false;let mut ready=true;
            while let Some(fi)=at{
                let frame = &self.graph.frames[fi];
                at = frame.parent;
                for write in frame.writes.iter().rev() {
                    let yes=write.when.assume(&path,true);if !yes.possible(){continue;}
                    let matches = write.fields.as_deref().filter(|fields| !fields.is_empty()).and_then(|fields| {
                        let (read_binding, read, read_word) = read_location.as_ref()?;
                        let (write_binding, target) = self.location(write.target)?;
                        let same_binding = *read_binding == write_binding
                            || (!self.graph.variables.contains(read_binding)
                                && !self.graph.variables.contains(&write_binding)
                                && self.graph.nodes[*read_binding] == self.graph.nodes[write_binding]);
                        if !same_binding || read.root != target.root || read.base != target.base {
                            return None;
                        }
                        let read_word = *read_word;
                        Some(fields.iter().filter_map(move |&(field, value)| {
                            let (_, word) = content_word(&target, field)?;
                            (word == read_word).then_some(value)
                        }).peekable())
                    });
                    if let Some(mut values) = matches && values.peek().is_some() {
                        let mut incoming = Borrow::default();
                        for value in values {
                            let value = self.node(value);
                            if value.empty() && !write.when.assume(&path, false).possible() {
                                ready = false;
                                break;
                            }
                            incoming.sources.union(&value.sources);
                        }
                        content.write(&incoming, &write.when, true);
                        continue;
                    }
                    let targets=self.node(write.target);if targets.empty(){ready=false;break;}
                    let mut hit=bounds::Clause::impossible();let mut miss=bounds::Clause::impossible();let mut incoming=Borrow::default();
                    for(target,selected)in targets.sources.entries(){
                        let selected=bounds::Clause::both(&yes,&selected);if !selected.possible(){continue;}
                        let mut complete=false;let mut possible=false;
                        if let Origin::Position(target)=target{
                            if let Some(fields)=&write.fields{
                                for (field,value)in fields{
                                    if let Some(full) = content_write_overlap(
                                        Some((origin, word)), position.as_ref(), target, *field, || Some(&selected),
                                    ) {
                                        let v = self.node(*value);
                                        if v.empty() && !write.when.assume(&path, false).possible() {
                                            ready = false;
                                            break;
                                        }
                                        incoming.sources.union_restricted(&v.sources, &selected);
                                        possible = true;
                                        complete |= full;
                                    } else if origin.symbolic_input() || target.symbolic_input()
                                        || matches!(origin.root, address::Root::Parameter(_))
                                        || matches!(target.root, address::Root::Parameter(_))
                                    {
                                        delayed = true;
                                    }
                                }
                            }else if position.as_ref().is_some_and(|p|address::MemoryEffect::of(&Borrow::source(Origin::Position(target.clone())).restricted(&selected),super::super::Access::Write,None).written_bytes(p,Scalar::U64,None)!=0){possible=true;incoming.sources.union_when(Origin::External, &selected);}
                        }else if matches!(target,Origin::External){possible=true;incoming.sources.union_when(Origin::External, &selected);}
                        if possible{hit=bounds::Clause::either(&hit,&selected);}if !complete{miss=bounds::Clause::either(&miss,&selected);}
                    }
                    let no=bounds::Clause::either(&write.when.assume(&path,false),&miss);content.write(&incoming,&bounds::Condition::paths(hit,no),true);
                }
                if !ready{break;}
            }
            let value=if !ready{Borrow::default()}else if delayed{self.deferred(id)}else if content.earlier.as_ref().is_some_and(|p|!p.possible()){content.with_initial(Borrow::default())}else if origin.symbolic_input(){self.deferred(id)}else{
                let base=match epoch{Epoch::Input=>input_contents(origin,word),Epoch::Local{memory,snapshots}=>evaluation.content_before(origin,word,*memory,snapshots.as_deref())};
                if base.sources.iter().any(|s|matches!(s,Origin::Contents{..})){self.deferred(id)}else{content.with_initial(base)}
            };output.sources.union_restricted(&value.sources, &path);
        }output
    }
}

// Physical construction for the original public contract wire.
crate::codec::contracts::codec!(Op);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Reader {
        identity: ReadCallback,
        position: ReadCallback,
        read: ReadCallback,
        element: ReadCallback,
        projection: ReadCallback,
        resources: ReadCallback,
        owners: ReadCallback,
        select: ReadCallback,
        initial: ReadCallback,
    }
    unsafe extern "C" fn build_identity(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Op>() = Op::Identity;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_position(context: Slot, destination: Slot) -> ReadStatus {
        let mut origin = <address::Origin as Native>::empty();
        #[repr(C)]
        struct Fields {
            origin: Slot,
        }
        #[repr(C)]
        struct Host {
            origin: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_position"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    origin: contracts::slot(&mut origin),
                },
                Host {
                    origin: contracts::read_value::<address::Origin>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Position(origin);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_read(context: Slot, destination: Slot) -> ReadStatus {
        let mut word = <u32 as Native>::empty();
        let mut frame = <Option<usize> as Native>::empty();
        #[repr(C)]
        struct Fields {
            word: Slot,
            frame: Slot,
        }
        #[repr(C)]
        struct Host {
            word: ReadCallback,
            frame: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_read"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    word: contracts::slot(&mut word),
                    frame: contracts::slot(&mut frame),
                },
                Host {
                    word: contracts::read_value::<u32>,
                    frame: contracts::read_value::<Option<usize>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Read {
                word,
                epoch: Epoch::Input,
                frame,
            };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_element(context: Slot, destination: Slot) -> ReadStatus {
        let mut fields = <Vec<usize> as Native>::empty();
        let mut index = <Option<bounds::Selector> as Native>::empty();
        #[repr(C)]
        struct Fields {
            fields: Slot,
            index: Slot,
        }
        #[repr(C)]
        struct Host {
            fields: ReadCallback,
            index: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_element"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    fields: contracts::slot(&mut fields),
                    index: contracts::slot(&mut index),
                },
                Host {
                    fields: contracts::read_value::<Vec<usize>>,
                    index: contracts::read_value::<Option<bounds::Selector>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Element { fields, index };
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_projection(context: Slot, destination: Slot) -> ReadStatus {
        let mut fields = <Vec<usize> as Native>::empty();
        #[repr(C)]
        struct Fields {
            fields: Slot,
        }
        #[repr(C)]
        struct Host {
            fields: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_projection"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    fields: contracts::slot(&mut fields),
                },
                Host {
                    fields: contracts::read_value::<Vec<usize>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Projection(fields);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_resources(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Op>() = Op::Resources;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_owners(_context: Slot, destination: Slot) -> ReadStatus {
        unsafe {
            *destination.cast::<Op>() = Op::Owners;
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_select(context: Slot, destination: Slot) -> ReadStatus {
        let mut indices = <Vec<Option<bounds::Selector>> as Native>::empty();
        #[repr(C)]
        struct Fields {
            indices: Slot,
        }
        #[repr(C)]
        struct Host {
            indices: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_select"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    indices: contracts::slot(&mut indices),
                },
                Host {
                    indices: contracts::read_value::<Vec<Option<bounds::Selector>>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Select(indices);
        }
        ReadStatus::Success
    }
    unsafe extern "C" fn build_initial(context: Slot, destination: Slot) -> ReadStatus {
        let mut origin = <address::Origin as Native>::empty();
        let mut frame = <Option<usize> as Native>::empty();
        #[repr(C)]
        struct Fields {
            origin: Slot,
            frame: Slot,
        }
        #[repr(C)]
        struct Host {
            origin: ReadCallback,
            frame: ReadCallback,
        }
        unsafe extern "C" {
            #[link_name = "bootstrap.contract_codec::contract_read_relation_op_initial"]
            fn payload(context: Slot, fields: Fields, host: Host) -> ReadStatus;
        }
        let status = unsafe {
            payload(
                context,
                Fields {
                    origin: contracts::slot(&mut origin),
                    frame: contracts::slot(&mut frame),
                },
                Host {
                    origin: contracts::read_value::<address::Origin>,
                    frame: contracts::read_value::<Option<usize>>,
                },
            )
        };
        if status != ReadStatus::Success {
            return status;
        }
        unsafe {
            *destination.cast::<Op>() = Op::Initial { origin, frame };
        }
        ReadStatus::Success
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_read_relation_op"]
        fn read(
            input: contracts::Bytes,
            position: *mut usize,
            context: Slot,
            output: Slot,
            host: Reader,
        ) -> ReadStatus;
    }
    impl Native for Op {
        fn empty() -> Self {
            Self::Identity
        }
        unsafe fn write(&self, destination: Slot) {
            match self {
                Self::Identity => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_identity"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Position(origin) => {
                    #[repr(C)]
                    struct Fields {
                        origin: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        origin: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_position"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                origin: contracts::address(origin).cast_mut(),
                            },
                            Host {
                                origin: contracts::write_value::<address::Origin>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Read { word, frame, .. } => {
                    #[repr(C)]
                    struct Fields {
                        word: Slot,
                        frame: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        word: WriteCallback,
                        frame: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_read"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                word: contracts::address(word).cast_mut(),
                                frame: contracts::address(frame).cast_mut(),
                            },
                            Host {
                                word: contracts::write_value::<u32>,
                                frame: contracts::write_value::<Option<usize>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Element { fields, index } => {
                    #[repr(C)]
                    struct Fields {
                        fields: Slot,
                        index: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        fields: WriteCallback,
                        index: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_element"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                fields: contracts::address(fields).cast_mut(),
                                index: contracts::address(index).cast_mut(),
                            },
                            Host {
                                fields: contracts::write_value::<Vec<usize>>,
                                index: contracts::write_value::<Option<bounds::Selector>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Projection(fields) => {
                    #[repr(C)]
                    struct Fields {
                        fields: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        fields: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_projection"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                fields: contracts::address(fields).cast_mut(),
                            },
                            Host {
                                fields: contracts::write_value::<Vec<usize>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Resources => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_resources"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Owners => {
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_owners"]
                        fn write(
                            output: Slot,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe { write(destination, contracts::append) }
                }
                Self::Select(indices) => {
                    #[repr(C)]
                    struct Fields {
                        indices: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        indices: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_select"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                indices: contracts::address(indices).cast_mut(),
                            },
                            Host {
                                indices: contracts::write_value::<Vec<Option<bounds::Selector>>>,
                            },
                            contracts::append,
                        )
                    }
                }
                Self::Initial { origin, frame } => {
                    #[repr(C)]
                    struct Fields {
                        origin: Slot,
                        frame: Slot,
                    }
                    #[repr(C)]
                    struct Host {
                        origin: WriteCallback,
                        frame: WriteCallback,
                    }
                    unsafe extern "C" {
                        #[link_name = "bootstrap.contract_codec::contract_write_relation_op_initial"]
                        fn write(
                            output: Slot,
                            fields: Fields,
                            host: Host,
                            append: unsafe extern "C" fn(Slot, *const u8, usize),
                        );
                    }
                    unsafe {
                        write(
                            destination,
                            Fields {
                                origin: contracts::address(origin).cast_mut(),
                                frame: contracts::address(frame).cast_mut(),
                            },
                            Host {
                                origin: contracts::write_value::<address::Origin>,
                                frame: contracts::write_value::<Option<usize>>,
                            },
                            contracts::append,
                        )
                    }
                }
            }
        }
        unsafe fn read(context: Slot, destination: &mut Self) -> ReadStatus {
            let (bytes, position) = unsafe { contracts::input(context) };
            unsafe {
                read(
                    bytes,
                    position,
                    context,
                    contracts::slot(destination),
                    Reader {
                        identity: build_identity,
                        position: build_position,
                        read: build_read,
                        element: build_element,
                        projection: build_projection,
                        resources: build_resources,
                        owners: build_owners,
                        select: build_select,
                        initial: build_initial,
                    },
                )
            }
        }
    }
};

crate::codec::contracts::codec!(Edge);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        from: Slot,
        op: Slot,
        when: Slot,
    }
    #[repr(C)]
    struct Writer {
        from: WriteCallback,
        op: WriteCallback,
        when: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        from: ReadCallback,
        op: ReadCallback,
        when: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_relation_edge"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_relation_edge"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Edge {
        fn empty() -> Self {
            Self {
                from: <usize as Native>::empty(),
                op: <Op as Native>::empty(),
                when: <bounds::Condition as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        from: contracts::address(&self.from).cast_mut(),
                        op: contracts::address(&self.op).cast_mut(),
                        when: contracts::address(&self.when).cast_mut(),
                    },
                    Writer {
                        from: contracts::write_value::<usize>,
                        op: contracts::write_value::<Op>,
                        when: contracts::write_value::<bounds::Condition>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        from: contracts::slot(&mut output.from),
                        op: contracts::slot(&mut output.op),
                        when: contracts::slot(&mut output.when),
                    },
                    Reader {
                        from: contracts::read_value::<usize>,
                        op: contracts::read_value::<Op>,
                        when: contracts::read_value::<bounds::Condition>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Node);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        seeds: Slot,
        edges: Slot,
    }
    #[repr(C)]
    struct Writer {
        seeds: WriteCallback,
        edges: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        seeds: ReadCallback,
        edges: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_relation_node"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_relation_node"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Node {
        fn empty() -> Self {
            Self {
                seeds: <Borrow as Native>::empty(),
                edges: <Vec<Edge> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        seeds: contracts::address(&self.seeds).cast_mut(),
                        edges: contracts::address(&self.edges).cast_mut(),
                    },
                    Writer {
                        seeds: contracts::write_value::<Borrow>,
                        edges: contracts::write_value::<Vec<Edge>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        seeds: contracts::slot(&mut output.seeds),
                        edges: contracts::slot(&mut output.edges),
                    },
                    Reader {
                        seeds: contracts::read_value::<Borrow>,
                        edges: contracts::read_value::<Vec<Edge>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Write);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        target: Slot,
        fields: Slot,
        when: Slot,
    }
    #[repr(C)]
    struct Writer {
        target: WriteCallback,
        fields: WriteCallback,
        when: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        target: ReadCallback,
        fields: ReadCallback,
        when: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_relation_write"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_relation_write"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Write {
        fn empty() -> Self {
            Self {
                target: <usize as Native>::empty(),
                fields: <Option<Vec<(u32, usize)>> as Native>::empty(),
                when: <bounds::Condition as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        target: contracts::address(&self.target).cast_mut(),
                        fields: contracts::address(&self.fields).cast_mut(),
                        when: contracts::address(&self.when).cast_mut(),
                    },
                    Writer {
                        target: contracts::write_value::<usize>,
                        fields: contracts::write_value::<Option<Vec<(u32, usize)>>>,
                        when: contracts::write_value::<bounds::Condition>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        target: contracts::slot(&mut output.target),
                        fields: contracts::slot(&mut output.fields),
                        when: contracts::slot(&mut output.when),
                    },
                    Reader {
                        target: contracts::read_value::<usize>,
                        fields: contracts::read_value::<Option<Vec<(u32, usize)>>>,
                        when: contracts::read_value::<bounds::Condition>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Frame);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        parent: Slot,
        writes: Slot,
    }
    #[repr(C)]
    struct Writer {
        parent: WriteCallback,
        writes: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        parent: ReadCallback,
        writes: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_relation_frame"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_relation_frame"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Frame {
        fn empty() -> Self {
            Self {
                parent: <Option<usize> as Native>::empty(),
                writes: <Vec<Write> as Native>::empty(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        parent: contracts::address(&self.parent).cast_mut(),
                        writes: contracts::address(&self.writes).cast_mut(),
                    },
                    Writer {
                        parent: contracts::write_value::<Option<usize>>,
                        writes: contracts::write_value::<Vec<Write>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        parent: contracts::slot(&mut output.parent),
                        writes: contracts::slot(&mut output.writes),
                    },
                    Reader {
                        parent: contracts::read_value::<Option<usize>>,
                        writes: contracts::read_value::<Vec<Write>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            ReadStatus::Success
        }
    }
};

crate::codec::contracts::codec!(Graph);

const _: () = {
    use crate::codec::ReadStatus;
    use crate::codec::contracts::{
        self, Native, Read as ReadCallback, Slot, Write as WriteCallback,
    };
    #[repr(C)]
    struct Fields {
        nodes: Slot,
        variables: Slot,
        frames: Slot,
    }
    #[repr(C)]
    struct Writer {
        nodes: WriteCallback,
        variables: WriteCallback,
        frames: WriteCallback,
    }
    #[repr(C)]
    struct Reader {
        nodes: ReadCallback,
        variables: ReadCallback,
        frames: ReadCallback,
    }
    unsafe extern "C" {
        #[link_name = "bootstrap.contract_codec::contract_write_relation_graph"]
        fn write(output: Slot, fields: Fields, host: Writer);
        #[link_name = "bootstrap.contract_codec::contract_read_relation_graph"]
        fn read(context: Slot, fields: Fields, host: Reader) -> ReadStatus;
    }
    impl Native for Graph {
        fn empty() -> Self {
            Self {
                nodes: <Vec<Node> as Native>::empty(),
                variables: <BTreeSet<usize> as Native>::empty(),
                frames: <Vec<Frame> as Native>::empty(),
                inputs: BTreeMap::new(),
            }
        }
        unsafe fn write(&self, output: Slot) {
            unsafe {
                write(
                    output,
                    Fields {
                        nodes: contracts::address(&self.nodes).cast_mut(),
                        variables: contracts::address(&self.variables).cast_mut(),
                        frames: contracts::address(&self.frames).cast_mut(),
                    },
                    Writer {
                        nodes: contracts::write_value::<Vec<Node>>,
                        variables: contracts::write_value::<BTreeSet<usize>>,
                        frames: contracts::write_value::<Vec<Frame>>,
                    },
                )
            }
        }
        unsafe fn read(context: Slot, output: &mut Self) -> ReadStatus {
            let status = unsafe {
                read(
                    context,
                    Fields {
                        nodes: contracts::slot(&mut output.nodes),
                        variables: contracts::slot(&mut output.variables),
                        frames: contracts::slot(&mut output.frames),
                    },
                    Reader {
                        nodes: contracts::read_value::<Vec<Node>>,
                        variables: contracts::read_value::<BTreeSet<usize>>,
                        frames: contracts::read_value::<Vec<Frame>>,
                    },
                )
            };
            if status != ReadStatus::Success {
                return status;
            }
            if output.nodes.is_empty()||output.variables.iter().any(|id|*id>=output.nodes.len())||output.nodes.iter().any(|n|n.edges.iter().any(|e|e.from>=output.nodes.len()||matches!(&e.op,Op::Read{frame:Some(f),..}|Op::Initial{frame:Some(f),..}if *f>=output.frames.len())))||output.frames.iter().enumerate().any(|(i,f)|f.parent.is_some_and(|p|p>=i)||f.writes.iter().any(|w|w.target>=output.nodes.len()||w.fields.iter().flatten().any(|(_,v)|*v>=output.nodes.len()))){return unsafe{contracts::fail(context,"invalid bound read relation")};}
            if output.order(&(0..output.nodes.len()).collect(), None).len()
                != output.nodes.len() - output.variables.len()
            {
                return unsafe {
                    contracts::fail(context, "cyclic read expression without an input binding")
                };
            }
            ReadStatus::Success
        }
    }
};
